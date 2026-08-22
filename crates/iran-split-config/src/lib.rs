use ipnet::IpNet;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    net::IpAddr,
    path::{Path, PathBuf},
};
use tempfile::NamedTempFile;
use thiserror::Error;

pub const CURRENT_SCHEMA_VERSION: u32 = 3;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub schema_version: u32,
    pub revision: u64,
    pub hiddify: HiddifyConfig,
    pub mihomo: MihomoConfig,
    pub rules: RulesConfig,
    pub behavior: BehaviorConfig,
    pub openvpn: OpenVpnConfig,
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
            openvpn: OpenVpnConfig::default(),
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
    pub direct_dns_preset: DirectDnsPreset,
    /// Used when [`DirectDnsPreset::Custom`]. Named presets ignore this list.
    pub direct_dns_servers: Vec<String>,
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
            direct_dns_preset: DirectDnsPreset::default(),
            direct_dns_servers: Vec::new(),
        }
    }
}

/// Resolvers for Iranian and user-pinned DIRECT domains (not VPN `DoH`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DirectDnsPreset {
    /// Mihomo fake-ip plus Cloudflare `DoH`. No Iranian nameserver policy.
    #[default]
    FakeIp,
    Shecan,
    Electro,
    Radar,
    Mokhaberat,
    Custom,
}

impl DirectDnsPreset {
    /// Built-in IPv4 resolvers. `FakeIp` and `Custom` have none.
    #[must_use]
    pub const fn servers(self) -> &'static [&'static str] {
        match self {
            Self::FakeIp | Self::Custom => &[],
            Self::Shecan => &["178.22.122.100", "185.51.200.2"],
            Self::Electro => &["78.157.42.100", "78.157.42.101"],
            Self::Radar => &["10.202.10.10", "10.202.10.11"],
            Self::Mokhaberat => &["5.200.200.200"],
        }
    }

    /// Whether DIRECT domains skip fake-ip and use these resolvers.
    #[must_use]
    pub const fn applies_direct_policy(self) -> bool {
        !matches!(self, Self::FakeIp)
    }
}

impl std::fmt::Display for DirectDnsPreset {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::FakeIp => "fake_ip",
            Self::Shecan => "shecan",
            Self::Electro => "electro",
            Self::Radar => "radar",
            Self::Mokhaberat => "mokhaberat",
            Self::Custom => "custom",
        })
    }
}

impl MihomoConfig {
    /// Addresses written into Mihomo `direct-nameserver` and nameserver-policy.
    #[must_use]
    pub fn direct_dns_resolvers(&self) -> Vec<String> {
        if self.direct_dns_preset == DirectDnsPreset::Custom {
            return self
                .direct_dns_servers
                .iter()
                .map(|server| server.trim().to_owned())
                .filter(|server| !server.is_empty())
                .collect();
        }
        self.direct_dns_preset
            .servers()
            .iter()
            .map(|server| (*server).to_owned())
            .collect()
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

/// Split-tunnel `OpenVPN` that starts after Hiddify and never owns the default
/// route.
///
/// `BiFlow` starts `OpenVPN` as a *side* tunnel: the helper always passes
/// `--pull-filter ignore "redirect-gateway"` and `--pull-filter ignore
/// "dhcp-option DNS"`, so a profile that asks to become the system gateway
/// cannot take the machine offline when the tunnel drops. Only the networks
/// the server scopes to itself (plus [`OpenVpnConfig::tunnel_routes`]) leave
/// through the `OpenVPN` device; everything else keeps following the existing
/// DIRECT / Hiddify split. See ADR 0067.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct OpenVpnConfig {
    /// Start `OpenVPN` as part of Connect, right after Hiddify is reachable.
    pub enabled: bool,
    /// Fail Connect when `OpenVPN` cannot start. Off by default so a broken
    /// profile degrades one component instead of the whole stack.
    pub required: bool,
    /// Keep the server's scoped `push route` directives. The default route is
    /// filtered out regardless; `false` adds `--route-nopull` and leaves
    /// [`OpenVpnConfig::tunnel_routes`] as the only way in.
    pub pull_routes: bool,
    /// TUN device `OpenVPN` owns. Must differ from the Mihomo device.
    pub device: String,
    pub start_timeout_seconds: u64,
    /// Firewall mark Mihomo stamps on `OpenVPN`-bound traffic (Linux only).
    pub routing_mark: u32,
    /// Policy-routing table that carries the `OpenVPN` default (Linux only).
    pub routing_table: u32,
    /// Absolute path to the `.ovpn` profile.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<PathBuf>,
    /// Optional `auth-user-pass` credentials file.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_file: Option<PathBuf>,
    /// Explicit `openvpn` binary. `None` discovers it on `PATH`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executable: Option<PathBuf>,
    /// Extra destinations reachable through the tunnel, as CIDRs. A default
    /// route is rejected here on purpose.
    pub tunnel_routes: Vec<String>,
}

impl Default for OpenVpnConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            required: false,
            pull_routes: true,
            device: "biflow-ovpn".into(),
            start_timeout_seconds: 45,
            routing_mark: 0x0000_b1f0,
            routing_table: 178,
            profile: None,
            auth_file: None,
            executable: None,
            tunnel_routes: Vec::new(),
        }
    }
}

