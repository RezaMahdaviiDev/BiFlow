use super::{commands, HelperServiceError, HelperSettings, Supervisor};
use iran_split_helper_winacl::create_helper_server;
use iran_split_ipc::{HelloReply, HelperCommand, HelperError, HelperReply, PROTOCOL_VERSION};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
};
use tokio::net::windows::named_pipe::NamedPipeServer;
use tracing::{info, warn};

const PIPE_NAME: &str = r"\\.\pipe\iran-split-helper-v1";
const INSTALL_ROOT: &str = r"C:\ProgramData\iran-split";
const TASK_NAME: &str = "BiFlowHelper";

/// Serves helper IPC on the versioned local named pipe.
///
/// # Errors
///
/// Returns an error when configuration cannot be loaded or the pipe cannot be
/// created.
pub async fn run_named_pipe(config_path: &Path) -> Result<(), HelperServiceError> {
    let settings = HelperSettings::load(config_path)?;
    let supervisor = Arc::new(Supervisor::new(settings));
    let mut server = create_helper_server(PIPE_NAME, true).map_err(HelperServiceError::Io)?;
    info!(
        event = "helper.listening",
        section = "helper_ipc",
        initiator = "helper_process",
        cause = "startup_complete",
        trace_route = "helper_process->named_pipe->ipc_listener",
        "helper listening"
    );

    loop {
        server.connect().await?;
        let connected = server;
        server = create_helper_server(PIPE_NAME, false).map_err(HelperServiceError::Io)?;
        let supervisor = Arc::clone(&supervisor);
        tokio::spawn(async move {
            if let Err(error) = handle_connection(connected, supervisor).await {
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

/// Copies the helper and Mihomo into ProgramData and starts a SYSTEM task.
///
/// # Errors
///
/// Returns an error when files cannot be copied, the configuration is unsafe,
/// or the scheduled task cannot be created.
pub fn install(
    mihomo_src: &Path,
    staging_dir: &Path,
    tun_name: &str,
) -> Result<(), HelperServiceError> {
    let helper_src = std::env::current_exe()?;
    let root = PathBuf::from(INSTALL_ROOT);
    let bin = root.join("bin");
    let helper_dest = bin.join("iran-split-helper.exe");
    let mihomo_dest = bin.join("mihomo.exe");
    let config_dest = root.join("helper.toml");
    fs::create_dir_all(&bin)?;
    fs::create_dir_all(root.join("runtime"))?;
    fs::create_dir_all(staging_dir)?;
    fs::copy(&helper_src, &helper_dest)?;
    fs::copy(mihomo_src, &mihomo_dest)?;
    let mihomo_sha256 = sha256_file(&mihomo_dest)?;
    let settings = HelperSettings {
        authorized_uid: 0,
        authorized_gid: 0,
        socket_path: PathBuf::from(PIPE_NAME),
        staging_dir: staging_dir.to_path_buf(),
        runtime_dir: root.join("runtime"),
        mihomo_binary: mihomo_dest,
        mihomo_sha256,
        tun_name: tun_name.to_owned(),
    };
    settings.validate()?;
    fs::write(
        &config_dest,
        format!(
            "authorized_uid = 0\nauthorized_gid = 0\nsocket_path = \"{}\"\nstaging_dir = \"{}\"\nruntime_dir = \"{}\"\nmihomo_binary = \"{}\"\nmihomo_sha256 = \"{}\"\ntun_name = \"{}\"\n",
            escape_toml(&settings.socket_path),
            escape_toml(&settings.staging_dir),
            escape_toml(&settings.runtime_dir),
            escape_toml(&settings.mihomo_binary),
            settings.mihomo_sha256,
            settings.tun_name
        ),
    )?;
    let tr = format!(
        "\"{}\" --config \"{}\"",
        helper_dest.display(),
        config_dest.display()
    );
    run_schtasks(&[
        "/Create", "/TN", TASK_NAME, "/SC", "ONSTART", "/RU", "SYSTEM", "/RL", "HIGHEST", "/F",
        "/TR", &tr,
    ])?;
    run_schtasks(&["/Run", "/TN", TASK_NAME])?;
    info!(
        event = "helper.installed",
        section = "helper_install",
        initiator = "elevated_installer",
        cause = "user_requested",
        trace_route = "desktop->elevated_helper->scheduled_task",
        "windows helper scheduled task installed"
    );
    Ok(())
}

/// Stops and removes the packaged Windows helper task.
///
/// # Errors
///
/// Returns an error when `schtasks` fails for a reason other than a missing
/// task.
pub fn uninstall() -> Result<(), HelperServiceError> {
    let _ = run_schtasks(&["/End", "/TN", TASK_NAME]);
    match run_schtasks(&["/Delete", "/TN", TASK_NAME, "/F"]) {
        Ok(()) => Ok(()),
        Err(error) if error.to_string().contains("cannot find the file") => Ok(()),
        Err(error) => Err(error),
    }
}

fn run_schtasks(args: &[&str]) -> Result<(), HelperServiceError> {
    let output = Command::new("schtasks").args(args).output()?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(HelperServiceError::Process(
        stderr.chars().take(512).collect(),
    ))
}

fn sha256_file(path: &Path) -> Result<String, HelperServiceError> {
    let bytes = fs::read(path)?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

fn escape_toml(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
}

async fn handle_connection(
    mut stream: NamedPipeServer,
    supervisor: Arc<Supervisor>,
) -> Result<(), HelperServiceError> {
    let hello = commands::read_request(&mut stream).await?;
    let HelperCommand::Hello {
        supported_protocols,
        ..
    } = &hello.payload
    else {
        commands::send_reply(
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
        commands::send_reply(
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
    commands::send_reply(
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
        let request = match commands::read_request(&mut stream).await {
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
        commands::send_reply(
            &mut stream,
            commands::execute_audited(&supervisor, request, "named_pipe").await,
        )
        .await?;
    }
}
