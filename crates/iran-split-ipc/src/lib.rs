use ipnet::IpNet;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{collections::BTreeMap, io, path::PathBuf};
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use uuid::Uuid;

pub const PROTOCOL_VERSION: u16 = 1;
pub const MAX_MESSAGE_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Envelope<T> {
    pub protocol_version: u16,
    pub request_id: Uuid,
    pub payload: T,
}

impl<T> Envelope<T> {
    #[must_use]
    pub fn new(payload: T) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            request_id: Uuid::new_v4(),
            payload,
        }
    }

    #[must_use]
    pub const fn reply<U>(&self, payload: U) -> Envelope<U> {
        Envelope {
            protocol_version: PROTOCOL_VERSION,
            request_id: self.request_id,
            payload,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", content = "arguments", rename_all = "snake_case")]
pub enum HelperCommand {
    Hello {
        client_version: String,
        supported_protocols: Vec<u16>,
    },
    GetServiceStatus,
    RegisterRuntimeGeneration {
        generation_id: Uuid,
        config_sha256: String,
    },
    StartMihomo {
        generation_id: Uuid,
        config_sha256: String,
    },
    StopMihomo,
    RestartMihomo {
        generation_id: Uuid,
        config_sha256: String,
    },
    GetMihomoProcessStatus,
    StartOpenVpn(OpenVpnRequest),
    StopOpenVpn,
    GetOpenVpnStatus,
    CleanupOwnedNetworkState,
    CollectServiceLogs {
        max_entries: u16,
    },
    PrepareForUpdate,
}

/// Everything the helper needs to run the `OpenVPN` side tunnel.
///
/// The desktop never sends raw `OpenVPN` arguments. It sends these vetted
/// fields and the helper builds the command line itself, always appending the
/// flags that keep the tunnel from becoming the system gateway.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenVpnRequest {
    pub profile: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_file: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executable: Option<PathBuf>,
    pub device: String,
    /// Keep the server's scoped `push route` directives (never the default).
    pub pull_routes: bool,
    /// Extra CIDRs the helper installs on the `OpenVPN` device.
    #[serde(default)]
    pub tunnel_routes: Vec<String>,
    pub routing_mark: u32,
    pub routing_table: u32,
    pub start_timeout_seconds: u64,
}

/// Live state of the `OpenVPN` side tunnel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct OpenVpnStatus {
    pub running: bool,
    pub pid: Option<u32>,
    pub device: Option<String>,
    /// Address the tunnel assigned to this machine.
    pub local_address: Option<String>,
    /// Remote transport address. Mihomo must keep this DIRECT or the tunnel's
    /// own packets loop back into the split TUN.
    pub server_endpoint: Option<String>,
    /// Scoped networks currently routed into the tunnel.
    #[serde(default)]
    pub routes: Vec<String>,
    pub started_at: Option<String>,
    pub last_error: Option<String>,
}

impl HelperCommand {
    /// Validates bounded command arguments before helper execution.
    ///
    /// # Errors
    ///
    /// Returns [`ProtocolError::InvalidMessage`] for an empty or oversized
    /// protocol list, malformed generation hash, or out-of-range log request.
    pub fn validate(&self) -> Result<(), ProtocolError> {
        match self {
            Self::Hello {
                supported_protocols,
                ..
            } if supported_protocols.is_empty() || supported_protocols.len() > 8 => {
                Err(ProtocolError::InvalidMessage(
                    "supported protocol list must contain 1-8 items".into(),
                ))
            }
            Self::RegisterRuntimeGeneration { config_sha256, .. }
            | Self::StartMihomo { config_sha256, .. }
            | Self::RestartMihomo { config_sha256, .. }
                if !valid_sha256(config_sha256) =>
            {
                Err(ProtocolError::InvalidMessage(
                    "generation SHA-256 must be 64 lowercase hexadecimal characters".into(),
                ))
            }
            Self::CollectServiceLogs { max_entries }
                if *max_entries == 0 || *max_entries > 2_000 =>
            {
                Err(ProtocolError::InvalidMessage(
                    "log request must contain between 1 and 2000 entries".into(),
                ))
            }
            Self::StartOpenVpn(request) => request.validate(),
            _ => Ok(()),
        }
    }

