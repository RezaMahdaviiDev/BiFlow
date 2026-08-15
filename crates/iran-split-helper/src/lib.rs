use iran_split_ipc::{CleanupReport, ProcessStatus, ServiceLogEntry};
use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    fs, io,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{Arc, LazyLock},
    time::Duration,
};
use thiserror::Error;
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::{Child, Command},
    sync::Mutex,
};
use uuid::Uuid;

const PROCESS_STOP_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_LOG_ENTRIES: usize = 2_000;
const GENERATION_FILES: [&str; 8] = [
    "config.yaml",
    "private.txt",
    "iran-domains.txt",
    "iran-networks.txt",
    "custom-direct-domains.txt",
    "custom-direct-ips.txt",
    "custom-vpn-domains.txt",
    "custom-vpn-ips.txt",
];

#[derive(Debug, Error)]
pub enum HelperServiceError {
    #[error("helper configuration I/O failed: {0}")]
    Io(#[from] io::Error),
    #[error("helper configuration is invalid TOML: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("helper configuration is unsafe: {0}")]
    UnsafeConfig(String),
    #[error("runtime generation is invalid: {0}")]
    InvalidGeneration(String),
    #[error("Mihomo binary integrity check failed")]
    BinaryIntegrity,
    #[error("Mihomo process failed: {0}")]
    Process(String),
    #[error("IPC failed: {0}")]
    Protocol(#[from] iran_split_ipc::ProtocolError),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HelperSettings {
    pub authorized_uid: u32,
    #[serde(default)]
    pub authorized_gid: u32,
    pub socket_path: PathBuf,
    pub staging_dir: PathBuf,
    pub runtime_dir: PathBuf,
    pub mihomo_binary: PathBuf,
    pub mihomo_sha256: String,
    pub tun_name: String,
}

impl HelperSettings {
    /// Loads and validates a root-owned helper configuration file.
    ///
    /// # Errors
    ///
    /// Returns an error when the path is unsafe, its contents cannot be read or
    /// decoded, or a setting violates the helper's security constraints.
    pub fn load(path: &Path) -> Result<Self, HelperServiceError> {
        let metadata = fs::symlink_metadata(path)?;
        if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
            return Err(HelperServiceError::UnsafeConfig(
                "helper config must be a regular, non-symlink file".into(),
            ));
        }
        #[cfg(unix)]
        validate_root_owned_private_file(&metadata)?;
        let settings: Self = toml::from_str(&fs::read_to_string(path)?)?;
        settings.validate()?;
        Ok(settings)
    }

    pub(crate) fn validate(&self) -> Result<(), HelperServiceError> {
        for (name, path) in [
            ("socket_path", &self.socket_path),
            ("staging_dir", &self.staging_dir),
            ("runtime_dir", &self.runtime_dir),
            ("mihomo_binary", &self.mihomo_binary),
        ] {
            if !path.is_absolute() || path.components().any(|part| part.as_os_str() == "..") {
                return Err(HelperServiceError::UnsafeConfig(format!(
                    "{name} must be an absolute normalized path"
                )));
            }
        }
        if self.staging_dir.starts_with(&self.runtime_dir)
            || self.runtime_dir.starts_with(&self.staging_dir)
        {
            return Err(HelperServiceError::UnsafeConfig(
                "staging and system runtime directories must not contain one another".into(),
            ));
        }
        if !valid_sha256(&self.mihomo_sha256) {
            return Err(HelperServiceError::UnsafeConfig(
                "mihomo_sha256 must be lowercase SHA-256".into(),
            ));
        }
        if self.tun_name.is_empty()
            || self.tun_name.len() > 15
            || !self
                .tun_name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
        {
            return Err(HelperServiceError::UnsafeConfig(
                "tun_name must be 1-15 safe interface-name characters".into(),
            ));
        }
        Ok(())
    }
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(unix)]
fn validate_root_owned_private_file(metadata: &fs::Metadata) -> Result<(), HelperServiceError> {
    use std::os::unix::fs::MetadataExt;
    if metadata.uid() != 0 || metadata.mode() & 0o022 != 0 {
        return Err(HelperServiceError::UnsafeConfig(
            "helper config must be root-owned and not group/world writable".into(),
        ));
    }
    Ok(())
}

#[derive(Debug)]
struct ManagedChild {
    child: Child,
    generation_id: Uuid,
    started_at: String,
}

#[derive(Debug)]
pub struct Supervisor {
    settings: HelperSettings,
    child: Mutex<Option<ManagedChild>>,
    registered: Mutex<HashMap<Uuid, String>>,
    logs: Arc<Mutex<VecDeque<ServiceLogEntry>>>,
}

impl Supervisor {
    #[must_use]
    pub fn new(settings: HelperSettings) -> Self {
        Self {
            settings,
            child: Mutex::new(None),
            registered: Mutex::new(HashMap::new()),
            logs: Arc::new(Mutex::new(VecDeque::with_capacity(MAX_LOG_ENTRIES))),
        }
    }

