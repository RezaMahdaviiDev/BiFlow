#![cfg(target_os = "linux")]

use async_trait::async_trait;
use iran_split_config::{AppConfig, ExecutableSetting};
use iran_split_core::{
    CleanupReport, ComponentPhase, ComponentStatus, CoreError, HelperStatus, OpenVpnState,
    PlatformBackend, ProcessStatus, ProviderSummary, ReadinessReport, RuntimeGeneration,
    RuntimeHealth, TunStatus,
};
use iran_split_ipc::{
    read_frame, validate_envelope, write_frame, Envelope, HelperCommand, HelperReply,
    OpenVpnRequest, OpenVpnStatus, PROTOCOL_VERSION,
};
use iran_split_mihomo::{
    generate_config, probe_hiddify_egress, validate_with_binary, ControllerClient, MihomoError,
    OpenVpnOutbound, Platform, RuntimePaths,
};
use iran_split_rules::{DirectRulesDocument, DirectTarget};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    net::IpAddr,
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
    time::Duration,
};
use tempfile::NamedTempFile;
use thiserror::Error;
use tokio::{
    net::{TcpStream, UnixStream},
    process::{Child, Command},
    sync::{Mutex, RwLock},
};
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};
use uuid::Uuid;

mod system_proxy;

const IPC_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Error)]
pub enum LinuxBackendError {
    #[error("helper IPC failed: {0}")]
    Protocol(#[from] iran_split_ipc::ProtocolError),
    #[error("helper connection failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("helper request timed out")]
    Timeout,
    #[error("helper response did not match the request")]
    ResponseMismatch,
    #[error("helper returned {code}: {message}")]
    Helper { code: String, message: String },
}

#[derive(Debug, Clone)]
pub struct HelperClient {
    socket_path: PathBuf,
}

impl HelperClient {
    #[must_use]
    pub fn new(socket_path: impl Into<PathBuf>) -> Self {
        Self {
            socket_path: socket_path.into(),
        }
    }

    /// Sends one validated command to the privileged helper.
    ///
    /// # Errors
    ///
    /// Returns an error when validation, connection, protocol negotiation, or
    /// the helper operation fails.
    pub async fn request(&self, command: HelperCommand) -> Result<HelperReply, LinuxBackendError> {
        let command_name = command.audit_name();
        let request_id = Uuid::new_v4();
        info!(
            event = "helper.request_started",
            section = "helper_ipc",
            initiator = "linux_platform_backend",
            cause = "backend_operation",
            trace_id = %request_id,
            trace_route = "desktop_engine->linux_platform_backend->helper_ipc",
            command = command_name,
            "helper request started"
        );
        if let Err(cause) = command.validate() {
            error!(
                event = "helper.request_failed",
                section = "helper_ipc",
                initiator = "linux_platform_backend",
                cause = %cause,
                trace_id = %request_id,
                trace_route = "desktop_engine->linux_platform_backend->helper_ipc->validation",
                command = command_name,
                "helper request validation failed"
            );
            return Err(cause.into());
        }
        let result = self.request_validated(command).await;
        match &result {
            Ok(_) => info!(
                event = "helper.request_completed",
                section = "helper_ipc",
                initiator = "linux_platform_backend",
                cause = "none",
                trace_id = %request_id,
                trace_route = "desktop_engine->linux_platform_backend->helper_ipc->reply",
                command = command_name,
                "helper request completed"
            ),
            Err(cause) => error!(
                event = "helper.request_failed",
                section = "helper_ipc",
                initiator = "linux_platform_backend",
                cause = %cause,
                trace_id = %request_id,
                trace_route = "desktop_engine->linux_platform_backend->helper_ipc->error",
                command = command_name,
                "helper request failed"
            ),
        }
        result
    }

