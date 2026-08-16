mod connect_prep;
mod deps;
mod diagnostics;
mod helper_install;
mod hiddify_reset;
mod network;
mod traffic;
mod tray;
mod version;
mod window_state;

use chrono::Utc;
use iran_split_config::{AppConfig, ConfigStore, ValidationIssue};
use iran_split_core::{
    Engine, LifecycleBusy, OperationAccepted, PlatformBackend, StackPhase, StackSnapshot,
};
use iran_split_mihomo::ControllerClient;
use iran_split_rules::{
    bundled_snapshot_is_complete, ensure_bundled_snapshot, CloudRuleStore, CloudRulesStatus,
    DirectRulesDocument, DohResolver, Outbound, RuleManager, RuleSet,
};
use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tauri::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, LogicalSize, Manager, Runtime, Size, Window, WindowEvent,
};
use tauri_plugin_updater::UpdaterExt;
use tracing::{error, info, warn};
use uuid::Uuid;

#[cfg(target_os = "linux")]
use iran_split_platform_linux::{LinuxBackend as NativeBackend, LinuxPaths};
#[cfg(target_os = "windows")]
use iran_split_platform_win::{WindowsBackend as NativeBackend, WindowsPaths, HELPER_PIPE};

#[derive(Debug)]
struct AppServices {
    config_store: ConfigStore,
    engine: Arc<Engine<NativeBackend>>,
    backend: Arc<NativeBackend>,
    rules: RuleManager,
    cloud_rules: CloudRuleStore,
    network: network::NetworkMonitor,
    paths: AppPaths,
    updates: Arc<UpdateCoordinator>,
    traffic_lock: tokio::sync::Mutex<()>,
}

struct UpdateCoordinator {
    lock: tokio::sync::Mutex<()>,
    cancel: AtomicBool,
}

impl std::fmt::Debug for UpdateCoordinator {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("UpdateCoordinator")
            .field("cancel", &self.cancel.load(Ordering::Relaxed))
            .finish_non_exhaustive()
    }
}

impl UpdateCoordinator {
    fn new() -> Self {
        Self {
            lock: tokio::sync::Mutex::new(()),
            cancel: AtomicBool::new(false),
        }
    }

    fn request_cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }

    fn begin(&self) -> Result<tokio::sync::MutexGuard<'_, ()>, String> {
        let guard = self
            .lock
            .try_lock()
            .map_err(|_| update_in_progress_message())?;
        self.cancel.store(false, Ordering::SeqCst);
        Ok(guard)
    }

    fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }
}

fn update_in_progress_message() -> String {
    "an update is already in progress".into()
}

fn update_check_cancelled(app: &AppHandle) -> bool {
    services(app)
        .ok()
        .is_some_and(|services| services.updates.is_cancelled())
}

#[derive(Debug, Clone)]
struct AppPaths {
    config: PathBuf,
    data: PathBuf,
    cache: PathBuf,
    resources: PathBuf,
    dependencies: PathBuf,
}

#[cfg(target_os = "linux")]
const PRODUCTION_HELPER_SOCKET: &str = "/run/iran-split/helper.sock";
#[cfg(target_os = "linux")]
const PRODUCTION_SYSTEM_RUNTIME: &str = "/var/lib/iran-split";

/// Written by the elevated installer in `iran-split-helper::install` (ADR 0029).
#[cfg(target_os = "windows")]
const WINDOWS_SYSTEM_RUNTIME: &str = r"C:\ProgramData\iran-split\runtime";
#[cfg(target_os = "windows")]
const WINDOWS_PROGRAMDATA_MIHOMO: &str = r"C:\ProgramData\iran-split\bin\mihomo.exe";

/// The pipe and system runtime root are fixed by the SYSTEM scheduled task, so
/// unlike Linux there is no development override to apply.
#[cfg(target_os = "windows")]
fn windows_helper_paths() -> (String, PathBuf) {
    (
        HELPER_PIPE.to_owned(),
        PathBuf::from(WINDOWS_SYSTEM_RUNTIME),
    )
}

/// The helper copies Mihomo next to itself, so that copy is the fallback when
/// the user has not installed one under the app data directory.
#[cfg(target_os = "windows")]
fn windows_programdata_mihomo() -> PathBuf {
    PathBuf::from(WINDOWS_PROGRAMDATA_MIHOMO)
}

#[cfg(target_os = "linux")]
fn linux_helper_paths() -> (PathBuf, PathBuf) {
    #[cfg(debug_assertions)]
    {
        linux_helper_paths_with_overrides(
            std::env::var_os("BIFLOW_DEV_HELPER_SOCKET"),
            std::env::var_os("BIFLOW_DEV_SYSTEM_RUNTIME"),
        )
    }
    #[cfg(not(debug_assertions))]
    {
        (
            PathBuf::from(PRODUCTION_HELPER_SOCKET),
            PathBuf::from(PRODUCTION_SYSTEM_RUNTIME),
        )
    }
}

#[cfg(all(target_os = "linux", debug_assertions))]
fn linux_helper_paths_with_overrides(
    socket: Option<std::ffi::OsString>,
    runtime: Option<std::ffi::OsString>,
) -> (PathBuf, PathBuf) {
    (
        socket.map_or_else(|| PathBuf::from(PRODUCTION_HELPER_SOCKET), PathBuf::from),
        runtime.map_or_else(|| PathBuf::from(PRODUCTION_SYSTEM_RUNTIME), PathBuf::from),
    )
}

#[cfg(target_os = "linux")]
fn linux_mihomo_binary(default: PathBuf) -> PathBuf {
    #[cfg(debug_assertions)]
    {
        linux_mihomo_binary_with_override(default, std::env::var_os("BIFLOW_DEV_MIHOMO_BINARY"))
    }
    #[cfg(not(debug_assertions))]
    {
        default
    }
}

#[cfg(all(target_os = "linux", debug_assertions))]
fn linux_mihomo_binary_with_override(
    default: PathBuf,
    override_path: Option<std::ffi::OsString>,
) -> PathBuf {
    override_path.map_or(default, PathBuf::from)
}

const BUNDLE_IDENTIFIER: &str = "app.biflow.desktop";

#[cfg(target_os = "linux")]
const WEBKIT_DISABLE_DMABUF_RENDERER: &str = "WEBKIT_DISABLE_DMABUF_RENDERER";
#[cfg(target_os = "linux")]
const WEBKIT_DISABLE_COMPOSITING_MODE: &str = "WEBKIT_DISABLE_COMPOSITING_MODE";
#[cfg(target_os = "linux")]
const LIBGL_ALWAYS_SOFTWARE: &str = "LIBGL_ALWAYS_SOFTWARE";

#[cfg(target_os = "linux")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LinuxWebviewWorkarounds {
    disable_dmabuf: bool,
    disable_compositing: bool,
    software_gl: bool,
}

#[cfg(target_os = "linux")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LinuxWebviewEnv {
    dmabuf_already_set: bool,
    compositing_already_set: bool,
    software_gl_already_set: bool,
}

#[cfg(target_os = "linux")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LinuxGpuKind {
    virtual_or_nvidia: bool,
    virtual_machine: bool,
}

fn single_instance_dbus_id(identifier: &str, version: &str) -> String {
    format!("{identifier}.v{}", version.replace('.', "_"))
}

#[cfg(target_os = "linux")]
fn linux_dmi_is_virtual(vendor: Option<&str>) -> bool {
    vendor.is_some_and(|value| {
        let vendor = value.trim().to_ascii_lowercase();
        vendor.contains("vmware")
            || vendor.contains("qemu")
            || vendor.contains("virtualbox")
            || vendor.contains("microsoft corporation")
            || vendor.contains("xen")
            || vendor.contains("bochs")
            || vendor.contains("parallels")
            || vendor.contains("amazon")
            || vendor.contains("google")
    })
}

#[cfg(target_os = "linux")]
fn linux_virtual_machine() -> bool {
    linux_dmi_is_virtual(
        fs::read_to_string("/sys/class/dmi/id/sys_vendor")
            .ok()
            .as_deref(),
    )
}

#[cfg(target_os = "linux")]
fn linux_nvidia_gpu() -> bool {
    Path::new("/dev/nvidia0").exists() || Path::new("/proc/driver/nvidia/version").exists()
}

#[cfg(target_os = "linux")]
fn linux_webview_workarounds(env: LinuxWebviewEnv, gpu: LinuxGpuKind) -> LinuxWebviewWorkarounds {
    LinuxWebviewWorkarounds {
        disable_dmabuf: !env.dmabuf_already_set,
        disable_compositing: gpu.virtual_or_nvidia && !env.compositing_already_set,
        software_gl: gpu.virtual_machine && !env.software_gl_already_set,
    }
}

#[cfg(target_os = "linux")]
fn linux_webview_reexec_needed(already_relaunched: bool, planned: LinuxWebviewWorkarounds) -> bool {
    !already_relaunched
        && (planned.disable_dmabuf || planned.disable_compositing || planned.software_gl)
}

#[cfg(target_os = "linux")]
fn apply_linux_webview_workarounds() {
    use std::os::unix::process::CommandExt;
    let virtual_machine = linux_virtual_machine();
    let planned = linux_webview_workarounds(
        LinuxWebviewEnv {
            dmabuf_already_set: std::env::var_os(WEBKIT_DISABLE_DMABUF_RENDERER).is_some(),
            compositing_already_set: std::env::var_os(WEBKIT_DISABLE_COMPOSITING_MODE).is_some(),
            software_gl_already_set: std::env::var_os(LIBGL_ALWAYS_SOFTWARE).is_some(),
        },
        LinuxGpuKind {
            virtual_or_nvidia: virtual_machine || linux_nvidia_gpu(),
            virtual_machine,
        },
    );
    if !linux_webview_reexec_needed(
        std::env::var_os("BIFLOW_WEBKIT_WORKAROUNDS").is_some(),
        planned,
    ) {
        return;
    }
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("/proc/self/exe"));
    let mut command = linux_webview_reexec_command(exe, std::env::args_os().skip(1), planned);
    // Replace this process so a waiting parent (and its terminal) does not linger.
    let cause = command.exec();
    eprintln!("BiFlow could not relaunch with WebKit view workarounds: {cause}");
}