    #[must_use]
    pub const fn settings(&self) -> &HelperSettings {
        &self.settings
    }

    /// Validates and publishes an immutable runtime generation.
    ///
    /// # Errors
    ///
    /// Returns an error when the generation path, files, or hash are invalid,
    /// or publishing the generation fails.
    pub async fn register_generation(
        &self,
        generation_id: Uuid,
        expected_sha256: &str,
    ) -> Result<(), HelperServiceError> {
        if !valid_sha256(expected_sha256) {
            return Err(HelperServiceError::InvalidGeneration(
                "invalid config SHA-256".into(),
            ));
        }
        let source_root = self.settings.staging_dir.join(generation_id.to_string());
        // A bare `?` here reports only "cannot find the file specified", which
        // hides the one fact that matters: which staging root the helper was
        // configured with. That path is recorded in helper.toml at install
        // time, so it is wrong for every later run whenever the installing
        // process saw a different profile than the app does now.
        let source_root_canonical = source_root.canonicalize().map_err(|error| {
            HelperServiceError::InvalidGeneration(format!(
                "staged generation {} is unreadable at {}: {error}",
                generation_id,
                source_root.display()
            ))
        })?;
        let staging_canonical = self.settings.staging_dir.canonicalize().map_err(|error| {
            HelperServiceError::InvalidGeneration(format!(
                "configured staging directory {} is unreadable: {error}",
                self.settings.staging_dir.display()
            ))
        })?;
        if !source_root_canonical.starts_with(&staging_canonical) {
            return Err(HelperServiceError::InvalidGeneration(
                "generation escaped staging directory".into(),
            ));
        }
        let config_source = checked_generation_file(&source_root_canonical, "config.yaml")?;
        let actual_hash = sha256_file(&config_source)?;
        if actual_hash != expected_sha256 {
            return Err(HelperServiceError::InvalidGeneration(
                "config hash does not match request".into(),
            ));
        }

        let generations_root = self.settings.runtime_dir.join("generations");
        fs::create_dir_all(&generations_root)?;
        let temporary_root = generations_root.join(format!(".{generation_id}.staging"));
        if temporary_root.exists() {
            fs::remove_dir_all(&temporary_root)?;
        }
        fs::create_dir(&temporary_root)?;
        #[cfg(unix)]
        set_directory_permissions(&temporary_root)?;
        #[cfg(not(unix))]
        set_directory_permissions(&temporary_root);
        for name in GENERATION_FILES {
            let source = checked_generation_file(&source_root_canonical, name)?;
            let destination = temporary_root.join(name);
            copy_new_file(&source, &destination)?;
        }
        let destination_root = generations_root.join(generation_id.to_string());
        if destination_root.exists() {
            fs::remove_dir_all(&destination_root)?;
        }
        fs::rename(&temporary_root, &destination_root)?;
        self.registered
            .lock()
            .await
            .insert(generation_id, expected_sha256.into());
        self.push_log("info", "runtime_generation_registered", BTreeMap::new())
            .await;
        Ok(())
    }

