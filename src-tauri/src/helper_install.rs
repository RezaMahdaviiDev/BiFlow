use super::services;
use iran_split_core::PlatformBackend;
use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};
use tauri::{AppHandle, Manager};
use tokio::process::Command;
use tracing::{error, info};

#[cfg(target_os = "linux")]
use sha2::{Digest, Sha256};
#[cfg(target_os = "linux")]
use std::os::unix::fs::PermissionsExt;

#[cfg(target_os = "linux")]
const LINUX_HELPER_ROOT: &str = "/usr/lib/biflow";
#[cfg(target_os = "linux")]
const PKEXEC: &str = "/usr/bin/pkexec";
/// `install-helper.sh` refuses `--authorized-uid 0`, so a root-owned UI can only
/// ever fail. Say why instead of forwarding that exit code as a generic failure.
#[cfg(target_os = "linux")]
const ROOT_APP_REJECTION: &str = "BiFlow is running as root, so the helper cannot be installed. The helper authorizes one non-root user. Quit BiFlow, start it as your normal user without sudo, then install the helper again.";
/// Keeps a runaway script error out of the dialog while preserving the reason.
#[cfg(any(target_os = "linux", target_os = "windows"))]
const MAX_DETAIL_CHARS: usize = 200;
/// `ERROR_CANCELLED`: the operator refused the UAC prompt.
#[cfg(target_os = "windows")]
const UAC_CANCELLED: i32 = 1223;
/// `CREATE_NO_WINDOW`: keeps the elevation shell from flashing a console.
#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
#[cfg(target_os = "windows")]
const WINDOWS_PROGRAMDATA_HELPER: &str = r"C:\ProgramData\iran-split\bin\iran-split-helper.exe";

#[derive(Debug, Clone, Serialize)]
pub struct InstallHelperResult {
    pub installed: bool,
}

/// Installs and starts the privileged helper for the current packaged app.
///
/// # Errors
///
/// Returns an error when bundled files are missing, elevation fails, or the
/// helper does not become reachable.
pub async fn install_helper(app: &AppHandle) -> Result<InstallHelperResult, String> {
    let services = services(app)?;
    let resource_root = app
        .path()
        .resource_dir()
        .map_err(|error| error.to_string())?;
    let exe_dir = std::env::current_exe()
        .map_err(|error| error.to_string())?
        .parent()
        .ok_or_else(|| "application executable has no parent directory".to_owned())?
        .to_path_buf();
    let tun_name = services
        .config_store
        .load_or_create()
        .map_err(|error| error.to_string())?
        .mihomo
        .tun_name;
    let staging_dir = services.paths.data.join("runtime/generations");
    fs::create_dir_all(&staging_dir).map_err(|error| error.to_string())?;
    #[cfg(target_os = "linux")]
    let payload_dir = services.paths.data.join("runtime/helper-install");
    info!(
        event = "helper.install_started",
        section = "helper_install",
        initiator = "tauri_command",
        cause = "user_requested",
        trace_route = "ui->tauri_command->install_helper",
        "privileged helper installation requested"
    );
    #[cfg(target_os = "linux")]
    install_linux(
        &resource_root,
        &exe_dir,
        &payload_dir,
        &staging_dir,
        &tun_name,
    )
    .await?;
    #[cfg(target_os = "windows")]
    install_windows(&resource_root, &exe_dir, &staging_dir, &tun_name).await?;
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        let _ = (resource_root, exe_dir, staging_dir, tun_name);
        return Err("helper installation is not supported on this platform".into());
    }
    wait_for_helper(app).await?;
    info!(
        event = "helper.install_completed",
        section = "helper_install",
        initiator = "tauri_command",
        cause = "none",
        trace_route = "ui->tauri_command->install_helper->helper_ready",
        "privileged helper installation completed"
    );
    Ok(InstallHelperResult { installed: true })
}

async fn wait_for_helper(app: &AppHandle) -> Result<(), String> {
    let services = services(app)?;
    for _ in 0..50 {
        match services.backend.helper_status().await {
            Ok(status) if status.available => {
                services.engine.refresh_health().await;
                return Ok(());
            }
            Ok(_) | Err(_) => tokio::time::sleep(Duration::from_millis(100)).await,
        }
    }
    error!(
        event = "helper.install_timeout",
        section = "helper_install",
        initiator = "tauri_command",
        cause = "helper_not_ready",
        trace_route = "ui->tauri_command->install_helper->wait",
        "installed helper did not become ready"
    );
    Err("the helper was installed but is not reachable yet".into())
}