    async fn request_validated(
        &self,
        command: HelperCommand,
    ) -> Result<HelperReply, LinuxBackendError> {
        let mut stream = tokio::time::timeout(IPC_TIMEOUT, UnixStream::connect(&self.socket_path))
            .await
            .map_err(|_| LinuxBackendError::Timeout)??;
        let hello = Envelope::new(HelperCommand::Hello {
            client_version: env!("CARGO_PKG_VERSION").into(),
            supported_protocols: vec![PROTOCOL_VERSION],
        });
        let hello_reply = exchange(&mut stream, &hello).await?;
        match hello_reply.payload {
            HelperReply::Hello(reply) if reply.selected_protocol == PROTOCOL_VERSION => {}
            HelperReply::Error(error) => {
                return Err(LinuxBackendError::Helper {
                    code: error.code,
                    message: error.message,
                });
            }
            _ => return Err(LinuxBackendError::ResponseMismatch),
        }
        let request = Envelope::new(command);
        let response = exchange(&mut stream, &request).await?;
        match response.payload {
            HelperReply::Error(error) => Err(LinuxBackendError::Helper {
                code: error.code,
                message: error.message,
            }),
            reply => Ok(reply),
        }
    }
}

async fn exchange(
    stream: &mut UnixStream,
    request: &Envelope<HelperCommand>,
) -> Result<Envelope<HelperReply>, LinuxBackendError> {
    tokio::time::timeout(IPC_TIMEOUT, write_frame(stream, request))
        .await
        .map_err(|_| LinuxBackendError::Timeout)??;
    let reply: Envelope<HelperReply> = tokio::time::timeout(IPC_TIMEOUT, read_frame(stream))
        .await
        .map_err(|_| LinuxBackendError::Timeout)??;
    validate_envelope(&reply)?;
    if reply.request_id != request.request_id {
        return Err(LinuxBackendError::ResponseMismatch);
    }
    Ok(reply)
}

#[derive(Debug, Clone)]
pub struct LinuxPaths {
    pub socket_path: PathBuf,
    pub user_data_dir: PathBuf,
    pub system_runtime_dir: PathBuf,
    pub resources_dir: PathBuf,
    pub rules_cache_dir: PathBuf,
    pub mihomo_binary: PathBuf,
}

#[derive(Debug, Clone)]
struct PreparedGeneration {
    generation: RuntimeGeneration,
    config_path: PathBuf,
}

#[derive(Debug)]
pub struct LinuxBackend {
    config: Arc<RwLock<AppConfig>>,
    helper: HelperClient,
    paths: LinuxPaths,
    prepared: Mutex<Option<PreparedGeneration>>,
    launched_hiddify: Mutex<Option<Child>>,
    hiddify_exit_ip: Mutex<Option<String>>,
    /// Last status the helper reported for the `OpenVPN` side tunnel.
    ///
    /// `prepare_runtime` reads it to name the device in the Mihomo
    /// configuration and to keep the tunnel's own server address DIRECT, which
    /// is why the tunnel has to start before the generation is built.
    openvpn: Mutex<Option<OpenVpnStatus>>,
}

impl LinuxBackend {
    #[must_use]
    pub fn new(config: AppConfig, paths: LinuxPaths) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
            helper: HelperClient::new(&paths.socket_path),
            paths,
            prepared: Mutex::new(None),
            launched_hiddify: Mutex::new(None),
            hiddify_exit_ip: Mutex::new(None),
            openvpn: Mutex::new(None),
        }
    }

    pub async fn update_config(&self, config: AppConfig) {
        *self.config.write().await = config;
    }

    /// Reads at most `maximum` recent helper service log entries.
    ///
    /// # Errors
    ///
    /// Returns an error when the helper exchange fails or returns an unexpected
    /// reply variant.
    pub async fn service_logs(
        &self,
        maximum: u16,
    ) -> Result<Vec<iran_split_ipc::ServiceLogEntry>, LinuxBackendError> {
        match self
            .helper
            .request(HelperCommand::CollectServiceLogs {
                max_entries: maximum,
            })
            .await?
        {
            HelperReply::Logs(logs) => Ok(logs),
            _ => Err(LinuxBackendError::ResponseMismatch),
        }
    }

    async fn helper_request(&self, command: HelperCommand) -> Result<HelperReply, CoreError> {
        self.helper
            .request(command)
            .await
            .map_err(|error| CoreError::Platform(error.to_string()))
    }

    /// Sends an `OpenVPN` command, surfacing helper refusals as an `OpenVPN`
    /// failure rather than a generic platform error, so the desktop can tell
    /// the user which component is at fault.
    async fn openvpn_request(&self, command: HelperCommand) -> Result<OpenVpnStatus, CoreError> {
        match self
            .helper
            .request(command)
            .await
            .map_err(|error| CoreError::OpenVpnFailed(error.to_string()))?
        {
            HelperReply::OpenVpnStatus(status) => Ok(status),
            _ => Err(CoreError::OpenVpnFailed(
                "helper returned an unexpected reply".into(),
            )),
        }
    }

    fn openvpn_component(status: Option<&OpenVpnStatus>) -> ComponentStatus {
        match status {
            Some(status) if status.running => ComponentStatus::new(
                ComponentPhase::Running,
                status
                    .device
                    .clone()
                    .map(|device| format!("{device} · {} routes", status.routes.len())),
            ),
            Some(status) => ComponentStatus::new(
                ComponentPhase::Degraded,
                status.last_error.clone().or_else(|| Some("stopped".into())),
            ),
            None => ComponentStatus::new(ComponentPhase::Stopped, None),
        }
    }

    async fn hiddify_listening(config: &AppConfig) -> bool {
        Self::tcp_listening(&config.hiddify.host, config.hiddify.port).await
    }

    async fn tcp_listening(host: &str, port: u16) -> bool {
        tokio::time::timeout(Duration::from_millis(750), TcpStream::connect((host, port)))
            .await
            .is_ok_and(|result| result.is_ok())
    }

    fn discover_hiddify(config: &AppConfig, data: &Path) -> Option<PathBuf> {
        if let ExecutableSetting::Path(path) = &config.hiddify.executable {
            return path.is_file().then(|| path.clone());
        }
        let mut candidates = vec![
            data.join("bin/hiddify"),
            data.join("bin/hiddify-app"),
            data.join("apps/Hiddify.AppImage"),
            PathBuf::from("/usr/bin/hiddify"),
            PathBuf::from("/usr/bin/hiddify-app"),
            PathBuf::from("/usr/local/bin/hiddify"),
            PathBuf::from("/opt/Hiddify/Hiddify"),
            PathBuf::from("/opt/hiddify/hiddify"),
            PathBuf::from("/opt/hiddify/hiddify-app"),
        ];
        if let Some(home) = std::env::var_os("HOME") {
            let home = PathBuf::from(home);
            candidates.push(home.join(".local/bin/hiddify"));
            candidates.push(home.join(".local/bin/hiddify-app"));
        }
        if let Some(path) = std::env::var_os("PATH") {
            for dir in std::env::split_paths(&path) {
                candidates.push(dir.join("hiddify"));
                candidates.push(dir.join("hiddify-app"));
            }
        }
        candidates.into_iter().find(|path| path.is_file())
    }

    fn helper_component(result: Result<HelperStatus, CoreError>) -> ComponentStatus {
        match result {
            Ok(status) if status.available && status.authorized => ComponentStatus::new(
                ComponentPhase::Running,
                status
                    .version
                    .map(|version| format!("Helper {version} is ready")),
            ),
            Ok(status) if status.available => ComponentStatus::new(
                ComponentPhase::Degraded,
                Some("Helper is running but this user is not authorized".into()),
            ),
            Ok(_) => ComponentStatus::new(
                ComponentPhase::Unavailable,
                Some("Helper service is not installed or running".into()),
            ),
            Err(error) => ComponentStatus::new(ComponentPhase::Error, Some(error.to_string())),
        }
    }

    fn hiddify_component(
        config: &AppConfig,
        listening: bool,
        executable: Option<&Path>,
    ) -> ComponentStatus {
        if listening {
            ComponentStatus::new(
                ComponentPhase::Running,
                Some(format!(
                    "Listening on {}:{}",
                    config.hiddify.host, config.hiddify.port
                )),
            )
        } else if let Some(path) = executable {
            ComponentStatus::new(
                ComponentPhase::Stopped,
                Some(format!("Installed at {}", path.display())),
            )
        } else {
            ComponentStatus::new(
                ComponentPhase::Unavailable,
                Some("Hiddify is not installed and its local proxy is not listening".into()),
            )
        }
    }

    async fn mihomo_component(
        config: &AppConfig,
        controller_listening: bool,
        executable: Option<&Path>,
    ) -> (ComponentStatus, ProviderSummary) {
        if !controller_listening {
            let component = executable.map_or_else(
                || {
                    ComponentStatus::new(
                        ComponentPhase::Unavailable,
                        Some("Mihomo is not installed and its controller is not listening".into()),
                    )
                },
                |path| {
                    ComponentStatus::new(
                        ComponentPhase::Stopped,
                        Some(format!("Installed at {}", path.display())),
                    )
                },
            );
            return (component, ProviderSummary::default());
        }
        let Ok(controller) = ControllerClient::new(
            &config.mihomo.controller_host,
            config.mihomo.controller_port,
            config.mihomo.controller_secret.clone(),
        ) else {
            return (
                ComponentStatus::new(
                    ComponentPhase::Error,
                    Some("Mihomo controller address is invalid".into()),
                ),
                ProviderSummary::default(),
            );
        };
        match controller.version().await {
            Ok(version) => {
                let providers = controller.provider_summary().await.map_or_else(
                    |_| ProviderSummary::default(),
                    |summary| ProviderSummary {
                        ready: summary.ready,
                        total: summary.total,
                        rules_loaded: summary.rules_loaded,
                        last_refresh: Some(chrono::Utc::now()),
                    },
                );
                (
                    ComponentStatus::new(
                        ComponentPhase::Running,
                        Some(format!("Controller {} is ready", version.version)),
                    ),
                    providers,
                )
            }
            Err(error) => (
                ComponentStatus::new(
                    ComponentPhase::Degraded,
                    Some(format!(
                        "Controller port is active but BiFlow cannot authenticate: {error}"
                    )),
                ),
                ProviderSummary::default(),
            ),
        }
    }

    fn tun_component(tun_name: &str) -> ComponentStatus {
        let active = Path::new("/sys/class/net").join(tun_name).exists();
        ComponentStatus::new(
            if active {
                ComponentPhase::Running
            } else {
                ComponentPhase::Stopped
            },
            Some(if active {
                format!("Interface {tun_name} is active")
            } else {
                format!("Interface {tun_name} is absent")
            }),
        )
    }

    fn dns_component(port: u16, listening: bool) -> ComponentStatus {
        ComponentStatus::new(
            if listening {
                ComponentPhase::Running
            } else {
                ComponentPhase::Stopped
            },
            Some(if listening {
                format!("DNS is listening on 127.0.0.1:{port}")
            } else {
                format!("No DNS listener on 127.0.0.1:{port}")
            }),
        )
    }

    /// Describes the running side tunnel to Mihomo, or `None` when there is
    /// nothing to describe.
    ///
    /// The server address matters as much as the device: without it Mihomo
    /// would route the tunnel's own packets back into the split TUN.
    async fn openvpn_outbound(&self, config: &AppConfig) -> Option<OpenVpnOutbound> {
        let status = self.openvpn.lock().await.clone()?;
        if !status.running {
            return None;
        }
        Some(OpenVpnOutbound {
            device: status
                .device
                .unwrap_or_else(|| config.openvpn.device.clone()),
            routing_mark: config.openvpn.routing_mark,
            server_endpoints: status.server_endpoint.into_iter().collect(),
        })
    }

    async fn prepared(&self) -> Result<PreparedGeneration, CoreError> {
        self.prepared
            .lock()
            .await
            .clone()
            .ok_or_else(|| CoreError::ConfigInvalid("runtime has not been prepared".into()))
    }

    async fn probe_hiddify_until_ready(
        &self,
        config: &AppConfig,
        cancel: CancellationToken,
    ) -> Result<(), CoreError> {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(45);
        let mut last_cause;
        loop {
            if cancel.is_cancelled() {
                return Err(CoreError::Cancelled);
            }
            match probe_hiddify_egress(
                &config.hiddify.host,
                config.hiddify.port,
                Duration::from_secs(2),
            )
            .await
            {
                Ok(exit_ip) => {
                    info!(
                        event = "hiddify.egress_ready",
                        section = "hiddify_process",
                        initiator = "linux_platform_backend",
                        cause = "socks_probe",
                        trace_route = "desktop_engine->linux_platform_backend->hiddify_egress",
                        "Hiddify SOCKS egress is reachable"
                    );
                    *self.hiddify_exit_ip.lock().await = Some(exit_ip);
                    return Ok(());
                }
                Err(error) => {
                    last_cause = error.to_string();
                    warn!(
                        event = "hiddify.egress_probe_failed",
                        section = "hiddify_process",
                        initiator = "linux_platform_backend",
                        cause = %error,
                        trace_route = "desktop_engine->linux_platform_backend->hiddify_egress",
                        "Hiddify SOCKS egress probe failed; retrying before TUN starts"
                    );
                }
            }
            if tokio::time::Instant::now() >= deadline {
                error!(
                    event = "hiddify.egress_probe_exhausted",
                    section = "hiddify_process",
                    initiator = "linux_platform_backend",
                    cause = last_cause.as_str(),
                    trace_route = "desktop_engine->linux_platform_backend->hiddify_egress",
                    "Hiddify was listening but SOCKS egress did not become ready"
                );
                return Err(CoreError::HiddifyEgressUnavailable);
            }
            tokio::select! {
                () = cancel.cancelled() => return Err(CoreError::Cancelled),
                () = tokio::time::sleep(Duration::from_millis(500)) => {}
            }
        }
    }
}

