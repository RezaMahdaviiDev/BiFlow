//! GitHub Releases in-app updater, following the `DBack` About-page flow.
//!
//! Check uses the public Releases API. Install downloads the platform package
//! and applies it with `pkexec apt-get` (`.deb`), a replace helper (`AppImage`),
//! or an elevated NSIS installer (Windows). Do not log asset URLs.

use serde::Deserialize;
use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
};
use tokio::io::AsyncWriteExt;
use tracing::{info, warn};

pub const LATEST_RELEASE_API: &str = "https://api.github.com/repos/devlifeX/BiFlow/releases/latest";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallKind {
    Deb,
    AppImage,
    Nsis,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub tag_name: String,
    pub version: String,
    pub notes: String,
    pub html_url: String,
    pub assets: Vec<Asset>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateInfo {
    pub available: bool,
    pub current_version: String,
    pub latest_version: String,
    pub notes: String,
    pub asset: Option<Asset>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyOutcome {
    /// Linux `.deb`: package is installed; the running process is still old.
    ManualRestart,
    /// A helper script will replace this process after exit.
    HelperRestart,
}

#[must_use]
pub fn normalize_version(raw: &str) -> String {
    let trimmed = raw.trim().trim_start_matches('v').trim_start_matches('V');
    if trimmed.is_empty() {
        return "0.0.0".into();
    }
    let mut parts: Vec<&str> = trimmed.split('.').take(3).collect();
    while parts.len() < 3 {
        parts.push("0");
    }
    parts.join(".")
}

#[must_use]
pub fn compare_versions(left: &str, right: &str) -> i32 {
    let left_parts = normalize_version(left);
    let right_parts = normalize_version(right);
    let left_nums = version_parts(&left_parts);
    let right_nums = version_parts(&right_parts);
    for index in 0..3 {
        if left_nums[index] < right_nums[index] {
            return -1;
        }
        if left_nums[index] > right_nums[index] {
            return 1;
        }
    }
    0
}

#[must_use]
pub fn is_newer(current: &str, candidate: &str) -> bool {
    compare_versions(candidate, current) > 0
}

fn version_parts(normalized: &str) -> [u64; 3] {
    let mut parts = [0_u64; 3];
    for (index, piece) in normalized.split('.').take(3).enumerate() {
        parts[index] = piece.parse().unwrap_or(0);
    }
    parts
}

#[must_use]
pub fn user_agent(current_version: &str) -> String {
    format!("BiFlow/{}", normalize_version(current_version))
}

#[must_use]
pub fn install_kind_from(appimage: Option<&OsStr>, windows: bool) -> InstallKind {
    if windows {
        InstallKind::Nsis
    } else if appimage.is_some() {
        InstallKind::AppImage
    } else {
        InstallKind::Deb
    }
}

#[must_use]
pub fn detect_install_kind() -> InstallKind {
    install_kind_from(
        std::env::var_os("APPIMAGE").as_deref(),
        cfg!(target_os = "windows"),
    )
}

/// Picks the GitHub Release asset for this install kind.
///
/// # Errors
///
/// Returns an error when the release has no matching `.deb`, `AppImage`, or NSIS
/// installer.
pub fn pick_asset(release: &Release, kind: InstallKind) -> Result<Asset, String> {
    let version = &release.version;
    let exact = match kind {
        InstallKind::Deb => format!("BiFlow_{version}_amd64.deb"),
        InstallKind::AppImage => format!("BiFlow_{version}_amd64.AppImage"),
        InstallKind::Nsis => format!("BiFlow_{version}_x64-setup.exe"),
    };
    if let Some(asset) = release.assets.iter().find(|asset| asset.name == exact) {
        return Ok(asset.clone());
    }
    let fallback = release.assets.iter().find(|asset| {
        let name = asset.name.to_ascii_lowercase();
        match kind {
            InstallKind::Deb => name.starts_with("biflow_") && name.ends_with("_amd64.deb"),
            InstallKind::AppImage => {
                name.starts_with("biflow_") && name.ends_with("_amd64.appimage")
            }
            InstallKind::Nsis => name.starts_with("biflow_") && name.ends_with("_x64-setup.exe"),
        }
    });
    fallback.cloned().ok_or_else(|| {
        "no update package for this platform is attached to the latest GitHub Release".into()
    })
}

#[derive(Deserialize)]
struct GithubReleasePayload {
    tag_name: Option<String>,
    body: Option<String>,
    html_url: Option<String>,
    #[serde(default)]
    assets: Vec<GithubAssetPayload>,
}

#[derive(Deserialize)]
struct GithubAssetPayload {
    name: Option<String>,
    browser_download_url: Option<String>,
    #[serde(default)]
    size: u64,
}

/// Parses a GitHub Releases JSON body.
///
/// # Errors
///
/// Returns an error when JSON is invalid or `tag_name` is missing.
pub fn parse_release(body: &[u8]) -> Result<Release, String> {
    let payload: GithubReleasePayload =
        serde_json::from_slice(body).map_err(|error| error.to_string())?;
    let tag_name = payload.tag_name.unwrap_or_default();
    let version = normalize_version(&tag_name);
    if version == "0.0.0" && tag_name.is_empty() {
        return Err("github release response missing tag_name".into());
    }
    let notes = payload.body.unwrap_or_default();
    let notes = notes.trim();
    let notes = if notes.chars().count() > 300 {
        let clipped: String = notes.chars().take(300).collect();
        format!("{clipped}…")
    } else {
        notes.to_owned()
    };
    Ok(Release {
        tag_name,
        version,
        notes,
        html_url: payload.html_url.unwrap_or_default(),
        assets: payload
            .assets
            .into_iter()
            .filter_map(|asset| {
                Some(Asset {
                    name: asset.name?,
                    url: asset.browser_download_url?,
                    size: asset.size,
                })
            })
            .collect(),
    })
}

/// Fetches `/releases/latest` and decides whether `current_version` is behind.
///
/// # Errors
///
/// Returns a network, HTTP, or parse error. A newer tag without a platform
/// asset is also an error, matching `DBack`.
pub async fn check(
    current_version: &str,
    kind: InstallKind,
    timeout: std::time::Duration,
) -> Result<UpdateInfo, String> {
    let current_version = {
        let trimmed = current_version.trim();
        if trimmed.is_empty() {
            "0.0.0"
        } else {
            trimmed
        }
    };
    let release = fetch_latest(current_version, timeout).await?;
    let mut info = UpdateInfo {
        available: is_newer(current_version, &release.version),
        current_version: normalize_version(current_version),
        latest_version: release.version.clone(),
        notes: release.notes.clone(),
        asset: None,
    };
    if !info.available {
        return Ok(info);
    }
    info.asset = Some(pick_asset(&release, kind)?);
    Ok(info)
}

async fn fetch_latest(
    current_version: &str,
    timeout: std::time::Duration,
) -> Result<Release, String> {
    let client = reqwest::Client::builder()
        .timeout(timeout)
        .user_agent(user_agent(current_version))
        .build()
        .map_err(|error| error.to_string())?;
    let response = client
        .get(LATEST_RELEASE_API)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|error| error.to_string())?;
    let status = response.status();
    let bytes = response.bytes().await.map_err(|error| error.to_string())?;
    if !status.is_success() {
        return Err(format!(
            "github releases API HTTP {status}: could not read the latest release"
        ));
    }
    parse_release(&bytes)
}

/// Downloads `asset` into `{temp}/biflow-update/{name}`.
///
/// # Errors
///
/// Returns an error when the HTTP body is empty or cannot be written.
pub async fn download_asset(
    current_version: &str,
    asset: &Asset,
    timeout: std::time::Duration,
    mut on_progress: impl FnMut(u64, Option<u64>),
    mut should_cancel: impl FnMut() -> bool,
) -> Result<PathBuf, String> {
    let dest_dir = std::env::temp_dir().join("biflow-update");
    tokio::fs::create_dir_all(&dest_dir)
        .await
        .map_err(|error| error.to_string())?;
    let dest = dest_dir.join(&asset.name);
    let part = dest_dir.join(format!("{}.part", asset.name));
    let client = reqwest::Client::builder()
        .timeout(timeout)
        .user_agent(user_agent(current_version))
        .build()
        .map_err(|error| error.to_string())?;
    let mut response = client
        .get(&asset.url)
        .send()
        .await
        .map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        return Err("update package download failed".into());
    }
    let total = response.content_length();
    let mut file = tokio::fs::File::create(&part)
        .await
        .map_err(|error| error.to_string())?;
    let mut written = 0_u64;
    while let Some(chunk) = response.chunk().await.map_err(|error| error.to_string())? {
        if should_cancel() {
            drop(file);
            remove_partial(&part).await;
            return Err("update download cancelled".into());
        }
        file.write_all(&chunk)
            .await
            .map_err(|error| error.to_string())?;
        written = written.saturating_add(u64::try_from(chunk.len()).unwrap_or(u64::MAX));
        on_progress(written, total);
    }
    file.flush().await.map_err(|error| error.to_string())?;
    drop(file);
    if written == 0 {
        remove_partial(&part).await;
        return Err("downloaded update package is empty".into());
    }
    tokio::fs::rename(&part, &dest)
        .await
        .map_err(|error| error.to_string())?;
    Ok(dest)
}