#[cfg(target_os = "linux")]
async fn install_linux(
    resource_root: &Path,
    exe_dir: &Path,
    payload_dir: &Path,
    staging_dir: &Path,
    tun_name: &str,
) -> Result<(), String> {
    let (uid, gid) = current_linux_ids()?;
    if let Some(rejection) = root_install_rejection(uid) {
        error!(
            event = "helper.install_rejected",
            section = "helper_install",
            initiator = "tauri_command",
            cause = "app_running_as_root",
            trace_route = "ui->tauri_command->install_helper->install_linux",
            "refusing to authorize the helper for root"
        );
        return Err(rejection.to_owned());
    }
    let helper_src = first_existing_file(&helper_binary_candidates(resource_root, exe_dir))
        .ok_or_else(|| "packaged helper binary is missing".to_owned())?;
    let mihomo_src = first_existing_file(&mihomo_candidates(resource_root, exe_dir))
        .ok_or_else(|| "packaged Mihomo binary is missing".to_owned())?;
    let script = first_existing_file(&install_script_candidates(resource_root, exe_dir))
        .ok_or_else(|| "helper install script is missing".to_owned())?;
    let unit = first_existing_file(&unit_candidates(resource_root, exe_dir));
    let helper_sha256 = sha256_file(&helper_src)?;
    let mihomo_sha256 = sha256_file(&mihomo_src)?;
    if !Path::new(PKEXEC).is_file() {
        return Err("pkexec is not installed; install policykit-1 and retry".into());
    }
    let payload = stage_privileged_payload(
        payload_dir,
        &script,
        &helper_src,
        &mihomo_src,
        unit.as_deref(),
    )?;
    if payload.staged {
        verify_staged_payload(&payload, &helper_sha256, &mihomo_sha256).inspect_err(|_| {
            discard_staged_payload(&payload);
        })?;
    }
    let mut command = Command::new(PKEXEC);
    command
        .arg(&payload.script)
        .arg("--authorized-uid")
        .arg(uid.to_string())
        .arg("--authorized-gid")
        .arg(gid.to_string())
        .arg("--staging-dir")
        .arg(staging_dir)
        .arg("--helper-src")
        .arg(&payload.helper)
        .arg("--mihomo-src")
        .arg(&payload.mihomo)
        .arg("--helper-sha256")
        .arg(&helper_sha256)
        .arg("--mihomo-sha256")
        .arg(&mihomo_sha256)
        .arg("--tun-name")
        .arg(tun_name);
    if let Some(unit) = payload.unit.as_ref() {
        command.arg("--unit-src").arg(unit);
    }
    let elevated = command.output().await.map_err(|error| error.to_string());
    discard_staged_payload(&payload);
    let output = elevated?;
    if output.status.success() {
        return Ok(());
    }
    // pkexec exits 126 only when the operator dismissed the polkit dialog. 127
    // means it could not run the script at all, which is a real failure.
    if output.status.code() == Some(126) {
        return Err("helper installation was cancelled".into());
    }
    let detail = last_error_line(&output.stderr);
    error!(
        event = "helper.install_failed",
        section = "helper_install",
        initiator = "tauri_command",
        cause = "install_script_failed",
        trace_route = "ui->tauri_command->install_helper->install_linux",
        exit_code = output.status.code().unwrap_or(-1),
        staged = payload.staged,
        detail = %detail,
        "privileged helper installation failed"
    );
    if detail.is_empty() {
        Err("privileged helper installation failed".into())
    } else {
        Err(format!("privileged helper installation failed: {detail}"))
    }
}

/// Absolute paths the elevated script reads. They are copies whenever the
/// packaged originals live somewhere root cannot follow.
#[cfg(target_os = "linux")]
pub(crate) struct PrivilegedPayload {
    pub script: PathBuf,
    pub helper: PathBuf,
    pub mihomo: PathBuf,
    pub unit: Option<PathBuf>,
    pub staged: bool,
    root: PathBuf,
}