    /// Starts Mihomo from a previously registered runtime generation.
    ///
    /// # Errors
    ///
    /// Returns an error when the binary fails integrity checks, the generation
    /// is not registered, or the child process cannot be managed.
    pub async fn start(
        &self,
        generation_id: Uuid,
        expected_sha256: &str,
    ) -> Result<ProcessStatus, HelperServiceError> {
        self.verify_binary()?;
        let registered = self.registered.lock().await;
        if registered.get(&generation_id).map(String::as_str) != Some(expected_sha256) {
            return Err(HelperServiceError::InvalidGeneration(
                "generation must be registered with the same hash before start".into(),
            ));
        }
        drop(registered);

        let mut current = self.child.lock().await;
        if let Some(managed) = current.as_mut() {
            if managed.child.try_wait()?.is_none() && managed.generation_id == generation_id {
                return Ok(process_status(Some(managed)));
            }
            stop_managed_child(managed).await?;
            *current = None;
        }

        let generation_root = self
            .settings
            .runtime_dir
            .join("generations")
            .join(generation_id.to_string());
        let config_path = checked_generation_file(&generation_root, "config.yaml")?;
        if sha256_file(&config_path)? != expected_sha256 {
            return Err(HelperServiceError::InvalidGeneration(
                "published config changed after registration".into(),
            ));
        }
        let mut child = Command::new(&self.settings.mihomo_binary)
            .arg("-d")
            .arg(&generation_root)
            .arg("-f")
            .arg(&config_path)
            .current_dir(&generation_root)
            .env_clear()
            .env(
                "PATH",
                if cfg!(windows) {
                    r"C:\Windows\System32;C:\Windows"
                } else {
                    "/usr/sbin:/usr/bin:/sbin:/bin"
                },
            )
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(false)
            .spawn()
            .map_err(|error| HelperServiceError::Process(error.to_string()))?;
        if let Some(stdout) = child.stdout.take() {
            capture_lines(stdout, Arc::clone(&self.logs), "info");
        }
        if let Some(stderr) = child.stderr.take() {
            capture_lines(stderr, Arc::clone(&self.logs), "warn");
        }
        let managed = ManagedChild {
            child,
            generation_id,
            started_at: now_string(),
        };
        let status = process_status(Some(&managed));
        *current = Some(managed);
        drop(current);
        self.push_log("info", "mihomo_started", BTreeMap::new())
            .await;
        Ok(status)
    }

    /// Stops the helper-owned Mihomo process.
    ///
    /// # Errors
    ///
    /// Returns an error when the process cannot be inspected or stopped.
    pub async fn stop(&self) -> Result<ProcessStatus, HelperServiceError> {
        let mut current = self.child.lock().await;
        if let Some(managed) = current.as_mut() {
            stop_managed_child(managed).await?;
        }
        *current = None;
        drop(current);
        self.push_log("info", "mihomo_stopped", BTreeMap::new())
            .await;
        Ok(ProcessStatus {
            running: false,
            pid: None,
            generation_id: None,
            started_at: None,
        })
    }

    /// Returns the current helper-owned process status.
    ///
    /// # Errors
    ///
    /// Returns an error when the child process status cannot be inspected.
    pub async fn status(&self) -> Result<ProcessStatus, HelperServiceError> {
        let mut current = self.child.lock().await;
        if let Some(managed) = current.as_mut() {
            if managed.child.try_wait()?.is_some() {
                *current = None;
            }
        }
        Ok(process_status(current.as_ref()))
    }

    /// Stops Mihomo and removes the helper-owned network interface.
    ///
    /// # Errors
    ///
    /// Returns an error when the process or network cleanup fails.
    pub async fn cleanup(&self) -> Result<CleanupReport, HelperServiceError> {
        let process_stopped = !self.stop().await?.running;
        let interface_path = Path::new("/sys/class/net").join(&self.settings.tun_name);
        if interface_path.exists() {
            #[cfg(unix)]
            delete_owned_interface(&self.settings.tun_name).await?;
            #[cfg(not(unix))]
            delete_owned_interface(&self.settings.tun_name);
        }
        let tun_removed = !interface_path.exists();
        let warnings = if tun_removed {
            Vec::new()
        } else {
            vec![format!(
                "owned interface {} remains after cleanup",
                self.settings.tun_name
            )]
        };
        let report = CleanupReport {
            process_stopped,
            tun_removed,
            routes_removed: 0,
            dns_restored: true,
            warnings,
        };
        self.push_log("info", "network_cleanup_finished", BTreeMap::new())
            .await;
        Ok(report)
    }