async fn remove_partial(part: &Path) {
    if let Err(cause) = tokio::fs::remove_file(part).await {
        warn!(
            event = "update.partial_cleanup_failed",
            section = "updates",
            initiator = "github_update",
            cause = %cause,
            trace_route = "github_update->temp_file",
            "incomplete update download could not be removed"
        );
    }
}

/// Applies a downloaded package. The caller must pause the stack first.
///
/// # Errors
///
/// Returns a platform error when `pkexec`, the helper script, or NSIS cannot
/// start.
pub async fn apply_package(
    kind: InstallKind,
    package: &Path,
    current_exe: &Path,
) -> Result<ApplyOutcome, String> {
    match kind {
        InstallKind::Deb => {
            install_deb(package).await?;
            Ok(ApplyOutcome::ManualRestart)
        }
        InstallKind::AppImage => {
            install_appimage(package).await?;
            Ok(ApplyOutcome::HelperRestart)
        }
        InstallKind::Nsis => {
            install_nsis(package, current_exe).await?;
            Ok(ApplyOutcome::HelperRestart)
        }
    }
}

async fn install_deb(package: &Path) -> Result<(), String> {
    info!(
        event = "update.deb_install_started",
        section = "updates",
        initiator = "github_update",
        cause = "linux_deb",
        trace_route = "tauri_command->github_update->pkexec",
        "installing the downloaded .deb with pkexec apt-get"
    );
    let output = tokio::process::Command::new("pkexec")
        .args([
            "env",
            "DEBIAN_FRONTEND=noninteractive",
            "apt-get",
            "install",
            "-y",
        ])
        .arg(package)
        .output()
        .await
        .map_err(|error| error.to_string())?;
    if output.status.success() {
        Ok(())
    } else if output.status.code() == Some(126) {
        Err("package install was cancelled".into())
    } else {
        Err("package install failed".into())
    }
}