#[async_trait]
impl PlatformBackend for LinuxBackend {
    async fn runtime_health(&self) -> RuntimeHealth {
        let config = self.config.read().await.clone();
        let hiddify_path = Self::discover_hiddify(&config, &self.paths.user_data_dir);
        let mihomo_path = self
            .paths
            .mihomo_binary
            .is_file()
            .then(|| self.paths.mihomo_binary.clone());
        let (helper_result, hiddify_listening, controller_listening, dns_listening) = tokio::join!(
            self.helper_status(),
            Self::hiddify_listening(&config),
            Self::tcp_listening(
                &config.mihomo.controller_host,
                config.mihomo.controller_port
            ),
            Self::tcp_listening(&config.mihomo.controller_host, config.mihomo.dns_port),
        );

        let helper = Self::helper_component(helper_result);
        let hiddify = Self::hiddify_component(&config, hiddify_listening, hiddify_path.as_deref());
        let (mihomo, providers) =
            Self::mihomo_component(&config, controller_listening, mihomo_path.as_deref()).await;
        let tun = Self::tun_component(&config.mihomo.tun_name);
        let dns = Self::dns_component(config.mihomo.dns_port, dns_listening);

        RuntimeHealth {
            helper,
            hiddify,
            openvpn: Self::openvpn_component(self.openvpn.lock().await.as_ref()),
            mihomo,
            tun,
            dns,
            providers,
        }
    }