    pub async fn logs(&self, maximum: usize) -> Vec<ServiceLogEntry> {
        let logs = self.logs.lock().await;
        logs.iter()
            .rev()
            .take(maximum.min(MAX_LOG_ENTRIES))
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    }

    fn verify_binary(&self) -> Result<(), HelperServiceError> {
        let metadata = fs::symlink_metadata(&self.settings.mihomo_binary)?;
        if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
            return Err(HelperServiceError::BinaryIntegrity);
        }
        if sha256_file(&self.settings.mihomo_binary)? != self.settings.mihomo_sha256 {
            return Err(HelperServiceError::BinaryIntegrity);
        }
        Ok(())
    }

    async fn push_log(&self, level: &str, event: &str, fields: BTreeMap<String, String>) {
        let mut logs = self.logs.lock().await;
        if logs.len() == MAX_LOG_ENTRIES {
            logs.pop_front();
        }
        logs.push_back(ServiceLogEntry {
            timestamp: now_string(),
            level: level.into(),
            event: event.into(),
            fields,
        });
    }
}

fn process_status(managed: Option<&ManagedChild>) -> ProcessStatus {
    managed.map_or(
        ProcessStatus {
            running: false,
            pid: None,
            generation_id: None,
            started_at: None,
        },
        |managed| ProcessStatus {
            running: true,
            pid: managed.child.id(),
            generation_id: Some(managed.generation_id),
            started_at: Some(managed.started_at.clone()),
        },
    )
}

async fn stop_managed_child(managed: &mut ManagedChild) -> Result<(), HelperServiceError> {
    if managed.child.try_wait()?.is_some() {
        return Ok(());
    }
    managed
        .child
        .start_kill()
        .map_err(|error| HelperServiceError::Process(error.to_string()))?;
    tokio::time::timeout(PROCESS_STOP_TIMEOUT, managed.child.wait())
        .await
        .map_err(|_| HelperServiceError::Process("process stop timed out".into()))?
        .map_err(|error| HelperServiceError::Process(error.to_string()))?;
    Ok(())
}

fn checked_generation_file(root: &Path, name: &str) -> Result<PathBuf, HelperServiceError> {
    if !GENERATION_FILES.contains(&name) {
        return Err(HelperServiceError::InvalidGeneration(
            "file is not in the runtime allowlist".into(),
        ));
    }
    let path = root.join(name);
    let metadata = fs::symlink_metadata(&path)?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(HelperServiceError::InvalidGeneration(format!(
            "{name} must be a regular non-symlink file"
        )));
    }
    let canonical = path.canonicalize()?;
    let canonical_root = root.canonicalize()?;
    if !canonical.starts_with(canonical_root) {
        return Err(HelperServiceError::InvalidGeneration(format!(
            "{name} escaped the generation directory"
        )));
    }
    if metadata.len() > 4 * 1024 * 1024 {
        return Err(HelperServiceError::InvalidGeneration(format!(
            "{name} exceeds the 4 MiB per-file limit"
        )));
    }
    Ok(canonical)
}

fn sha256_file(path: &Path) -> Result<String, HelperServiceError> {
    let bytes = fs::read(path)?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

/// Copies `source` onto `destination` unless they already resolve to the same
/// file. Windows `fs::copy` onto self fails; a leftover `ProgramData` helper
/// used as the elevate source hits that path.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn copy_file_unless_same(
    source: &Path,
    destination: &Path,
) -> Result<(), HelperServiceError> {
    if paths_refer_to_same_file(source, destination) {
        return Ok(());
    }
    fs::copy(source, destination)?;
    Ok(())
}

