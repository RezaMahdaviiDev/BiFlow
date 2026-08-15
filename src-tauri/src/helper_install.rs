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
const LINUX_HELPER_ROOT: &str = "/usr/lib/biflow";
#[cfg(target_os = "linux")]
const PKEXEC: &str = "/usr/bin/pkexec";
/// `install-helper.sh` refuses `--authorized-uid 0`, so a root-owned UI can only
/// ever fail. Say why instead of forwarding that exit code as a generic failure.
#[cfg(target_os = "linux")]
const ROOT_APP_REJECTION: &str = "BiFlow is running as root, so the helper cannot be installed. The helper authorizes one non-root user. Quit BiFlow, start it as your normal user without sudo, then install the helper again.";
/// Keeps a runaway script error out of the dialog while preserving the reason.
#[cfg(target_os = "linux")]
const MAX_DETAIL_CHARS: usize = 200;
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
    info!(
        event = "helper.install_started",
        section = "helper_install",
        initiator = "tauri_command",
        cause = "user_requested",
        trace_route = "ui->tauri_command->install_helper",
        "privileged helper installation requested"
    );
    #[cfg(target_os = "linux")]
    install_linux(&resource_root, &exe_dir, &staging_dir, &tun_name).await?;
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
    let mut command = Command::new(PKEXEC);
    command
        .arg(&script)
        .arg("--authorized-uid")
        .arg(uid.to_string())
        .arg("--authorized-gid")
        .arg(gid.to_string())
        .arg("--staging-dir")
        .arg(staging_dir)
        .arg("--helper-src")
        .arg(&helper_src)
        .arg("--mihomo-src")
        .arg(&mihomo_src)
        .arg("--helper-sha256")
        .arg(&helper_sha256)
        .arg("--mihomo-sha256")
        .arg(&mihomo_sha256)
        .arg("--tun-name")
        .arg(tun_name);
    if let Some(unit) = unit {
        command.arg("--unit-src").arg(unit);
    }
    let output = command.output().await.map_err(|error| error.to_string())?;
    if output.status.success() {
        return Ok(());
    }
    if output.status.code() == Some(126) || output.status.code() == Some(127) {
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
        detail = %detail,
        "privileged helper installation failed"
    );
    if detail.is_empty() {
        Err("privileged helper installation failed".into())
    } else {
        Err(format!("privileged helper installation failed: {detail}"))
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

/// `install-helper.sh` reports the reason it stopped on its last stderr line.
#[cfg(target_os = "linux")]
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
    let status = Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            &format!(
                "Start-Process -FilePath {} -ArgumentList {} -Verb RunAs -Wait",
                powershell_quote(&helper_src.to_string_lossy()),
                powershell_quote(&format!(
                    "--install --mihomo {} --staging-dir {} --tun-name {}",
                    helper_src_arg(&mihomo_src),
                    helper_src_arg(staging_dir),
                    tun_name
                )),
            ),
        ])
        .status()
        .await
        .map_err(|error| error.to_string())?;
    if status.success() {
        return Ok(());
    }
    Err("privileged helper installation failed".into())
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

    #[cfg(target_os = "linux")]
    use super::{
        helper_binary_candidates, last_error_line, parse_proc_status_ids, root_install_rejection,
        LINUX_HELPER_ROOT, MAX_DETAIL_CHARS,
    };
    #[cfg(target_os = "linux")]
    use std::path::PathBuf;

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