    #[must_use]
    pub const fn audit_name(&self) -> &'static str {
        match self {
            Self::Hello { .. } => "hello",
            Self::GetServiceStatus => "get_service_status",
            Self::RegisterRuntimeGeneration { .. } => "register_runtime_generation",
            Self::StartMihomo { .. } => "start_mihomo",
            Self::StopMihomo => "stop_mihomo",
            Self::RestartMihomo { .. } => "restart_mihomo",
            Self::GetMihomoProcessStatus => "get_mihomo_process_status",
            Self::StartOpenVpn(_) => "start_openvpn",
            Self::StopOpenVpn => "stop_openvpn",
            Self::GetOpenVpnStatus => "get_openvpn_status",
            Self::CleanupOwnedNetworkState => "cleanup_owned_network_state",
            Self::CollectServiceLogs { .. } => "collect_service_logs",
            Self::PrepareForUpdate => "prepare_for_update",
        }
    }
}

impl OpenVpnRequest {
    /// Rejects a request the helper must not act on.
    ///
    /// # Errors
    ///
    /// Returns [`ProtocolError::InvalidMessage`] when a path is relative or
    /// traversing, the device name is unusable, the policy-routing numbers are
    /// out of range, or a tunnel route is malformed or a default route.
    pub fn validate(&self) -> Result<(), ProtocolError> {
        for (label, path) in [
            ("profile", Some(&self.profile)),
            ("auth file", self.auth_file.as_ref()),
            ("executable", self.executable.as_ref()),
        ]
        .into_iter()
        .filter_map(|(label, path)| path.map(|path| (label, path)))
        {
            if !path.is_absolute() || path.components().any(|part| part.as_os_str() == "..") {
                return Err(ProtocolError::InvalidMessage(format!(
                    "OpenVPN {label} path must be absolute and free of parent traversal"
                )));
            }
        }
        if !valid_device_name(&self.device) {
            return Err(ProtocolError::InvalidMessage(
                "OpenVPN device must be 1-15 characters of letters, digits, dashes, or underscores"
                    .into(),
            ));
        }
        if self.routing_mark == 0 {
            return Err(ProtocolError::InvalidMessage(
                "OpenVPN routing mark cannot be zero".into(),
            ));
        }
        if self.routing_table == 0 || self.routing_table > 252 {
            return Err(ProtocolError::InvalidMessage(
                "OpenVPN routing table must be between 1 and 252".into(),
            ));
        }
        if self.start_timeout_seconds == 0 || self.start_timeout_seconds > 300 {
            return Err(ProtocolError::InvalidMessage(
                "OpenVPN start timeout must be between 1 and 300 seconds".into(),
            ));
        }
        if self.tunnel_routes.len() > 64 {
            return Err(ProtocolError::InvalidMessage(
                "OpenVPN accepts at most 64 tunnel routes".into(),
            ));
        }
        for route in &self.tunnel_routes {
            match route.trim().parse::<IpNet>() {
                Ok(network) if network.prefix_len() > 0 => {}
                Ok(_) => {
                    return Err(ProtocolError::InvalidMessage(
                        "an OpenVPN default route would take the whole system offline".into(),
                    ))
                }
                Err(_) => {
                    return Err(ProtocolError::InvalidMessage(
                        "OpenVPN tunnel routes must be CIDR networks".into(),
                    ))
                }
            }
        }
        Ok(())
    }
}