#[cfg(target_os = "linux")]
fn linux_webview_reexec_command(
    exe: PathBuf,
    args: impl IntoIterator<Item = std::ffi::OsString>,
    planned: LinuxWebviewWorkarounds,
) -> std::process::Command {
    let mut command = std::process::Command::new(exe);
    command.args(args);
    command.env("BIFLOW_WEBKIT_WORKAROUNDS", "1");
    if planned.disable_dmabuf {
        command.env(WEBKIT_DISABLE_DMABUF_RENDERER, "1");
    }
    if planned.disable_compositing {
        command.env(WEBKIT_DISABLE_COMPOSITING_MODE, "1");
    }
    if planned.software_gl {
        command.env(LIBGL_ALWAYS_SOFTWARE, "1");
    }
    command
}

#[cfg(target_os = "linux")]
fn log_linux_webview_workarounds() {
    info!(
        event = "webview.linux_workarounds",
        section = "window",
        initiator = "application_process",
        cause = "webkitgtk_dmabuf_blank_view",
        trace_route = "application_process->run->webkit_env",
        dmabuf_disabled = std::env::var_os(WEBKIT_DISABLE_DMABUF_RENDERER).is_some(),
        compositing_disabled = std::env::var_os(WEBKIT_DISABLE_COMPOSITING_MODE).is_some(),
        software_gl = std::env::var_os(LIBGL_ALWAYS_SOFTWARE).is_some(),
        "Linux WebKit view workarounds applied"
    );
}

impl AppPaths {
    fn discover(app: &AppHandle) -> Result<Self, String> {
        let config = dirs::config_dir()
            .ok_or("configuration directory is unavailable")?
            .join("biflow")
            .join("config.toml");
        let data = dirs::data_local_dir()
            .ok_or("local data directory is unavailable")?
            .join("biflow");
        let cache = dirs::cache_dir()
            .ok_or("cache directory is unavailable")?
            .join("biflow");
        let resource_root = app
            .path()
            .resource_dir()
            .map_err(|error| error.to_string())?;
        let resources = packaged_rule_snapshot_dir(&resource_root);
        let dependencies = resource_root.join("dependencies");
        fs::create_dir_all(&data).map_err(|error| error.to_string())?;
        fs::create_dir_all(&cache).map_err(|error| error.to_string())?;
        Ok(Self {
            config,
            data,
            cache,
            resources,
            dependencies,
        })
    }
}

fn packaged_rule_snapshot_dir(resource_root: &Path) -> PathBuf {
    [
        resource_root.join("rules"),
        resource_root.join("resources").join("rules"),
        resource_root.join("_up_").join("resources").join("rules"),
    ]
    .into_iter()
    .find(|dir| bundled_snapshot_is_complete(dir))
    .unwrap_or_else(|| resource_root.join("rules"))
}

#[derive(Debug, Clone, Serialize)]
struct BootstrapResult {
    app_version: String,
    platform: String,
    mock_mode: bool,
    snapshot: StackSnapshot,
    settings: AppConfig,
    direct_rules: DirectRulesDocument,
    cloud_rules: CloudRulesStatus,
    dependencies: Vec<deps::DependencyStatus>,
    network_status: network::NetworkStatus,
}

#[derive(Debug, Clone, Serialize)]
struct RouteTestResult {
    target: String,
    outbound: Outbound,
    reason: String,
    matched_rule: Option<String>,
    reachable: Option<bool>,
    tested_at: String,
}

#[derive(Debug, Clone, Serialize)]
struct DiagnosticStep {
    id: String,
    label: String,
    status: String,
    detail: Option<String>,
    started_at: Option<String>,
    finished_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct DiagnosticsReport {
    operation_id: Uuid,
    steps: Vec<DiagnosticStep>,
    finished: bool,
}

#[derive(Debug, Clone, Serialize)]
struct ExportResult {
    path: String,
    files: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "IPC shape matches the About page channel flags"
)]
struct UpdateStatus {
    available: bool,
    version: Option<String>,
    notes: Option<String>,
    app_available: bool,
    rules_available: bool,
    thirdparty_available: bool,
}

#[derive(Debug, Clone, Serialize)]
struct UpdateProgress {
    phase: String,
    percent: Option<u8>,
    version: Option<String>,
    error: Option<String>,
}

fn update_download_percent(downloaded: usize, total: Option<u64>) -> Option<u8> {
    let total = total?;
    if total == 0 {
        return Some(0);
    }
    let downloaded = u128::from(u64::try_from(downloaded).unwrap_or(u64::MAX));
    let total = u128::from(total);
    Some(u8::try_from((downloaded.saturating_mul(100) / total).min(100)).unwrap_or(100))
}

fn emit_update_progress<R: Runtime>(app: &AppHandle<R>, progress: UpdateProgress) {
    if let Err(cause) = app.emit("update-progress", progress) {
        warn!(
            event = "update.progress_emit_failed",
            section = "updates",
            initiator = "install_update",
            cause = %cause,
            trace_route = "install_update->frontend_event",
            "update progress event could not be emitted"
        );
    }
}

#[cfg(target_os = "linux")]
fn linux_updater_self_replace_supported() -> bool {
    linux_updater_self_replace_supported_from(std::env::var_os("APPIMAGE").as_ref())
}

#[cfg(target_os = "linux")]
fn linux_updater_self_replace_supported_from(appimage: Option<&std::ffi::OsString>) -> bool {
    appimage.is_some()
}

#[cfg(target_os = "linux")]
fn open_linux_deb_release(
    app: &AppHandle,
    operation_id: Uuid,
) -> Result<OperationAccepted, String> {
    info!(
        event = "update.manual_package",
        section = "updates",
        initiator = "install_update",
        cause = "linux_deb_package",
        trace_route = "tauri_command->open_external_url",
        trace_id = %operation_id,
        "linux package cannot self-replace; opening the GitHub Release"
    );
    deps::open_allowlisted_url("https://github.com/devlifeX/BiFlow/releases/latest")
        .map_err(|error| error.to_string())?;
    emit_update_progress(
        app,
        UpdateProgress {
            phase: "manual".into(),
            percent: None,
            version: None,
            error: None,
        },
    );
    Ok(OperationAccepted {
        operation_id,
        already_complete: true,
    })
}

async fn pause_stack_for_update(services: &AppServices) -> Result<(), String> {
    if matches!(
        services.engine.snapshot().phase,
        StackPhase::Running | StackPhase::Degraded
    ) {
        services
            .engine
            .pause_stack()
            .await
            .map_err(|error| error.to_string())?;
        services
            .engine
            .wait_for_phase(StackPhase::Paused, Duration::from_secs(25))
            .await
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn services<R: Runtime>(app: &AppHandle<R>) -> Result<&AppServices, String> {
    app.try_state::<AppServices>()
        .map(|state| state.inner())
        .ok_or_else(|| "application services are not initialized".into())
}

#[tauri::command]
async fn bootstrap_app(app: AppHandle) -> Result<BootstrapResult, String> {
    diagnostics::trace_action("startup", "tauri_command", "bootstrap_app", async move {
        let services = services(&app)?;
        services.engine.refresh_health().await;
        let settings = services
            .config_store
            .load_or_create()
            .map_err(|error| error.to_string())?
            .redacted();
        Ok(BootstrapResult {
            app_version: version::app_version().to_owned(),
            platform: std::env::consts::OS.into(),
            mock_mode: false,
            snapshot: services.engine.snapshot(),
            settings,
            direct_rules: services.rules.list().await,
            cloud_rules: services
                .cloud_rules
                .status()
                .map_err(|error| error.to_string())?,
            dependencies: deps::dependency_status(&services.paths.data),
            network_status: network::NetworkStatus::default(),
        })
    })
    .await
}

#[tauri::command]
async fn get_network_status(app: AppHandle) -> Result<network::NetworkStatus, String> {
    diagnostics::trace_action(
        "network",
        "tauri_command",
        "get_network_status",
        async move { Ok(services(&app)?.network.check().await) },
    )
    .await
}

const TRAFFIC_PROBE_TIMEOUT: Duration = Duration::from_secs(3);

#[tauri::command]
async fn get_traffic_totals(app: AppHandle) -> Result<traffic::TrafficTotals, String> {
    diagnostics::trace_action(
        "traffic",
        "tauri_command",
        "get_traffic_totals",
        async move {
            let services = services(&app)?;
            let _guard = services.traffic_lock.lock().await;
            let path = services.paths.data.join("traffic-totals.json");
            let mut store = traffic::load(&path);
            let connected = matches!(
                services.engine.snapshot().phase,
                StackPhase::Running | StackPhase::Degraded
            );
            let (session_sent, session_received) = if connected {
                match session_connection_totals(services).await {
                    Ok(totals) => totals,
                    Err(cause) => {
                        warn!(
                            event = "traffic.session_probe_failed",
                            section = "traffic",
                            initiator = "get_traffic_totals",
                            cause = %cause,
                            trace_route = "tauri_command->mihomo_controller",
                            "session traffic totals were unavailable; using last known session"
                        );
                        (store.last_session_sent, store.last_session_received)
                    }
                }
            } else {
                (0, 0)
            };
            let totals = traffic::accumulate(&mut store, session_sent, session_received, connected);
            if let Err(cause) = traffic::save(&path, &store) {
                warn!(
                    event = "traffic.persist_failed",
                    section = "traffic",
                    initiator = "get_traffic_totals",
                    cause = %cause,
                    trace_route = "tauri_command->traffic_totals_file",
                    "lifetime traffic totals could not be written"
                );
            }
            Ok(totals)
        },
    )
    .await
}

async fn session_connection_totals(services: &AppServices) -> Result<(u64, u64), String> {
    let config = services
        .config_store
        .load()
        .or_else(|_| services.config_store.load_or_create())
        .map_err(|error| error.to_string())?;
    let client = ControllerClient::new(
        &config.mihomo.controller_host,
        config.mihomo.controller_port,
        &config.mihomo.controller_secret,
    )
    .map_err(|error| error.to_string())?;
    tokio::time::timeout(TRAFFIC_PROBE_TIMEOUT, client.connection_totals())
        .await
        .map_err(|_| "traffic probe timed out".to_owned())?
        .map_err(|error| error.to_string())
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri injects AppHandle command arguments by value"
)]
#[tauri::command]
fn get_stack_snapshot(app: AppHandle) -> Result<StackSnapshot, String> {
    diagnostics::trace_sync("stack", "tauri_command", "get_stack_snapshot", || {
        Ok(services(&app)?.engine.snapshot())
    })
}

#[tauri::command]
async fn start_stack(app: AppHandle) -> Result<OperationAccepted, String> {
    diagnostics::trace_action("stack", "tauri_command", "start_stack", async move {
        start_stack_inner(&app).await
    })
    .await
}

async fn start_stack_inner<R: Runtime>(app: &AppHandle<R>) -> Result<OperationAccepted, String> {
    let engine = &services(app)?.engine;
    if engine.snapshot().phase == StackPhase::Running {
        return Ok(OperationAccepted {
            operation_id: uuid::Uuid::new_v4(),
            already_complete: true,
        });
    }
    engine
        .reserve_lifecycle(LifecycleBusy::Connecting)
        .await
        .map_err(|error| error.to_string())?;
    if let Err(error) = prepare_stack_start(app).await {
        engine.release_lifecycle(LifecycleBusy::Connecting).await;
        return Err(error);
    }
    engine
        .start_stack()
        .await
        .map_err(|error| error.to_string())
}

async fn prepare_stack_start<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    let services = services(app)?;
    let helper_ready = connect_prep::helper_is_ready(services.engine.snapshot().helper.phase);
    let statuses = deps::dependency_status(&services.paths.data);
    let hiddify = statuses
        .iter()
        .any(|item| item.id == "hiddify" && item.installed);
    let mihomo = statuses
        .iter()
        .any(|item| item.id == "mihomo" && item.installed);
    for requirement in connect_prep::missing_requirements(helper_ready, hiddify, mihomo) {
        info!(
            event = "connect.install_required",
            section = "stack",
            initiator = "prepare_stack_start",
            cause = "missing_dependency",
            trace_route = "start_stack->prepare_stack_start",
            requirement = requirement.as_str(),
            "installing a required service before connect"
        );
        match requirement {
            connect_prep::ConnectRequirement::Helper => {
                helper_install::install_helper(app).await?;
                services.engine.refresh_health().await;
                if !connect_prep::helper_is_ready(services.engine.snapshot().helper.phase) {
                    return Err("privileged helper is still unavailable after installation".into());
                }
            }
            connect_prep::ConnectRequirement::Hiddify => {
                install_required_dependency(services, deps::DependencyId::Hiddify).await?;
            }
            connect_prep::ConnectRequirement::Mihomo => {
                install_required_dependency(services, deps::DependencyId::Mihomo).await?;
            }
        }
    }
    services.engine.refresh_health().await;
    Ok(())
}

