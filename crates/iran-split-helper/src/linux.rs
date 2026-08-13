use super::{HelperServiceError, HelperSettings, Supervisor};
use iran_split_ipc::{
    read_frame, validate_envelope, write_frame, Envelope, HelloReply, HelperCommand, HelperError,
    HelperReply, ServiceStatus, PROTOCOL_VERSION,
};
use nix::sys::socket::{getsockopt, sockopt::PeerCredentials};
use std::{fs, os::unix::fs::PermissionsExt, path::Path, sync::Arc, time::Duration};
use tokio::{net::UnixListener, net::UnixStream, time::timeout};
use tracing::{error, info, warn};

const IO_TIMEOUT: Duration = Duration::from_secs(5);

/// Runs the Linux helper service and accepts authenticated local IPC clients.
///
/// # Errors
///
/// Returns an error when configuration, socket setup, or client acceptance
/// fails.
pub async fn run_linux(config_path: &Path) -> Result<(), HelperServiceError> {
    let settings = HelperSettings::load(config_path)?;
    let socket_path = settings.socket_path.clone();
    let socket_parent = socket_path
        .parent()
        .ok_or_else(|| HelperServiceError::UnsafeConfig("socket has no parent".into()))?;
    fs::create_dir_all(socket_parent)?;
    fs::set_permissions(socket_parent, fs::Permissions::from_mode(0o750))?;
    if socket_path.exists() {
        let metadata = fs::symlink_metadata(&socket_path)?;
        if !metadata.file_type().is_socket() {
            return Err(HelperServiceError::UnsafeConfig(
                "refusing to replace a non-socket at socket_path".into(),
            ));
        }
        fs::remove_file(&socket_path)?;
    }
    let listener = UnixListener::bind(&socket_path)?;
    fs::set_permissions(&socket_path, fs::Permissions::from_mode(0o660))?;
    let supervisor = Arc::new(Supervisor::new(settings));
    info!(
        event = "helper.listening",
        section = "helper_ipc",
        initiator = "helper_process",
        cause = "startup_complete",
        trace_route = "helper_process->unix_socket->ipc_listener",
        path = %socket_path.display(),
        "helper listening"
    );

    loop {
        let (stream, _) = listener.accept().await?;
        let supervisor = Arc::clone(&supervisor);
        tokio::spawn(async move {
            if let Err(error) = handle_connection(stream, supervisor).await {
                warn!(
                    event = "helper.connection_failed",
                    section = "helper_ipc",
                    initiator = "ipc_client",
                    cause = %error,
                    trace_route = "ipc_client->helper_connection->command_handler",
                    "helper connection closed with an error"
                );
            }
        });
    }
}

async fn handle_connection(
    mut stream: UnixStream,
    supervisor: Arc<Supervisor>,
) -> Result<(), HelperServiceError> {
    let credentials = getsockopt(&stream, PeerCredentials)
        .map_err(|error| HelperServiceError::Io(std::io::Error::from_raw_os_error(error as i32)))?;
    let peer_uid = credentials.uid();
    if peer_uid != supervisor.settings().authorized_uid && peer_uid != 0 {
        warn!(
            event = "helper.peer_rejected",
            section = "helper_security",
            initiator = "ipc_peer",
            cause = "unauthorized_uid",
            trace_route = "ipc_peer->credential_check->reject",
            peer_uid,
            "rejected unauthorized helper peer"
        );
        return Ok(());
    }

    let hello = read_request(&mut stream).await?;
    let HelperCommand::Hello {
        supported_protocols,
        ..
    } = &hello.payload
    else {
        send_reply(
            &mut stream,
            hello.reply(HelperReply::Error(HelperError {
                code: "HELLO_REQUIRED".into(),
                message: "the first command must negotiate the protocol".into(),
                retryable: false,
            })),
        )
        .await?;
        return Ok(());
    };
    if !supported_protocols.contains(&PROTOCOL_VERSION) {
        send_reply(
            &mut stream,
            hello.reply(HelperReply::Error(HelperError {
                code: "PROTOCOL_MISMATCH".into(),
                message: "no supported protocol version overlaps".into(),
                retryable: false,
            })),
        )
        .await?;
        return Ok(());
    }
    send_reply(
        &mut stream,
        hello.reply(HelperReply::Hello(HelloReply {
            helper_version: env!("CARGO_PKG_VERSION").into(),
            selected_protocol: PROTOCOL_VERSION,
            capabilities: vec![
                "runtime_generation_v1".into(),
                "owned_network_cleanup_v1".into(),
                "bounded_logs_v1".into(),
            ],
        })),
    )
    .await?;

    loop {
        let request = match read_request(&mut stream).await {
            Ok(request) => request,
            Err(HelperServiceError::Protocol(iran_split_ipc::ProtocolError::Io(error)))
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::UnexpectedEof | std::io::ErrorKind::ConnectionReset
                ) =>
            {
                return Ok(());
            }
            Err(error) => return Err(error),
        };
        request.payload.validate()?;
        send_reply(
            &mut stream,
            execute_audited(&supervisor, request, peer_uid).await,
        )
        .await?;
    }
}