impl OpenVpnConfig {
    /// Whether the stack should try to bring the `OpenVPN` side tunnel up.
    #[must_use]
    pub const fn active(&self) -> bool {
        self.enabled && self.profile.is_some()
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
        validate_direct_dns(&self.mihomo, &mut issues);
        validate_openvpn(&self.openvpn, &self.mihomo, &mut issues);

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
        // A `.ovpn` path leaks the profile (often the provider and account)
        // and the auth file leaks where credentials live, so exports keep the
        // file name only.
        value.openvpn.profile = value.openvpn.profile.as_deref().map(file_name_only);
        value.openvpn.auth_file = value.openvpn.auth_file.as_deref().map(file_name_only);
        value.openvpn.executable = value.openvpn.executable.as_deref().map(file_name_only);
        value
    }
}

/// Rejects any `OpenVPN` setting that could take the machine offline.
///
/// The important rule is `TUNNEL_ROUTE_DEFAULT`: a `0.0.0.0/0` or `::/0` entry
/// would turn the side tunnel into the system gateway, which is exactly the
/// failure this integration exists to prevent.
fn validate_openvpn(
    openvpn: &OpenVpnConfig,
    mihomo: &MihomoConfig,
    issues: &mut Vec<ValidationIssue>,
) {
    let device = openvpn.device.trim();
    if device.is_empty() || device.len() > 64 {
        issues.push(issue(
            "openvpn.device",
            "INVALID_DEVICE",
            "OpenVPN device name must contain between 1 and 64 characters",
        ));
    } else if device == mihomo.tun_name.trim() {
        issues.push(issue(
            "openvpn.device",
            "DEVICE_CONFLICT",
            "OpenVPN device must differ from the Mihomo TUN device",
        ));
    }
    if openvpn.start_timeout_seconds == 0 || openvpn.start_timeout_seconds > 300 {
        issues.push(issue(
            "openvpn.start_timeout_seconds",
            "OUT_OF_RANGE",
            "start timeout must be between 1 and 300 seconds",
        ));
    }
    if openvpn.routing_mark == 0 {
        issues.push(issue(
            "openvpn.routing_mark",
            "INVALID_MARK",
            "routing mark cannot be zero",
        ));
    }
    if openvpn.routing_table == 0 || openvpn.routing_table > 252 {
        issues.push(issue(
            "openvpn.routing_table",
            "OUT_OF_RANGE",
            "routing table must be between 1 and 252 so the main tables stay untouched",
        ));
    }
    for path in [
        ("openvpn.profile", openvpn.profile.as_ref()),
        ("openvpn.auth_file", openvpn.auth_file.as_ref()),
        ("openvpn.executable", openvpn.executable.as_ref()),
    ]
    .into_iter()
    .filter_map(|(field, value)| value.map(|path| (field, path)))
    {
        if !path.1.is_absolute() {
            issues.push(issue(path.0, "PATH_NOT_ABSOLUTE", "path must be absolute"));
        }
    }
    if openvpn.enabled && openvpn.profile.is_none() {
        issues.push(issue(
            "openvpn.profile",
            "PROFILE_REQUIRED",
            "an enabled OpenVPN side tunnel needs a .ovpn profile",
        ));
    }
    if openvpn.tunnel_routes.len() > 64 {
        issues.push(issue(
            "openvpn.tunnel_routes",
            "TOO_MANY_ROUTES",
            "OpenVPN accepts at most 64 tunnel routes",
        ));
    }
    for route in &openvpn.tunnel_routes {
        match route.trim().parse::<IpNet>() {
            Ok(network) if network.prefix_len() == 0 => issues.push(issue(
                "openvpn.tunnel_routes",
                "TUNNEL_ROUTE_DEFAULT",
                "a default route would send the whole system through OpenVPN",
            )),
            Ok(_) => {}
            Err(_) => issues.push(issue(
                "openvpn.tunnel_routes",
                "TUNNEL_ROUTE_INVALID",
                "tunnel routes must be CIDR networks such as 10.8.0.0/24",
            )),
        }
    }
}