async fn install_required_dependency(
    services: &AppServices,
    id: deps::DependencyId,
) -> Result<(), String> {
    let result = deps::install_dependency(id, &services.paths.data, &services.paths.dependencies)
        .await
        .map_err(|error| error.to_string())?;
    if !result.installed {
        return Err(format!("{} installation did not complete", id.as_str()));
    }
    let statuses = deps::dependency_status(&services.paths.data);
    if !statuses
        .iter()
        .any(|item| item.id == id.as_str() && item.installed)
    {
        return Err(format!(
            "{} is still missing after installation",
            id.as_str()
        ));
    }
    Ok(())
}

#[tauri::command]
async fn stop_stack(app: AppHandle) -> Result<OperationAccepted, String> {
    diagnostics::trace_action("stack", "tauri_command", "stop_stack", async move {
        services(&app)?
            .engine
            .stop_stack()
            .await
            .map_err(|error| error.to_string())
    })
    .await
}

#[tauri::command]
async fn pause_stack(app: AppHandle) -> Result<OperationAccepted, String> {
    diagnostics::trace_action("stack", "tauri_command", "pause_stack", async move {
        services(&app)?
            .engine
            .pause_stack()
            .await
            .map_err(|error| error.to_string())
    })
    .await
}

#[tauri::command]
async fn resume_stack(app: AppHandle) -> Result<OperationAccepted, String> {
    diagnostics::trace_action("stack", "tauri_command", "resume_stack", async move {
        services(&app)?
            .engine
            .resume_stack()
            .await
            .map_err(|error| error.to_string())
    })
    .await
}

#[tauri::command]
async fn restart_stack(app: AppHandle) -> Result<OperationAccepted, String> {
    diagnostics::trace_action("stack", "tauri_command", "restart_stack", async move {
        let services = services(&app)?;
        services
            .engine
            .stop_stack()
            .await
            .map_err(|error| error.to_string())?;
        services
            .engine
            .wait_for_phase(StackPhase::Stopped, Duration::from_secs(25))
            .await
            .map_err(|error| error.to_string())?;
        services
            .engine
            .start_stack()
            .await
            .map_err(|error| error.to_string())
    })
    .await
}

#[tauri::command]
async fn cancel_operation(app: AppHandle, operation_id: Uuid) -> Result<bool, String> {
    diagnostics::trace_action("stack", "tauri_command", "cancel_operation", async move {
        info!(operation_id = %operation_id, "operation cancellation requested");
        let services = services(&app)?;
        services.updates.request_cancel();
        Ok(services.engine.cancel_operation(operation_id).await)
    })
    .await
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri injects AppHandle command arguments by value"
)]
#[tauri::command]
fn get_settings(app: AppHandle) -> Result<AppConfig, String> {
    diagnostics::trace_sync("settings", "tauri_command", "get_settings", || {
        Ok(services(&app)?
            .config_store
            .load_or_create()
            .map_err(|error| error.to_string())?
            .redacted())
    })
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri injects AppHandle command arguments by value"
)]
#[tauri::command]
fn validate_settings(mut draft: AppConfig, app: AppHandle) -> Result<Vec<ValidationIssue>, String> {
    diagnostics::trace_sync("settings", "tauri_command", "validate_settings", || {
        let current = services(&app)?
            .config_store
            .load_or_create()
            .map_err(|error| error.to_string())?;
        draft.mihomo.controller_secret = current.mihomo.controller_secret;
        let issues = draft.validate();
        if !issues.is_empty() {
            warn!(
                event = "settings.validation_failed",
                section = "settings",
                initiator = "tauri_command",
                cause = "invalid_draft",
                issue_count = issues.len(),
                "settings validation returned issues"
            );
        }
        Ok(issues)
    })
}

#[tauri::command]
async fn save_settings(
    mut draft: AppConfig,
    expected_revision: u64,
    app: AppHandle,
) -> Result<AppConfig, String> {
    diagnostics::trace_action("settings", "tauri_command", "save_settings", async move {
        info!(expected_revision, "saving redacted settings revision");
        let services = services(&app)?;
        let current = services
            .config_store
            .load_or_create()
            .map_err(|error| error.to_string())?;
        draft.mihomo.controller_secret = current.mihomo.controller_secret;
        let saved = services
            .config_store
            .save(draft, expected_revision)
            .map_err(|error| error.to_string())?;
        #[cfg(target_os = "linux")]
        services.backend.update_config(saved.clone()).await;
        Ok(saved.redacted())
    })
    .await
}

#[tauri::command]
async fn list_direct_rules(app: AppHandle) -> Result<DirectRulesDocument, String> {
    diagnostics::trace_action("rules", "tauri_command", "list_direct_rules", async move {
        Ok(services(&app)?.rules.list().await)
    })
    .await
}

#[tauri::command]
async fn pin_route(
    input: String,
    outbound: String,
    expected_revision: u64,
    app: AppHandle,
) -> Result<DirectRulesDocument, String> {
    diagnostics::trace_action("rules", "tauri_command", "pin_route", async move {
        let outbound = match outbound.as_str() {
            "direct" => iran_split_rules::Outbound::Direct,
            "vpn" => iran_split_rules::Outbound::Vpn,
            other => return Err(format!("unknown outbound: {other}")),
        };
        info!(
            expected_revision,
            outbound = ?outbound,
            input_kind = if input.parse::<std::net::IpAddr>().is_ok() {
                "ip"
            } else {
                "domain"
            },
            "pinning a host to one outbound without logging its value"
        );
        services(&app)?
            .rules
            .pin(&input, outbound, expected_revision)
            .await
            .map_err(|error| error.to_string())
    })
    .await
}

#[tauri::command]
async fn add_direct_rule(
    input: String,
    expected_revision: u64,
    app: AppHandle,
) -> Result<DirectRulesDocument, String> {
    diagnostics::trace_action("rules", "tauri_command", "add_direct_rule", async move {
        info!(
            expected_revision,
            input_kind = if input.parse::<std::net::IpAddr>().is_ok() {
                "ip"
            } else {
                "domain"
            },
            "adding direct rule without logging its value"
        );
        services(&app)?
            .rules
            .add(&input, expected_revision)
            .await
            .map_err(|error| error.to_string())
    })
    .await
}

#[tauri::command]
async fn remove_direct_rule(
    input: String,
    expected_revision: u64,
    app: AppHandle,
) -> Result<DirectRulesDocument, String> {
    diagnostics::trace_action("rules", "tauri_command", "remove_direct_rule", async move {
        info!(
            expected_revision,
            "removing direct rule without logging its value"
        );
        services(&app)?
            .rules
            .remove(&input, expected_revision)
            .await
            .map_err(|error| error.to_string())
    })
    .await
}

#[tauri::command]
async fn refresh_direct_rules(app: AppHandle) -> Result<DirectRulesDocument, String> {
    diagnostics::trace_action(
        "rules",
        "tauri_command",
        "refresh_direct_rules",
        async move {
            services(&app)?
                .rules
                .refresh()
                .await
                .map_err(|error| error.to_string())
        },
    )
    .await
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri injects AppHandle command arguments by value"
)]
#[tauri::command]
fn get_cloud_rules_status(app: AppHandle) -> Result<CloudRulesStatus, String> {
    diagnostics::trace_sync(
        "cloud_rules",
        "tauri_command",
        "get_cloud_rules_status",
        || {
            services(&app)?
                .cloud_rules
                .status()
                .map_err(|error| error.to_string())
        },
    )
}

#[tauri::command]
async fn sync_cloud_rules(app: AppHandle) -> Result<CloudRulesStatus, String> {
    diagnostics::trace_action(
        "cloud_rules",
        "tauri_command",
        "sync_cloud_rules",
        async move {
            services(&app)?
                .cloud_rules
                .sync()
                .await
                .map_err(|error| error.to_string())
        },
    )
    .await
}

#[tauri::command]
async fn install_helper(app: AppHandle) -> Result<helper_install::InstallHelperResult, String> {
    diagnostics::trace_action("helper", "tauri_command", "install_helper", async move {
        helper_install::install_helper(&app).await
    })
    .await
}

