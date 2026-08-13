use async_trait::async_trait;
use iran_split_core::{
    CleanupReport, ComponentPhase, ComponentStatus, CoreError, HelperStatus, PlatformBackend,
    ProcessStatus, ProviderSummary, ReadinessReport, RuntimeGeneration, RuntimeHealth, TunStatus,
};
use tokio_util::sync::CancellationToken;

pub const HELPER_PIPE: &str = r"\\.\pipe\iran-split-helper-v1";

/// The Windows adapter deliberately exposes only the shared `PlatformBackend`
/// contract. Native service and named-pipe code is compiled on Windows; other
/// targets retain this typed unavailable implementation so workspace tests can
/// validate contracts without pretending Windows was exercised.
#[derive(Debug, Default)]
pub struct WindowsBackend;

fn unavailable<T>() -> Result<T, CoreError> {
    Err(CoreError::Platform(
        "Windows backend must run on a signed Windows build with the installed helper service"
            .into(),
    ))
}

#[async_trait]
impl PlatformBackend for WindowsBackend {
    async fn runtime_health(&self) -> RuntimeHealth {
        let helper = match self.helper_status().await {
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
            Ok(_) | Err(_) => ComponentStatus::new(
                ComponentPhase::Unavailable,
                Some("Windows helper service is not available".into()),
            ),
        };
        let unavailable = || {
            ComponentStatus::new(
                ComponentPhase::Unavailable,
                Some("Windows runtime status probe is not available".into()),
            )
        };
        RuntimeHealth {
            helper,
            hiddify: unavailable(),
            mihomo: unavailable(),
            tun: unavailable(),
            dns: unavailable(),
            providers: ProviderSummary::default(),
        }
    }

    async fn helper_status(&self) -> Result<HelperStatus, CoreError> {
        #[cfg(windows)]
        {
            return windows_impl::helper_status().await;
        }
        #[cfg(not(windows))]
        unavailable()
    }

    async fn ensure_hiddify(&self, _cancel: CancellationToken) -> Result<(), CoreError> {
        unavailable()
    }

    async fn prepare_runtime(&self) -> Result<RuntimeGeneration, CoreError> {
        unavailable()
    }

    async fn validate_runtime(&self, _generation: &RuntimeGeneration) -> Result<(), CoreError> {
        unavailable()
    }

    async fn start_core(&self, _generation: &RuntimeGeneration) -> Result<(), CoreError> {
        unavailable()
    }

    async fn stop_core(&self) -> Result<(), CoreError> {
        unavailable()
    }

    async fn core_process(&self) -> Result<ProcessStatus, CoreError> {
        unavailable()
    }

    async fn tun_status(&self) -> Result<TunStatus, CoreError> {
        unavailable()
    }

    async fn check_readiness(
        &self,
        _cancel: CancellationToken,
    ) -> Result<ReadinessReport, CoreError> {
        unavailable()
    }

    async fn cleanup_owned_state(&self) -> Result<CleanupReport, CoreError> {
        unavailable()
    }
}

#[cfg(windows)]
mod windows_impl {
    use super::{CoreError, HelperStatus, HELPER_PIPE};
    use iran_split_ipc::{
        read_frame, validate_envelope, write_frame, Envelope, HelperCommand, HelperReply,
        PROTOCOL_VERSION,
    };
    use std::io;
    use std::time::Duration;
    use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeClient};
    use windows::Win32::Foundation::ERROR_PIPE_BUSY;

    const PIPE_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
    const PIPE_RETRY_DELAY: Duration = Duration::from_millis(50);

    fn is_pipe_busy(error: &io::Error) -> bool {
        error
            .raw_os_error()
            .and_then(|code| u32::try_from(code).ok())
            == Some(ERROR_PIPE_BUSY.0)
    }

    async fn connect_helper() -> Result<NamedPipeClient, CoreError> {
        tokio::time::timeout(PIPE_CONNECT_TIMEOUT, async {
            loop {
                match ClientOptions::new().open(HELPER_PIPE) {
                    Ok(pipe) => return Ok(pipe),
                    Err(error) if is_pipe_busy(&error) => {
                        tokio::time::sleep(PIPE_RETRY_DELAY).await;
                    }
                    Err(_) => return Err(CoreError::HelperUnavailable),
                }
            }
        })
        .await
        .map_err(|_| CoreError::HelperUnavailable)?
    }

    pub async fn helper_status() -> Result<HelperStatus, CoreError> {
        let mut pipe = connect_helper().await?;
        let hello = Envelope::new(HelperCommand::Hello {
            client_version: env!("CARGO_PKG_VERSION").into(),
            supported_protocols: vec![PROTOCOL_VERSION],
        });
        write_frame(&mut pipe, &hello)
            .await
            .map_err(|error| CoreError::Platform(error.to_string()))?;
        let response: Envelope<HelperReply> = read_frame(&mut pipe)
            .await
            .map_err(|error| CoreError::Platform(error.to_string()))?;
        validate_envelope(&response).map_err(|error| CoreError::Platform(error.to_string()))?;
        match response.payload {
            HelperReply::Hello(reply) if reply.selected_protocol == PROTOCOL_VERSION => {
                Ok(HelperStatus {
                    available: true,
                    authorized: true,
                    version: Some(reply.helper_version),
                })
            }
            HelperReply::Error(error) if error.code == "UNAUTHORIZED" => {
                Err(CoreError::HelperUnauthorized)
            }
            _ => Err(CoreError::Platform(
                "Windows helper negotiation failed".into(),
            )),
        }
    }

    #[cfg(test)]
    mod tests {
        use super::{io, is_pipe_busy};

        #[test]
        fn detects_busy_pipe_errors_for_bounded_retries() {
            let error = io::Error::from_raw_os_error(231);
            assert!(is_pipe_busy(&error));
            assert!(!is_pipe_busy(&io::Error::from_raw_os_error(2)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::HELPER_PIPE;

    #[test]
    fn pipe_name_is_versioned_and_fixed() {
        assert_eq!(HELPER_PIPE, r"\\.\pipe\iran-split-helper-v1");
        assert!(!HELPER_PIPE.contains(".."));
    }
}
