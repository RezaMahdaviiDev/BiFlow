use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    net::IpAddr,
    path::{Path, PathBuf},
};
use tempfile::NamedTempFile;
use thiserror::Error;

pub const CURRENT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub schema_version: u32,
    pub revision: u64,
    pub hiddify: HiddifyConfig,
    pub mihomo: MihomoConfig,
    pub rules: RulesConfig,
    pub behavior: BehaviorConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            revision: 0,
            hiddify: HiddifyConfig::default(),
            mihomo: MihomoConfig::default(),
            rules: RulesConfig::default(),
            behavior: BehaviorConfig::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct HiddifyConfig {
    pub host: String,
    pub port: u16,
    pub executable: ExecutableSetting,
    pub start_timeout_seconds: u64,
    pub stop_with_stack: bool,
}

impl Default for HiddifyConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".into(),
            port: 12_334,
            executable: ExecutableSetting::Auto,
            start_timeout_seconds: 45,
            stop_with_stack: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ExecutableSetting {
    #[default]
    Auto,
    Path(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MihomoConfig {
    pub controller_host: String,
    pub controller_port: u16,
    pub controller_secret: String,
    pub mixed_port: u16,
    pub dns_port: u16,
    pub tun_name: String,
    pub log_level: LogLevel,
}

impl Default for MihomoConfig {
    fn default() -> Self {
        Self {
            controller_host: "127.0.0.1".into(),
            controller_port: 19_090,
            controller_secret: generate_secret(),
            mixed_port: 17_890,
            dns_port: 1_053,
            tun_name: "clash-iran".into(),
            log_level: LogLevel::Info,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Error,
    Warn,
    #[default]
    Info,
    Debug,
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            Self::Error => "error",
            Self::Warn => "warn",
            Self::Info => "info",
            Self::Debug => "debug",
        };
        formatter.write_str(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RulesConfig {
    pub refresh_interval_minutes: u64,
    pub upstream_refresh_hours: u64,
}

impl Default for RulesConfig {
    fn default() -> Self {
        Self {
            refresh_interval_minutes: 15,
            upstream_refresh_hours: 24,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct BehaviorConfig {
    pub launch_at_login: bool,
    pub connect_at_launch: bool,
    pub close_to_tray: bool,
}

impl Default for BehaviorConfig {
    fn default() -> Self {
        Self {
            launch_at_login: false,
            connect_at_launch: false,
            close_to_tray: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationIssue {
    pub field: String,
    pub code: String,
    pub message: String,
}

impl AppConfig {
    #[must_use]
    pub fn validate(&self) -> Vec<ValidationIssue> {
        let mut issues = Vec::new();
        validate_loopback("hiddify.host", &self.hiddify.host, &mut issues);
        validate_loopback(
            "mihomo.controller_host",
            &self.mihomo.controller_host,
            &mut issues,
        );

        if self.mihomo.controller_secret.trim().len() < 32 {
            issues.push(issue(
                "mihomo.controller_secret",
                "SECRET_TOO_SHORT",
                "controller secret must contain at least 32 characters",
            ));
        }
        if self.hiddify.start_timeout_seconds == 0 || self.hiddify.start_timeout_seconds > 300 {
            issues.push(issue(
                "hiddify.start_timeout_seconds",
                "OUT_OF_RANGE",
                "start timeout must be between 1 and 300 seconds",
            ));
        }
        if self.rules.refresh_interval_minutes == 0 || self.rules.upstream_refresh_hours == 0 {
            issues.push(issue(
                "rules",
                "OUT_OF_RANGE",
                "refresh intervals must be non-zero",
            ));
        }
        if self.mihomo.tun_name.trim().is_empty() || self.mihomo.tun_name.len() > 64 {
            issues.push(issue(
                "mihomo.tun_name",
                "INVALID_TUN_NAME",
                "TUN name must contain between 1 and 64 characters",
            ));
        }

        let ports = [
            ("hiddify.port", self.hiddify.port),
            ("mihomo.controller_port", self.mihomo.controller_port),
            ("mihomo.mixed_port", self.mihomo.mixed_port),
            ("mihomo.dns_port", self.mihomo.dns_port),
        ];
        for (index, (field, port)) in ports.iter().enumerate() {
            if *port == 0 {
                issues.push(issue(field, "INVALID_PORT", "port cannot be zero"));
            }
            if ports[..index].iter().any(|(_, previous)| previous == port) {
                issues.push(issue(
                    field,
                    "PORT_CONFLICT",
                    "configured ports must be unique",
                ));
            }
        }
        issues
    }

    #[must_use]
    pub fn redacted(&self) -> Self {
        let mut value = self.clone();
        value.mihomo.controller_secret = "[REDACTED]".into();
        if let ExecutableSetting::Path(path) = &value.hiddify.executable {
            if let Some(name) = path.file_name() {
                value.hiddify.executable = ExecutableSetting::Path(PathBuf::from(name));
            }
        }
        value
    }
}

fn validate_loopback(field: &str, value: &str, issues: &mut Vec<ValidationIssue>) {
    match value.parse::<IpAddr>() {
        Ok(address) if address.is_loopback() => {}
        _ => issues.push(issue(
            field,
            "LOOPBACK_REQUIRED",
            "address must be a numeric loopback address",
        )),
    }
}

fn issue(field: &str, code: &str, message: &str) -> ValidationIssue {
    ValidationIssue {
        field: field.into(),
        code: code.into(),
        message: message.into(),
    }
}

fn generate_secret() -> String {
    let mut bytes = [0_u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("configuration I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("configuration is not valid TOML: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("configuration could not be serialized: {0}")]
    Serialize(#[from] toml::ser::Error),
    #[error("unsupported configuration schema version {0}")]
    UnsupportedSchema(u32),
    #[error("configuration validation failed")]
    Validation(Vec<ValidationIssue>),
    #[error("configuration revision conflict: expected {expected}, found {actual}")]
    RevisionConflict { expected: u64, actual: u64 },
    #[error("atomic configuration publication failed: {0}")]
    Persist(#[from] tempfile::PersistError),
}

#[derive(Debug, Clone)]
pub struct ConfigStore {
    path: PathBuf,
}

impl ConfigStore {
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn load_or_create(&self) -> Result<AppConfig, ConfigError> {
        if !self.path.exists() {
            let config = AppConfig::default();
            self.write_atomic(&config)?;
            return Ok(config);
        }
        self.load()
    }

    pub fn load(&self) -> Result<AppConfig, ConfigError> {
        let source = fs::read_to_string(&self.path)?;
        let mut value: toml::Value = toml::from_str(&source)?;
        let schema = value
            .get("schema_version")
            .and_then(toml::Value::as_integer)
            .unwrap_or(0);
        let schema = u32::try_from(schema).unwrap_or(u32::MAX);
        if schema > CURRENT_SCHEMA_VERSION {
            return Err(ConfigError::UnsupportedSchema(schema));
        }
        if schema < CURRENT_SCHEMA_VERSION {
            self.backup()?;
            migrate(&mut value, schema)?;
        }
        let config: AppConfig = value.try_into()?;
        let issues = config.validate();
        if !issues.is_empty() {
            return Err(ConfigError::Validation(issues));
        }
        if schema < CURRENT_SCHEMA_VERSION {
            self.write_atomic(&config)?;
        }
        Ok(config)
    }

    pub fn save(
        &self,
        mut config: AppConfig,
        expected_revision: u64,
    ) -> Result<AppConfig, ConfigError> {
        let current = self.load()?;
        if current.revision != expected_revision {
            return Err(ConfigError::RevisionConflict {
                expected: expected_revision,
                actual: current.revision,
            });
        }
        let issues = config.validate();
        if !issues.is_empty() {
            return Err(ConfigError::Validation(issues));
        }
        config.schema_version = CURRENT_SCHEMA_VERSION;
        config.revision = current.revision.saturating_add(1);
        self.write_atomic(&config)?;
        Ok(config)
    }

    fn backup(&self) -> Result<(), ConfigError> {
        let backup = self.path.with_extension("toml.bak");
        fs::copy(&self.path, backup)?;
        Ok(())
    }

    fn write_atomic(&self, config: &AppConfig) -> Result<(), ConfigError> {
        let parent = self.path.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)?;
        let content = toml::to_string_pretty(config)?;
        let mut temporary = NamedTempFile::new_in(parent)?;
        temporary.write_all(content.as_bytes())?;
        temporary.as_file().sync_all()?;
        set_private_permissions(temporary.path())?;
        temporary.persist(&self.path)?;
        sync_directory(parent)?;
        Ok(())
    }
}

fn migrate(value: &mut toml::Value, from: u32) -> Result<(), ConfigError> {
    if from != 0 {
        return Err(ConfigError::UnsupportedSchema(from));
    }
    let table = value
        .as_table_mut()
        .ok_or_else(|| ConfigError::UnsupportedSchema(from))?;
    table.insert(
        "schema_version".into(),
        toml::Value::Integer(i64::from(CURRENT_SCHEMA_VERSION)),
    );
    table.entry("revision").or_insert(toml::Value::Integer(0));
    Ok(())
}

#[cfg(unix)]
fn set_private_permissions(path: &Path) -> Result<(), std::io::Error> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn set_private_permissions(_path: &Path) -> Result<(), std::io::Error> {
    Ok(())
}

fn sync_directory(path: &Path) -> Result<(), std::io::Error> {
    let directory = OpenOptions::new().read(true).open(path)?;
    directory.sync_all()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid_and_secret_is_random() {
        let first = AppConfig::default();
        let second = AppConfig::default();
        assert!(first.validate().is_empty());
        assert_ne!(
            first.mihomo.controller_secret,
            second.mihomo.controller_secret
        );
        assert_eq!(first.mihomo.controller_secret.len(), 64);
    }

    #[test]
    fn rejects_remote_controller_and_conflicting_ports() {
        let mut config = AppConfig::default();
        config.mihomo.controller_host = "0.0.0.0".into();
        config.mihomo.mixed_port = config.mihomo.controller_port;
        let issues = config.validate();
        assert!(issues.iter().any(|item| item.code == "LOOPBACK_REQUIRED"));
        assert!(issues.iter().any(|item| item.code == "PORT_CONFLICT"));
    }

    #[test]
    fn persists_atomically_and_checks_revision() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("config.toml");
        let store = ConfigStore::new(&path);
        let mut config = store.load_or_create().expect("create config");
        config.behavior.connect_at_launch = true;
        let saved = store.save(config.clone(), 0).expect("save config");
        assert_eq!(saved.revision, 1);
        assert!(store.save(config, 0).is_err());
        assert!(store.load().expect("reload").behavior.connect_at_launch);
    }

    #[test]
    fn redaction_removes_secret_and_parent_path() {
        let mut config = AppConfig::default();
        config.hiddify.executable =
            ExecutableSetting::Path(PathBuf::from("/home/alice/Hiddify.AppImage"));
        let redacted = config.redacted();
        assert_eq!(redacted.mihomo.controller_secret, "[REDACTED]");
        assert_eq!(
            redacted.hiddify.executable,
            ExecutableSetting::Path(PathBuf::from("Hiddify.AppImage"))
        );
    }
}