#[tauri::command]
async fn fresh_hiddify_start(app: AppHandle) -> Result<hiddify_reset::FreshStartReport, String> {
    diagnostics::trace_action(
        "hiddify_reset",
        "tauri_command",
        "fresh_hiddify_start",
        async move { hiddify_reset::fresh_start(&app).await },
    )
    .await
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri injects AppHandle command arguments by value"
)]
#[tauri::command]
fn list_dependencies(app: AppHandle) -> Result<Vec<deps::DependencyStatus>, String> {
    diagnostics::trace_sync("dependencies", "tauri_command", "list_dependencies", || {
        Ok(deps::dependency_status(&services(&app)?.paths.data))
    })
}

#[tauri::command]
async fn install_dependency(id: String, app: AppHandle) -> Result<deps::InstallResult, String> {
    diagnostics::trace_action(
        "dependencies",
        "tauri_command",
        "install_dependency",
        async move {
            let parsed = deps::DependencyId::parse(&id).map_err(|error| error.to_string())?;
            info!(
                dependency_id = parsed.as_str(),
                "dependency installation requested"
            );
            let services = services(&app)?;
            let result = deps::install_dependency(
                parsed,
                &services.paths.data,
                &services.paths.dependencies,
            )
            .await
            .map_err(|error| error.to_string())?;
            services.engine.refresh_health().await;
            Ok(result)
        },
    )
    .await
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri deserializes command strings into owned values"
)]
#[tauri::command]
fn get_install_guide(id: String) -> Result<deps::InstallGuide, String> {
    diagnostics::trace_sync("dependencies", "tauri_command", "get_install_guide", || {
        let parsed = deps::DependencyId::parse(&id).map_err(|error| error.to_string())?;
        info!(
            dependency_id = parsed.as_str(),
            "dependency guide requested"
        );
        Ok(deps::install_guide(parsed))
    })
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri deserializes command strings into owned values"
)]
#[tauri::command]
fn open_external_url(url: String) -> Result<(), String> {
    diagnostics::trace_sync("dependencies", "tauri_command", "open_external_url", || {
        info!("opening an allowlisted external URL without logging its value");
        deps::open_allowlisted_url(&url).map_err(|error| error.to_string())
    })
}

