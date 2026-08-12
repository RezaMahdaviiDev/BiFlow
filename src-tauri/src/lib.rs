mod deps;
mod version;

use chrono::Utc;
use iran_split_config::{AppConfig, ConfigStore, ValidationIssue};
use iran_split_core::{Engine, OperationAccepted, PlatformBackend, StackPhase, StackSnapshot};
use iran_split_rules::{
    CloudRuleStore, CloudRulesStatus, DirectRulesDocument, DohResolver, Outbound, RuleManager,
    RuleSet,
};
use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, Runtime,
};
use tauri_plugin_updater::UpdaterExt;
use uuid::Uuid;

#[cfg(target_os = "linux")]
use iran_split_platform_linux::{LinuxBackend as NativeBackend, LinuxPaths};
#[cfg(target_os = "windows")]
use iran_split_platform_win::WindowsBackend as NativeBackend;

#[derive(Debug)]
struct AppServices {
    config_store: ConfigStore,
    engine: Arc<Engine<NativeBackend>>,
    backend: Arc<NativeBackend>,
    rules: RuleManager,
    cloud_rules: CloudRuleStore,
    paths: AppPaths,
}

#[derive(Debug, Clone)]
struct AppPaths {
    config: PathBuf,
    data: PathBuf,
    cache: PathBuf,
    resources: PathBuf,
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
        let resources = app
            .path()
            .resource_dir()
            .map_err(|error| error.to_string())?
            .join("rules");
        fs::create_dir_all(&data).map_err(|error| error.to_string())?;
        fs::create_dir_all(&cache).map_err(|error| error.to_string())?;
        Ok(Self {
            config,
            data,
            cache,
            resources,
        })
    }
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

fn services<R: Runtime>(app: &AppHandle<R>) -> Result<&AppServices, String> {
    app.try_state::<AppServices>()
        .map(|state| state.inner())
        .ok_or_else(|| "application services are not initialized".into())
}

#[tauri::command]
async fn bootstrap_app(app: AppHandle) -> Result<BootstrapResult, String> {
    let services = services(&app)?;
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
    })
}

#[tauri::command]
fn get_stack_snapshot(app: AppHandle) -> Result<StackSnapshot, String> {
    Ok(services(&app)?.engine.snapshot())
}