#[cfg_attr(not(windows), allow(dead_code))]
fn paths_refer_to_same_file(left: &Path, right: &Path) -> bool {
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

fn copy_new_file(source: &Path, destination: &Path) -> Result<(), HelperServiceError> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    let mut output = options.open(destination)?;
    let mut input = fs::File::open(source)?;
    io::copy(&mut input, &mut output)?;
    output.sync_all()?;
    #[cfg(unix)]
    set_file_permissions(destination)?;
    #[cfg(not(unix))]
    set_file_permissions(destination);
    Ok(())
}

#[cfg(unix)]
fn set_file_permissions(path: &Path) -> Result<(), HelperServiceError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_file_permissions(_path: &Path) {}

#[cfg(unix)]
fn set_directory_permissions(path: &Path) -> Result<(), HelperServiceError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_directory_permissions(_path: &Path) {}

fn now_string() -> String {
    // UTC RFC3339 without pulling wall-clock parsing into the IPC contract.
    format!(
        "{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |value| value.as_secs())
    )
}

fn capture_lines<R>(reader: R, logs: Arc<Mutex<VecDeque<ServiceLogEntry>>>, level: &'static str)
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let mut lines = BufReader::new(reader).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let mut entries = logs.lock().await;
            if entries.len() == MAX_LOG_ENTRIES {
                entries.pop_front();
            }
            entries.push_back(ServiceLogEntry {
                timestamp: now_string(),
                level: level.into(),
                event: "mihomo_output".into(),
                fields: BTreeMap::from([("message".into(), redact(&line))]),
            });
        }
    });
}

static SENSITIVE_VALUE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(authorization|bearer|secret|token|password)(\s*[:=]\s*|\s+)[^\s,;]+")
        .expect("redaction regex is valid")
});
static SUBSCRIPTION_URL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)https?://[^\s]+(?:sub|subscription|token)[^\s]*")
        .expect("subscription regex is valid")
});

#[must_use]
pub fn redact(input: &str) -> String {
    let bounded: String = input.chars().take(8_192).collect();
    let value = SENSITIVE_VALUE.replace_all(&bounded, "$1=[REDACTED]");
    SUBSCRIPTION_URL
        .replace_all(&value, "[REDACTED_URL]")
        .into_owned()
}

#[cfg(unix)]
async fn delete_owned_interface(name: &str) -> Result<(), HelperServiceError> {
    let binary = [Path::new("/usr/sbin/ip"), Path::new("/usr/bin/ip")]
        .into_iter()
        .find(|path| path.is_file())
        .ok_or_else(|| HelperServiceError::Process("ip utility was not found".into()))?;
    let output = Command::new(binary)
        .args(["link", "delete", "dev", name])
        .env_clear()
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .await?;
    if !output.status.success() {
        return Err(HelperServiceError::Process(
            String::from_utf8_lossy(&output.stderr)
                .chars()
                .take(1_024)
                .collect(),
        ));
    }
    Ok(())
}

#[cfg(not(unix))]
fn delete_owned_interface(_name: &str) {}

mod commands;

#[cfg(unix)]
mod linux;

#[cfg(unix)]
pub use linux::run_linux;

#[cfg(windows)]
mod windows;

#[cfg(windows)]
pub use windows::{install, persist_install_error, run_named_pipe, uninstall};