#[tauri::command]
async fn test_route(target: String, app: AppHandle) -> Result<RouteTestResult, String> {
    diagnostics::trace_action("routing", "tauri_command", "test_route", async move {
        let services = services(&app)?;
        let document = services.rules.list().await;
        let domains = read_snapshot_lines(&services.cloud_rules.resolve("iran-domains.txt"))?;
        let domains = domains
            .into_iter()
            .map(|line| line.trim_start_matches("+.").to_owned())
            .collect::<Vec<_>>();
        let cidrs = read_snapshot_lines(&services.cloud_rules.resolve("private.txt"))?
            .into_iter()
            .chain(read_snapshot_lines(
                &services.cloud_rules.resolve("iran-networks.txt"),
            )?)
            .map(|line| {
                line.parse()
                    .map_err(|error| format!("invalid bundled CIDR: {error}"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let decision = RuleSet::from_sources(&document, domains, cidrs)
            .decide(&target)
            .map_err(|error| error.to_string())?;
        info!(
            outbound = ?decision.outbound,
            reason = ?decision.reason,
            target_kind = if target.parse::<std::net::IpAddr>().is_ok() {
                "ip"
            } else {
                "domain"
            },
            "route test completed without logging its target"
        );
        Ok(RouteTestResult {
            target,
            outbound: decision.outbound,
            reason: format!("{:?}", decision.reason).to_lowercase(),
            matched_rule: decision.matched_rule,
            reachable: None,
            tested_at: Utc::now().to_rfc3339(),
        })
    })
    .await
}

#[tauri::command]
async fn run_full_diagnostics(app: AppHandle) -> Result<DiagnosticsReport, String> {
    diagnostics::trace_action(
        "diagnostics",
        "tauri_command",
        "run_full_diagnostics",
        async move {
            let services = services(&app)?;
            let operation_id = Uuid::new_v4();
            let started = Utc::now().to_rfc3339();
            let helper = services.backend.helper_status().await;
            let snapshot = services.engine.snapshot();
            let config = services
                .config_store
                .load_or_create()
                .map_err(|error| error.to_string())?;
            let helper_ok = helper
                .as_ref()
                .is_ok_and(|status| status.available && status.authorized);
            let steps = vec![
                diagnostic_step(
                    "helper",
                    "Helper authorization",
                    helper_ok,
                    helper.err().map(|error| error.to_string()),
                    &started,
                ),
                diagnostic_step(
                    "config",
                    "Configuration validation",
                    config.validate().is_empty(),
                    None,
                    &started,
                ),
                diagnostic_step(
                    "core",
                    "Mihomo process",
                    snapshot.mihomo.phase == iran_split_core::ComponentPhase::Running,
                    Some(format!("phase: {:?}", snapshot.mihomo.phase)),
                    &started,
                ),
                diagnostic_step(
                    "providers",
                    "Rule providers",
                    snapshot.providers.total > 0
                        && snapshot.providers.ready == snapshot.providers.total,
                    Some(format!(
                        "{} of {} ready",
                        snapshot.providers.ready, snapshot.providers.total
                    )),
                    &started,
                ),
                diagnostic_step(
                    "tun",
                    "Owned TUN state",
                    snapshot.tun.phase != iran_split_core::ComponentPhase::Error,
                    Some(format!("phase: {:?}", snapshot.tun.phase)),
                    &started,
                ),
                diagnostic_step(
                    "egress",
                    "Foreign egress",
                    snapshot.exit_ip.is_some(),
                    snapshot.exit_ip.clone(),
                    &started,
                ),
            ];
            for step in &steps {
                if step.status == "warning" {
                    warn!(
                        event = "diagnostics.step_warning",
                        section = "diagnostics",
                        initiator = "full_diagnostics",
                        cause = step.detail.as_deref().unwrap_or("check did not pass"),
                        trace_id = %operation_id,
                        trace_route = "tauri_command->full_diagnostics->diagnostic_step",
                        step_id = step.id,
                        "diagnostic step reported a warning"
                    );
                }
            }
            Ok(DiagnosticsReport {
                operation_id,
                steps,
                finished: true,
            })
        },
    )
    .await
}

fn diagnostic_step(
    id: &str,
    label: &str,
    passed: bool,
    detail: Option<String>,
    started: &str,
) -> DiagnosticStep {
    DiagnosticStep {
        id: id.into(),
        label: label.into(),
        status: if passed { "passed" } else { "warning" }.into(),
        detail,
        started_at: Some(started.into()),
        finished_at: Some(Utc::now().to_rfc3339()),
    }
}

#[tauri::command]
async fn query_logs(
    app: AppHandle,
    maximum: u16,
) -> Result<Vec<iran_split_ipc::ServiceLogEntry>, String> {
    diagnostics::trace_action("diagnostics", "tauri_command", "query_logs", async move {
        #[cfg(target_os = "linux")]
        {
            services(&app)?
                .backend
                .service_logs(maximum.clamp(1, 2_000))
                .await
                .map_err(|error| error.to_string())
        }
        #[cfg(target_os = "windows")]
        {
            let _ = (app, maximum);
            Ok(Vec::new())
        }
    })
    .await
}

#[tauri::command]
fn get_debug_log_status(_app: AppHandle) -> Result<diagnostics::DebugLogStatus, String> {
    diagnostics::trace_sync(
        "diagnostics",
        "tauri_command",
        "get_debug_log_status",
        diagnostics::status,
    )
}

#[tauri::command]
fn reveal_debug_log(_app: AppHandle) -> Result<diagnostics::DebugLogStatus, String> {
    diagnostics::trace_sync(
        "diagnostics",
        "tauri_command",
        "reveal_debug_log",
        diagnostics::reveal,
    )
}

#[tauri::command]
fn delete_debug_log(_app: AppHandle) -> Result<diagnostics::DebugLogStatus, String> {
    diagnostics::clear()
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri injects AppHandle command arguments by value"
)]
#[tauri::command]
fn export_support_bundle(app: AppHandle) -> Result<ExportResult, String> {
    diagnostics::trace_sync(
        "diagnostics",
        "tauri_command",
        "export_support_bundle",
        || {
            let services = services(&app)?;
            let bundle = services
                .paths
                .cache
                .join(format!("support-{}", Uuid::new_v4()));
            fs::create_dir_all(&bundle).map_err(|error| error.to_string())?;
            let settings = services
                .config_store
                .load_or_create()
                .map_err(|error| error.to_string())?
                .redacted();
            let files = vec![
                "versions.json",
                "config-redacted.json",
                "snapshot.json",
                "debug.log",
            ];
            write_json(
                &bundle.join(files[0]),
                &serde_json::json!({
                    "app": app.package_info().version.to_string(),
                    "os": std::env::consts::OS,
                    "arch": std::env::consts::ARCH,
                }),
            )?;
            write_json(&bundle.join(files[1]), &settings)?;
            write_json(&bundle.join(files[2]), &services.engine.snapshot())?;
            diagnostics::copy_log(&bundle.join(files[3]))?;
            Ok(ExportResult {
                path: bundle.to_string_lossy().into_owned(),
                files: files.into_iter().map(str::to_owned).collect(),
            })
        },
    )
}

/// Reaching `releases/latest/download/latest.json` fails intermittently on a
/// cold or congested link — DNS, TLS, or a GitHub redirect hiccup — and the
/// same check succeeds moments later. Retry here so a transient failure never
/// becomes a Retry button the operator has to press.
const UPDATE_CHECK_ATTEMPTS: u32 = 4;
const UPDATE_CHECK_FIRST_BACKOFF: Duration = Duration::from_millis(600);
const UPDATE_CHECK_ATTEMPT_TIMEOUT: Duration = Duration::from_secs(8);
const UPDATE_INSTALL_TIMEOUT: Duration = Duration::from_secs(10 * 60);
/// How long to wait after launch before the first background check, and the
/// interval between later ones.
const UPDATE_BACKGROUND_DELAY: Duration = Duration::from_secs(90);
const UPDATE_BACKGROUND_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);

async fn check_update_once(app: &AppHandle) -> Result<UpdateStatus, String> {
    let update = tokio::time::timeout(UPDATE_CHECK_ATTEMPT_TIMEOUT, async {
        app.updater()
            .map_err(|error| error.to_string())?
            .check()
            .await
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|_| "update check timed out".to_owned())??;
    Ok(update.map_or(
        UpdateStatus {
            available: false,
            version: None,
            notes: None,
            app_available: false,
            rules_available: false,
            thirdparty_available: false,
        },
        |update| UpdateStatus {
            available: true,
            version: Some(update.version.clone()),
            notes: update.body.clone(),
            app_available: true,
            rules_available: false,
            thirdparty_available: false,
        },
    ))
}

fn merge_update_channels(
    mut status: UpdateStatus,
    rules_available: bool,
    thirdparty_available: bool,
) -> UpdateStatus {
    status.rules_available = rules_available;
    status.thirdparty_available = thirdparty_available;
    status.available = status.app_available || rules_available || thirdparty_available;
    status
}

async fn enrich_update_channels(app: &AppHandle, status: &mut UpdateStatus) {
    let Ok(services) = services(app) else {
        return;
    };
    match services.cloud_rules.peek_remote_revision().await {
        Ok(remote) => {
            status.rules_available =
                services.cloud_rules.cached_revision().as_deref() != Some(remote.as_str());
        }
        Err(cause) => {
            warn!(
                event = "update.rules_probe_failed",
                section = "updates",
                initiator = "check_for_update",
                cause = %cause,
                trace_route = "updater->cloud_rule_store->manifest",
                "rule snapshot revision could not be compared"
            );
        }
    }
    let thirdparty_available = deps::dependency_status(&services.paths.data)
        .into_iter()
        .any(|item| item.id == "mihomo" && !item.installed);
    *status = merge_update_channels(status.clone(), status.rules_available, thirdparty_available);
}

async fn collect_update_status(
    app: &AppHandle,
    initiator: &'static str,
) -> Result<UpdateStatus, String> {
    let mut status = check_update_with_retry(app, initiator).await?;
    enrich_update_channels(app, &mut status).await;
    Ok(status)
}

async fn apply_sidecar_updates(app: &AppHandle, operation_id: Uuid) -> Result<(), String> {
    let services = services(app)?;
    info!(
        event = "update.sidecars_started",
        section = "updates",
        initiator = "install_update",
        cause = "versioned_assets",
        trace_route = "tauri_command->cloud_rules->mihomo_install",
        trace_id = %operation_id,
        "applying versioned rule and third-party updates"
    );
    emit_update_progress(
        app,
        UpdateProgress {
            phase: "installing".into(),
            percent: Some(10),
            version: None,
            error: None,
        },
    );
    if let Err(cause) = services.cloud_rules.sync().await {
        warn!(
            event = "update.rules_sync_failed",
            section = "updates",
            initiator = "install_update",
            cause = %cause,
            trace_route = "install_update->cloud_rule_store->sync",
            trace_id = %operation_id,
            "cloud rule update failed; last good snapshot remains"
        );
    }
    let mihomo_missing = deps::dependency_status(&services.paths.data)
        .into_iter()
        .any(|item| item.id == "mihomo" && !item.installed);
    if mihomo_missing {
        deps::install_dependency(
            deps::DependencyId::Mihomo,
            &services.paths.data,
            &services.paths.dependencies,
        )
        .await
        .map_err(|error| error.to_string())?;
        services.engine.refresh_health().await;
    }
    Ok(())
}

/// Doubles the wait after each failed attempt. Attempt `0` is immediate.
#[must_use]
fn update_check_backoff(attempt: u32) -> Duration {
    UPDATE_CHECK_FIRST_BACKOFF.saturating_mul(1 << attempt.min(4))
}

async fn check_update_with_retry(
    app: &AppHandle,
    initiator: &'static str,
) -> Result<UpdateStatus, String> {
    let mut last_error = String::new();
    for attempt in 0..UPDATE_CHECK_ATTEMPTS {
        if update_check_cancelled(app) {
            return Err("update check cancelled".into());
        }
        match check_update_once(app).await {
            Ok(status) => {
                if attempt > 0 {
                    info!(
                        event = "update.check_recovered",
                        section = "updates",
                        initiator = initiator,
                        cause = "retry_succeeded",
                        trace_route = "updater_plugin->github_release->latest_json",
                        attempts = attempt + 1,
                        "update check succeeded after a transient failure"
                    );
                }
                return Ok(status);
            }
            Err(error) => {
                last_error = error;
                warn!(
                    event = "update.check_attempt_failed",
                    section = "updates",
                    initiator = initiator,
                    cause = last_error.as_str(),
                    trace_route = "updater_plugin->github_release->latest_json",
                    attempt = attempt + 1,
                    attempts = UPDATE_CHECK_ATTEMPTS,
                    "update check attempt failed"
                );
            }
        }
        if attempt + 1 < UPDATE_CHECK_ATTEMPTS {
            if update_check_cancelled(app) {
                return Err("update check cancelled".into());
            }
            tokio::time::sleep(update_check_backoff(attempt)).await;
        }
    }
    error!(
        event = "update.check_failed",
        section = "updates",
        initiator = initiator,
        cause = last_error.as_str(),
        trace_route = "updater_plugin->github_release->latest_json",
        attempts = UPDATE_CHECK_ATTEMPTS,
        "update check failed after every retry"
    );
    Err(last_error)
}

#[tauri::command]
async fn check_for_update(app: AppHandle) -> Result<UpdateStatus, String> {
    diagnostics::trace_action("updates", "tauri_command", "check_for_update", async move {
        let _guard = services(&app)?.updates.begin()?;
        collect_update_status(&app, "tauri_command").await
    })
    .await
}

/// Keeps the About page current without anyone pressing Check. A failure stays
/// silent: the retries already ran, and a background poll must never replace a
/// real state with an error banner the operator did not ask for.
fn spawn_background_update_checks(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(UPDATE_BACKGROUND_DELAY).await;
        loop {
            let Ok(services) = services(&app) else {
                tokio::time::sleep(UPDATE_BACKGROUND_INTERVAL).await;
                continue;
            };
            let Ok(_guard) = services.updates.begin() else {
                tokio::time::sleep(UPDATE_BACKGROUND_INTERVAL).await;
                continue;
            };
            match collect_update_status(&app, "background_poll").await {
                Ok(status) if status.available => {
                    info!(
                        event = "update.background_found",
                        section = "updates",
                        initiator = "background_poll",
                        cause = "release_published",
                        trace_route = "background_poll->updater_plugin->update_progress",
                        update_version = status.version.as_deref().unwrap_or("unknown"),
                        "a newer signed release is available"
                    );
                    emit_update_progress(
                        &app,
                        UpdateProgress {
                            phase: "available".into(),
                            percent: None,
                            version: status.version,
                            error: None,
                        },
                    );
                }
                Ok(_) | Err(_) => {}
            }
            tokio::time::sleep(UPDATE_BACKGROUND_INTERVAL).await;
        }
    });
}

async fn download_and_install_signed_update(
    app: &AppHandle,
    update: tauri_plugin_updater::Update,
    operation_id: Uuid,
    target_version: &str,
) -> Result<(), String> {
    emit_update_progress(
        app,
        UpdateProgress {
            phase: "downloading".into(),
            percent: Some(0),
            version: Some(target_version.to_owned()),
            error: None,
        },
    );
    let mut downloaded = 0usize;
    let app_for_progress = app.clone();
    let version_for_progress = target_version.to_owned();
    let download = update.download_and_install(
        move |chunk_length, content_length| {
            downloaded = downloaded.saturating_add(chunk_length);
            emit_update_progress(
                &app_for_progress,
                UpdateProgress {
                    phase: "downloading".into(),
                    percent: update_download_percent(downloaded, content_length),
                    version: Some(version_for_progress.clone()),
                    error: None,
                },
            );
        },
        || {
            emit_update_progress(
                app,
                UpdateProgress {
                    phase: "installing".into(),
                    percent: Some(100),
                    version: Some(target_version.to_owned()),
                    error: None,
                },
            );
        },
    );
    tokio::time::timeout(UPDATE_INSTALL_TIMEOUT, download)
        .await
        .map_err(|_| "update download timed out".to_owned())?
        .map_err(|error| {
            let message = error.to_string();
            emit_update_progress(
                app,
                UpdateProgress {
                    phase: "failed".into(),
                    percent: None,
                    version: Some(target_version.to_owned()),
                    error: Some(message.clone()),
                },
            );
            error!(
                event = "update.install_failed",
                section = "updates",
                initiator = "install_update",
                cause = "download_or_install_error",
                trace_route = "tauri_command->updater_plugin",
                trace_id = %operation_id,
                update_version = target_version,
                "application update could not be installed"
            );
            message
        })
}

fn schedule_update_restart(app: &AppHandle, operation_id: Uuid) -> OperationAccepted {
    diagnostics::flush();
    let restart_app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(250)).await;
        restart_app.restart();
    });
    OperationAccepted {
        operation_id,
        already_complete: false,
    }
}

async fn perform_complete_update_install(
    app: &AppHandle,
    operation_id: Uuid,
) -> Result<OperationAccepted, String> {
    apply_sidecar_updates(app, operation_id).await?;
    let status = collect_update_status(app, "install_update").await?;
    if !status.app_available {
        emit_update_progress(
            app,
            UpdateProgress {
                phase: if status.available {
                    "available".into()
                } else {
                    "current".into()
                },
                percent: Some(100),
                version: status.version,
                error: None,
            },
        );
        return Ok(OperationAccepted {
            operation_id,
            already_complete: true,
        });
    }
    perform_signed_update_install(app, operation_id).await
}

async fn perform_signed_update_install(
    app: &AppHandle,
    operation_id: Uuid,
) -> Result<OperationAccepted, String> {
    pause_stack_for_update(services(app)?).await?;
    #[cfg(target_os = "linux")]
    if !linux_updater_self_replace_supported() {
        return open_linux_deb_release(app, operation_id);
    }
    let update = match check_update_with_retry(app, "install_update").await {
        Ok(status) if status.app_available => app
            .updater()
            .map_err(|error| error.to_string())?
            .check()
            .await
            .map_err(|error| error.to_string())?
            .ok_or("no update is available")?,
        Ok(_) => return Err("no update is available".into()),
        Err(error) => return Err(error),
    };
    let target_version = update.version.clone();
    info!(
        event = "update.download_started",
        section = "updates",
        initiator = "install_update",
        cause = "update_available",
        trace_route = "tauri_command->updater_plugin->download",
        trace_id = %operation_id,
        update_version = %target_version,
        "application update download started"
    );
    download_and_install_signed_update(app, update, operation_id, &target_version).await?;
    info!(
        event = "update.install_succeeded",
        section = "updates",
        initiator = "install_update",
        cause = "download_and_install_complete",
        trace_route = "tauri_command->updater_plugin->restart",
        trace_id = %operation_id,
        update_version = %target_version,
        "application update installed; relaunching"
    );
    emit_update_progress(
        app,
        UpdateProgress {
            phase: "restarting".into(),
            percent: Some(100),
            version: Some(target_version),
            error: None,
        },
    );
    Ok(schedule_update_restart(app, operation_id))
}