fn valid_device_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 15
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "result", content = "value", rename_all = "snake_case")]
pub enum HelperReply {
    Hello(HelloReply),
    ServiceStatus(ServiceStatus),
    GenerationRegistered { generation_id: Uuid },
    ProcessStatus(ProcessStatus),
    OpenVpnStatus(OpenVpnStatus),
    CleanupReport(CleanupReport),
    Logs(Vec<ServiceLogEntry>),
    ReadyForUpdate,
    Ack,
    Error(HelperError),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HelloReply {
    pub helper_version: String,
    pub selected_protocol: u16,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceStatus {
    pub helper_version: String,
    pub protocol_version: u16,
    pub authorized: bool,
    pub active_generation: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessStatus {
    pub running: bool,
    pub pid: Option<u32>,
    pub generation_id: Option<Uuid>,
    pub started_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "each flag reports one independent piece of owned state the helper had to release"
)]
pub struct CleanupReport {
    pub process_stopped: bool,
    pub tun_removed: bool,
    pub routes_removed: u32,
    pub dns_restored: bool,
    /// `true` once the `OpenVPN` side tunnel and its owned routes are gone.
    /// `default` so reports written before the side tunnel existed still load.
    #[serde(default)]
    pub openvpn_stopped: bool,
    pub warnings: Vec<String>,
}

impl CleanupReport {
    #[must_use]
    pub fn clean(&self) -> bool {
        self.process_stopped
            && self.tun_removed
            && self.dns_restored
            && self.openvpn_stopped
            && self.warnings.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceLogEntry {
    pub timestamp: String,
    pub level: String,
    pub event: String,
    #[serde(default)]
    pub fields: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HelperError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

#[derive(Debug, Error)]
pub enum ProtocolError {
    #[error("IPC I/O failed: {0}")]
    Io(#[from] io::Error),
    #[error("IPC JSON is malformed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("IPC message length {actual} exceeds the {maximum}-byte limit")]
    Oversized { actual: usize, maximum: usize },
    #[error("unsupported protocol version {actual}; expected {expected}")]
    VersionMismatch { actual: u16, expected: u16 },
    #[error("invalid IPC message: {0}")]
    InvalidMessage(String),
}

/// Writes one length-prefixed JSON message to an asynchronous stream.
///
/// # Errors
///
/// Returns [`ProtocolError`] when serialization fails, the encoded message is
/// oversized, or the stream cannot be written or flushed.
pub async fn write_frame<W, T>(writer: &mut W, message: &T) -> Result<(), ProtocolError>
where
    W: AsyncWrite + Unpin,
    T: Serialize,
{
    let bytes = serde_json::to_vec(message)?;
    if bytes.len() > MAX_MESSAGE_BYTES {
        return Err(ProtocolError::Oversized {
            actual: bytes.len(),
            maximum: MAX_MESSAGE_BYTES,
        });
    }
    let length = u32::try_from(bytes.len()).map_err(|_| ProtocolError::Oversized {
        actual: bytes.len(),
        maximum: MAX_MESSAGE_BYTES,
    })?;
    writer.write_all(&length.to_be_bytes()).await?;
    writer.write_all(&bytes).await?;
    writer.flush().await?;
    Ok(())
}

/// Reads one bounded length-prefixed JSON message from an asynchronous stream.
///
/// # Errors
///
/// Returns [`ProtocolError`] for I/O failure, a zero-length or oversized frame,
/// or malformed JSON.
pub async fn read_frame<R, T>(reader: &mut R) -> Result<T, ProtocolError>
where
    R: AsyncRead + Unpin,
    T: DeserializeOwned,
{
    let mut length_bytes = [0_u8; 4];
    reader.read_exact(&mut length_bytes).await?;
    let length = u32::from_be_bytes(length_bytes) as usize;
    if length > MAX_MESSAGE_BYTES {
        return Err(ProtocolError::Oversized {
            actual: length,
            maximum: MAX_MESSAGE_BYTES,
        });
    }
    if length == 0 {
        return Err(ProtocolError::InvalidMessage(
            "zero-length IPC frame".into(),
        ));
    }
    let mut bytes = vec![0_u8; length];
    reader.read_exact(&mut bytes).await?;
    Ok(serde_json::from_slice(&bytes)?)
}

/// Verifies that an envelope uses this crate's protocol version.
///
/// # Errors
///
/// Returns [`ProtocolError::VersionMismatch`] when the peer uses another
/// protocol version.
pub fn validate_envelope<T>(message: &Envelope<T>) -> Result<(), ProtocolError> {
    if message.protocol_version != PROTOCOL_VERSION {
        return Err(ProtocolError::VersionMismatch {
            actual: message.protocol_version,
            expected: PROTOCOL_VERSION,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncWriteExt;

    #[tokio::test]
    async fn framed_round_trip_preserves_request_id() {
        let (mut client, mut server) = tokio::io::duplex(4_096);
        let expected = Envelope::new(HelperCommand::GetServiceStatus);
        let request_id = expected.request_id;
        let sender = tokio::spawn(async move { write_frame(&mut client, &expected).await });
        let actual: Envelope<HelperCommand> = read_frame(&mut server).await.expect("read frame");
        sender.await.expect("join").expect("write frame");
        assert_eq!(actual.request_id, request_id);
        validate_envelope(&actual).expect("valid envelope");
    }

    #[tokio::test]
    async fn rejects_oversized_length_before_allocating() {
        let (mut client, mut server) = tokio::io::duplex(16);
        let oversized = u32::try_from(MAX_MESSAGE_BYTES).expect("message limit fits u32") + 1;
        client
            .write_all(&oversized.to_be_bytes())
            .await
            .expect("write length");
        let error = read_frame::<_, Envelope<HelperCommand>>(&mut server)
            .await
            .expect_err("oversized frame");
        assert!(matches!(error, ProtocolError::Oversized { .. }));
    }

    #[test]
    fn validates_generation_hash_and_log_bound() {
        let invalid = HelperCommand::StartMihomo {
            generation_id: Uuid::new_v4(),
            config_sha256: "../config".into(),
        };
        assert!(invalid.validate().is_err());
        assert!(HelperCommand::CollectServiceLogs { max_entries: 2_001 }
            .validate()
            .is_err());
    }

    fn openvpn_request() -> OpenVpnRequest {
        OpenVpnRequest {
            // temp_dir is absolute on whichever OS runs the test; a literal
            // `/etc/...` is not absolute on Windows.
            profile: std::env::temp_dir().join("office.ovpn"),
            auth_file: None,
            executable: None,
            device: "biflow-ovpn".into(),
            pull_routes: true,
            tunnel_routes: vec!["10.8.0.0/24".into()],
            routing_mark: 0x0000_b1f0,
            routing_table: 178,
            start_timeout_seconds: 45,
        }
    }

    #[test]
    fn openvpn_request_rejects_traversal_and_default_routes() {
        assert!(openvpn_request().validate().is_ok());

        let traversing = OpenVpnRequest {
            profile: std::env::temp_dir().join("..").join("id_rsa"),
            ..openvpn_request()
        };
        assert!(traversing.validate().is_err());

        let relative = OpenVpnRequest {
            profile: PathBuf::from("office.ovpn"),
            ..openvpn_request()
        };
        assert!(relative.validate().is_err());

        let default_route = OpenVpnRequest {
            tunnel_routes: vec!["0.0.0.0/0".into()],
            ..openvpn_request()
        };
        assert!(default_route.validate().is_err());

        let reserved_table = OpenVpnRequest {
            routing_table: 254,
            ..openvpn_request()
        };
        assert!(reserved_table.validate().is_err());

        let hostile_device = OpenVpnRequest {
            device: "tun0; rm -rf /".into(),
            ..openvpn_request()
        };
        assert!(hostile_device.validate().is_err());
    }

    #[test]
    fn cleanup_is_not_clean_until_openvpn_is_gone() {
        let mut report = CleanupReport {
            process_stopped: true,
            tun_removed: true,
            routes_removed: 2,
            dns_restored: true,
            openvpn_stopped: false,
            warnings: Vec::new(),
        };
        assert!(!report.clean());
        report.openvpn_stopped = true;
        assert!(report.clean());
    }

    #[test]
    fn openvpn_commands_are_audited_by_name() {
        assert_eq!(
            HelperCommand::StartOpenVpn(openvpn_request()).audit_name(),
            "start_openvpn"
        );
        assert_eq!(HelperCommand::StopOpenVpn.audit_name(), "stop_openvpn");
        assert_eq!(
            HelperCommand::GetOpenVpnStatus.audit_name(),
            "get_openvpn_status"
        );
    }
}
