use async_trait::async_trait;
use iran_split_core::{
    CleanupReport, CoreError, HelperStatus, PlatformBackend, ProcessStatus, ReadinessReport,
    RuntimeGeneration, TunStatus,
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
    use super::*;
    use iran_split_ipc::{
        read_frame, validate_envelope, write_frame, Envelope, HelperCommand, HelperReply,
        PROTOCOL_VERSION,
    };
    use tokio::net::windows::named_pipe::ClientOptions;
    use std::time::Duration;

    pub async fn helper_status() -> Result<HelperStatus, CoreError> {
        let mut pipe = tokio::time::timeout(
            Duration::from_secs(5),
            ClientOptions::new().open(HELPER_PIPE),
        )
        .await
        .map_err(|_| CoreError::HelperUnavailable)?
        .map_err(|_| CoreError::HelperUnavailable)?;
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
            HelperReply::Hello(reply) if reply.selected_protocol == PROTOCOL_VERSION => Ok(
                HelperStatus {
                    available: true,
                    authorized: true,
                    version: Some(reply.helper_version),
                },
            ),
            HelperReply::Error(error) if error.code == "UNAUTHORIZED" => {
                Err(CoreError::HelperUnauthorized)
            }
            _ => Err(CoreError::Platform(
                "Windows helper negotiation failed".into(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipe_name_is_versioned_and_fixed() {
        assert_eq!(HELPER_PIPE, r"\\.\pipe\iran-split-helper-v1");
        assert!(!HELPER_PIPE.contains(".."));
    }
}