/// An `AppImage` mounts itself through FUSE as the calling user, and without
/// `allow_other` that mount denies every other uid — root included. `pkexec`
/// then fails with `Error accessing …: Permission denied` before the script
/// runs, so copy the payload onto a normal filesystem root can read.
///
/// A `.deb` install already sits in root-owned `/usr/lib/biflow`, and running
/// that copy keeps the polkit action's `exec.path` annotation matching, so it
/// is used as-is.
#[cfg(target_os = "linux")]
fn stage_privileged_payload(
    payload_dir: &Path,
    script: &Path,
    helper: &Path,
    mihomo: &Path,
    unit: Option<&Path>,
) -> Result<PrivilegedPayload, String> {
    if !needs_staging(script) {
        return Ok(PrivilegedPayload {
            script: script.to_path_buf(),
            helper: helper.to_path_buf(),
            mihomo: mihomo.to_path_buf(),
            unit: unit.map(Path::to_path_buf),
            staged: false,
            root: payload_dir.to_path_buf(),
        });
    }
    if payload_dir.exists() {
        fs::remove_dir_all(payload_dir).map_err(|error| error.to_string())?;
    }
    fs::create_dir_all(payload_dir).map_err(|error| error.to_string())?;
    // 0700 keeps another unprivileged user from swapping the payload between
    // the polkit prompt and the elevated read.
    fs::set_permissions(payload_dir, fs::Permissions::from_mode(0o700))
        .map_err(|error| error.to_string())?;
    Ok(PrivilegedPayload {
        script: copy_payload_file(script, payload_dir, 0o755)?,
        helper: copy_payload_file(helper, payload_dir, 0o755)?,
        mihomo: copy_payload_file(mihomo, payload_dir, 0o755)?,
        unit: unit
            .map(|unit| copy_payload_file(unit, payload_dir, 0o644))
            .transpose()?,
        staged: true,
        root: payload_dir.to_path_buf(),
    })
}

/// `/usr/lib/biflow` is written by the `.deb` and owned by root.
#[cfg(target_os = "linux")]
#[must_use]
pub(crate) fn needs_staging(script: &Path) -> bool {
    !script.starts_with(LINUX_HELPER_ROOT)
}

#[cfg(target_os = "linux")]
fn copy_payload_file(source: &Path, directory: &Path, mode: u32) -> Result<PathBuf, String> {
    let name = source
        .file_name()
        .ok_or_else(|| format!("{} has no file name", source.display()))?;
    let destination = directory.join(name);
    fs::copy(source, &destination)
        .map_err(|error| format!("cannot stage {}: {error}", source.display()))?;
    fs::set_permissions(&destination, fs::Permissions::from_mode(mode))
        .map_err(|error| error.to_string())?;
    Ok(destination)
}

