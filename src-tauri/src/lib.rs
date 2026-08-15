mod deps;
mod diagnostics;
mod helper_install;
mod hiddify_reset;
mod network;
mod version;

use chrono::Utc;
use iran_split_config::{AppConfig, ConfigStore, ValidationIssue};
use iran_split_core::{Engine, OperationAccepted, PlatformBackend, StackPhase, StackSnapshot};
use iran_split_rules::{
    bundled_snapshot_is_complete, ensure_bundled_snapshot, CloudRuleStore, CloudRulesStatus,
    DirectRulesDocument, DohResolver, Outbound, RuleManager, RuleSet,
};
use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tauri::{
    menu::{Menu, MenuEvent, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, Runtime, Window, WindowEvent,
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
            .join("biflow/config.toml");
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
struct UpdateStatus {
    available: bool,
    version: Option<String>,
    notes: Option<String>,
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
        services(&app)?
            .engine
            .start_stack()
            .await
            .map_err(|error| error.to_string())
    })
    .await
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
        Ok(services(&app)?.engine.cancel_operation(operation_id).await)
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
/// How long to wait after launch before the first background check, and the
/// interval between later ones.
const UPDATE_BACKGROUND_DELAY: Duration = Duration::from_secs(90);
const UPDATE_BACKGROUND_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);

async fn check_update_once(app: &AppHandle) -> Result<UpdateStatus, String> {
    let update = app
        .updater()
        .map_err(|error| error.to_string())?
        .check()
        .await
        .map_err(|error| error.to_string())?;
    Ok(update.map_or(
        UpdateStatus {
            available: false,
            version: None,
            notes: None,
        },
        |update| UpdateStatus {
            available: true,
            version: Some(update.version.clone()),
            notes: update.body.clone(),
        },
    ))
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
        check_update_with_retry(&app, "tauri_command").await
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
            match check_update_with_retry(&app, "background_poll").await {
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
    update
        .download_and_install(
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
        )
        .await
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

async fn perform_signed_update_install(
    app: &AppHandle,
    operation_id: Uuid,
) -> Result<OperationAccepted, String> {
    pause_stack_for_update(services(app)?).await?;
    #[cfg(target_os = "linux")]
    if !linux_updater_self_replace_supported() {
        return open_linux_deb_release(app, operation_id);
    }
    let update = app
        .updater()
        .map_err(|error| error.to_string())?
        .check()
        .await
        .map_err(|error| error.to_string())?
        .ok_or("no update is available")?;
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
        perform_signed_update_install(&app, Uuid::new_v4()).await
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
            services(&app)?
                .engine
                .start_stack()
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

fn disconnect_and_quit_from_tray<R: Runtime>(app: &AppHandle<R>) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let result =
            diagnostics::trace_action("lifecycle", "tray_menu", "disconnect_and_quit", async {
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
                Ok::<(), String>(())
            })
            .await;
        if let Err(cause) = result {
            error!(
                event = "shutdown.disconnect_failed",
                section = "lifecycle",
                initiator = "tray_menu",
                cause,
                trace_route = "tray_menu->stop_stack->application_exit",
                "disconnect before quit failed"
            );
        }
        diagnostics::flush();
        app.exit(0);
    });
}

fn handle_tray_menu<R: Runtime>(app: &AppHandle<R>, event: &MenuEvent) {
    match event.id.as_ref() {
        "open" => {
            info!(
                event = "window.open_requested",
                section = "window",
                initiator = "tray_menu",
                cause = "open_selected",
                trace_route = "tray_menu->show_main",
                "main window open requested"
            );
            show_main(app);
        }
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
        "disconnect_quit" => disconnect_and_quit_from_tray(app),
        "about" => {
            info!(
                event = "window.about_requested",
                section = "window",
                initiator = "tray_menu",
                cause = "about_selected",
                trace_route = "tray_menu->show_main->app-navigate",
                "about page requested from tray"
            );
            show_main(app);
            if let Err(cause) = app.emit("app-navigate", "about") {
                warn!(
                    event = "navigation.emit_failed",
                    section = "window",
                    initiator = "tray_menu",
                    cause = %cause,
                    trace_route = "tray_menu->emit(app-navigate)",
                    "about navigation event could not be emitted"
                );
            }
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

fn setup_tray<R: Runtime>(app: &tauri::App<R>) -> tauri::Result<()> {
    let connect = MenuItem::with_id(app, "connect", "Connect", true, None::<&str>)?;
    let pause = MenuItem::with_id(app, "pause", "Pause", true, None::<&str>)?;
    let resume = MenuItem::with_id(app, "resume", "Resume", true, None::<&str>)?;
    let disconnect = MenuItem::with_id(app, "disconnect", "Disconnect", true, None::<&str>)?;
    let open = MenuItem::with_id(app, "open", "Open", true, None::<&str>)?;
    let about = MenuItem::with_id(app, "about", "About", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit UI", true, None::<&str>)?;
    let disconnect_quit = MenuItem::with_id(
        app,
        "disconnect_quit",
        "Disconnect & Quit",
        true,
        None::<&str>,
    )?;
    let menu = Menu::with_items(
        app,
        &[
            &connect,
            &pause,
            &resume,
            &disconnect,
            &open,
            &about,
            &quit,
            &disconnect_quit,
        ],
    )?;
    let icon = app.default_window_icon().cloned().ok_or_else(|| {
        tauri::Error::from(std::io::Error::other("default window icon is missing"))
    })?;
    TrayIconBuilder::new()
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
    let handle = app.handle().clone();
    app.manage(services);
    tauri::async_runtime::spawn(async move {
        while snapshots.changed().await.is_ok() {
            if let Err(cause) = handle.emit("stack-snapshot", snapshots.borrow().clone()) {
                warn!(
                    event = "snapshot.emit_failed",
                    section = "stack",
                    initiator = "snapshot_watcher",
                    cause = %cause,
                    trace_route = "engine->snapshot_watcher->frontend_event",
                    "stack snapshot event could not be emitted"
                );
            }
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

fn handle_window_event<R: Runtime>(window: &Window<R>, event: &WindowEvent) {
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
        packaged_rule_snapshot_dir, single_instance_dbus_id, update_check_backoff,
        update_download_percent, UpdateProgress, BUNDLE_IDENTIFIER, UPDATE_CHECK_ATTEMPTS,
        UPDATE_CHECK_FIRST_BACKOFF,
    };
    use std::{fs, time::Duration};

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