#[tauri::command]
async fn install_update(app: AppHandle) -> Result<OperationAccepted, String> {
    diagnostics::trace_action("updates", "tauri_command", "install_update", async move {
        let _guard = services(&app)?.updates.begin()?;
        perform_complete_update_install(&app, Uuid::new_v4()).await
    })
    .await
}

fn read_snapshot_lines(path: &Path) -> Result<Vec<String>, String> {
    let source = fs::read_to_string(path).map_err(|error| error.to_string())?;
    Ok(source
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_owned)
        .collect())
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    fs::write(
        path,
        serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())
}

#[allow(
    clippy::too_many_lines,
    reason = "Linux and Windows backend construction stay in one startup path"
)]
fn create_services(app: &AppHandle) -> Result<AppServices, String> {
    info!(
        event = "services.initializing",
        section = "startup",
        initiator = "tauri_setup",
        cause = "application_start",
        trace_route = "application_process->tauri_setup->create_services",
        "application services initializing"
    );
    let paths = AppPaths::discover(app)?;
    let config_store = ConfigStore::new(&paths.config);
    let config = config_store
        .load_or_create()
        .map_err(|error| error.to_string())?;
    let rules_cache = paths.cache.join("rules");
    fs::create_dir_all(&rules_cache).map_err(|error| error.to_string())?;
    let bundled_rules = open_bundled_rules_dir(&paths)?;
    #[cfg(target_os = "linux")]
    let mihomo_binary = linux_mihomo_binary(
        deps::first_existing(&deps::mihomo_candidates(&paths.data))
            .unwrap_or_else(|| paths.data.join("bin/mihomo")),
    );
    #[cfg(target_os = "linux")]
    let backend = {
        let (socket_path, system_runtime_dir) = linux_helper_paths();
        info!(
            event = "helper.paths_selected",
            section = "startup",
            initiator = "create_services",
            cause = "platform_configuration",
            trace_route = "application_process->create_services->linux_backend",
            socket_path = %socket_path.display(),
            runtime_path = %system_runtime_dir.display(),
            "Linux helper paths selected"
        );
        Arc::new(NativeBackend::new(
            config,
            LinuxPaths {
                socket_path,
                user_data_dir: paths.data.clone(),
                system_runtime_dir,
                resources_dir: bundled_rules.clone(),
                rules_cache_dir: rules_cache.clone(),
                mihomo_binary,
            },
        ))
    };
    #[cfg(target_os = "windows")]
    let backend = {
        let (pipe_name, system_runtime_dir) = windows_helper_paths();
        // The helper installer records this same staging root in helper.toml,
        // so a generation staged here is the one SYSTEM is allowed to publish.
        let mihomo_binary = deps::first_existing(&deps::mihomo_candidates(&paths.data))
            .unwrap_or_else(windows_programdata_mihomo);
        info!(
            event = "helper.paths_selected",
            section = "startup",
            initiator = "create_services",
            cause = "platform_configuration",
            trace_route = "application_process->create_services->windows_backend",
            pipe_name = pipe_name.as_str(),
            runtime_path = %system_runtime_dir.display(),
            mihomo_binary = %mihomo_binary.display(),
            "Windows helper paths selected"
        );
        Arc::new(NativeBackend::new(
            config,
            WindowsPaths {
                pipe_name,
                user_data_dir: paths.data.clone(),
                system_runtime_dir,
                resources_dir: bundled_rules.clone(),
                rules_cache_dir: rules_cache.clone(),
                mihomo_binary,
            },
        ))
    };
    let runtime = tauri::async_runtime::handle();
    let engine = Engine::new(Arc::clone(&backend), runtime.inner());
    let rules = RuleManager::load(
        paths.data.join("direct-rules.json"),
        Arc::new(DohResolver::default()),
    )
    .map_err(|error| error.to_string())?;
    let cloud_rules = CloudRuleStore::load(bundled_rules, rules_cache);
    let network = network::NetworkMonitor::new().map_err(|error| error.to_string())?;
    info!(
        event = "services.initialized",
        section = "startup",
        initiator = "tauri_setup",
        cause = "none",
        trace_route = "application_process->tauri_setup->create_services",
        "application services initialized"
    );
    Ok(AppServices {
        config_store,
        engine,
        backend,
        rules,
        cloud_rules,
        network,
        paths,
        updates: Arc::new(UpdateCoordinator::new()),
        traffic_lock: tokio::sync::Mutex::new(()),
    })
}

fn open_bundled_rules_dir(paths: &AppPaths) -> Result<PathBuf, String> {
    let fallback = paths.data.join("bundled-rules");
    let bundled =
        ensure_bundled_snapshot(&paths.resources, &fallback).map_err(|error| error.to_string())?;
    if bundled != paths.resources {
        warn!(
            event = "cloud_rules.embedded_snapshot_used",
            section = "startup",
            initiator = "create_services",
            cause = "packaged_rules_missing",
            trace_route = "application_process->create_services->bundled_rules",
            "packaged Iran rule snapshot was missing; using the embedded copy"
        );
    }
    Ok(bundled)
}

fn handle_tray_icon<R: Runtime>(tray: &TrayIcon<R>, event: &TrayIconEvent) {
    if matches!(
        event,
        TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            ..
        }
    ) {
        info!(
            event = "window.open_requested",
            section = "window",
            initiator = "tray_icon",
            cause = "left_click",
            trace_route = "tray_icon->show_main",
            "main window open requested"
        );
        show_main(tray.app_handle());
    }
}

fn connect_from_tray<R: Runtime>(app: &AppHandle<R>) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let result = diagnostics::trace_action("stack", "tray_menu", "start_stack", async move {
            start_stack_inner(&app).await
        })
        .await;
        if let Err(cause) = result {
            error!(
                event = "tray.action_failed",
                section = "stack",
                initiator = "tray_menu",
                cause,
                trace_route = "tray_menu->start_stack",
                "tray connect action failed"
            );
        }
    });
}

fn pause_from_tray<R: Runtime>(app: &AppHandle<R>) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let result = diagnostics::trace_action("stack", "tray_menu", "pause_stack", async move {
            services(&app)?
                .engine
                .pause_stack()
                .await
                .map_err(|error| error.to_string())
        })
        .await;
        if let Err(cause) = result {
            error!(
                event = "tray.action_failed",
                section = "stack",
                initiator = "tray_menu",
                cause,
                trace_route = "tray_menu->pause_stack",
                "tray pause action failed"
            );
        }
    });
}

fn resume_from_tray<R: Runtime>(app: &AppHandle<R>) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let result = diagnostics::trace_action("stack", "tray_menu", "resume_stack", async move {
            services(&app)?
                .engine
                .resume_stack()
                .await
                .map_err(|error| error.to_string())
        })
        .await;
        if let Err(cause) = result {
            error!(
                event = "tray.action_failed",
                section = "stack",
                initiator = "tray_menu",
                cause,
                trace_route = "tray_menu->resume_stack",
                "tray resume action failed"
            );
        }
    });
}

fn disconnect_from_tray<R: Runtime>(app: &AppHandle<R>) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let result = diagnostics::trace_action("stack", "tray_menu", "stop_stack", async move {
            services(&app)?
                .engine
                .stop_stack()
                .await
                .map_err(|error| error.to_string())
        })
        .await;
        if let Err(cause) = result {
            error!(
                event = "tray.action_failed",
                section = "stack",
                initiator = "tray_menu",
                cause,
                trace_route = "tray_menu->stop_stack",
                "tray disconnect action failed"
            );
        }
    });
}

fn handle_tray_menu<R: Runtime>(app: &AppHandle<R>, event: &MenuEvent) {
    match event.id.as_ref() {
        "connect" => connect_from_tray(app),
        "pause" => pause_from_tray(app),
        "resume" => resume_from_tray(app),
        "disconnect" => disconnect_from_tray(app),
        "quit" => {
            info!(
                event = "session.quit_requested",
                section = "lifecycle",
                initiator = "tray_menu",
                cause = "quit_selected",
                trace_route = "tray_menu->application_exit",
                "quit requested without disconnect"
            );
            app.exit(0);
        }
        unknown => warn!(
            event = "tray.unknown_action",
            section = "tray",
            initiator = "tray_menu",
            cause = "unknown_menu_id",
            trace_route = "tray_menu->event_dispatch",
            menu_id = unknown,
            "unknown tray action ignored"
        ),
    }
}

fn build_tray_menu<R: Runtime>(
    app: &AppHandle<R>,
    phase: StackPhase,
    busy: Option<LifecycleBusy>,
) -> tauri::Result<Menu<R>> {
    let labels = tray::labels_for(phase);
    let enabled = tray::actions_enabled(busy);
    let connection = MenuItem::with_id(
        app,
        labels.connection_id,
        labels.connection_label,
        enabled,
        None::<&str>,
    )?;
    let pause = MenuItem::with_id(
        app,
        labels.pause_id,
        labels.pause_label,
        enabled,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let first_separator = PredefinedMenuItem::separator(app)?;
    let second_separator = PredefinedMenuItem::separator(app)?;
    Menu::with_items(
        app,
        &[
            &connection,
            &first_separator,
            &pause,
            &second_separator,
            &quit,
        ],
    )
}

fn apply_tray_menu<R: Runtime>(app: &AppHandle<R>, snapshot: &StackSnapshot) {
    let Ok(menu) = build_tray_menu(app, snapshot.phase, snapshot.busy) else {
        warn!(
            event = "tray.menu_build_failed",
            section = "tray",
            initiator = "apply_tray_menu",
            cause = "menu_construction",
            trace_route = "snapshot_watcher->build_tray_menu",
            "tray menu could not be rebuilt"
        );
        return;
    };
    let Some(icon) = app.tray_by_id("main") else {
        return;
    };
    if let Err(cause) = icon.set_menu(Some(menu)) {
        warn!(
            event = "tray.menu_update_failed",
            section = "tray",
            initiator = "apply_tray_menu",
            cause = %cause,
            trace_route = "snapshot_watcher->tray.set_menu",
            "tray menu could not be replaced"
        );
    }
}

fn setup_tray<R: Runtime>(app: &tauri::App<R>) -> tauri::Result<()> {
    let snapshot = app
        .try_state::<AppServices>()
        .map_or_else(StackSnapshot::default, |services| {
            services.engine.snapshot()
        });
    let menu = build_tray_menu(app.handle(), snapshot.phase, snapshot.busy)?;
    let icon = app.default_window_icon().cloned().ok_or_else(|| {
        tauri::Error::from(std::io::Error::other("default window icon is missing"))
    })?;
    TrayIconBuilder::with_id("main")
        .icon(icon)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| handle_tray_icon(tray, &event))
        .on_menu_event(|app, event| handle_tray_menu(app, &event))
        .build(app)?;
    Ok(())
}