/// The hashes come from the packaged originals, so a copy that does not match
/// them is rejected before the operator is asked to authenticate. The elevated
/// script verifies the same digests again.
#[cfg(target_os = "linux")]
fn verify_staged_payload(
    payload: &PrivilegedPayload,
    helper_sha256: &str,
    mihomo_sha256: &str,
) -> Result<(), String> {
    if sha256_file(&payload.helper)? != helper_sha256 {
        return Err("staged helper binary failed checksum verification".into());
    }
    if sha256_file(&payload.mihomo)? != mihomo_sha256 {
        return Err("staged Mihomo binary failed checksum verification".into());
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn discard_staged_payload(payload: &PrivilegedPayload) {
    if !payload.staged {
        return;
    }
    if let Err(error) = fs::remove_dir_all(&payload.root) {
        error!(
            event = "helper.install_staging_cleanup_failed",
            section = "helper_install",
            initiator = "tauri_command",
            cause = %error,
            trace_route = "ui->tauri_command->install_helper->install_linux",
            "could not remove the staged helper payload"
        );
    }
}

/// The helper authorizes exactly one non-root uid, so a root-owned UI can never
/// be its client. Returns the operator-facing reason when `uid` is not usable.
#[cfg(target_os = "linux")]
#[must_use]
pub(crate) fn root_install_rejection(uid: u32) -> Option<&'static str> {
    if uid == 0 {
        Some(ROOT_APP_REJECTION)
    } else {
        None
    }
}

/// `install-helper.sh` and `PowerShell` both report why they stopped on their
/// last stderr line.
#[cfg(any(target_os = "linux", target_os = "windows"))]
#[must_use]
pub(crate) fn last_error_line(stderr: &[u8]) -> String {
    let text = String::from_utf8_lossy(stderr);
    let Some(line) = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .next_back()
    else {
        return String::new();
    };
    if line.chars().count() <= MAX_DETAIL_CHARS {
        return line.to_owned();
    }
    line.chars().take(MAX_DETAIL_CHARS).collect::<String>() + "…"
}

#[cfg(target_os = "windows")]
async fn install_windows(
    resource_root: &Path,
    exe_dir: &Path,
    staging_dir: &Path,
    tun_name: &str,
) -> Result<(), String> {
    let helper_src = first_existing_file(&windows_helper_candidates(resource_root, exe_dir))
        .ok_or_else(|| "packaged helper binary is missing".to_owned())?;
    let mihomo_src = first_existing_file(&windows_mihomo_candidates(resource_root, exe_dir))
        .ok_or_else(|| "packaged Mihomo binary is missing".to_owned())?;
    let script = elevate_script(
        &helper_src.to_string_lossy(),
        &format!(
            "--install --mihomo {} --staging-dir {} --tun-name {}",
            helper_src_arg(&mihomo_src),
            helper_src_arg(staging_dir),
            tun_name
        ),
    );
    // The desktop app is a `windows` subsystem binary (ADR 0030); without this
    // flag the elevation helper flashes a console window over the UI.
    let output = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .await
        .map_err(|error| error.to_string())?;
    if output.status.success() {
        return Ok(());
    }
    let code = output.status.code().unwrap_or(-1);
    if code == UAC_CANCELLED {
        return Err("helper installation was cancelled".into());
    }
    let detail = last_error_line(&output.stderr);
    error!(
        event = "helper.install_failed",
        section = "helper_install",
        initiator = "tauri_command",
        cause = "elevated_helper_failed",
        trace_route = "ui->tauri_command->install_helper->install_windows",
        exit_code = code,
        detail = %detail,
        "privileged helper installation failed"
    );
    if detail.is_empty() {
        Err(format!(
            "privileged helper installation failed (exit code {code})"
        ))
    } else {
        Err(format!("privileged helper installation failed: {detail}"))
    }
}

/// `Start-Process -Wait` alone reports `PowerShell`'s own exit code, so a helper
/// that failed — or a dismissed UAC prompt — still looked like success and the
/// install only surfaced later as an unreachable-helper timeout. Re-raise the
/// elevated process's exit code, and map a refused prompt to `ERROR_CANCELLED`.
#[cfg(target_os = "windows")]
#[must_use]
pub(crate) fn elevate_script(program: &str, arguments: &str) -> String {
    format!(
        "$ErrorActionPreference = 'Stop'; \
         try {{ $process = Start-Process -FilePath {} -ArgumentList {} -Verb RunAs -Wait -PassThru }} \
         catch {{ Write-Error $_.Exception.Message; exit {UAC_CANCELLED} }}; \
         exit $process.ExitCode",
        powershell_quote(program),
        powershell_quote(arguments),
    )
}

#[cfg(target_os = "linux")]
fn helper_binary_candidates(resource_root: &Path, exe_dir: &Path) -> Vec<PathBuf> {
    vec![
        PathBuf::from(LINUX_HELPER_ROOT).join("iran-split-helper"),
        resource_root.join("helper/iran-split-helper"),
        exe_dir.join("helper/iran-split-helper"),
        resource_root.join("_up_/resources/helper/iran-split-helper"),
    ]
}

#[cfg(target_os = "linux")]
fn mihomo_candidates(resource_root: &Path, exe_dir: &Path) -> Vec<PathBuf> {
    vec![
        PathBuf::from(LINUX_HELPER_ROOT).join("mihomo"),
        resource_root.join("dependencies/mihomo"),
        exe_dir.join("dependencies/mihomo"),
        resource_root.join("_up_/resources/dependencies/mihomo"),
    ]
}

#[cfg(target_os = "linux")]
fn install_script_candidates(resource_root: &Path, exe_dir: &Path) -> Vec<PathBuf> {
    vec![
        PathBuf::from(LINUX_HELPER_ROOT).join("install-helper.sh"),
        resource_root.join("helper/install-helper.sh"),
        exe_dir.join("helper/install-helper.sh"),
        resource_root.join("_up_/resources/helper/install-helper.sh"),
    ]
}

#[cfg(target_os = "linux")]
fn unit_candidates(resource_root: &Path, exe_dir: &Path) -> Vec<PathBuf> {
    vec![
        PathBuf::from(LINUX_HELPER_ROOT).join("iran-split-helper.service"),
        resource_root.join("helper/iran-split-helper.service"),
        exe_dir.join("helper/iran-split-helper.service"),
        resource_root.join("_up_/resources/helper/iran-split-helper.service"),
    ]
}

#[cfg(target_os = "windows")]
fn windows_helper_candidates(resource_root: &Path, exe_dir: &Path) -> Vec<PathBuf> {
    vec![
        PathBuf::from(WINDOWS_PROGRAMDATA_HELPER),
        resource_root.join("helper/iran-split-helper.exe"),
        exe_dir.join("helper/iran-split-helper.exe"),
        exe_dir.join("iran-split-helper.exe"),
    ]
}

#[cfg(target_os = "windows")]
fn windows_mihomo_candidates(resource_root: &Path, exe_dir: &Path) -> Vec<PathBuf> {
    vec![
        PathBuf::from(r"C:\ProgramData\iran-split\bin\mihomo.exe"),
        resource_root.join("dependencies/mihomo.exe"),
        exe_dir.join("dependencies/mihomo.exe"),
    ]
}

#[must_use]
pub(crate) fn first_existing_file(candidates: &[PathBuf]) -> Option<PathBuf> {
    candidates.iter().find(|path| path.is_file()).cloned()
}

#[cfg(target_os = "linux")]
fn sha256_file(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

#[cfg(target_os = "linux")]
fn current_linux_ids() -> Result<(u32, u32), String> {
    let status = fs::read_to_string("/proc/self/status").map_err(|error| error.to_string())?;
    parse_proc_status_ids(&status).ok_or_else(|| "could not read process uid/gid".into())
}

#[cfg(target_os = "linux")]
#[must_use]
pub(crate) fn parse_proc_status_ids(status: &str) -> Option<(u32, u32)> {
    let mut uid = None;
    let mut gid = None;
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("Uid:") {
            uid = rest.split_whitespace().next()?.parse().ok();
        }
        if let Some(rest) = line.strip_prefix("Gid:") {
            gid = rest.split_whitespace().next()?.parse().ok();
        }
    }
    Some((uid?, gid?))
}

#[cfg(target_os = "windows")]
fn powershell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(target_os = "windows")]
fn helper_src_arg(path: &Path) -> String {
    format!("\"{}\"", path.display())
}

