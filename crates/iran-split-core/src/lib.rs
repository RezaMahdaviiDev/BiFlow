use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc, time::Duration};
use thiserror::Error;
use tokio::sync::{mpsc, watch, Mutex};
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum StackPhase {
    #[default]
    Uninitialized,
    Stopped,
    StartingHiddify,
    PreparingRuntime,
    ValidatingConfig,
    StartingCore,
    CheckingReadiness,
    Running,
    Degraded,
    Stopping,
    Recovering,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ComponentPhase {
    #[default]
    Unknown,
    Checking,
    Stopped,
    Starting,
    Running,
    Degraded,
    Unavailable,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComponentStatus {
    pub phase: ComponentPhase,
    pub message: Option<String>,
    pub since: DateTime<Utc>,
}

impl ComponentStatus {
    #[must_use]
    pub fn new(phase: ComponentPhase, message: Option<String>) -> Self {
        Self {
            phase,
            message,
            since: Utc::now(),
        }
    }
}

impl Default for ComponentStatus {
    fn default() -> Self {
        Self::new(
            ComponentPhase::Checking,
            Some("Inspecting current status".into()),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ProviderSummary {
    pub ready: u32,
    pub total: u32,
    pub rules_loaded: u64,
    pub last_refresh: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BackendKind {
    #[default]
    ExternalHiddify,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    HiddifyNotFound,
    HiddifyPortBusy,
    HiddifyEgressUnavailable,
    ConfigInvalid,
    HelperUnavailable,
    HelperUnauthorized,
    MihomoNotFound,
    MihomoStartFailed,
    ControllerTimeout,
    ProviderNotReady,
    TunCleanupFailed,
    RouteTestFailed,
    UpdateSignatureInvalid,
    OperationInProgress,
    OperationCancelled,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", content = "value", rename_all = "snake_case")]
pub enum Remediation {
    Retry,
    OpenSettings,
    InstallHelper,
    ChooseHiddifyExecutable,
    InstallDependency,
    RunDiagnostics,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppError {
    pub code: ErrorCode,
    pub message_key: String,
    pub retryable: bool,
    pub remediation: Option<Remediation>,
    pub technical_details: Option<String>,
    pub correlation_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StackSnapshot {
    pub revision: u64,
    pub phase: StackPhase,
    pub operation_id: Option<Uuid>,
    pub helper: ComponentStatus,
    pub hiddify: ComponentStatus,
    pub mihomo: ComponentStatus,
    pub tun: ComponentStatus,
    pub dns: ComponentStatus,
    pub providers: ProviderSummary,
    pub exit_ip: Option<String>,
    pub backend: BackendKind,
    pub last_error: Option<AppError>,
    pub updated_at: DateTime<Utc>,
}

impl Default for StackSnapshot {
    fn default() -> Self {
        Self {
            revision: 0,
            phase: StackPhase::Uninitialized,
            operation_id: None,
            helper: ComponentStatus::default(),
            hiddify: ComponentStatus::default(),
            mihomo: ComponentStatus::default(),
            tun: ComponentStatus::default(),
            dns: ComponentStatus::default(),
            providers: ProviderSummary::default(),
            exit_ip: None,
            backend: BackendKind::default(),
            last_error: None,
            updated_at: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationAccepted {
    pub operation_id: Uuid,
    pub already_complete: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeGeneration {
    pub generation_id: Uuid,
    pub config_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HelperStatus {
    pub available: bool,
    pub authorized: bool,
    pub version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProcessStatus {
    pub running: bool,
    pub pid: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TunStatus {
    pub active: bool,
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CleanupReport {
    pub process_stopped: bool,
    pub tun_removed: bool,
    pub dns_restored: bool,
    pub routes_removed: u32,
    pub warnings: Vec<String>,
}

impl CleanupReport {
    #[must_use]
    pub fn clean(&self) -> bool {
        self.process_stopped && self.tun_removed && self.dns_restored && self.warnings.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ReadinessReport {
    pub controller_ready: bool,
    pub egress_ready: bool,
    pub providers: ProviderSummary,
    pub exit_ip: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RuntimeHealth {
    pub helper: ComponentStatus,
    pub hiddify: ComponentStatus,
    pub mihomo: ComponentStatus,
    pub tun: ComponentStatus,
    pub dns: ComponentStatus,
    pub providers: ProviderSummary,
}

impl ReadinessReport {
    #[must_use]
    pub fn ready(&self) -> bool {
        self.controller_ready
            && self.egress_ready
            && self.providers.total > 0
            && self.providers.ready == self.providers.total
    }
}

#[derive(Debug, Error, Clone)]
pub enum CoreError {
    #[error("helper is unavailable")]
    HelperUnavailable,
    #[error("helper rejected the current user")]
    HelperUnauthorized,
    #[error("Hiddify executable was not found")]
    HiddifyNotFound,
    #[error("Hiddify egress did not become ready")]
    HiddifyEgressUnavailable,
    #[error("runtime configuration is invalid: {0}")]
    ConfigInvalid(String),
    #[error("Mihomo executable was not found")]
    MihomoNotFound,
    #[error("Mihomo failed to start: {0}")]
    MihomoStartFailed(String),
    #[error("Mihomo controller readiness timed out")]
    ControllerTimeout,
    #[error("rule providers are not ready")]
    ProviderNotReady,
    #[error("owned TUN or route state could not be cleaned: {0}")]
    TunCleanupFailed(String),
    #[error("operation was cancelled")]
    Cancelled,
    #[error("operation queue is unavailable")]
    QueueUnavailable,
    #[error("platform operation failed: {0}")]
    Platform(String),
}

impl CoreError {
    #[must_use]
    pub fn to_app_error(&self, correlation_id: Uuid) -> AppError {
        let (code, key, retryable, remediation) = match self {
            Self::HelperUnavailable => (
                ErrorCode::HelperUnavailable,
                "errors.helperUnavailable",
                true,
                Some(Remediation::InstallHelper),
            ),
            Self::HelperUnauthorized => (
                ErrorCode::HelperUnauthorized,
                "errors.helperUnauthorized",
                false,
                Some(Remediation::InstallHelper),
            ),
            Self::HiddifyNotFound => (
                ErrorCode::HiddifyNotFound,
                "errors.hiddifyNotFound",
                false,
                Some(Remediation::InstallDependency),
            ),
            Self::HiddifyEgressUnavailable => (
                ErrorCode::HiddifyEgressUnavailable,
                "errors.hiddifyEgressUnavailable",
                true,
                Some(Remediation::RunDiagnostics),
            ),
            Self::ConfigInvalid(_) => (
                ErrorCode::ConfigInvalid,
                "errors.configInvalid",
                false,
                Some(Remediation::OpenSettings),
            ),
            Self::MihomoNotFound => (
                ErrorCode::MihomoNotFound,
                "errors.mihomoNotFound",
                false,
                Some(Remediation::InstallDependency),
            ),
            Self::MihomoStartFailed(_) => (
                ErrorCode::MihomoStartFailed,
                "errors.mihomoStartFailed",
                true,
                Some(Remediation::RunDiagnostics),
            ),
            Self::ControllerTimeout => (
                ErrorCode::ControllerTimeout,
                "errors.controllerTimeout",
                true,
                Some(Remediation::Retry),
            ),
            Self::ProviderNotReady => (
                ErrorCode::ProviderNotReady,
                "errors.providerNotReady",
                true,
                Some(Remediation::Retry),
            ),
            Self::TunCleanupFailed(_) => (
                ErrorCode::TunCleanupFailed,
                "errors.tunCleanupFailed",
                true,
                Some(Remediation::RunDiagnostics),
            ),
            Self::Cancelled => (
                ErrorCode::OperationCancelled,
                "errors.operationCancelled",
                true,
                Some(Remediation::Retry),
            ),
            Self::QueueUnavailable | Self::Platform(_) => (
                ErrorCode::Internal,
                "errors.internal",
                true,
                Some(Remediation::RunDiagnostics),
            ),
        };
        AppError {
            code,
            message_key: key.into(),
            retryable,
            remediation,
            technical_details: Some(self.to_string()),
            correlation_id,
        }
    }
}

#[async_trait]
pub trait PlatformBackend: Send + Sync + 'static {
    async fn runtime_health(&self) -> RuntimeHealth;
    async fn helper_status(&self) -> Result<HelperStatus, CoreError>;
    async fn ensure_hiddify(&self, cancel: CancellationToken) -> Result<(), CoreError>;
    async fn prepare_runtime(&self) -> Result<RuntimeGeneration, CoreError>;
    async fn validate_runtime(&self, generation: &RuntimeGeneration) -> Result<(), CoreError>;
    async fn start_core(&self, generation: &RuntimeGeneration) -> Result<(), CoreError>;
    async fn stop_core(&self) -> Result<(), CoreError>;
    async fn core_process(&self) -> Result<ProcessStatus, CoreError>;
    async fn tun_status(&self) -> Result<TunStatus, CoreError>;
    async fn check_readiness(
        &self,
        cancel: CancellationToken,
    ) -> Result<ReadinessReport, CoreError>;
    async fn cleanup_owned_state(&self) -> Result<CleanupReport, CoreError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum OperationKind {
    Reconcile,
    Start,
    Stop,
}

#[derive(Debug, Clone)]
struct OperationRecord {
    kind: OperationKind,
    cancel: CancellationToken,
}

#[derive(Debug)]
struct WorkItem {
    id: Uuid,
    kind: OperationKind,
    cancel: CancellationToken,
}

pub struct Engine<B: PlatformBackend> {
    backend: Arc<B>,
    snapshots: watch::Sender<StackSnapshot>,
    queue: mpsc::Sender<WorkItem>,
    operations: Mutex<HashMap<Uuid, OperationRecord>>,
    pending: Mutex<HashMap<OperationKind, Uuid>>,
}

impl<B: PlatformBackend> std::fmt::Debug for Engine<B> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Engine")
            .field("snapshot", &*self.snapshots.borrow())
            .finish_non_exhaustive()
    }
}

impl<B: PlatformBackend> Engine<B> {
    /// Creates an engine and schedules its worker on `runtime`.
    ///
    /// Passing the runtime explicitly allows GUI setup code to construct the
    /// engine outside an entered Tokio context without panicking.
    #[must_use]
    pub fn new(backend: Arc<B>, runtime: &tokio::runtime::Handle) -> Arc<Self> {
        let (queue, receiver) = mpsc::channel(16);
        let (snapshots, _) = watch::channel(StackSnapshot::default());
        let engine = Arc::new(Self {
            backend,
            snapshots,
            queue,
            operations: Mutex::new(HashMap::new()),
            pending: Mutex::new(HashMap::new()),
        });
        runtime.spawn(Self::worker(Arc::clone(&engine), receiver));
        engine
    }

    #[must_use]
    pub fn snapshot(&self) -> StackSnapshot {
        self.snapshots.borrow().clone()
    }

    pub fn subscribe(&self) -> watch::Receiver<StackSnapshot> {
        self.snapshots.subscribe()
    }

    pub async fn refresh_health(&self) {
        let health = self.backend.runtime_health().await;
        self.update(|snapshot| apply_health(snapshot, health));
    }

    /// Queues startup reconciliation of helper and network state.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::QueueUnavailable`] when the operation worker is no
    /// longer available.
    pub async fn reconcile_startup(&self) -> Result<OperationAccepted, CoreError> {
        self.accept(OperationKind::Reconcile).await
    }

    /// Queues a stack start, or reports success when already running.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::QueueUnavailable`] when the operation worker is no
    /// longer available.
    pub async fn start_stack(&self) -> Result<OperationAccepted, CoreError> {
        if self.snapshot().phase == StackPhase::Running {
            return Ok(OperationAccepted {
                operation_id: Uuid::new_v4(),
                already_complete: true,
            });
        }
        self.accept(OperationKind::Start).await
    }

    /// Cancels an in-progress start and queues a stack stop.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::QueueUnavailable`] when the operation worker is no
    /// longer available.
    pub async fn stop_stack(&self) -> Result<OperationAccepted, CoreError> {
        if self.snapshot().phase == StackPhase::Stopped && self.operations.lock().await.is_empty() {
            return Ok(OperationAccepted {
                operation_id: Uuid::new_v4(),
                already_complete: true,
            });
        }
        {
            let operations = self.operations.lock().await;
            for operation in operations.values() {
                if operation.kind == OperationKind::Start {
                    operation.cancel.cancel();
                }
            }
        }
        self.accept(OperationKind::Stop).await
    }

    /// Queues a stop followed by a start.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::QueueUnavailable`] when either operation cannot be
    /// queued.
    pub async fn restart_stack(&self) -> Result<(OperationAccepted, OperationAccepted), CoreError> {
        let stop = self.stop_stack().await?;
        let start = self.accept(OperationKind::Start).await?;
        Ok((stop, start))
    }

    pub async fn cancel_operation(&self, operation_id: Uuid) -> bool {
        let operations = self.operations.lock().await;
        if let Some(operation) = operations.get(&operation_id) {
            operation.cancel.cancel();
            true
        } else {
            false
        }
    }

    async fn accept(&self, kind: OperationKind) -> Result<OperationAccepted, CoreError> {
        let mut pending = self.pending.lock().await;
        if let Some(operation_id) = pending.get(&kind) {
            return Ok(OperationAccepted {
                operation_id: *operation_id,
                already_complete: false,
            });
        }
        let operation_id = Uuid::new_v4();
        let cancel = CancellationToken::new();
        self.operations.lock().await.insert(
            operation_id,
            OperationRecord {
                kind,
                cancel: cancel.clone(),
            },
        );
        pending.insert(kind, operation_id);
        if self
            .queue
            .send(WorkItem {
                id: operation_id,
                kind,
                cancel,
            })
            .await
            .is_err()
        {
            pending.remove(&kind);
            self.operations.lock().await.remove(&operation_id);
            return Err(CoreError::QueueUnavailable);
        }
        Ok(OperationAccepted {
            operation_id,
            already_complete: false,
        })
    }

    async fn worker(engine: Arc<Self>, mut receiver: mpsc::Receiver<WorkItem>) {
        while let Some(item) = receiver.recv().await {
            info!(operation_id = %item.id, kind = ?item.kind, "operation started");
            let result = match item.kind {
                OperationKind::Reconcile => engine.run_reconcile(item.id, &item.cancel).await,
                OperationKind::Start => engine.run_start(item.id, &item.cancel).await,
                OperationKind::Stop => engine.run_stop(item.id).await,
            };
            if let Err(error) = result {
                if !matches!(error, CoreError::Cancelled) {
                    error!(operation_id = %item.id, %error, "operation failed");
                    let health = engine.backend.runtime_health().await;
                    engine.update(|snapshot| {
                        apply_health(snapshot, health);
                        snapshot.phase = StackPhase::Error;
                        snapshot.last_error = Some(error.to_app_error(item.id));
                    });
                }
            }
            engine.update(|snapshot| snapshot.operation_id = None);
            engine.operations.lock().await.remove(&item.id);
            engine.pending.lock().await.remove(&item.kind);
            info!(operation_id = %item.id, kind = ?item.kind, "operation finished");
        }
    }

    async fn run_reconcile(
        &self,
        operation_id: Uuid,
        cancel: &CancellationToken,
    ) -> Result<(), CoreError> {
        self.transition(StackPhase::Recovering, operation_id);
        check_cancelled(cancel)?;
        let health = self.backend.runtime_health().await;
        let helper_ready = health.helper.phase == ComponentPhase::Running;
        self.update(|snapshot| apply_health(snapshot, health));
        if helper_ready {
            let process = self.backend.core_process().await.unwrap_or_default();
            let tun = self.backend.tun_status().await.unwrap_or_default();
            if process.running || tun.active {
                warn!("found owned runtime state during startup reconciliation");
                let report = self.backend.cleanup_owned_state().await?;
                if !report.clean() {
                    return Err(CoreError::TunCleanupFailed(report.warnings.join("; ")));
                }
            }
        }
        let health = self.backend.runtime_health().await;
        self.set_stopped(health);
        Ok(())
    }

    async fn run_start(
        &self,
        operation_id: Uuid,
        cancel: &CancellationToken,
    ) -> Result<(), CoreError> {
        if self.snapshot().phase == StackPhase::Running {
            return Ok(());
        }
        let mut core_started = false;
        let result = self
            .start_steps(operation_id, cancel, &mut core_started)
            .await;
        if let Err(error) = result {
            if core_started || matches!(error, CoreError::Cancelled) {
                if let Err(rollback_error) = self.rollback(operation_id).await {
                    return Err(CoreError::TunCleanupFailed(format!(
                        "start failed ({error}); rollback failed ({rollback_error})"
                    )));
                }
            }
            if matches!(error, CoreError::Cancelled) {
                let health = self.backend.runtime_health().await;
                self.set_stopped(health);
            }
            return Err(error);
        }
        Ok(())
    }

    async fn start_steps(
        &self,
        operation_id: Uuid,
        cancel: &CancellationToken,
        core_started: &mut bool,
    ) -> Result<(), CoreError> {
        let helper = self.backend.helper_status().await?;
        if !helper.available {
            return Err(CoreError::HelperUnavailable);
        }
        if !helper.authorized {
            return Err(CoreError::HelperUnauthorized);
        }
        self.update(|snapshot| {
            snapshot.helper = ComponentStatus::new(
                ComponentPhase::Running,
                helper.version.map(|version| format!("Helper {version}")),
            );
        });

        self.transition(StackPhase::StartingHiddify, operation_id);
        self.update(|snapshot| {
            snapshot.hiddify = ComponentStatus::new(ComponentPhase::Starting, None);
        });
        self.backend.ensure_hiddify(cancel.clone()).await?;
        check_cancelled(cancel)?;
        self.update(|snapshot| {
            snapshot.hiddify = ComponentStatus::new(ComponentPhase::Running, None);
        });

        self.transition(StackPhase::PreparingRuntime, operation_id);
        let generation = self.backend.prepare_runtime().await?;
        check_cancelled(cancel)?;

        self.transition(StackPhase::ValidatingConfig, operation_id);
        self.backend.validate_runtime(&generation).await?;
        check_cancelled(cancel)?;

        self.transition(StackPhase::StartingCore, operation_id);
        self.update(|snapshot| {
            snapshot.mihomo = ComponentStatus::new(ComponentPhase::Starting, None);
            snapshot.tun = ComponentStatus::new(ComponentPhase::Starting, None);
            snapshot.dns = ComponentStatus::new(ComponentPhase::Starting, None);
        });
        self.backend.start_core(&generation).await?;
        *core_started = true;
        check_cancelled(cancel)?;

        self.transition(StackPhase::CheckingReadiness, operation_id);
        let readiness = self.backend.check_readiness(cancel.clone()).await?;
        if !readiness.controller_ready {
            return Err(CoreError::ControllerTimeout);
        }
        if !readiness.egress_ready {
            return Err(CoreError::HiddifyEgressUnavailable);
        }
        if readiness.providers.total == 0 || readiness.providers.ready != readiness.providers.total
        {
            return Err(CoreError::ProviderNotReady);
        }
        let process = self.backend.core_process().await?;
        let tun = self.backend.tun_status().await?;
        if !process.running || !tun.active {
            return Err(CoreError::MihomoStartFailed(
                "process or TUN disappeared during readiness checks".into(),
            ));
        }
        self.update(|snapshot| {
            snapshot.phase = StackPhase::Running;
            snapshot.operation_id = Some(operation_id);
            snapshot.mihomo = ComponentStatus::new(ComponentPhase::Running, None);
            snapshot.tun = ComponentStatus::new(ComponentPhase::Running, tun.name);
            snapshot.dns = ComponentStatus::new(ComponentPhase::Running, None);
            snapshot.providers = readiness.providers;
            snapshot.exit_ip = readiness.exit_ip;
            snapshot.last_error = None;
        });
        Ok(())
    }

    async fn rollback(&self, operation_id: Uuid) -> Result<(), CoreError> {
        self.transition(StackPhase::Recovering, operation_id);
        let _ = self.backend.stop_core().await;
        let report = self.backend.cleanup_owned_state().await?;
        let tun = self.backend.tun_status().await?;
        if tun.active || !report.clean() {
            return Err(CoreError::TunCleanupFailed(format!(
                "cleanup warnings: {}",
                report.warnings.join("; ")
            )));
        }
        Ok(())
    }

    async fn run_stop(&self, operation_id: Uuid) -> Result<(), CoreError> {
        self.transition(StackPhase::Stopping, operation_id);
        self.backend.stop_core().await?;
        let report = self.backend.cleanup_owned_state().await?;
        let tun = self.backend.tun_status().await?;
        if tun.active || !report.clean() {
            return Err(CoreError::TunCleanupFailed(format!(
                "TUN active: {}; warnings: {}",
                tun.active,
                report.warnings.join("; ")
            )));
        }
        let health = self.backend.runtime_health().await;
        self.set_stopped(health);
        Ok(())
    }

    fn transition(&self, phase: StackPhase, operation_id: Uuid) {
        self.update(|snapshot| {
            snapshot.phase = phase;
            snapshot.operation_id = Some(operation_id);
        });
    }

    fn set_stopped(&self, health: RuntimeHealth) {
        self.update(|snapshot| {
            apply_health(snapshot, health);
            snapshot.phase = StackPhase::Stopped;
            snapshot.operation_id = None;
            snapshot.exit_ip = None;
            snapshot.last_error = None;
        });
    }

    fn update(&self, update: impl FnOnce(&mut StackSnapshot)) {
        let mut snapshot = self.snapshots.borrow().clone();
        update(&mut snapshot);
        snapshot.revision = snapshot.revision.saturating_add(1);
        snapshot.updated_at = Utc::now();
        self.snapshots.send_replace(snapshot);
    }

    /// Waits until the stack reaches `desired`, reports an error phase, or times out.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::Platform`] if the stack enters its error phase,
    /// [`CoreError::QueueUnavailable`] if snapshot delivery closes, or
    /// [`CoreError::ControllerTimeout`] when `timeout` elapses.
    pub async fn wait_for_phase(
        &self,
        desired: StackPhase,
        timeout: Duration,
    ) -> Result<StackSnapshot, CoreError> {
        let mut receiver = self.subscribe();
        tokio::time::timeout(timeout, async {
            loop {
                let snapshot = receiver.borrow().clone();
                if snapshot.phase == desired {
                    return Ok(snapshot);
                }
                if snapshot.phase == StackPhase::Error {
                    return Err(CoreError::Platform(snapshot.last_error.map_or_else(
                        || "unknown error".into(),
                        |error| error.technical_details.unwrap_or(error.message_key),
                    )));
                }
                receiver
                    .changed()
                    .await
                    .map_err(|_| CoreError::QueueUnavailable)?;
            }
        })
        .await
        .map_err(|_| CoreError::ControllerTimeout)?
    }
}

fn apply_health(snapshot: &mut StackSnapshot, health: RuntimeHealth) {
    snapshot.helper = health.helper;
    snapshot.hiddify = health.hiddify;
    snapshot.mihomo = health.mihomo;
    snapshot.tun = health.tun;
    snapshot.dns = health.dns;
    snapshot.providers = health.providers;
}

fn check_cancelled(cancel: &CancellationToken) -> Result<(), CoreError> {
    if cancel.is_cancelled() {
        Err(CoreError::Cancelled)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    #[derive(Debug, Default)]
    struct FakeBackend {
        process: AtomicBool,
        tun: AtomicBool,
        fail_readiness: AtomicBool,
        slow_hiddify: AtomicBool,
        hiddify_missing: AtomicBool,
        helper_missing: AtomicBool,
        starts: AtomicUsize,
        cleanups: AtomicUsize,
    }

    #[async_trait]
    impl PlatformBackend for FakeBackend {
        async fn runtime_health(&self) -> RuntimeHealth {
            let helper = if self.helper_missing.load(Ordering::SeqCst) {
                ComponentStatus::new(ComponentPhase::Unavailable, Some("Helper missing".into()))
            } else {
                ComponentStatus::new(ComponentPhase::Running, Some("Helper test".into()))
            };
            let hiddify = if self.hiddify_missing.load(Ordering::SeqCst) {
                ComponentStatus::new(ComponentPhase::Unavailable, Some("Hiddify missing".into()))
            } else {
                ComponentStatus::new(ComponentPhase::Running, Some("Hiddify ready".into()))
            };
            let running = self.process.load(Ordering::SeqCst);
            let tun = self.tun.load(Ordering::SeqCst);
            RuntimeHealth {
                helper,
                hiddify,
                mihomo: ComponentStatus::new(
                    if running {
                        ComponentPhase::Running
                    } else {
                        ComponentPhase::Stopped
                    },
                    None,
                ),
                tun: ComponentStatus::new(
                    if tun {
                        ComponentPhase::Running
                    } else {
                        ComponentPhase::Stopped
                    },
                    Some("test-tun".into()),
                ),
                dns: ComponentStatus::new(
                    if running {
                        ComponentPhase::Running
                    } else {
                        ComponentPhase::Stopped
                    },
                    None,
                ),
                providers: if running {
                    ProviderSummary {
                        ready: 1,
                        total: 1,
                        rules_loaded: 100,
                        last_refresh: Some(Utc::now()),
                    }
                } else {
                    ProviderSummary::default()
                },
            }
        }

        async fn helper_status(&self) -> Result<HelperStatus, CoreError> {
            if self.helper_missing.load(Ordering::SeqCst) {
                return Ok(HelperStatus::default());
            }
            Ok(HelperStatus {
                available: true,
                authorized: true,
                version: Some("test".into()),
            })
        }

        async fn ensure_hiddify(&self, cancel: CancellationToken) -> Result<(), CoreError> {
            if self.hiddify_missing.load(Ordering::SeqCst) {
                return Err(CoreError::HiddifyNotFound);
            }
            if self.slow_hiddify.load(Ordering::SeqCst) {
                tokio::select! {
                    () = cancel.cancelled() => return Err(CoreError::Cancelled),
                    () = tokio::time::sleep(Duration::from_secs(2)) => {}
                }
            }
            Ok(())
        }

        async fn prepare_runtime(&self) -> Result<RuntimeGeneration, CoreError> {
            Ok(RuntimeGeneration {
                generation_id: Uuid::new_v4(),
                config_sha256: "a".repeat(64),
            })
        }

        async fn validate_runtime(&self, _generation: &RuntimeGeneration) -> Result<(), CoreError> {
            Ok(())
        }

        async fn start_core(&self, _generation: &RuntimeGeneration) -> Result<(), CoreError> {
            self.starts.fetch_add(1, Ordering::SeqCst);
            self.process.store(true, Ordering::SeqCst);
            self.tun.store(true, Ordering::SeqCst);
            Ok(())
        }

        async fn stop_core(&self) -> Result<(), CoreError> {
            self.process.store(false, Ordering::SeqCst);
            Ok(())
        }

        async fn core_process(&self) -> Result<ProcessStatus, CoreError> {
            Ok(ProcessStatus {
                running: self.process.load(Ordering::SeqCst),
                pid: Some(42),
            })
        }

        async fn tun_status(&self) -> Result<TunStatus, CoreError> {
            Ok(TunStatus {
                active: self.tun.load(Ordering::SeqCst),
                name: Some("test-tun".into()),
            })
        }

        async fn check_readiness(
            &self,
            _cancel: CancellationToken,
        ) -> Result<ReadinessReport, CoreError> {
            let fail = self.fail_readiness.load(Ordering::SeqCst);
            Ok(ReadinessReport {
                controller_ready: !fail,
                egress_ready: !fail,
                providers: ProviderSummary {
                    ready: u32::from(!fail),
                    total: 1,
                    rules_loaded: 100,
                    last_refresh: Some(Utc::now()),
                },
                exit_ip: Some("203.0.113.10".into()),
            })
        }

        async fn cleanup_owned_state(&self) -> Result<CleanupReport, CoreError> {
            self.cleanups.fetch_add(1, Ordering::SeqCst);
            self.process.store(false, Ordering::SeqCst);
            self.tun.store(false, Ordering::SeqCst);
            Ok(CleanupReport {
                process_stopped: true,
                tun_removed: true,
                dns_restored: true,
                routes_removed: 2,
                warnings: vec![],
            })
        }
    }

    #[tokio::test]
    async fn start_and_stop_are_idempotent() {
        let backend = Arc::new(FakeBackend::default());
        let engine = Engine::new(Arc::clone(&backend), &tokio::runtime::Handle::current());
        let first = engine.start_stack().await.expect("start accepted");
        let duplicate = engine.start_stack().await.expect("duplicate accepted");
        assert_eq!(first.operation_id, duplicate.operation_id);
        engine
            .wait_for_phase(StackPhase::Running, Duration::from_secs(2))
            .await
            .expect("running");
        assert_eq!(backend.starts.load(Ordering::SeqCst), 1);
        assert!(
            engine
                .start_stack()
                .await
                .expect("idempotent")
                .already_complete
        );

        engine.stop_stack().await.expect("stop accepted");
        engine
            .wait_for_phase(StackPhase::Stopped, Duration::from_secs(2))
            .await
            .expect("stopped");
        assert!(!backend.tun.load(Ordering::SeqCst));
        assert!(
            engine
                .stop_stack()
                .await
                .expect("idempotent")
                .already_complete
        );
    }

    #[tokio::test]
    async fn failed_readiness_rolls_back_owned_state() {
        let backend = Arc::new(FakeBackend::default());
        backend.fail_readiness.store(true, Ordering::SeqCst);
        let engine = Engine::new(Arc::clone(&backend), &tokio::runtime::Handle::current());
        engine.start_stack().await.expect("start accepted");
        let mut receiver = engine.subscribe();
        tokio::time::timeout(Duration::from_secs(2), async {
            while receiver.borrow().phase != StackPhase::Error {
                receiver.changed().await.expect("snapshot update");
            }
        })
        .await
        .expect("error phase");
        assert!(!backend.tun.load(Ordering::SeqCst));
        assert_eq!(backend.cleanups.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn stop_cancels_an_in_progress_start_then_cleans() {
        let backend = Arc::new(FakeBackend::default());
        backend.slow_hiddify.store(true, Ordering::SeqCst);
        let engine = Engine::new(Arc::clone(&backend), &tokio::runtime::Handle::current());
        engine.start_stack().await.expect("start accepted");
        tokio::time::sleep(Duration::from_millis(20)).await;
        engine.stop_stack().await.expect("stop accepted");
        engine
            .wait_for_phase(StackPhase::Stopped, Duration::from_secs(2))
            .await
            .expect("stopped");
        assert!(!backend.tun.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn startup_reconciliation_removes_orphans() {
        let backend = Arc::new(FakeBackend::default());
        backend.process.store(true, Ordering::SeqCst);
        backend.tun.store(true, Ordering::SeqCst);
        let engine = Engine::new(Arc::clone(&backend), &tokio::runtime::Handle::current());
        engine
            .reconcile_startup()
            .await
            .expect("reconcile accepted");
        engine
            .wait_for_phase(StackPhase::Stopped, Duration::from_secs(2))
            .await
            .expect("stopped");
        assert_eq!(backend.cleanups.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn startup_without_helper_reports_exact_component_states() {
        let backend = Arc::new(FakeBackend::default());
        backend.helper_missing.store(true, Ordering::SeqCst);
        let engine = Engine::new(Arc::clone(&backend), &tokio::runtime::Handle::current());

        engine
            .reconcile_startup()
            .await
            .expect("reconcile accepted");
        let snapshot = engine
            .wait_for_phase(StackPhase::Stopped, Duration::from_secs(2))
            .await
            .expect("stopped");

        assert_eq!(snapshot.helper.phase, ComponentPhase::Unavailable);
        assert_eq!(snapshot.hiddify.phase, ComponentPhase::Running);
        assert_eq!(snapshot.mihomo.phase, ComponentPhase::Stopped);
        assert_eq!(snapshot.tun.phase, ComponentPhase::Stopped);
        assert_eq!(snapshot.dns.phase, ComponentPhase::Stopped);
        assert!(snapshot.last_error.is_none());
    }

    #[test]
    fn snapshot_contract_serializes_in_snake_case() {
        let snapshot = StackSnapshot {
            phase: StackPhase::CheckingReadiness,
            ..StackSnapshot::default()
        };
        let value = serde_json::to_value(snapshot).expect("serialize");
        assert_eq!(value["phase"], "checking_readiness");
    }

    #[test]
    fn engine_can_be_constructed_outside_an_entered_runtime() {
        let runtime = tokio::runtime::Runtime::new().expect("test runtime");
        let engine = Engine::new(Arc::new(FakeBackend::default()), runtime.handle());

        assert_eq!(engine.snapshot().phase, StackPhase::Uninitialized);
    }

    #[test]
    fn missing_hiddify_and_mihomo_ask_the_ui_to_install() {
        let hiddify = CoreError::HiddifyNotFound.to_app_error(Uuid::nil());
        assert_eq!(hiddify.code, ErrorCode::HiddifyNotFound);
        assert_eq!(hiddify.remediation, Some(Remediation::InstallDependency));
        let mihomo = CoreError::MihomoNotFound.to_app_error(Uuid::nil());
        assert_eq!(mihomo.code, ErrorCode::MihomoNotFound);
        assert_eq!(mihomo.remediation, Some(Remediation::InstallDependency));
    }

    #[tokio::test]
    async fn missing_hiddify_marks_the_stack_error() {
        let backend = Arc::new(FakeBackend::default());
        backend.hiddify_missing.store(true, Ordering::SeqCst);
        let engine = Engine::new(Arc::clone(&backend), &tokio::runtime::Handle::current());
        engine.start_stack().await.expect("start accepted");
        let mut receiver = engine.subscribe();
        tokio::time::timeout(Duration::from_secs(2), async {
            while receiver.borrow().phase != StackPhase::Error {
                receiver.changed().await.expect("snapshot update");
            }
        })
        .await
        .expect("error phase");
        let error = engine.snapshot().last_error.expect("last error");
        assert_eq!(error.code, ErrorCode::HiddifyNotFound);
        assert_eq!(error.remediation, Some(Remediation::InstallDependency));
    }
}