/// Safe `install.log` text for a clap parse failure. Omits argv and paths —
/// those are user content. Help and version are not install failures.
#[must_use]
pub fn clap_install_error_message(error: &clap::Error) -> Option<String> {
    use clap::error::ErrorKind;
    match error.kind() {
        ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => None,
        ErrorKind::UnknownArgument => Some("unexpected argument".into()),
        ErrorKind::MissingRequiredArgument
        | ErrorKind::InvalidValue
        | ErrorKind::ValueValidation => Some("missing or invalid install argument".into()),
        _ => Some("helper argument error".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redaction_removes_common_credentials_and_bounds_output() {
        let input = format!(
            "Authorization: Bearer-abc token=secret {}",
            "x".repeat(9_000)
        );
        let output = redact(&input);
        assert!(!output.contains("Bearer-abc"));
        assert!(!output.contains("token=secret"));
        assert!(output.len() < input.len());
    }

    #[test]
    fn rejects_unsafe_tun_name_and_hash() {
        let settings = HelperSettings {
            authorized_uid: 1_000,
            authorized_gid: 1_000,
            socket_path: "/run/iran-split/helper.sock".into(),
            staging_dir: "/home/user/.local/share/iran-split/runtime".into(),
            runtime_dir: "/var/lib/iran-split".into(),
            mihomo_binary: "/opt/iran-split/mihomo".into(),
            mihomo_sha256: "not-a-hash".into(),
            tun_name: "../../tun".into(),
        };
        assert!(settings.validate().is_err());
    }

    // A leading `/` is root-relative on Windows, not absolute, so `validate`
    // rejects this Linux fixture there. The Windows layout is covered below.
    #[cfg(unix)]
    #[test]
    fn helper_toml_defaults_missing_gid() {
        let parsed: HelperSettings = toml::from_str(
            r#"
authorized_uid = 1000
socket_path = "/run/iran-split/helper.sock"
staging_dir = "/home/user/.local/share/biflow/runtime/generations"
runtime_dir = "/var/lib/iran-split"
mihomo_binary = "/usr/lib/biflow/mihomo"
mihomo_sha256 = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
tun_name = "clash-iran"
"#,
        )
        .expect("toml");
        assert_eq!(parsed.authorized_gid, 0);
        assert!(parsed.validate().is_ok());
    }

    #[cfg(not(unix))]
    #[test]
    fn permission_helpers_are_noops_off_unix() {
        let path = Path::new("unused");
        set_file_permissions(path);
        set_directory_permissions(path);
        delete_owned_interface("unused");
    }

    #[cfg(unix)]
    #[test]
    fn production_linux_helper_paths_are_absolute() {
        assert!(Path::new("/run/iran-split/helper.sock").is_absolute());
        assert!(Path::new("/var/lib/iran-split").is_absolute());
        assert!(Path::new("/usr/lib/biflow/iran-split-helper").is_absolute());
    }

    #[test]
    fn clap_install_error_message_omits_paths_and_help() {
        use clap::error::ErrorKind;
        assert_eq!(
            clap_install_error_message(&clap::Error::new(ErrorKind::UnknownArgument)).as_deref(),
            Some("unexpected argument")
        );
        assert_eq!(
            clap_install_error_message(&clap::Error::new(ErrorKind::MissingRequiredArgument))
                .as_deref(),
            Some("missing or invalid install argument")
        );
        assert_eq!(
            clap_install_error_message(&clap::Error::new(ErrorKind::DisplayHelp)),
            None
        );
        let with_path = clap::Error::raw(
            ErrorKind::UnknownArgument,
            r"unexpected argument 'C:\Program Files\BiFlow\dependencies\mihomo.exe'",
        );
        let message = clap_install_error_message(&with_path).expect("message");
        assert!(!message.contains("Program Files"));
        assert!(!message.contains('\\'));
    }

    #[test]
    fn windows_main_persists_clap_errors_before_exit() {
        let source = include_str!("main.rs");
        assert!(source.contains("Arguments::try_parse()"));
        assert!(source.contains("clap_install_error_message"));
        assert!(source.contains("persist_install_error"));
        assert!(source.contains("error.exit()"));
    }

    #[test]
    fn copy_file_unless_same_is_a_noop_for_identical_paths() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("helper.bin");
        fs::write(&path, b"payload").expect("write");
        copy_file_unless_same(&path, &path).expect("same-file copy");
        assert_eq!(fs::read(&path).expect("read"), b"payload");

        let other = directory.path().join("other.bin");
        copy_file_unless_same(&path, &other).expect("distinct copy");
        assert_eq!(fs::read(&other).expect("copied"), b"payload");
    }

    // A Windows counterpart belongs here, but every assertion about Windows
    // path semantics has to be executed on Windows to be trusted. Add it once
    // `pnpm github:action-test` can run the windows-2025 test job (ADR 0031).
}
