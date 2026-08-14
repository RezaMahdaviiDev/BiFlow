use futures_util::StreamExt;
use iran_split_config::AppConfig;
use iran_split_rules::{DirectRulesDocument, DirectTarget};
use reqwest::{header, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    net::IpAddr,
    path::{Path, PathBuf},
    time::Duration,
};
use thiserror::Error;
use tokio::{process::Command, sync::mpsc};
use tokio_tungstenite::{connect_async, tungstenite::client::IntoClientRequest};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Error)]
pub enum MihomoError {
    #[error("Mihomo configuration is invalid: {0}")]
    InvalidConfig(String),
    #[error("Mihomo configuration serialization failed: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("Mihomo controller request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Mihomo controller returned HTTP {0}")]
    UnexpectedStatus(StatusCode),
    #[error("Mihomo validation process failed: {0}")]
    ValidationProcess(std::io::Error),
    #[error("Mihomo rejected the generated configuration: {0}")]
    ValidationRejected(String),
    #[error("Mihomo readiness check timed out: {0}")]
    ReadinessTimeout(String),
    #[error("operation was cancelled")]
    Cancelled,
    #[error("log WebSocket failed: {0}")]
    WebSocket(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Linux,
    Windows,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimePaths {
    pub private_networks: PathBuf,
    pub iran_domains: PathBuf,
    pub iran_networks: PathBuf,
    pub custom_direct_domains: PathBuf,
    pub custom_direct_ips: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedConfig {
    pub yaml: String,
    pub sha256: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
struct MihomoConfigDocument {
    mixed_port: u16,
    allow_lan: bool,
    bind_address: String,
    mode: String,
    log_level: String,
    external_controller: String,
    secret: String,
    ipv6: bool,
    tun: TunConfig,
    dns: DnsConfig,
    sniffer: SnifferConfig,
    proxies: Vec<ProxyConfig>,
    proxy_groups: Vec<ProxyGroup>,
    rule_providers: BTreeMap<String, RuleProvider>,
    rules: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
#[expect(
    clippy::struct_excessive_bools,
    reason = "this serializable DTO mirrors Mihomo's TUN configuration schema"
)]
struct TunConfig {
    enable: bool,
    stack: String,
    device: String,
    auto_route: bool,
    auto_detect_interface: bool,
    strict_route: bool,
    dns_hijack: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
struct DnsConfig {
    enable: bool,
    listen: String,
    ipv6: bool,
    enhanced_mode: String,
    fake_ip_range: String,
    fake_ip_filter: Vec<String>,
    default_nameserver: Vec<String>,
    nameserver: Vec<String>,
    proxy_server_nameserver: Vec<String>,
    direct_nameserver: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
#[expect(
    clippy::struct_excessive_bools,
    reason = "this serializable DTO mirrors Mihomo's sniffer configuration schema"
)]
struct SnifferConfig {
    enable: bool,
    force_dns_mapping: bool,
    parse_pure_ip: bool,
    override_destination: bool,
    sniff: BTreeMap<String, SniffPorts>,
}

#[derive(Debug, Serialize)]
struct SniffPorts {
    ports: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ProxyConfig {
    name: String,
    #[serde(rename = "type")]
    kind: String,
    server: String,
    port: u16,
    udp: bool,
}

#[derive(Debug, Serialize)]
struct ProxyGroup {
    name: String,
    #[serde(rename = "type")]
    kind: String,
    proxies: Vec<String>,
}

#[derive(Debug, Serialize)]
struct RuleProvider {
    #[serde(rename = "type")]
    kind: String,
    behavior: String,
    format: String,
    path: String,
}

/// Generates a validated Mihomo YAML document and its SHA-256 digest.
///
/// # Errors
///
/// Returns [`MihomoError::InvalidConfig`] when application or custom-rule
/// settings are invalid, or [`MihomoError::Yaml`] when serialization fails.
#[expect(
    clippy::too_many_lines,
    reason = "the function assembles one declarative Mihomo configuration document"
)]
pub fn generate_config(
    app: &AppConfig,
    platform: Platform,
    _paths: &RuntimePaths,
    custom_rules: &DirectRulesDocument,
) -> Result<GeneratedConfig, MihomoError> {
    let issues = app.validate();
    if !issues.is_empty() {
        return Err(MihomoError::InvalidConfig(
            issues
                .into_iter()
                .map(|issue| format!("{}: {}", issue.field, issue.message))
                .collect::<Vec<_>>()
                .join("; "),
        ));
    }
    if app.mihomo.controller_secret.trim().is_empty() {
        return Err(MihomoError::InvalidConfig(
            "controller secret must not be empty".into(),
        ));
    }

    let mut rules = process_bypass_rules(platform);
    rules.extend([
        "DOMAIN-SUFFIX,localhost,DIRECT".into(),
        "IP-CIDR,127.0.0.0/8,DIRECT,no-resolve".into(),
        "IP-CIDR6,::1/128,DIRECT,no-resolve".into(),
        "RULE-SET,private-networks,DIRECT,no-resolve".into(),
        "RULE-SET,custom-direct-domains,DIRECT".into(),
        "RULE-SET,custom-direct-ips,DIRECT,no-resolve".into(),
        "RULE-SET,iran-domains,DIRECT".into(),
        "RULE-SET,iran-networks,DIRECT,no-resolve".into(),
        "MATCH,VPN".into(),
    ]);

    let document = MihomoConfigDocument {
        mixed_port: app.mihomo.mixed_port,
        allow_lan: false,
        bind_address: "127.0.0.1".into(),
        mode: "rule".into(),
        log_level: app.mihomo.log_level.to_string(),
        external_controller: format!(
            "{}:{}",
            app.mihomo.controller_host, app.mihomo.controller_port
        ),
        secret: app.mihomo.controller_secret.clone(),
        ipv6: true,
        tun: TunConfig {
            enable: true,
            stack: "mixed".into(),
            device: app.mihomo.tun_name.clone(),
            auto_route: true,
            auto_detect_interface: true,
            strict_route: platform == Platform::Windows,
            dns_hijack: vec!["any:53".into(), "tcp://any:53".into()],
        },
        dns: DnsConfig {
            enable: true,
            listen: format!("127.0.0.1:{}", app.mihomo.dns_port),
            ipv6: true,
            enhanced_mode: "fake-ip".into(),
            fake_ip_range: "198.18.0.1/16".into(),
            fake_ip_filter: vec![
                "+.lan".into(),
                "+.local".into(),
                "localhost.ptlogin2.qq.com".into(),
            ],
            default_nameserver: vec!["1.1.1.1".into(), "8.8.8.8".into()],
            nameserver: vec![
                "https://1.1.1.1/dns-query".into(),
                "https://8.8.8.8/dns-query".into(),
            ],
            proxy_server_nameserver: vec!["8.8.8.8".into(), "1.1.1.1".into()],
            direct_nameserver: vec!["178.22.122.100".into(), "185.51.200.2".into()],
        },
        sniffer: SnifferConfig {
            enable: true,
            force_dns_mapping: true,
            parse_pure_ip: true,
            override_destination: true,
            sniff: BTreeMap::from([
                (
                    "HTTP".into(),
                    SniffPorts {
                        ports: vec!["80".into(), "8080-8880".into()],
                    },
                ),
                (
                    "TLS".into(),
                    SniffPorts {
                        ports: vec!["443".into(), "8443".into()],
                    },
                ),
                (
                    "QUIC".into(),
                    SniffPorts {
                        ports: vec!["443".into(), "8443".into()],
                    },
                ),
            ]),
        },
        proxies: vec![ProxyConfig {
            name: "Hiddify".into(),
            kind: "socks5".into(),
            server: app.hiddify.host.clone(),
            port: app.hiddify.port,
            udp: true,
        }],
        proxy_groups: vec![ProxyGroup {
            name: "VPN".into(),
            kind: "select".into(),
            proxies: vec!["Hiddify".into()],
        }],
        rule_providers: providers(),
        rules,
    };
    validate_custom_rules(custom_rules)?;
    let yaml = serde_yaml::to_string(&document)?;
    let sha256 = hex::encode(Sha256::digest(yaml.as_bytes()));
    Ok(GeneratedConfig { yaml, sha256 })
}

fn process_bypass_rules(platform: Platform) -> Vec<String> {
    match platform {
        Platform::Linux => vec![
            "PROCESS-NAME,hiddify,DIRECT".into(),
            "PROCESS-NAME-WILDCARD,*Hiddify*,DIRECT".into(),
            "PROCESS-NAME,tailscaled,DIRECT".into(),
            "PROCESS-NAME,iran-split-desktop,DIRECT".into(),
            "PROCESS-NAME,iran-split-desk,DIRECT".into(),
            "PROCESS-NAME,BiFlow,DIRECT".into(),
        ],
        Platform::Windows => vec![
            "PROCESS-NAME,hiddify.exe,DIRECT".into(),
            "PROCESS-NAME,Hiddify.exe,DIRECT".into(),
            "PROCESS-NAME,HiddifyNext.exe,DIRECT".into(),
            "PROCESS-NAME-WILDCARD,*Hiddify*,DIRECT".into(),
            "PROCESS-NAME,tailscaled.exe,DIRECT".into(),
            "PROCESS-NAME,iran-split-desktop.exe,DIRECT".into(),
            "PROCESS-NAME,BiFlow.exe,DIRECT".into(),
        ],
    }
}

fn providers() -> BTreeMap<String, RuleProvider> {
    [
        ("private-networks", "ipcidr", "private.txt"),
        ("iran-domains", "domain", "iran-domains.txt"),
        ("iran-networks", "ipcidr", "iran-networks.txt"),
        (
            "custom-direct-domains",
            "domain",
            "custom-direct-domains.txt",
        ),
        ("custom-direct-ips", "ipcidr", "custom-direct-ips.txt"),
    ]
    .into_iter()
    .map(|(name, behavior, path)| {
        (
            name.into(),
            RuleProvider {
                kind: "file".into(),
                behavior: behavior.into(),
                format: "text".into(),
                path: path.into(),
            },
        )
    })
    .collect()
}

const OPTIONAL_RULE_PROVIDERS: [&str; 2] = ["custom-direct-domains", "custom-direct-ips"];

fn summarize_rule_providers(value: &Value) -> Result<ProviderStatus, MihomoError> {
    let providers = value
        .get("providers")
        .and_then(Value::as_object)
        .ok_or_else(|| MihomoError::InvalidConfig("controller omitted providers map".into()))?;
    let total = u32::try_from(providers.len()).unwrap_or(u32::MAX);
    let ready = providers
        .iter()
        .filter(|(name, provider)| rule_provider_is_ready(name, provider))
        .count();
    let rules_loaded = providers
        .values()
        .filter_map(|provider| provider.get("ruleCount").and_then(Value::as_u64))
        .sum();
    Ok(ProviderStatus {
        ready: u32::try_from(ready).unwrap_or(u32::MAX),
        total,
        rules_loaded,
    })
}

fn rule_provider_is_ready(name: &str, provider: &Value) -> bool {
    if !provider.get("error").is_none_or(Value::is_null) {
        return false;
    }
    if OPTIONAL_RULE_PROVIDERS.contains(&name) {
        return true;
    }
    provider
        .get("ruleCount")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        > 0
}

fn validate_custom_rules(document: &DirectRulesDocument) -> Result<(), MihomoError> {
    for rule in &document.rules {
        if let DirectTarget::Domain(domain) = &rule.target {
            if domain.contains(',') || domain.contains('\n') || domain.contains('\r') {
                return Err(MihomoError::InvalidConfig(
                    "custom domain contains a provider control character".into(),
                ));
            }
        }
    }
    Ok(())
}

/// Asks a Mihomo binary to validate a generated configuration file.
///
/// # Errors
///
/// Returns an error when the validation process cannot run, times out, or
/// rejects the configuration.
pub async fn validate_with_binary(
    binary: &Path,
    config_path: &Path,
    timeout: Duration,
) -> Result<(), MihomoError> {
    let workdir = config_path.parent().ok_or_else(|| {
        MihomoError::InvalidConfig("configuration path must include a parent directory".into())
    })?;
    let output = tokio::time::timeout(
        timeout,
        Command::new(binary)
            .arg("-t")
            .arg("-d")
            .arg(workdir)
            .arg("-f")
            .arg(config_path)
            .kill_on_drop(true)
            .output(),
    )
    .await
    .map_err(|_| {
        MihomoError::ReadinessTimeout("configuration validation process timed out".into())
    })?
    .map_err(MihomoError::ValidationProcess)?;
    if !output.status.success() {
        let details = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let details = details.trim();
        let message = if details.is_empty() {
            "Mihomo rejected the configuration without details".into()
        } else {
            details.chars().take(4_096).collect()
        };
        return Err(MihomoError::ValidationRejected(message));
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct ControllerClient {
    base_url: String,
    secret: String,
    client: reqwest::Client,
}

impl ControllerClient {
    /// Creates a client restricted to a loopback Mihomo controller.
    ///
    /// # Errors
    ///
    /// Returns an error when `host` is not a loopback IP address, the secret is
    /// empty, or the HTTP client cannot be constructed.
    pub fn new(host: &str, port: u16, secret: impl Into<String>) -> Result<Self, MihomoError> {
        let address: IpAddr = host.parse().map_err(|_| {
            MihomoError::InvalidConfig("controller host must be an IP address".into())
        })?;
        if !address.is_loopback() {
            return Err(MihomoError::InvalidConfig(
                "controller must use a loopback address".into(),
            ));
        }
        let secret = secret.into();
        if secret.trim().is_empty() {
            return Err(MihomoError::InvalidConfig(
                "controller secret must not be empty".into(),
            ));
        }
        let host = match address {
            IpAddr::V4(_) => address.to_string(),
            IpAddr::V6(_) => format!("[{address}]"),
        };
        Ok(Self {
            base_url: format!("http://{host}:{port}"),
            secret,
            client: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(2))
                .timeout(Duration::from_secs(5))
                .build()?,
        })
    }

    fn get(&self, path: &str) -> reqwest::RequestBuilder {
        self.client
            .get(format!("{}{path}", self.base_url))
            .bearer_auth(&self.secret)
    }

    /// Reads the running Mihomo version.
    ///
    /// # Errors
    ///
    /// Returns an error when the controller request or response decoding fails.
    pub async fn version(&self) -> Result<VersionResponse, MihomoError> {
        Ok(self
            .get("/version")
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?)
    }

    /// Reads the active Mihomo configuration from the controller.
    ///
    /// # Errors
    ///
    /// Returns an error when the controller request or response decoding fails.
    pub async fn configs(&self) -> Result<Value, MihomoError> {
        Ok(self
            .get("/configs")
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?)
    }

    /// Summarizes the readiness and rule count of configured rule providers.
    ///
    /// # Errors
    ///
    /// Returns an error when the controller request fails, its response cannot
    /// be decoded, or it omits the provider map.
    pub async fn provider_summary(&self) -> Result<ProviderStatus, MihomoError> {
        let value = self
            .get("/providers/rules")
            .send()
            .await?
            .error_for_status()?
            .json::<Value>()
            .await?;
        summarize_rule_providers(&value)
    }

    /// Replaces the active Mihomo configuration without restarting the process.
    ///
    /// # Errors
    ///
    /// Returns an error when the controller request fails or does not return
    /// HTTP 204 No Content.
    pub async fn hot_reload(&self, config_path: &Path) -> Result<(), MihomoError> {
        let response = self
            .client
            .put(format!("{}/configs?force=true", self.base_url))
            .bearer_auth(&self.secret)
            .json(&serde_json::json!({ "path": config_path }))
            .send()
            .await?;
        if response.status() != StatusCode::NO_CONTENT {
            return Err(MihomoError::UnexpectedStatus(response.status()));
        }
        Ok(())
    }

    /// Waits until Mihomo and every rule provider are ready.
    ///
    /// # Errors
    ///
    /// Returns [`MihomoError::Cancelled`] when cancelled or
    /// [`MihomoError::ReadinessTimeout`] after the supplied timeout.
    pub async fn wait_until_ready(
        &self,
        timeout: Duration,
        cancel: CancellationToken,
    ) -> Result<ProviderStatus, MihomoError> {
        let deadline = tokio::time::Instant::now() + timeout;
        let mut last_status;
        loop {
            if cancel.is_cancelled() {
                return Err(MihomoError::Cancelled);
            }
            match (self.version().await, self.provider_summary().await) {
                (Ok(_), Ok(providers)) => {
                    last_status = format!(
                        "providers {}/{} ready, {} rules loaded",
                        providers.ready, providers.total, providers.rules_loaded
                    );
                    if providers.total > 0 && providers.ready == providers.total {
                        return Ok(providers);
                    }
                }
                (Err(error), _) => {
                    last_status = format!("controller unavailable: {error}");
                }
                (Ok(_), Err(error)) => {
                    last_status = format!("provider status unavailable: {error}");
                }
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(MihomoError::ReadinessTimeout(last_status));
            }
            tokio::select! {
                () = cancel.cancelled() => return Err(MihomoError::Cancelled),
                () = tokio::time::sleep(Duration::from_millis(250)) => {}
            }
        }
    }

    /// Opens a controller WebSocket and returns its asynchronous log receiver.
    #[must_use]
    pub fn stream_logs(&self, level: &str) -> mpsc::Receiver<Result<MihomoLog, MihomoError>> {
        let (sender, receiver) = mpsc::channel(128);
        let url = format!(
            "{}/logs?level={level}",
            self.base_url.replacen("http", "ws", 1)
        );
        let secret = self.secret.clone();
        tokio::spawn(async move {
            let mut request = match url.into_client_request() {
                Ok(request) => request,
                Err(error) => {
                    let _ = sender
                        .send(Err(MihomoError::WebSocket(error.to_string())))
                        .await;
                    return;
                }
            };
            let value = match format!("Bearer {secret}").parse() {
                Ok(value) => value,
                Err(error) => {
                    let _ = sender
                        .send(Err(MihomoError::WebSocket(format!(
                            "invalid authorization header: {error}"
                        ))))
                        .await;
                    return;
                }
            };
            request.headers_mut().insert(header::AUTHORIZATION, value);
            let (mut stream, _) = match connect_async(request).await {
                Ok(connection) => connection,
                Err(error) => {
                    let _ = sender
                        .send(Err(MihomoError::WebSocket(error.to_string())))
                        .await;
                    return;
                }
            };
            while let Some(message) = stream.next().await {
                let result = message
                    .map_err(|error| MihomoError::WebSocket(error.to_string()))
                    .and_then(|message| {
                        serde_json::from_slice::<MihomoLog>(&message.into_data())
                            .map_err(|error| MihomoError::WebSocket(error.to_string()))
                    });
                if sender.send(result).await.is_err() {
                    break;
                }
            }
        });
        receiver
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct VersionResponse {
    pub version: String,
    #[serde(default)]
    pub meta: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderStatus {
    pub ready: u32,
    pub total: u32,
    pub rules_loaded: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct MihomoLog {
    #[serde(rename = "type")]
    pub level: String,
    pub payload: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExitIpResponse {
    pub ip: String,
}

/// Resolves the public egress IP through the configured Hiddify SOCKS proxy.
///
/// # Errors
///
/// Returns an error when the proxy/client cannot be configured, the request
/// fails, or the service returns an invalid IP address.
pub async fn probe_hiddify_egress(
    host: &str,
    port: u16,
    timeout: Duration,
) -> Result<String, MihomoError> {
    let proxy = reqwest::Proxy::all(format!("socks5h://{host}:{port}"))?;
    let client = reqwest::Client::builder()
        .proxy(proxy)
        .connect_timeout(timeout)
        .timeout(timeout)
        .build()?;
    client
        .get("https://www.gstatic.com/generate_204")
        .send()
        .await?
        .error_for_status()?;
    let ip_response = client
        .get("https://api.ipify.org?format=json")
        .send()
        .await
        .ok()
        .filter(|item| item.status().is_success());
    if let Some(ip_response) = ip_response {
        if let Ok(payload) = ip_response.json::<ExitIpResponse>().await {
            if payload.ip.parse::<IpAddr>().is_ok() {
                return Ok(payload.ip);
            }
        }
    }
    Ok("unknown".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use iran_split_rules::{DirectRule, DirectTarget};
    use std::net::Ipv4Addr;

    fn paths() -> RuntimePaths {
        RuntimePaths {
            private_networks: "/runtime/private.txt".into(),
            iran_domains: "/runtime/iran-domains.txt".into(),
            iran_networks: "/runtime/iran-networks.txt".into(),
            custom_direct_domains: "/runtime/custom-direct-domains.txt".into(),
            custom_direct_ips: "/runtime/custom-direct-ips.txt".into(),
        }
    }

    #[test]
    fn generated_config_is_loopback_secret_and_precedence_safe() {
        let app = AppConfig::default();
        let custom = DirectRulesDocument {
            revision: 1,
            rules: vec![DirectRule {
                target: DirectTarget::Ip(Ipv4Addr::new(203, 0, 113, 1).into()),
                resolved_ips: vec![],
                created_at: chrono::Utc::now(),
                refreshed_at: None,
            }],
        };
        let generated = generate_config(&app, Platform::Linux, &paths(), &custom).expect("config");
        assert!(generated
            .yaml
            .contains("external-controller: 127.0.0.1:19090"));
        assert!(generated.yaml.contains("secret:"));
        assert!(generated.yaml.contains("PROCESS-NAME,hiddify,DIRECT"));
        assert!(generated
            .yaml
            .contains("PROCESS-NAME-WILDCARD,*Hiddify*,DIRECT"));
        assert!(generated
            .yaml
            .contains("PROCESS-NAME,iran-split-desk,DIRECT"));
        assert!(generated.yaml.contains("MATCH,VPN"));
        assert!(generated.yaml.contains("path: private.txt"));
        assert!(!generated.yaml.contains("/runtime/"));
        assert_eq!(generated.sha256.len(), 64);
    }

    #[test]
    fn windows_enables_strict_route_and_hiddify_bypass() {
        let generated = generate_config(
            &AppConfig::default(),
            Platform::Windows,
            &paths(),
            &DirectRulesDocument::default(),
        )
        .expect("config");
        assert!(generated.yaml.contains("strict-route: true"));
        assert!(generated.yaml.contains("PROCESS-NAME,Hiddify.exe,DIRECT"));
        assert!(generated
            .yaml
            .contains("PROCESS-NAME-WILDCARD,*Hiddify*,DIRECT"));
        assert!(generated.yaml.contains("PROCESS-NAME,BiFlow.exe,DIRECT"));
    }

    #[test]
    fn controller_rejects_remote_binding_and_empty_secret() {
        assert!(ControllerClient::new("0.0.0.0", 9090, "secret").is_err());
        assert!(ControllerClient::new("127.0.0.1", 9090, "").is_err());
    }

    #[test]
    fn empty_custom_providers_count_as_ready() {
        let summary = summarize_rule_providers(&serde_json::json!({
            "providers": {
                "custom-direct-domains": { "ruleCount": 0 },
                "custom-direct-ips": { "ruleCount": 0 },
                "iran-domains": { "ruleCount": 12 },
                "iran-networks": { "ruleCount": 4 },
                "private-networks": { "ruleCount": 8 }
            }
        }))
        .expect("summary");
        assert_eq!(summary.ready, 5);
        assert_eq!(summary.total, 5);
        assert_eq!(summary.rules_loaded, 24);
    }

    #[test]
    fn bundled_provider_without_rules_is_not_ready() {
        let summary = summarize_rule_providers(&serde_json::json!({
            "providers": {
                "custom-direct-domains": { "ruleCount": 0 },
                "iran-domains": { "ruleCount": 0 },
                "iran-networks": { "ruleCount": 4 },
                "private-networks": { "ruleCount": 8 }
            }
        }))
        .expect("summary");
        assert_eq!(summary.ready, 3);
        assert_eq!(summary.total, 4);
    }

    #[test]
    fn provider_error_is_not_ready() {
        let summary = summarize_rule_providers(&serde_json::json!({
            "providers": {
                "custom-direct-domains": { "error": "read failed", "ruleCount": 0 },
                "iran-domains": { "ruleCount": 12 }
            }
        }))
        .expect("summary");
        assert_eq!(summary.ready, 1);
        assert_eq!(summary.total, 2);
    }

    #[tokio::test]
    async fn validates_generated_config_with_vendored_mihomo_binary() {
        let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .expect("workspace root");
        let (mihomo, platform) = if cfg!(windows) {
            (
                workspace.join("vendor/mihomo/windows-x86_64/mihomo.exe"),
                Platform::Windows,
            )
        } else {
            (
                workspace.join("vendor/mihomo/linux-x86_64/mihomo"),
                Platform::Linux,
            )
        };
        if !mihomo.is_file() {
            eprintln!("skipping vendored Mihomo validation: {}", mihomo.display());
            return;
        }

        let generation = tempfile::tempdir().expect("tempdir");
        let rules = workspace.join("resources/rules");
        for name in ["private.txt", "iran-domains.txt", "iran-networks.txt"] {
            std::fs::copy(rules.join(name), generation.path().join(name)).expect("rule file");
        }
        std::fs::write(generation.path().join("custom-direct-domains.txt"), "")
            .expect("custom domains");
        std::fs::write(generation.path().join("custom-direct-ips.txt"), "").expect("custom ips");

        let generated = generate_config(
            &AppConfig::default(),
            platform,
            &paths(),
            &DirectRulesDocument::default(),
        )
        .expect("config");
        let config_path = generation.path().join("config.yaml");
        std::fs::write(&config_path, generated.yaml.as_bytes()).expect("config yaml");

        validate_with_binary(&mihomo, &config_path, Duration::from_secs(30))
            .await
            .expect("mihomo should accept the generated configuration");
    }
}