    async fn helper_status(&self) -> Result<HelperStatus, CoreError> {
        match self.helper.request(HelperCommand::GetServiceStatus).await {
            Ok(HelperReply::ServiceStatus(status)) => Ok(HelperStatus {
                available: true,
                authorized: status.authorized,
                version: Some(status.helper_version),
            }),
            Ok(_) => Err(CoreError::Platform("unexpected helper status reply".into())),
            Err(LinuxBackendError::Io(error))
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
                ) =>
            {
                Ok(HelperStatus::default())
            }
            Err(error) => Err(CoreError::Platform(error.to_string())),
        }
    }

    async fn ensure_hiddify(&self, cancel: CancellationToken) -> Result<(), CoreError> {
        let config = self.config.read().await.clone();
        if !Self::hiddify_listening(&config).await {
            let executable = Self::discover_hiddify(&config, &self.paths.user_data_dir)
                .ok_or(CoreError::HiddifyNotFound)?;
            let child = Command::new(executable)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .kill_on_drop(false)
                .spawn()
                .map_err(|error| CoreError::Platform(error.to_string()))?;
            *self.launched_hiddify.lock().await = Some(child);
            let deadline = tokio::time::Instant::now()
                + Duration::from_secs(config.hiddify.start_timeout_seconds);
            loop {
                if Self::hiddify_listening(&config).await {
                    break;
                }
                if tokio::time::Instant::now() >= deadline {
                    return Err(CoreError::HiddifyEgressUnavailable);
                }
                tokio::select! {
                    () = cancel.cancelled() => return Err(CoreError::Cancelled),
                    () = tokio::time::sleep(Duration::from_millis(250)) => {}
                }
            }
        }
        self.probe_hiddify_until_ready(&config, cancel).await
    }

    async fn ensure_openvpn(
        &self,
        cancel: CancellationToken,
    ) -> Result<Option<OpenVpnState>, CoreError> {
        let config = self.config.read().await.clone();
        let Some(profile) = config
            .openvpn
            .active()
            .then(|| config.openvpn.profile.clone())
            .flatten()
        else {
            // Not enabled is not a failure: most users never run a side tunnel.
            self.openvpn.lock().await.take();
            return Ok(None);
        };
        if cancel.is_cancelled() {
            return Err(CoreError::Cancelled);
        }
        let status = self
            .openvpn_request(HelperCommand::StartOpenVpn(OpenVpnRequest {
                profile,
                auth_file: config.openvpn.auth_file.clone(),
                executable: config.openvpn.executable.clone(),
                device: config.openvpn.device.clone(),
                pull_routes: config.openvpn.pull_routes,
                tunnel_routes: config.openvpn.tunnel_routes.clone(),
                routing_mark: config.openvpn.routing_mark,
                routing_table: config.openvpn.routing_table,
                start_timeout_seconds: config.openvpn.start_timeout_seconds,
            }))
            .await?;
        let state = OpenVpnState {
            device: status
                .device
                .clone()
                .unwrap_or_else(|| config.openvpn.device.clone()),
            local_address: status.local_address.clone(),
            routes: status.routes.clone(),
        };
        *self.openvpn.lock().await = Some(status);
        Ok(Some(state))
    }

    async fn openvpn_required(&self) -> bool {
        self.config.read().await.openvpn.required
    }

    async fn stop_openvpn(&self) -> Result<(), CoreError> {
        self.openvpn.lock().await.take();
        self.openvpn_request(HelperCommand::StopOpenVpn).await?;
        Ok(())
    }

    async fn prepare_runtime(&self) -> Result<RuntimeGeneration, CoreError> {
        let config = self.config.read().await.clone();
        let generation_id = Uuid::new_v4();
        let staging_root = self
            .paths
            .user_data_dir
            .join("runtime")
            .join("generations")
            .join(generation_id.to_string());
        fs::create_dir_all(&staging_root).map_err(|error| platform_error(&error))?;
        let runtime_paths = RuntimePaths {
            private_networks: PathBuf::from("private.txt"),
            iran_domains: PathBuf::from("iran-domains.txt"),
            iran_business_domains: PathBuf::from("iran-business-domains.txt"),
            iran_networks: PathBuf::from("iran-networks.txt"),
            custom_direct_domains: PathBuf::from("custom-direct-domains.txt"),
            custom_direct_ips: PathBuf::from("custom-direct-ips.txt"),
            custom_vpn_domains: PathBuf::from("custom-vpn-domains.txt"),
            custom_vpn_ips: PathBuf::from("custom-vpn-ips.txt"),
            custom_openvpn_domains: PathBuf::from("custom-openvpn-domains.txt"),
            custom_openvpn_ips: PathBuf::from("custom-openvpn-ips.txt"),
        };
        let rules_path = self.paths.user_data_dir.join("direct-rules.json");
        let custom: DirectRulesDocument = if rules_path.exists() {
            serde_json::from_slice(&fs::read(rules_path).map_err(|error| platform_error(&error))?)
                .map_err(|error| CoreError::ConfigInvalid(error.to_string()))?
        } else {
            DirectRulesDocument::default()
        };
        let outbound = self.openvpn_outbound(&config).await;
        let generated = generate_config(
            &config,
            Platform::Linux,
            &runtime_paths,
            &custom,
            outbound.as_ref(),
        )
        .map_err(|error| CoreError::ConfigInvalid(error.to_string()))?;
        copy_rule_file(
            &self.paths.resources_dir,
            &self.paths.rules_cache_dir,
            &staging_root,
            "private.txt",
        )?;
        copy_rule_file(
            &self.paths.resources_dir,
            &self.paths.rules_cache_dir,
            &staging_root,
            "iran-domains.txt",
        )?;
        copy_rule_file(
            &self.paths.resources_dir,
            &self.paths.rules_cache_dir,
            &staging_root,
            "iran-networks.txt",
        )?;
        copy_rule_file(
            &self.paths.resources_dir,
            &self.paths.rules_cache_dir,
            &staging_root,
            "iran-business-domains.txt",
        )?;
        write_custom_provider_files(&staging_root, &custom)?;
        write_atomic(&staging_root.join("config.yaml"), generated.yaml.as_bytes())?;
        let generation = RuntimeGeneration {
            generation_id,
            config_sha256: generated.sha256,
        };
        *self.prepared.lock().await = Some(PreparedGeneration {
            generation: generation.clone(),
            config_path: staging_root.join("config.yaml"),
        });
        Ok(generation)
    }

    async fn validate_runtime(&self, generation: &RuntimeGeneration) -> Result<(), CoreError> {
        if !self.paths.mihomo_binary.is_file() {
            return Err(CoreError::MihomoNotFound);
        }
        let prepared = self.prepared().await?;
        if prepared.generation != *generation {
            return Err(CoreError::ConfigInvalid(
                "generation differs from the latest prepared runtime".into(),
            ));
        }
        validate_with_binary(
            &self.paths.mihomo_binary,
            &prepared.config_path,
            Duration::from_secs(10),
        )
        .await
        .map_err(|error| CoreError::ConfigInvalid(error.to_string()))
    }

    async fn start_core(&self, generation: &RuntimeGeneration) -> Result<(), CoreError> {
        match self
            .helper_request(HelperCommand::RegisterRuntimeGeneration {
                generation_id: generation.generation_id,
                config_sha256: generation.config_sha256.clone(),
            })
            .await?
        {
            HelperReply::GenerationRegistered { generation_id }
                if generation_id == generation.generation_id => {}
            _ => return Err(CoreError::Platform("generation registration failed".into())),
        }
        match self
            .helper_request(HelperCommand::StartMihomo {
                generation_id: generation.generation_id,
                config_sha256: generation.config_sha256.clone(),
            })
            .await?
        {
            HelperReply::ProcessStatus(status) if status.running => Ok(()),
            _ => Err(CoreError::MihomoStartFailed(
                "helper did not report a running process".into(),
            )),
        }
    }

    async fn stop_core(&self) -> Result<(), CoreError> {
        match self.helper_request(HelperCommand::StopMihomo).await? {
            HelperReply::ProcessStatus(status) if !status.running => {}
            _ => return Err(CoreError::Platform("helper did not stop Mihomo".into())),
        }
        Ok(())
    }

    async fn stop_user_proxy(&self) -> Result<(), CoreError> {
        let config = self.config.read().await.clone();
        if !config.hiddify.stop_with_stack {
            return Ok(());
        }
        if let Some(mut child) = self.launched_hiddify.lock().await.take() {
            let trace_id = Uuid::new_v4();
            if let Err(cause) = child.start_kill() {
                warn!(
                    event = "hiddify.stop_signal_failed",
                    section = "hiddify_process",
                    initiator = "linux_platform_backend",
                    cause = %cause,
                    trace_id = %trace_id,
                    trace_route = "desktop_engine->linux_platform_backend->hiddify_process",
                    "could not send the stop signal to the Hiddify child process"
                );
            }
            match tokio::time::timeout(Duration::from_secs(5), child.wait()).await {
                Ok(Ok(status)) => info!(
                    event = "hiddify.process_stopped",
                    section = "hiddify_process",
                    initiator = "linux_platform_backend",
                    cause = "stop_with_stack",
                    trace_id = %trace_id,
                    trace_route = "desktop_engine->linux_platform_backend->hiddify_process",
                    exit_status = %status,
                    "Hiddify child process stopped"
                ),
                Ok(Err(cause)) => warn!(
                    event = "hiddify.wait_failed",
                    section = "hiddify_process",
                    initiator = "linux_platform_backend",
                    cause = %cause,
                    trace_id = %trace_id,
                    trace_route = "desktop_engine->linux_platform_backend->hiddify_process",
                    "could not collect the stopped Hiddify child process"
                ),
                Err(cause) => warn!(
                    event = "hiddify.stop_timed_out",
                    section = "hiddify_process",
                    initiator = "linux_platform_backend",
                    cause = %cause,
                    trace_id = %trace_id,
                    trace_route = "desktop_engine->linux_platform_backend->hiddify_process",
                    timeout_seconds = 5_u64,
                    "Hiddify child process did not stop before the timeout"
                ),
            }
        }
        Ok(())
    }

    async fn clear_hiddify_system_proxy(&self) -> Result<bool, CoreError> {
        let config = self.config.read().await.clone();
        let persist = system_proxy::snapshot_path(&self.paths.user_data_dir);
        system_proxy::clear_if_hiddify(&config.hiddify.host, config.hiddify.port, &persist)
            .await
            .map(|cleared| cleared.is_some())
    }

    async fn restore_hiddify_system_proxy(&self) -> Result<(), CoreError> {
        let persist = system_proxy::snapshot_path(&self.paths.user_data_dir);
        system_proxy::restore(&persist).await
    }

    async fn core_process(&self) -> Result<ProcessStatus, CoreError> {
        match self
            .helper_request(HelperCommand::GetMihomoProcessStatus)
            .await?
        {
            HelperReply::ProcessStatus(status) => Ok(ProcessStatus {
                running: status.running,
                pid: status.pid,
            }),
            _ => Err(CoreError::Platform(
                "unexpected process status reply".into(),
            )),
        }
    }

    async fn tun_status(&self) -> Result<TunStatus, CoreError> {
        let config = self.config.read().await;
        let name = config.mihomo.tun_name.clone();
        Ok(TunStatus {
            active: Path::new("/sys/class/net").join(&name).exists(),
            name: Some(name),
        })
    }

    async fn check_readiness(
        &self,
        cancel: CancellationToken,
    ) -> Result<ReadinessReport, CoreError> {
        let config = self.config.read().await.clone();
        let controller = ControllerClient::new(
            &config.mihomo.controller_host,
            config.mihomo.controller_port,
            config.mihomo.controller_secret,
        )
        .map_err(|error| CoreError::ConfigInvalid(error.to_string()))?;
        info!(
            event = "mihomo.readiness_wait_started",
            section = "runtime_health",
            initiator = "linux_platform_backend",
            cause = "core_started",
            trace_route = "desktop_engine->linux_platform_backend->mihomo_controller",
            "waiting for the Mihomo controller and rule providers"
        );
        let providers = match controller
            .wait_until_ready(Duration::from_secs(20), cancel.clone())
            .await
        {
            Ok(providers) => {
                info!(
                    event = "mihomo.readiness_wait_completed",
                    section = "runtime_health",
                    initiator = "linux_platform_backend",
                    cause = "none",
                    trace_route = "desktop_engine->linux_platform_backend->mihomo_controller",
                    ready = providers.ready,
                    total = providers.total,
                    rules_loaded = providers.rules_loaded,
                    "Mihomo controller and rule providers are ready"
                );
                providers
            }
            Err(error) => {
                error!(
                    event = "mihomo.readiness_wait_failed",
                    section = "runtime_health",
                    initiator = "linux_platform_backend",
                    cause = %error,
                    trace_route = "desktop_engine->linux_platform_backend->mihomo_controller",
                    "Mihomo readiness wait failed"
                );
                return Err(readiness_error(error));
            }
        };
        if cancel.is_cancelled() {
            return Err(CoreError::Cancelled);
        }
        let exit_ip = self.hiddify_exit_ip.lock().await.clone();
        if exit_ip.is_none() {
            error!(
                event = "hiddify.egress_missing_after_tun",
                section = "runtime_health",
                initiator = "linux_platform_backend",
                cause = "pre_tun_probe_missing",
                trace_route = "desktop_engine->linux_platform_backend->hiddify_egress",
                "Hiddify egress was not confirmed before TUN start"
            );
            return Err(CoreError::HiddifyEgressUnavailable);
        }
        Ok(ReadinessReport {
            controller_ready: true,
            egress_ready: true,
            providers: ProviderSummary {
                ready: providers.ready,
                total: providers.total,
                rules_loaded: providers.rules_loaded,
                last_refresh: Some(chrono::Utc::now()),
            },
            exit_ip,
        })
    }

    async fn cleanup_owned_state(&self) -> Result<CleanupReport, CoreError> {
        match self
            .helper_request(HelperCommand::CleanupOwnedNetworkState)
            .await?
        {
            HelperReply::CleanupReport(report) => {
                // The helper tore the side tunnel down as part of cleanup, so
                // the cached status would otherwise describe a tunnel that no
                // longer exists.
                self.openvpn.lock().await.take();
                Ok(CleanupReport {
                    process_stopped: report.process_stopped,
                    tun_removed: report.tun_removed,
                    dns_restored: report.dns_restored,
                    openvpn_stopped: report.openvpn_stopped,
                    routes_removed: report.routes_removed,
                    warnings: report.warnings,
                })
            }
            _ => Err(CoreError::Platform("unexpected cleanup reply".into())),
        }
    }
}