fn validate_direct_dns(mihomo: &MihomoConfig, issues: &mut Vec<ValidationIssue>) {
    if !mihomo.direct_dns_preset.applies_direct_policy() {
        return;
    }
    let servers = mihomo.direct_dns_resolvers();
    if servers.is_empty() {
        issues.push(issue(
            "mihomo.direct_dns_servers",
            "DIRECT_DNS_REQUIRED",
            "DIRECT DNS needs at least one resolver address",
        ));
        return;
    }
    if servers.len() > 4 {
        issues.push(issue(
            "mihomo.direct_dns_servers",
            "DIRECT_DNS_TOO_MANY",
            "DIRECT DNS accepts at most four resolver addresses",
        ));
    }
    for server in servers {
        match server.parse::<IpAddr>() {
            Ok(address) if is_usable_direct_dns(address) => {}
            Ok(_) => issues.push(issue(
                "mihomo.direct_dns_servers",
                "DIRECT_DNS_INVALID",
                "DIRECT DNS cannot be loopback, unspecified, multicast, or fake-ip",
            )),
            Err(_) => issues.push(issue(
                "mihomo.direct_dns_servers",
                "DIRECT_DNS_INVALID",
                "DIRECT DNS resolvers must be IP addresses, not host names",
            )),
        }
    }
}

fn file_name_only(path: &Path) -> PathBuf {
    path.file_name()
        .map_or_else(|| PathBuf::from("[REDACTED]"), PathBuf::from)
}

fn is_usable_direct_dns(address: IpAddr) -> bool {
    if address.is_loopback() || address.is_unspecified() || address.is_multicast() {
        return false;
    }
    match address {
        IpAddr::V4(value) => {
            let octets = value.octets();
            !(octets[0] == 198 && (octets[1] == 18 || octets[1] == 19))
        }
        IpAddr::V6(_) => true,
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

    /// Loads the stored configuration, creating a validated default when absent.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError`] when the file cannot be read, parsed, migrated,
    /// validated, or atomically persisted.
    pub fn load_or_create(&self) -> Result<AppConfig, ConfigError> {
        if !self.path.exists() {
            let config = AppConfig::default();
            self.write_atomic(&config)?;
            return Ok(config);
        }
        self.load()
    }

    /// Loads, migrates, and validates the stored configuration.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError`] for I/O or TOML failures, unsupported schema
    /// versions, failed migrations, or invalid configuration values.
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

    /// Validates and atomically saves a configuration at the expected revision.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError`] when the current configuration cannot be loaded,
    /// the revision conflicts, validation fails, or atomic persistence fails.
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
        #[cfg(unix)]
        set_private_permissions(temporary.path())?;
        #[cfg(not(unix))]
        set_private_permissions(temporary.path());
        temporary.persist(&self.path)?;
        #[cfg(unix)]
        sync_directory(parent)?;
        #[cfg(not(unix))]
        sync_directory(parent);
        Ok(())
    }
}