async fn execute_audited(
    supervisor: &Supervisor,
    request: Envelope<HelperCommand>,
    peer_uid: u32,
) -> Envelope<HelperReply> {
    info!(
        event = "helper.command_received",
        section = "helper_command",
        initiator = "authorized_ipc_peer",
        cause = "ipc_request",
        trace_id = %request.request_id,
        trace_route = "desktop->helper_ipc->command_handler",
        peer_uid,
        request_id = %request.request_id,
        command = request.payload.audit_name(),
        "authorized helper command"
    );
    let request_id = request.request_id;
    let protocol_version = request.protocol_version;
    let reply = execute(supervisor, request.payload).await;
    if let HelperReply::Error(reply_error) = &reply {
        error!(
            event = "helper.command_failed",
            section = "helper_command",
            initiator = "command_handler",
            cause = %reply_error.message,
            trace_id = %request_id,
            trace_route = "desktop->helper_ipc->command_handler->error_reply",
            error_code = reply_error.code,
            retryable = reply_error.retryable,
            "helper command failed"
        );
    } else {
        info!(
            event = "helper.command_completed",
            section = "helper_command",
            initiator = "command_handler",
            cause = "none",
            trace_id = %request_id,
            trace_route = "desktop->helper_ipc->command_handler->reply",
            "helper command completed"
        );
    }
    Envelope {
        protocol_version,
        request_id,
        payload: reply,
    }
}

async fn execute(supervisor: &Supervisor, command: HelperCommand) -> HelperReply {
    let result: Result<HelperReply, HelperServiceError> = async {
        Ok(match command {
            HelperCommand::Hello { .. } => HelperReply::Error(HelperError {
                code: "HELLO_ALREADY_COMPLETED".into(),
                message: "protocol negotiation is already complete".into(),
                retryable: false,
            }),
            HelperCommand::GetServiceStatus => HelperReply::ServiceStatus(ServiceStatus {
                helper_version: env!("CARGO_PKG_VERSION").into(),
                protocol_version: PROTOCOL_VERSION,
                authorized: true,
                active_generation: supervisor.status().await?.generation_id,
            }),
            HelperCommand::RegisterRuntimeGeneration {
                generation_id,
                config_sha256,
            } => {
                supervisor
                    .register_generation(generation_id, &config_sha256)
                    .await?;
                HelperReply::GenerationRegistered { generation_id }
            }
            HelperCommand::StartMihomo {
                generation_id,
                config_sha256,
            } => HelperReply::ProcessStatus(supervisor.start(generation_id, &config_sha256).await?),
            HelperCommand::StopMihomo => HelperReply::ProcessStatus(supervisor.stop().await?),
            HelperCommand::RestartMihomo {
                generation_id,
                config_sha256,
            } => {
                supervisor.stop().await?;
                HelperReply::ProcessStatus(supervisor.start(generation_id, &config_sha256).await?)
            }
            HelperCommand::GetMihomoProcessStatus => {
                HelperReply::ProcessStatus(supervisor.status().await?)
            }
            HelperCommand::CleanupOwnedNetworkState => {
                HelperReply::CleanupReport(supervisor.cleanup().await?)
            }
            HelperCommand::CollectServiceLogs { max_entries } => {
                HelperReply::Logs(supervisor.logs(usize::from(max_entries)).await)
            }
            HelperCommand::PrepareForUpdate => {
                let report = supervisor.cleanup().await?;
                if !report.clean() {
                    return Err(HelperServiceError::Process(
                        "owned network state remains before update".into(),
                    ));
                }
                HelperReply::ReadyForUpdate
            }
        })
    }
    .await;
    result.unwrap_or_else(|error| {
        HelperReply::Error(HelperError {
            code: helper_error_code(&error).into(),
            message: error.to_string(),
            retryable: matches!(
                error,
                HelperServiceError::Io(_) | HelperServiceError::Process(_)
            ),
        })
    })
}

fn helper_error_code(error: &HelperServiceError) -> &'static str {
    match error {
        HelperServiceError::InvalidGeneration(_) => "INVALID_GENERATION",
        HelperServiceError::BinaryIntegrity => "BINARY_INTEGRITY_FAILED",
        HelperServiceError::Process(_) => "PROCESS_FAILED",
        HelperServiceError::UnsafeConfig(_) | HelperServiceError::Toml(_) => {
            "HELPER_CONFIG_INVALID"
        }
        HelperServiceError::Io(_) => "IO_FAILED",
        HelperServiceError::Protocol(_) => "PROTOCOL_ERROR",
    }
}

async fn read_request(
    stream: &mut UnixStream,
) -> Result<Envelope<HelperCommand>, HelperServiceError> {
    let request = timeout(IO_TIMEOUT, read_frame(stream))
        .await
        .map_err(|_| {
            HelperServiceError::Io(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "helper request timed out",
            ))
        })??;
    validate_envelope(&request)?;
    Ok(request)
}

async fn send_reply(
    stream: &mut UnixStream,
    reply: Envelope<HelperReply>,
) -> Result<(), HelperServiceError> {
    timeout(IO_TIMEOUT, write_frame(stream, &reply))
        .await
        .map_err(|_| {
            HelperServiceError::Io(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "helper response timed out",
            ))
        })??;
    Ok(())
}

use std::os::unix::fs::FileTypeExt;