fn platform_error(error: &std::io::Error) -> CoreError {
    CoreError::Platform(error.to_string())
}

fn readiness_error(error: MihomoError) -> CoreError {
    match error {
        MihomoError::Cancelled => CoreError::Cancelled,
        MihomoError::ReadinessTimeout(_) => CoreError::ControllerTimeout,
        other => CoreError::MihomoStartFailed(other.to_string()),
    }
}

fn copy_rule_file(
    bundled: &Path,
    cache: &Path,
    staging: &Path,
    name: &str,
) -> Result<(), CoreError> {
    let source = iran_split_rules::resolve_provider_path(cache, bundled, name);
    let metadata = fs::symlink_metadata(&source).map_err(|error| platform_error(&error))?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(CoreError::ConfigInvalid(format!(
            "bootstrap rule {name} is not a regular file"
        )));
    }
    let bytes = fs::read(source).map_err(|error| platform_error(&error))?;
    write_atomic(&staging.join(name), &bytes)
}

fn write_custom_provider_files(
    staging: &Path,
    document: &DirectRulesDocument,
) -> Result<(), CoreError> {
    let (domains, ips) = split_targets(&document.rules);
    write_lines(&staging.join("custom-direct-domains.txt"), &domains)?;
    write_lines(&staging.join("custom-direct-ips.txt"), &ips)?;
    // The VPN providers are always written, empty included: Mihomo fails to
    // load a rule-set whose file is missing, and an empty one is treated as
    // ready by OPTIONAL_RULE_PROVIDERS.
    let (vpn_domains, vpn_ips) = split_targets(&document.vpn_rules);
    write_lines(&staging.join("custom-vpn-domains.txt"), &vpn_domains)?;
    write_lines(&staging.join("custom-vpn-ips.txt"), &vpn_ips)?;
    let (side_domains, side_ips) = split_targets(&document.openvpn_rules);
    write_lines(&staging.join("custom-openvpn-domains.txt"), &side_domains)?;
    write_lines(&staging.join("custom-openvpn-ips.txt"), &side_ips)?;
    Ok(())
}