async fn install_appimage(package: &Path) -> Result<(), String> {
    let Some(appimage) = std::env::var_os("APPIMAGE") else {
        return Err("APPIMAGE is not set".into());
    };
    let dest = PathBuf::from(appimage);
    let script = std::env::temp_dir()
        .join("biflow-update")
        .join("apply-update.sh");
    if let Some(parent) = script.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|error| error.to_string())?;
    }
    let body = format!(
        "#!/bin/sh\nPID={}\nSRC={}\nDST={}\nwhile kill -0 \"$PID\" 2>/dev/null; do sleep 1; done\nmv -f \"$SRC\" \"$DST\"\nchmod +x \"$DST\"\nexec \"$DST\"\n",
        std::process::id(),
        sh_single_quote(&package.to_string_lossy()),
        sh_single_quote(&dest.to_string_lossy()),
    );
    tokio::fs::write(&script, body)
        .await
        .map_err(|error| error.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = tokio::fs::metadata(&script)
            .await
            .map_err(|error| error.to_string())?
            .permissions();
        permissions.set_mode(0o700);
        tokio::fs::set_permissions(&script, permissions)
            .await
            .map_err(|error| error.to_string())?;
    }
    tokio::process::Command::new("sh")
        .arg(&script)
        .spawn()
        .map_err(|error| error.to_string())?;
    Ok(())
}