#[cfg(test)]
mod tests {
    use super::first_existing_file;
    use std::fs;

    #[cfg(target_os = "windows")]
    use super::{elevate_script, UAC_CANCELLED};
    #[cfg(target_os = "linux")]
    use super::{
        helper_binary_candidates, needs_staging, parse_proc_status_ids, root_install_rejection,
        stage_privileged_payload, LINUX_HELPER_ROOT,
    };
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    use super::{last_error_line, MAX_DETAIL_CHARS};
    #[cfg(target_os = "linux")]
    use std::os::unix::fs::PermissionsExt;
    #[cfg(target_os = "linux")]
    use std::path::{Path, PathBuf};

    #[cfg(target_os = "linux")]
    #[test]
    fn parse_proc_status_ids_reads_real_and_effective() {
        let sample = "Uid:\t1000\t1000\t1000\t1000\nGid:\t1001\t1001\t1001\t1001\n";
        assert_eq!(parse_proc_status_ids(sample), Some((1000, 1001)));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn helper_candidates_prefer_system_install_root() {
        let resource = PathBuf::from("/tmp/resources");
        let exe = PathBuf::from("/tmp/app");
        let candidates = helper_binary_candidates(&resource, &exe);
        assert_eq!(
            candidates[0],
            PathBuf::from(LINUX_HELPER_ROOT).join("iran-split-helper")
        );
        assert!(candidates
            .iter()
            .any(|path| path.ends_with("helper/iran-split-helper")));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn root_cannot_be_the_authorized_helper_user() {
        let rejection = root_install_rejection(0).expect("root is rejected");
        assert!(rejection.contains("running as root"));
        assert!(rejection.contains("without sudo"));
        assert!(root_install_rejection(1000).is_none());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn only_a_root_owned_script_skips_staging() {
        assert!(!needs_staging(Path::new(
            "/usr/lib/biflow/install-helper.sh"
        )));
        assert!(needs_staging(Path::new(
            "/tmp/.mount_BiFlowAgjuzm/usr/lib/BiFlow/helper/install-helper.sh"
        )));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn staging_copies_the_payload_out_of_an_unreadable_mount() {
        let source = tempfile::tempdir().expect("tempdir");
        let destination = tempfile::tempdir().expect("tempdir");
        let payload_dir = destination.path().join("helper-install");
        for (name, body) in [
            ("install-helper.sh", "#!/bin/sh\n"),
            ("iran-split-helper", "helper"),
            ("mihomo", "mihomo"),
            ("iran-split-helper.service", "[Unit]\n"),
        ] {
            fs::write(source.path().join(name), body).expect("write");
        }
        let unit = source.path().join("iran-split-helper.service");
        let payload = stage_privileged_payload(
            &payload_dir,
            &source.path().join("install-helper.sh"),
            &source.path().join("iran-split-helper"),
            &source.path().join("mihomo"),
            Some(&unit),
        )
        .expect("stage");

        assert!(payload.staged);
        assert_eq!(payload.script, payload_dir.join("install-helper.sh"));
        assert_eq!(
            payload.unit,
            Some(payload_dir.join("iran-split-helper.service"))
        );
        assert_eq!(
            fs::read_to_string(&payload.helper).expect("read staged helper"),
            "helper"
        );
        let mode = |path: &Path| fs::metadata(path).expect("metadata").permissions().mode() & 0o777;
        assert_eq!(mode(&payload_dir), 0o700);
        assert_eq!(mode(&payload.script), 0o755);
        assert_eq!(mode(&payload.mihomo), 0o755);
        assert_eq!(mode(payload.unit.as_ref().expect("unit")), 0o644);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_deb_install_runs_the_root_owned_script_in_place() {
        let destination = tempfile::tempdir().expect("tempdir");
        let script = PathBuf::from(LINUX_HELPER_ROOT).join("install-helper.sh");
        let payload = stage_privileged_payload(
            &destination.path().join("helper-install"),
            &script,
            &PathBuf::from(LINUX_HELPER_ROOT).join("iran-split-helper"),
            &PathBuf::from(LINUX_HELPER_ROOT).join("mihomo"),
            None,
        )
        .expect("stage");

        assert!(!payload.staged);
        assert_eq!(payload.script, script);
        assert!(payload.unit.is_none());
        assert!(!destination.path().join("helper-install").exists());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn elevation_reraises_the_child_exit_code_and_maps_a_refused_prompt() {
        let script = elevate_script(r"C:\Program Files\BiFlow\helper.exe", "--install --tun x");
        assert!(script.contains("-PassThru"));
        assert!(script.contains("exit $process.ExitCode"));
        assert!(script.contains(&format!("exit {UAC_CANCELLED}")));
        assert!(script.contains("$ErrorActionPreference = 'Stop'"));
        assert!(script.contains(r"'C:\Program Files\BiFlow\helper.exe'"));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn elevation_escapes_single_quotes_in_paths() {
        let script = elevate_script(r"C:\Users\o'brien\helper.exe", "--install");
        assert!(script.contains(r"'C:\Users\o''brien\helper.exe'"));
    }

    #[cfg(any(target_os = "linux", target_os = "windows"))]
    #[test]
    fn last_error_line_reports_the_final_script_message() {
        assert_eq!(
            last_error_line(b"install -d ...\nauthorized-uid must not be root\n\n"),
            "authorized-uid must not be root"
        );
        assert_eq!(last_error_line(b"   \n\n"), "");
        let long = last_error_line(&vec![b'x'; MAX_DETAIL_CHARS + 50]);
        assert_eq!(long.chars().count(), MAX_DETAIL_CHARS + 1);
        assert!(long.ends_with('…'));
    }

    #[test]
    fn first_existing_file_skips_missing_paths() {
        let directory = tempfile::tempdir().expect("tempdir");
        let missing = directory.path().join("missing");
        let present = directory.path().join("present");
        fs::write(&present, b"ok").expect("write");
        assert_eq!(
            first_existing_file(&[missing, present.clone()]),
            Some(present)
        );
    }
}