fn split_targets(rules: &[iran_split_rules::DirectRule]) -> (Vec<String>, Vec<String>) {
    let mut domains = Vec::new();
    let mut ips = Vec::new();
    for rule in rules {
        match &rule.target {
            DirectTarget::Domain(domain) => domains.push(format!("+.{domain}")),
            DirectTarget::Ip(address) => ips.push(host_cidr(*address)),
        }
    }
    domains.sort();
    domains.dedup();
    ips.sort();
    ips.dedup();
    (domains, ips)
}

fn host_cidr(address: IpAddr) -> String {
    match address {
        IpAddr::V4(address) => format!("{address}/32"),
        IpAddr::V6(address) => format!("{address}/128"),
    }
}

fn write_lines(path: &Path, lines: &[String]) -> Result<(), CoreError> {
    let mut content = lines.join("\n");
    if !content.is_empty() {
        content.push('\n');
    }
    write_atomic(path, content.as_bytes())
}

fn write_atomic(path: &Path, content: &[u8]) -> Result<(), CoreError> {
    let parent = path
        .parent()
        .ok_or_else(|| CoreError::ConfigInvalid("runtime file has no parent".into()))?;
    fs::create_dir_all(parent).map_err(|error| platform_error(&error))?;
    let mut temporary = NamedTempFile::new_in(parent).map_err(|error| platform_error(&error))?;
    temporary
        .write_all(content)
        .map_err(|error| platform_error(&error))?;
    temporary
        .as_file()
        .sync_all()
        .map_err(|error| platform_error(&error))?;
    temporary
        .persist(path)
        .map_err(|error| platform_error(&error.error))?;
    Ok(())
}