fn show_main<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        if let Err(cause) = window.show() {
            error!(
                event = "window.show_failed",
                section = "window",
                initiator = "show_main",
                cause = %cause,
                trace_route = "show_main->window.show",
                "main window could not be shown"
            );
        }
        if let Err(cause) = window.set_focus() {
            warn!(
                event = "window.focus_failed",
                section = "window",
                initiator = "show_main",
                cause = %cause,
                trace_route = "show_main->window.set_focus",
                "main window could not be focused"
            );
        }
    } else {
        error!(
            event = "window.missing",
            section = "window",
            initiator = "show_main",
            cause = "main_window_not_found",
            trace_route = "show_main->get_webview_window",
            "main window is unavailable"
        );
    }
}

fn initialize_diagnostics() {
    let path = diagnostics::default_log_path().expect("debug.log directory is unavailable");
    diagnostics::initialize(&path, version::app_version())
        .unwrap_or_else(|error| panic!("BiFlow debug.log initialization failed: {error}"));
}

fn setup_application(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let services = create_services(app.handle()).map_err(|cause| {
        error!(
            event = "startup.services_failed",
            section = "startup",
            initiator = "tauri_setup",
            cause,
            trace_route = "application_process->tauri_setup->create_services",
            "application service initialization failed"
        );
        std::io::Error::other(cause)
    })?;
    let mut snapshots = services.engine.subscribe();
    let engine = Arc::clone(&services.engine);
    let health_engine = Arc::clone(&services.engine);
    let data_dir = services.paths.data.clone();
    let handle = app.handle().clone();
    app.manage(services);
    tauri::async_runtime::spawn(async move {
        while snapshots.changed().await.is_ok() {
            let snapshot = snapshots.borrow().clone();
            if let Err(cause) = handle.emit("stack-snapshot", snapshot.clone()) {
                warn!(
                    event = "snapshot.emit_failed",
                    section = "stack",
                    initiator = "snapshot_watcher",
                    cause = %cause,
                    trace_route = "engine->snapshot_watcher->frontend_event",
                    "stack snapshot event could not be emitted"
                );
            }
            apply_tray_menu(&handle, &snapshot);
        }
        warn!(
            event = "snapshot.channel_closed",
            section = "stack",
            initiator = "snapshot_watcher",
            cause = "engine_snapshot_sender_closed",
            trace_route = "engine->snapshot_watcher",
            "stack snapshot watcher stopped"
        );
    });
    spawn_background_update_checks(app.handle());
    tauri::async_runtime::spawn(async move {
        if let Err(cause) = diagnostics::trace_action(
            "startup",
            "tauri_setup",
            "reconcile_startup",
            engine.reconcile_startup(),
        )
        .await
        {
            error!(
                event = "startup.reconciliation_failed",
                section = "startup",
                initiator = "tauri_setup",
                cause = %cause,
                trace_route = "tauri_setup->engine->reconcile_startup",
                "startup reconciliation could not be queued"
            );
        }
    });
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(10)).await;
            if matches!(
                health_engine.snapshot().phase,
                StackPhase::Stopped
                    | StackPhase::Running
                    | StackPhase::Paused
                    | StackPhase::Degraded
                    | StackPhase::Error
            ) {
                health_engine.refresh_health().await;
            }
        }
    });
    if let Some(window) = app.get_webview_window("main") {
        restore_main_window_size(&window, &data_dir);
    }
    setup_tray(app).map_err(|cause| {
        error!(
            event = "startup.tray_failed",
            section = "startup",
            initiator = "tauri_setup",
            cause = %cause,
            trace_route = "tauri_setup->setup_tray",
            "system tray initialization failed"
        );
        cause
    })?;
    info!(
        event = "startup.completed",
        section = "startup",
        initiator = "tauri_setup",
        cause = "none",
        trace_route = "application_process->tauri_setup->event_loop",
        "application setup completed"
    );
    Ok(())
}

fn restore_main_window_size<R: Runtime>(window: &tauri::WebviewWindow<R>, data: &Path) {
    let saved = window_state::load(&data.join("window-size.json"));
    let (work_width, work_height) = monitor_work_area_logical(window)
        .unwrap_or((window_state::DEFAULT_WIDTH, window_state::DEFAULT_HEIGHT));
    let size = window_state::clamp_logical(saved.width, saved.height, work_width, work_height);
    if let Err(cause) = window.set_min_size(Some(Size::Logical(LogicalSize::new(
        window_state::MIN_WIDTH,
        window_state::MIN_HEIGHT,
    )))) {
        warn!(
            event = "window.min_size_failed",
            section = "window",
            initiator = "restore_main_window_size",
            cause = %cause,
            trace_route = "tauri_setup->window.set_min_size",
            "minimum window size could not be applied"
        );
    }
    if let Err(cause) = window.set_size(Size::Logical(LogicalSize::new(size.width, size.height))) {
        warn!(
            event = "window.size_restore_failed",
            section = "window",
            initiator = "restore_main_window_size",
            cause = %cause,
            trace_route = "tauri_setup->window.set_size",
            "saved window size could not be applied"
        );
    } else {
        info!(
            event = "window.size_restored",
            section = "window",
            initiator = "restore_main_window_size",
            cause = "persisted_size",
            trace_route = "tauri_setup->window.set_size",
            "main window size restored within the current work area"
        );
    }
}

fn monitor_work_area_logical<R: Runtime>(window: &tauri::WebviewWindow<R>) -> Option<(f64, f64)> {
    let monitor = window.current_monitor().ok().flatten()?;
    let scale = monitor.scale_factor();
    if scale <= 0.0 {
        return None;
    }
    let work = monitor.work_area();
    Some((
        f64::from(work.size.width) / scale,
        f64::from(work.size.height) / scale,
    ))
}

fn persist_main_window_size<R: Runtime>(window: &Window<R>) {
    let Ok(services) = services(window.app_handle()) else {
        return;
    };
    let Ok(physical) = window.inner_size() else {
        return;
    };
    let scale = window.scale_factor().unwrap_or(1.0);
    if scale <= 0.0 {
        return;
    }
    let logical = physical.to_logical::<f64>(scale);
    let (work_width, work_height) = window
        .current_monitor()
        .ok()
        .flatten()
        .and_then(|monitor| {
            let scale = monitor.scale_factor();
            if scale <= 0.0 {
                return None;
            }
            let work = monitor.work_area();
            Some((
                f64::from(work.size.width) / scale,
                f64::from(work.size.height) / scale,
            ))
        })
        .unwrap_or((window_state::DEFAULT_WIDTH, window_state::DEFAULT_HEIGHT));
    let size = window_state::clamp_logical(logical.width, logical.height, work_width, work_height);
    if let Err(cause) = window_state::save(&services.paths.data.join("window-size.json"), size) {
        warn!(
            event = "window.size_persist_failed",
            section = "window",
            initiator = "persist_main_window_size",
            cause = %cause,
            trace_route = "window_control->window_size_file",
            "window size could not be written"
        );
    }
}