fn migrate(value: &mut toml::Value, from: u32) -> Result<(), ConfigError> {
    if from >= CURRENT_SCHEMA_VERSION {
        return Err(ConfigError::UnsupportedSchema(from));
    }
    let table = value
        .as_table_mut()
        .ok_or_else(|| ConfigError::UnsupportedSchema(from))?;
    if from == 0 {
        table.entry("revision").or_insert(toml::Value::Integer(0));
    }
    if from < 2 {
        if let Some(mihomo) = table.get_mut("mihomo").and_then(toml::Value::as_table_mut) {
            if mihomo
                .get("direct_dns_preset")
                .and_then(toml::Value::as_str)
                == Some("shecan")
            {
                mihomo.insert(
                    "direct_dns_preset".into(),
                    toml::Value::String("fake_ip".into()),
                );
            }
        }
    }
    table.insert(
        "schema_version".into(),
        toml::Value::Integer(i64::from(CURRENT_SCHEMA_VERSION)),
    );
    Ok(())
}

#[cfg(unix)]
fn set_private_permissions(path: &Path) -> Result<(), std::io::Error> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn set_private_permissions(_path: &Path) {}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<(), std::io::Error> {
    let directory = fs::OpenOptions::new().read(true).open(path)?;
    directory.sync_all()
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) {}

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
        assert_eq!(first.mihomo.direct_dns_preset, DirectDnsPreset::FakeIp);
        assert_eq!(first.mihomo.direct_dns_preset.to_string(), "fake_ip");
        assert!(first.mihomo.direct_dns_resolvers().is_empty());
    }

    #[test]
    fn mokhaberat_and_radar_presets_are_valid_addresses() {
        let mut config = AppConfig::default();
        config.mihomo.direct_dns_preset = DirectDnsPreset::Mokhaberat;
        assert!(config.validate().is_empty());
        assert_eq!(config.mihomo.direct_dns_resolvers(), ["5.200.200.200"]);
        config.mihomo.direct_dns_preset = DirectDnsPreset::Radar;
        assert!(config.validate().is_empty());
        assert_eq!(
            config.mihomo.direct_dns_resolvers(),
            ["10.202.10.10", "10.202.10.11"]
        );
    }

    #[test]
    fn custom_direct_dns_rejects_empty_and_loopback() {
        let mut config = AppConfig::default();
        config.mihomo.direct_dns_preset = DirectDnsPreset::Custom;
        assert!(config
            .validate()
            .iter()
            .any(|item| item.code == "DIRECT_DNS_REQUIRED"));
        config.mihomo.direct_dns_servers = vec!["127.0.0.1".into()];
        assert!(config
            .validate()
            .iter()
            .any(|item| item.code == "DIRECT_DNS_INVALID"));
        config.mihomo.direct_dns_servers = vec!["5.200.200.200".into(), "1.1.1.1".into()];
        assert!(config.validate().is_empty());
    }

    #[test]
    fn openvpn_defaults_are_off_and_valid() {
        let config = AppConfig::default();
        assert!(config.validate().is_empty());
        assert!(!config.openvpn.enabled);
        assert!(!config.openvpn.required);
        assert!(!config.openvpn.active());
        assert!(config.openvpn.pull_routes);
        assert_ne!(config.openvpn.device, config.mihomo.tun_name);
    }

    #[test]
    fn openvpn_rejects_a_default_tunnel_route() {
        let mut config = AppConfig::default();
        config.openvpn.tunnel_routes = vec!["10.8.0.0/24".into(), "0.0.0.0/0".into()];
        assert!(config
            .validate()
            .iter()
            .any(|item| item.code == "TUNNEL_ROUTE_DEFAULT"));
        config.openvpn.tunnel_routes = vec!["::/0".into()];
        assert!(config
            .validate()
            .iter()
            .any(|item| item.code == "TUNNEL_ROUTE_DEFAULT"));
        config.openvpn.tunnel_routes = vec!["10.8.0.0/24".into(), "192.168.44.0/24".into()];
        assert!(config.validate().is_empty());
    }

    #[test]
    fn openvpn_rejects_device_conflict_and_missing_profile() {
        let mut config = AppConfig::default();
        config.openvpn.device.clone_from(&config.mihomo.tun_name);
        assert!(config
            .validate()
            .iter()
            .any(|item| item.code == "DEVICE_CONFLICT"));
        config.openvpn.device = "biflow-ovpn".into();
        config.openvpn.enabled = true;
        assert!(config
            .validate()
            .iter()
            .any(|item| item.code == "PROFILE_REQUIRED"));
        config.openvpn.profile = Some(PathBuf::from("office.ovpn"));
        assert!(config
            .validate()
            .iter()
            .any(|item| item.code == "PATH_NOT_ABSOLUTE"));
        config.openvpn.profile = Some(PathBuf::from("/etc/openvpn/office.ovpn"));
        assert!(config.validate().is_empty());
        assert!(config.openvpn.active());
    }

    #[test]
    fn redaction_keeps_only_openvpn_file_names() {
        let mut config = AppConfig::default();
        config.openvpn.profile = Some(PathBuf::from("/home/reza/vpn/office.ovpn"));
        config.openvpn.auth_file = Some(PathBuf::from("/home/reza/vpn/secret.txt"));
        let redacted = config.redacted();
        assert_eq!(redacted.openvpn.profile, Some(PathBuf::from("office.ovpn")));
        assert_eq!(
            redacted.openvpn.auth_file,
            Some(PathBuf::from("secret.txt"))
        );
        assert_eq!(redacted.mihomo.controller_secret, "[REDACTED]");
    }

    #[test]
    fn schema_v2_config_without_openvpn_migrates_to_defaults() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("config.toml");
        let config = AppConfig {
            schema_version: 2,
            ..AppConfig::default()
        };
        let mut table = toml::Table::try_from(&config).expect("table");
        table.remove("openvpn");
        fs::write(&path, toml::to_string(&table).expect("toml")).expect("write");
        let loaded = ConfigStore::new(&path).load().expect("load");
        assert_eq!(loaded.schema_version, CURRENT_SCHEMA_VERSION);
        assert_eq!(loaded.openvpn, OpenVpnConfig::default());
    }

    #[test]
    fn schema_v1_implicit_shecan_migrates_to_fake_ip() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("config.toml");
        let mut config = AppConfig {
            schema_version: 1,
            ..AppConfig::default()
        };
        config.mihomo.direct_dns_preset = DirectDnsPreset::Shecan;
        fs::write(&path, toml::to_string(&config).expect("toml")).expect("write");
        let loaded = ConfigStore::new(&path).load().expect("load");
        assert_eq!(loaded.schema_version, CURRENT_SCHEMA_VERSION);
        assert_eq!(loaded.mihomo.direct_dns_preset, DirectDnsPreset::FakeIp);
    }

    #[test]
    fn schema_v1_mokhaberat_survives_direct_dns_migration() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("config.toml");
        let mut config = AppConfig {
            schema_version: 1,
            ..AppConfig::default()
        };
        config.mihomo.direct_dns_preset = DirectDnsPreset::Mokhaberat;
        fs::write(&path, toml::to_string(&config).expect("toml")).expect("write");
        let loaded = ConfigStore::new(&path).load().expect("load");
        assert_eq!(loaded.schema_version, CURRENT_SCHEMA_VERSION);
        assert_eq!(loaded.mihomo.direct_dns_preset, DirectDnsPreset::Mokhaberat);
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

    #[cfg(unix)]
    #[test]
    fn private_permissions_restrict_to_owner_read_write() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("secret.toml");
        fs::write(&path, "x").expect("write");
        set_private_permissions(&path).expect("chmod");
        let mode = fs::metadata(&path).expect("metadata").permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[cfg(not(unix))]
    #[test]
    fn private_permissions_are_a_noop_off_unix() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("secret.toml");
        fs::write(&path, "x").expect("write");
        set_private_permissions(&path);
        assert!(path.is_file());
    }
}