#[allow(dead_code)]
fn hash_bytes(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn preparation_publishes_only_allowlisted_generation_files() {
        let directory = tempfile::tempdir().expect("tempdir");
        let resources = directory.path().join("resources");
        fs::create_dir_all(&resources).expect("resources");
        for name in [
            "private.txt",
            "iran-domains.txt",
            "iran-networks.txt",
            "iran-business-domains.txt",
        ] {
            fs::write(resources.join(name), "example\n").expect("fixture");
        }
        let paths = LinuxPaths {
            socket_path: directory.path().join("helper.sock"),
            user_data_dir: directory.path().join("user-data"),
            system_runtime_dir: PathBuf::from("/var/lib/iran-split"),
            resources_dir: resources,
            rules_cache_dir: directory.path().join("rules-cache"),
            mihomo_binary: directory.path().join("mihomo"),
        };
        let backend = LinuxBackend::new(AppConfig::default(), paths.clone());
        let generation = backend.prepare_runtime().await.expect("prepare");
        let root = paths
            .user_data_dir
            .join("runtime")
            .join("generations")
            .join(generation.generation_id.to_string());
        let names = fs::read_dir(&root)
            .expect("generation")
            .map(|entry| entry.expect("entry").file_name())
            .collect::<std::collections::HashSet<_>>();
        // config.yaml, four bundled providers, two custom-direct, two
        // custom-vpn, two custom-openvpn.
        assert_eq!(names.len(), 11);
        assert!(names.contains(std::ffi::OsStr::new("iran-business-domains.txt")));
        assert!(names.contains(std::ffi::OsStr::new("custom-vpn-domains.txt")));
        assert!(names.contains(std::ffi::OsStr::new("custom-vpn-ips.txt")));
        // Mihomo refuses to load a rule-set whose file is missing, so the
        // side-tunnel providers ship even when no tunnel is configured.
        assert!(names.contains(std::ffi::OsStr::new("custom-openvpn-domains.txt")));
        assert!(names.contains(std::ffi::OsStr::new("custom-openvpn-ips.txt")));
        assert!(names.contains(std::ffi::OsStr::new("config.yaml")));
        let config = fs::read_to_string(root.join("config.yaml")).expect("config");
        assert!(config.contains("path: private.txt"));
        assert!(!config.contains("/var/lib/iran-split/generations/"));
    }

    #[test]
    fn discovers_installed_hiddify_appimage() {
        let directory = tempfile::tempdir().expect("tempdir");
        let apps = directory.path().join("apps");
        fs::create_dir_all(&apps).expect("apps");
        let appimage = apps.join("Hiddify.AppImage");
        fs::write(&appimage, b"elf").expect("appimage");
        let found = LinuxBackend::discover_hiddify(&AppConfig::default(), directory.path());
        assert_eq!(found, Some(appimage));
    }
}