#[tauri::command]
async fn start_stack(app: AppHandle) -> Result<OperationAccepted, String> {
    services(&app)?
        .engine
        .start_stack()
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn stop_stack(app: AppHandle) -> Result<OperationAccepted, String> {
    services(&app)?
        .engine
        .stop_stack()
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn restart_stack(app: AppHandle) -> Result<OperationAccepted, String> {
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
}

#[tauri::command]
async fn cancel_operation(app: AppHandle, operation_id: Uuid) -> Result<bool, String> {
    Ok(services(&app)?.engine.cancel_operation(operation_id).await)
}

#[tauri::command]
fn get_settings(app: AppHandle) -> Result<AppConfig, String> {
    Ok(services(&app)?
        .config_store
        .load_or_create()
        .map_err(|error| error.to_string())?
        .redacted())
}

#[tauri::command]
fn validate_settings(mut draft: AppConfig, app: AppHandle) -> Result<Vec<ValidationIssue>, String> {
    let current = services(&app)?
        .config_store
        .load_or_create()
        .map_err(|error| error.to_string())?;
    draft.mihomo.controller_secret = current.mihomo.controller_secret;
    Ok(draft.validate())
}

#[tauri::command]
async fn save_settings(
    mut draft: AppConfig,
    expected_revision: u64,
    app: AppHandle,
) -> Result<AppConfig, String> {
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
}

#[tauri::command]
async fn list_direct_rules(app: AppHandle) -> Result<DirectRulesDocument, String> {
    Ok(services(&app)?.rules.list().await)
}

#[tauri::command]
async fn add_direct_rule(
    input: String,
    expected_revision: u64,
    app: AppHandle,
) -> Result<DirectRulesDocument, String> {
    services(&app)?
        .rules
        .add(&input, expected_revision)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn remove_direct_rule(
    input: String,
    expected_revision: u64,
    app: AppHandle,
) -> Result<DirectRulesDocument, String> {
    services(&app)?
        .rules
        .remove(&input, expected_revision)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn refresh_direct_rules(app: AppHandle) -> Result<DirectRulesDocument, String> {
    services(&app)?
        .rules
        .refresh()
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn get_cloud_rules_status(app: AppHandle) -> Result<CloudRulesStatus, String> {
    services(&app)?
        .cloud_rules
        .status()
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn sync_cloud_rules(app: AppHandle) -> Result<CloudRulesStatus, String> {
    services(&app)?
        .cloud_rules
        .sync()
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn list_dependencies(app: AppHandle) -> Result<Vec<deps::DependencyStatus>, String> {
    Ok(deps::dependency_status(&services(&app)?.paths.data))
}

#[tauri::command]
async fn install_dependency(id: String, app: AppHandle) -> Result<deps::InstallResult, String> {
    let parsed = deps::DependencyId::parse(&id).map_err(|error| error.to_string())?;
    let data = services(&app)?.paths.data.clone();
    match deps::install_dependency(parsed, &data).await {
        Ok(result) => Ok(result),
        Err(error) => Err(error.to_string()),
    }
}

#[tauri::command]
fn get_install_guide(id: String) -> Result<deps::InstallGuide, String> {
    let parsed = deps::DependencyId::parse(&id).map_err(|error| error.to_string())?;
    Ok(deps::install_guide(parsed))
}

#[tauri::command]
fn open_external_url(url: String) -> Result<(), String> {
    deps::open_allowlisted_url(&url).map_err(|error| error.to_string())
}

#[tauri::command]
async fn test_route(target: String, app: AppHandle) -> Result<RouteTestResult, String> {
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
    Ok(RouteTestResult {
        target,
        outbound: decision.outbound,
        reason: format!("{:?}", decision.reason).to_lowercase(),
        matched_rule: decision.matched_rule,
        reachable: None,
        tested_at: Utc::now().to_rfc3339(),
    })
}

#[tauri::command]
async fn run_full_diagnostics(app: AppHandle) -> Result<DiagnosticsReport, String> {
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
            snapshot.providers.total > 0 && snapshot.providers.ready == snapshot.providers.total,
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
    Ok(DiagnosticsReport {
        operation_id,
        steps,
        finished: true,
    })
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
    #[cfg(target_os = "linux")]
    {
        return services(&app)?
            .backend
            .service_logs(maximum.clamp(1, 2_000))
            .await
            .map_err(|error| error.to_string());
    }
    #[cfg(target_os = "windows")]
    {
        let _ = (app, maximum);
        Ok(Vec::new())
    }
}

#[tauri::command]
fn export_support_bundle(app: AppHandle) -> Result<ExportResult, String> {
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
    let files = vec!["versions.json", "config-redacted.json", "snapshot.json"];
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
    Ok(ExportResult {
        path: bundle.to_string_lossy().into_owned(),
        files: files.into_iter().map(str::to_owned).collect(),
    })
}

#[tauri::command]
async fn check_for_update(app: AppHandle) -> Result<UpdateStatus, String> {
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

#[tauri::command]
async fn install_update(app: AppHandle) -> Result<OperationAccepted, String> {
    let services = services(&app)?;
    let operation_id = Uuid::new_v4();
    if matches!(
        services.engine.snapshot().phase,
        StackPhase::Running | StackPhase::Degraded
    ) {
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
    }
    let update = app
        .updater()
        .map_err(|error| error.to_string())?
        .check()
        .await
        .map_err(|error| error.to_string())?
        .ok_or("no update is available")?;
    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|error| error.to_string())?;
    Ok(OperationAccepted {
        operation_id,
        already_complete: false,
    })
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
    let paths = AppPaths::discover(app)?;
    let config_store = ConfigStore::new(&paths.config);
    let config = config_store
        .load_or_create()
        .map_err(|error| error.to_string())?;
    let rules_cache = paths.cache.join("rules");
    fs::create_dir_all(&rules_cache).map_err(|error| error.to_string())?;
    #[cfg(target_os = "linux")]
    let mihomo_binary = deps::first_existing(&deps::mihomo_candidates(&paths.data))
        .unwrap_or_else(|| paths.data.join("bin/mihomo"));
    #[cfg(target_os = "linux")]
    let backend = Arc::new(NativeBackend::new(
        config,
        LinuxPaths {
            socket_path: "/run/iran-split/helper.sock".into(),
            user_data_dir: paths.data.clone(),
            system_runtime_dir: "/var/lib/iran-split".into(),
            resources_dir: paths.resources.clone(),
            rules_cache_dir: rules_cache.clone(),
            mihomo_binary,
        },
    ));
    #[cfg(target_os = "windows")]
    let _ = config;
    #[cfg(target_os = "windows")]
    let backend = Arc::new(NativeBackend::default());
    let engine = Engine::new(Arc::clone(&backend));
    let rules = RuleManager::load(
        paths.data.join("direct-rules.json"),
        Arc::new(DohResolver::default()),
    )
    .map_err(|error| error.to_string())?;
    let cloud_rules = CloudRuleStore::load(paths.resources.clone(), rules_cache);
    Ok(AppServices {
        config_store,
        engine,
        backend,
        rules,
        cloud_rules,
        paths,
    })
}

fn setup_tray<R: Runtime>(app: &tauri::App<R>) -> tauri::Result<()> {
    let connect = MenuItem::with_id(app, "connect", "Connect", true, None::<&str>)?;
    let disconnect = MenuItem::with_id(app, "disconnect", "Disconnect", true, None::<&str>)?;
    let open = MenuItem::with_id(app, "open", "Open", true, None::<&str>)?;
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
        &[&connect, &disconnect, &open, &quit, &disconnect_quit],
    )?;
    TrayIconBuilder::new()
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
            ) {
                show_main(tray.app_handle());
            }
        })
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_main(app),
            "connect" => {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    if let Ok(services) = services(&app) {
                        let _ = services.engine.start_stack().await;
                    }
                });
            }
            "disconnect" => {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    if let Ok(services) = services(&app) {
                        let _ = services.engine.stop_stack().await;
                    }
                });
            }
            "quit" => app.exit(0),
            "disconnect_quit" => {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    if let Ok(services) = services(&app) {
                        if services.engine.stop_stack().await.is_ok() {
                            let _ = services
                                .engine
                                .wait_for_phase(StackPhase::Stopped, Duration::from_secs(25))
                                .await;
                        }
                    }
                    app.exit(0);
                });
            }
            _ => {}
        })
        .build(app)?;
    Ok(())
}

fn show_main<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            show_main(app)
        }))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_autostart::Builder::new().build())
        .setup(|app| {
            let services =
                create_services(app.handle()).map_err(|error| std::io::Error::other(error))?;
            let mut snapshots = services.engine.subscribe();
            let engine = Arc::clone(&services.engine);
            let handle = app.handle().clone();
            app.manage(services);
            tauri::async_runtime::spawn(async move {
                while snapshots.changed().await.is_ok() {
                    let _ = handle.emit("stack-snapshot", snapshots.borrow().clone());
                }
            });
            tauri::async_runtime::spawn(async move {
                let _ = engine.reconcile_startup().await;
            });
            setup_tray(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            bootstrap_app,
            get_stack_snapshot,
            start_stack,
            stop_stack,
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
            get_install_guide,
            open_external_url,
            run_full_diagnostics,
            test_route,
            query_logs,
            export_support_bundle,
            check_for_update,
            install_update,
        ]);

    builder
        .run(tauri::generate_context!())
        .expect("BiFlow failed to start");
}