async fn install_nsis(package: &Path, current_exe: &Path) -> Result<(), String> {
    let script = std::env::temp_dir()
        .join("biflow-update")
        .join("apply-update.bat");
    let dest = current_exe.to_string_lossy().replace('"', "");
    let src = package.to_string_lossy().replace('"', "");
    let body = format!(
        "@echo off\r\nsetlocal\r\nset PID={pid}\r\n:wait\r\ntasklist /FI \"PID eq %PID%\" 2>NUL | find /I \"%PID%\" >NUL\r\nif not errorlevel 1 (\r\n  timeout /t 1 /nobreak >NUL\r\n  goto wait\r\n)\r\npowershell -NoProfile -NonInteractive -Command \"Start-Process -FilePath '{src}' -ArgumentList '/S' -Verb RunAs -Wait\"\r\nstart \"\" \"{dest}\"\r\ndel \"%~f0\"\r\n",
        pid = std::process::id(),
        src = src.replace('\'', "''"),
        dest = dest,
    );
    if let Some(parent) = script.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|error| error.to_string())?;
    }
    tokio::fs::write(&script, body)
        .await
        .map_err(|error| error.to_string())?;
    let script_path = script.to_string_lossy().into_owned();
    let mut command = tokio::process::Command::new("cmd");
    command.args(["/C", "start", "", &script_path]);
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command.spawn().map_err(|error| error.to_string())?;
    warn!(
        event = "update.nsis_helper_started",
        section = "updates",
        initiator = "github_update",
        cause = "windows_nsis",
        trace_route = "tauri_command->github_update->apply_helper",
        "NSIS helper will install after this process exits"
    );
    Ok(())
}

fn sh_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_strips_v_and_pads() {
        assert_eq!(normalize_version("v3.6"), "3.6.0");
        assert_eq!(normalize_version("3.5.0"), "3.5.0");
        assert_eq!(normalize_version(""), "0.0.0");
    }

    #[test]
    fn is_newer_requires_a_greater_triple() {
        assert!(is_newer("3.5.0", "3.6.0"));
        assert!(!is_newer("3.6.0", "3.6.0"));
        assert!(!is_newer("3.6.0", "3.5.9"));
    }

    #[test]
    fn parse_release_reads_tag_and_assets() {
        let release = parse_release(
            br#"{
              "tag_name": "v3.6.0",
              "body": "notes",
              "html_url": "https://github.com/devlifeX/BiFlow/releases/tag/v3.6.0",
              "assets": [
                {
                  "name": "BiFlow_3.6.0_amd64.deb",
                  "browser_download_url": "https://example.invalid/BiFlow_3.6.0_amd64.deb",
                  "size": 12
                }
              ]
            }"#,
        )
        .expect("release");
        assert_eq!(release.version, "3.6.0");
        assert!(release.html_url.contains("releases/tag"));
        let asset = pick_asset(&release, InstallKind::Deb).expect("deb");
        assert_eq!(asset.name, "BiFlow_3.6.0_amd64.deb");
    }

    #[test]
    fn pick_asset_falls_back_to_platform_suffix() {
        let release = Release {
            tag_name: "v9.9.9".into(),
            version: "9.9.9".into(),
            notes: String::new(),
            html_url: String::new(),
            assets: vec![Asset {
                name: "BiFlow_9.9.9_x64-setup.exe".into(),
                url: "https://example.invalid/setup.exe".into(),
                size: 1,
            }],
        };
        assert!(pick_asset(&release, InstallKind::Deb).is_err());
        let nsis = pick_asset(&release, InstallKind::Nsis).expect("nsis");
        assert!(nsis.name.ends_with("_x64-setup.exe"));
    }

    #[test]
    fn install_kind_prefers_appimage_env_on_linux() {
        assert_eq!(
            install_kind_from(Some(OsStr::new("/tmp/BiFlow.AppImage")), false),
            InstallKind::AppImage
        );
        assert_eq!(install_kind_from(None, false), InstallKind::Deb);
        assert_eq!(install_kind_from(None, true), InstallKind::Nsis);
    }
}