fn handle_window_event<R: Runtime>(window: &Window<R>, event: &WindowEvent) {
    if let WindowEvent::Resized(_) = event {
        persist_main_window_size(window);
    }
    if let WindowEvent::CloseRequested { api, .. } = event {
        info!(
            event = "window.close_requested",
            section = "window",
            initiator = "window_control",
            cause = "user_close",
            trace_route = "window_control->hide_main_window",
            "main window close requested; application remains in tray"
        );
        api.prevent_close();
        if let Err(cause) = window.hide() {
            error!(
                event = "window.hide_failed",
                section = "window",
                initiator = "window_control",
                cause = %cause,
                trace_route = "window_control->window.hide",
                "main window could not be hidden"
            );
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
/// Starts the `BiFlow` Tauri application and blocks on its event loop.
///
/// # Panics
///
/// Panics when the diagnostic log or Tauri event loop cannot initialize.
pub fn run() {
    #[cfg(target_os = "linux")]
    apply_linux_webview_workarounds();
    initialize_diagnostics();
    #[cfg(target_os = "linux")]
    log_linux_webview_workarounds();
    let builder = tauri::Builder::default()
        .plugin(
            tauri_plugin_single_instance::Builder::new()
                .dbus_id(single_instance_dbus_id(
                    BUNDLE_IDENTIFIER,
                    version::app_version(),
                ))
                .callback(|app, _, _| {
                    info!(
                        event = "window.open_requested",
                        section = "window",
                        initiator = "second_process",
                        cause = "single_instance_activation",
                        trace_route = "second_process->single_instance_plugin->show_main",
                        "existing application instance activated"
                    );
                    show_main(app);
                })
                .build(),
        )
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_autostart::Builder::new().build())
        .setup(setup_application)
        .on_window_event(handle_window_event)
        .invoke_handler(tauri::generate_handler![
            bootstrap_app,
            get_stack_snapshot,
            get_network_status,
            get_traffic_totals,
            start_stack,
            stop_stack,
            pause_stack,
            resume_stack,
            restart_stack,
            cancel_operation,
            get_settings,
            validate_settings,
            save_settings,
            list_direct_rules,
            add_direct_rule,
            pin_route,
            remove_direct_rule,
            refresh_direct_rules,
            get_cloud_rules_status,
            sync_cloud_rules,
            list_dependencies,
            install_dependency,
            install_helper,
            get_install_guide,
            open_external_url,
            run_full_diagnostics,
            test_route,
            query_logs,
            get_debug_log_status,
            reveal_debug_log,
            delete_debug_log,
            export_support_bundle,
            fresh_hiddify_start,
            check_for_update,
            install_update,
        ]);

    let app = builder
        .build(tauri::generate_context!())
        .expect("BiFlow failed to build");
    app.run(|_, event| match event {
        tauri::RunEvent::ExitRequested { code, .. } => diagnostics::exit_requested(code),
        tauri::RunEvent::Exit => diagnostics::close_session(),
        _ => {}
    });
}

#[cfg(test)]
mod tests {
    use super::{
        merge_update_channels, packaged_rule_snapshot_dir, single_instance_dbus_id,
        update_check_backoff, update_download_percent, UpdateProgress, UpdateStatus,
        BUNDLE_IDENTIFIER, UPDATE_CHECK_ATTEMPTS, UPDATE_CHECK_FIRST_BACKOFF,
    };
    use std::{fs, time::Duration};

    #[test]
    fn update_check_attempt_timeout_bounds_a_hang() {
        assert_eq!(super::UPDATE_CHECK_ATTEMPT_TIMEOUT, Duration::from_secs(8));
        assert_eq!(super::UPDATE_INSTALL_TIMEOUT, Duration::from_secs(10 * 60));
        assert_eq!(
            super::update_in_progress_message(),
            "an update is already in progress"
        );
    }

    #[test]
    fn update_check_backoff_grows_and_stays_bounded() {
        assert_eq!(update_check_backoff(0), UPDATE_CHECK_FIRST_BACKOFF);
        assert_eq!(update_check_backoff(1), UPDATE_CHECK_FIRST_BACKOFF * 2);
        assert_eq!(update_check_backoff(2), UPDATE_CHECK_FIRST_BACKOFF * 4);
        // Every attempt after the last still yields a finite, capped wait.
        assert_eq!(update_check_backoff(9), UPDATE_CHECK_FIRST_BACKOFF * 16);

        let waits: Vec<Duration> = (0..UPDATE_CHECK_ATTEMPTS - 1)
            .map(update_check_backoff)
            .collect();
        assert!(!waits.is_empty(), "one attempt is not a retry");
        let total: Duration = waits.iter().sum();
        assert!(
            total < Duration::from_secs(10),
            "a flaky check must not stall the About page for {total:?}"
        );
    }

    #[test]
    fn update_download_percent_is_bounded() {
        assert_eq!(update_download_percent(0, Some(100)), Some(0));
        assert_eq!(update_download_percent(50, Some(100)), Some(50));
        assert_eq!(update_download_percent(100, Some(100)), Some(100));
        assert_eq!(update_download_percent(150, Some(100)), Some(100));
        assert_eq!(update_download_percent(10, None), None);
    }

    #[test]
    fn packaged_rule_snapshot_dir_finds_complete_nested_layout() {
        let directory = tempfile::tempdir().expect("tempdir");
        let nested = directory
            .path()
            .join("_up_")
            .join("resources")
            .join("rules");
        fs::create_dir_all(&nested).expect("nested rules");
        for name in ["iran-domains.txt", "iran-networks.txt", "private.txt"] {
            fs::write(nested.join(name), b"ok").expect("rule file");
        }
        assert_eq!(packaged_rule_snapshot_dir(directory.path()), nested);
    }

    #[test]
    fn packaged_rule_snapshot_dir_defaults_to_rules_when_missing() {
        let directory = tempfile::tempdir().expect("tempdir");
        assert_eq!(
            packaged_rule_snapshot_dir(directory.path()),
            directory.path().join("rules")
        );
    }

    #[test]
    fn merge_update_channels_marks_any_pending_channel() {
        let none = merge_update_channels(
            UpdateStatus {
                available: false,
                version: None,
                notes: None,
                app_available: false,
                rules_available: false,
                thirdparty_available: false,
            },
            false,
            false,
        );
        assert!(!none.available);

        let rules_only = merge_update_channels(
            UpdateStatus {
                available: false,
                version: None,
                notes: None,
                app_available: false,
                rules_available: false,
                thirdparty_available: false,
            },
            true,
            false,
        );
        assert!(rules_only.available);
        assert!(rules_only.rules_available);
        assert!(!rules_only.app_available);

        let app = merge_update_channels(
            UpdateStatus {
                available: true,
                version: Some("3.1.0".into()),
                notes: None,
                app_available: true,
                rules_available: false,
                thirdparty_available: false,
            },
            false,
            true,
        );
        assert!(app.available);
        assert!(app.app_available);
        assert!(app.thirdparty_available);
        assert_eq!(app.version.as_deref(), Some("3.1.0"));
    }

    #[test]
    fn update_progress_serializes_expected_phases() {
        let progress = UpdateProgress {
            phase: "downloading".into(),
            percent: Some(42),
            version: Some("1.2.0".into()),
            error: None,
        };
        let json = serde_json::to_value(progress).expect("serialize update progress");
        assert_eq!(json["phase"], "downloading");
        assert_eq!(json["percent"], 42);
    }

    #[cfg(target_os = "linux")]
    use super::*;

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_dmi_detects_vmware_and_ignores_bare_metal() {
        assert!(linux_dmi_is_virtual(Some("VMware, Inc.\n")));
        assert!(linux_dmi_is_virtual(Some("QEMU")));
        assert!(!linux_dmi_is_virtual(Some("Dell Inc.")));
        assert!(!linux_dmi_is_virtual(None));
    }

    #[test]
    fn single_instance_id_includes_the_full_package_version() {
        assert_eq!(
            single_instance_dbus_id(BUNDLE_IDENTIFIER, "1.2.5"),
            "app.biflow.desktop.v1_2_5"
        );
        assert_ne!(
            single_instance_dbus_id(BUNDLE_IDENTIFIER, "1.2.5"),
            single_instance_dbus_id(BUNDLE_IDENTIFIER, "1.2.6")
        );
        let config = include_str!("../tauri.conf.json");
        assert!(
            config.contains(&format!("\"identifier\": \"{BUNDLE_IDENTIFIER}\"")),
            "BUNDLE_IDENTIFIER must match tauri.conf.json"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_webview_workarounds_disable_dmabuf_and_vmware_compositing() {
        let unset = LinuxWebviewEnv {
            dmabuf_already_set: false,
            compositing_already_set: false,
            software_gl_already_set: false,
        };
        let virtual_gpu = linux_webview_workarounds(
            unset,
            LinuxGpuKind {
                virtual_or_nvidia: true,
                virtual_machine: true,
            },
        );
        assert!(virtual_gpu.disable_dmabuf);
        assert!(virtual_gpu.disable_compositing);
        assert!(virtual_gpu.software_gl);
        let nvidia = linux_webview_workarounds(
            unset,
            LinuxGpuKind {
                virtual_or_nvidia: true,
                virtual_machine: false,
            },
        );
        assert!(nvidia.disable_compositing);
        assert!(!nvidia.software_gl);
        let respected = linux_webview_workarounds(
            LinuxWebviewEnv {
                dmabuf_already_set: true,
                compositing_already_set: true,
                software_gl_already_set: true,
            },
            LinuxGpuKind {
                virtual_or_nvidia: true,
                virtual_machine: true,
            },
        );
        assert!(!respected.disable_dmabuf);
        assert!(!respected.disable_compositing);
        assert!(!respected.software_gl);
        let typical = linux_webview_workarounds(
            unset,
            LinuxGpuKind {
                virtual_or_nvidia: false,
                virtual_machine: false,
            },
        );
        assert!(typical.disable_dmabuf);
        assert!(!typical.disable_compositing);
        assert!(!typical.software_gl);
        assert!(linux_webview_reexec_needed(false, virtual_gpu));
        assert!(!linux_webview_reexec_needed(true, virtual_gpu));
        assert!(!linux_webview_reexec_needed(false, respected));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_webview_reexec_command_sets_workaround_env() {
        use std::ffi::OsStr;
        let command = linux_webview_reexec_command(
            PathBuf::from("/usr/bin/BiFlow"),
            [std::ffi::OsString::from("--flag")],
            LinuxWebviewWorkarounds {
                disable_dmabuf: true,
                disable_compositing: true,
                software_gl: false,
            },
        );
        assert_eq!(command.get_program(), OsStr::new("/usr/bin/BiFlow"));
        let args: Vec<_> = command.get_args().collect();
        assert_eq!(args, [OsStr::new("--flag")]);
        let env: Vec<(String, String)> = command
            .get_envs()
            .filter_map(|(key, value)| {
                Some((
                    key.to_string_lossy().into_owned(),
                    value?.to_string_lossy().into_owned(),
                ))
            })
            .collect();
        assert!(env
            .iter()
            .any(|(key, value)| key == "BIFLOW_WEBKIT_WORKAROUNDS" && value == "1"));
        assert!(env
            .iter()
            .any(|(key, value)| key == WEBKIT_DISABLE_DMABUF_RENDERER && value == "1"));
        assert!(env
            .iter()
            .any(|(key, value)| key == WEBKIT_DISABLE_COMPOSITING_MODE && value == "1"));
        assert!(!env.iter().any(|(key, _)| key == LIBGL_ALWAYS_SOFTWARE));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_deb_packages_do_not_self_replace() {
        assert!(!linux_updater_self_replace_supported_from(None));
        assert!(linux_updater_self_replace_supported_from(Some(
            &std::ffi::OsString::from("/tmp/BiFlow.AppImage"),
        )));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn production_linux_helper_paths_are_fixed() {
        assert_eq!(PRODUCTION_HELPER_SOCKET, "/run/iran-split/helper.sock");
        assert_eq!(PRODUCTION_SYSTEM_RUNTIME, "/var/lib/iran-split");
    }

    #[cfg(all(target_os = "linux", debug_assertions))]
    #[test]
    fn debug_linux_helper_paths_accept_development_overrides() {
        const SOCKET: &str = "/run/biflow-dev-test/helper.sock";
        const RUNTIME: &str = "/run/biflow-dev-test/runtime";
        let paths = linux_helper_paths_with_overrides(Some(SOCKET.into()), Some(RUNTIME.into()));

        assert_eq!(paths, (PathBuf::from(SOCKET), PathBuf::from(RUNTIME)));
    }

    #[cfg(all(target_os = "linux", debug_assertions))]
    #[test]
    fn debug_linux_mihomo_path_accepts_development_override() {
        const MIHOMO: &str = "/run/biflow-dev-test/mihomo";
        let path = linux_mihomo_binary_with_override(
            PathBuf::from("/default/mihomo"),
            Some(MIHOMO.into()),
        );

        assert_eq!(path, PathBuf::from(MIHOMO));
    }
}
