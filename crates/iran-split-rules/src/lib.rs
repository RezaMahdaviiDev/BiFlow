mod cloud;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use ipnet::IpNet;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs,
    io::Write,
    net::{IpAddr, SocketAddr},
    path::{Path, PathBuf},
    sync::Arc,
};
use tempfile::NamedTempFile;
use thiserror::Error;
use tokio::sync::Mutex;

pub use cloud::{
    provider_entry_count, resolve_provider_path, CloudRuleSetStatus, CloudRuleStore,
    CloudRulesStatus, CloudSyncError, RuleFetcher,
};

#[derive(Debug, Error)]
pub enum RuleError {
    #[error("invalid direct rule: {0}")]
    InvalidRule(String),
    #[error("rule I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("rule data is invalid: {0}")]
    Json(#[from] serde_json::Error),
    #[error("rule publication failed: {0}")]
    Persist(#[from] tempfile::PersistError),
    #[error("DNS resolution failed: {0}")]
    Resolve(String),
    #[error("rule revision conflict: expected {expected}, found {actual}")]
    RevisionConflict { expected: u64, actual: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum DirectTarget {
    Domain(String),
    Ip(IpAddr),
}

impl DirectTarget {
    /// Parses an exact domain or IP address into a direct-routing target.
    ///
    /// # Errors
    ///
    /// Returns [`RuleError::InvalidRule`] when the input is not a valid IP
    /// address or normalized domain.
    pub fn parse(input: &str) -> Result<Self, RuleError> {
        let input = input.trim();
        if let Ok(address) = input.parse::<IpAddr>() {
            return Ok(Self::Ip(address));
        }
        normalize_domain(input).map(Self::Domain)
    }

    #[must_use]
    pub fn display_value(&self) -> String {
        match self {
            Self::Domain(domain) => domain.clone(),
            Self::Ip(address) => address.to_string(),
        }
    }
}

/// Normalizes and validates an exact domain name for direct routing.
///
/// # Errors
///
/// Returns [`RuleError::InvalidRule`] for URLs, paths, wildcards, user info,
/// invalid IDNA, or malformed DNS labels.
pub fn normalize_domain(input: &str) -> Result<String, RuleError> {
    let candidate = input.trim().trim_end_matches('.').to_lowercase();
    if candidate.is_empty()
        || candidate.len() > 253
        || candidate.contains("://")
        || candidate.contains('/')
        || candidate.contains('*')
        || candidate.contains('@')
    {
        return Err(RuleError::InvalidRule(
            "enter an exact domain without a URL, path, wildcard, or user info".into(),
        ));
    }
    let ascii = idna::domain_to_ascii(&candidate)
        .map_err(|_| RuleError::InvalidRule("domain cannot be converted to IDNA ASCII".into()))?;
    let labels: Vec<_> = ascii.split('.').collect();
    if labels.len() < 2
        || labels.iter().any(|label| {
            label.is_empty()
                || label.len() > 63
                || label.starts_with('-')
                || label.ends_with('-')
                || !label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
    {
        return Err(RuleError::InvalidRule(
            "domain must contain valid DNS labels and a suffix".into(),
        ));
    }
    Ok(ascii)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectRule {
    pub target: DirectTarget,
    pub resolved_ips: Vec<IpAddr>,
    pub created_at: DateTime<Utc>,
    pub refreshed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DirectRulesDocument {
    pub revision: u64,
    pub rules: Vec<DirectRule>,
}

#[async_trait]
pub trait Resolver: Send + Sync {
    async fn resolve(&self, domain: &str) -> Result<Vec<IpAddr>, RuleError>;
}

#[derive(Debug, Default)]
pub struct SystemResolver;

#[async_trait]
impl Resolver for SystemResolver {
    async fn resolve(&self, domain: &str) -> Result<Vec<IpAddr>, RuleError> {
        let addresses = tokio::net::lookup_host((domain, 0))
            .await
            .map_err(|error| RuleError::Resolve(error.to_string()))?;
        Ok(unique_addresses(addresses.map(|address| address.ip())))
    }
}

#[derive(Debug, Clone)]
pub struct DohResolver {
    client: reqwest::Client,
    endpoint: &'static str,
}

impl Default for DohResolver {
    fn default() -> Self {
        Self {
            client: reqwest::Client::new(),
            endpoint: "https://cloudflare-dns.com/dns-query",
        }
    }
}

#[derive(Debug, Deserialize)]
struct DohResponse {
    #[serde(default, rename = "Answer")]
    answers: Vec<DohAnswer>,
}

#[derive(Debug, Deserialize)]
struct DohAnswer {
    data: String,
}

#[async_trait]
impl Resolver for DohResolver {
    async fn resolve(&self, domain: &str) -> Result<Vec<IpAddr>, RuleError> {
        let mut values = Vec::new();
        for record_type in ["A", "AAAA"] {
            let response = self
                .client
                .get(self.endpoint)
                .header(reqwest::header::ACCEPT, "application/dns-json")
                .query(&[("name", domain), ("type", record_type)])
                .send()
                .await
                .and_then(reqwest::Response::error_for_status)
                .map_err(|error| RuleError::Resolve(error.to_string()))?
                .json::<DohResponse>()
                .await
                .map_err(|error| RuleError::Resolve(error.to_string()))?;
            values.extend(
                response
                    .answers
                    .into_iter()
                    .filter_map(|answer| answer.data.parse::<IpAddr>().ok()),
            );
        }
        Ok(unique_addresses(values))
    }
}

fn unique_addresses(values: impl IntoIterator<Item = IpAddr>) -> Vec<IpAddr> {
    let mut values: Vec<_> = values
        .into_iter()
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    values.sort_unstable();
    values
}

#[derive(Clone)]
pub struct RuleManager {
    path: PathBuf,
    resolver: Arc<dyn Resolver>,
    document: Arc<Mutex<DirectRulesDocument>>,
}

impl std::fmt::Debug for RuleManager {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RuleManager")
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

impl RuleManager {
    /// Loads the direct-rule document at `path`, or starts empty when absent.
    ///
    /// # Errors
    ///
    /// Returns [`RuleError`] when the document cannot be read or decoded.
    pub fn load(path: impl Into<PathBuf>, resolver: Arc<dyn Resolver>) -> Result<Self, RuleError> {
        let path = path.into();
        let document = if path.exists() {
            serde_json::from_slice(&fs::read(&path)?)?
        } else {
            DirectRulesDocument::default()
        };
        Ok(Self {
            path,
            resolver,
            document: Arc::new(Mutex::new(document)),
        })
    }

    pub async fn list(&self) -> DirectRulesDocument {
        self.document.lock().await.clone()
    }

    /// Adds an exact domain or IP rule at the expected document revision.
    ///
    /// # Errors
    ///
    /// Returns [`RuleError`] for invalid input, DNS resolution failure, a
    /// revision conflict, or an atomic persistence failure.
    pub async fn add(
        &self,
        input: &str,
        expected_revision: u64,
    ) -> Result<DirectRulesDocument, RuleError> {
        let target = DirectTarget::parse(input)?;
        let resolved_ips = match &target {
            DirectTarget::Domain(domain) => self.resolver.resolve(domain).await?,
            DirectTarget::Ip(address) => vec![*address],
        };
        let mut document = self.document.lock().await;
        ensure_revision(&document, expected_revision)?;
        if document.rules.iter().any(|rule| rule.target == target) {
            return Ok(document.clone());
        }
        let now = Utc::now();
        document.rules.push(DirectRule {
            target,
            resolved_ips,
            created_at: now,
            refreshed_at: Some(now),
        });
        document
            .rules
            .sort_by_key(|rule| rule.target.display_value());
        document.revision = document.revision.saturating_add(1);
        publish(&self.path, &document)?;
        Ok(document.clone())
    }

    /// Removes an exact domain or IP rule at the expected document revision.
    ///
    /// # Errors
    ///
    /// Returns [`RuleError`] for invalid input, a revision conflict, or an
    /// atomic persistence failure.
    pub async fn remove(
        &self,
        input: &str,
        expected_revision: u64,
    ) -> Result<DirectRulesDocument, RuleError> {
        let target = DirectTarget::parse(input)?;
        let mut document = self.document.lock().await;
        ensure_revision(&document, expected_revision)?;
        let original_length = document.rules.len();
        document.rules.retain(|rule| rule.target != target);
        if document.rules.len() != original_length {
            document.revision = document.revision.saturating_add(1);
            publish(&self.path, &document)?;
        }
        Ok(document.clone())
    }

    /// Refreshes resolved IP addresses for every stored domain rule.
    ///
    /// # Errors
    ///
    /// Returns [`RuleError`] when DNS resolution or atomic persistence fails.
    pub async fn refresh(&self) -> Result<DirectRulesDocument, RuleError> {
        let domains = {
            let document = self.document.lock().await;
            document
                .rules
                .iter()
                .filter_map(|rule| match &rule.target {
                    DirectTarget::Domain(domain) => Some(domain.clone()),
                    DirectTarget::Ip(_) => None,
                })
                .collect::<Vec<_>>()
        };
        let mut resolved = Vec::with_capacity(domains.len());
        for domain in domains {
            resolved.push((domain.clone(), self.resolver.resolve(&domain).await?));
        }
        let now = Utc::now();
        let mut document = self.document.lock().await;
        for rule in &mut document.rules {
            if let DirectTarget::Domain(domain) = &rule.target {
                if let Some((_, addresses)) = resolved.iter().find(|(name, _)| name == domain) {
                    rule.resolved_ips.clone_from(addresses);
                    rule.refreshed_at = Some(now);
                }
            }
        }
        document.revision = document.revision.saturating_add(1);
        publish(&self.path, &document)?;
        Ok(document.clone())
    }
}

fn ensure_revision(
    document: &DirectRulesDocument,
    expected_revision: u64,
) -> Result<(), RuleError> {
    if document.revision != expected_revision {
        return Err(RuleError::RevisionConflict {
            expected: expected_revision,
            actual: document.revision,
        });
    }
    Ok(())
}

fn publish(path: &Path, document: &DirectRulesDocument) -> Result<(), RuleError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let mut temporary = NamedTempFile::new_in(parent)?;
    temporary.write_all(&serde_json::to_vec_pretty(document)?)?;
    temporary.as_file().sync_all()?;
    temporary.persist(path)?;
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outbound {
    Direct,
    Vpn,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionReason {
    CustomRule,
    PrivateOrLocal,
    IranDomain,
    IranCidr,
    DefaultProxy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteDecision {
    pub outbound: Outbound,
    pub reason: DecisionReason,
    pub matched_rule: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct RuleSet {
    custom_domains: HashSet<String>,
    custom_ips: HashSet<IpAddr>,
    iran_domains: HashSet<String>,
    iran_cidrs: Vec<IpNet>,
}

impl RuleSet {
    pub fn from_sources(
        custom: &DirectRulesDocument,
        iran_domains: impl IntoIterator<Item = String>,
        iran_cidrs: impl IntoIterator<Item = IpNet>,
    ) -> Self {
        let mut set = Self {
            iran_domains: iran_domains.into_iter().collect(),
            iran_cidrs: iran_cidrs.into_iter().collect(),
            ..Self::default()
        };
        for rule in &custom.rules {
            match &rule.target {
                DirectTarget::Domain(domain) => {
                    set.custom_domains.insert(domain.clone());
                }
                DirectTarget::Ip(address) => {
                    set.custom_ips.insert(*address);
                }
            }
            set.custom_ips.extend(rule.resolved_ips.iter().copied());
        }
        set
    }

    /// Chooses the outbound route for an exact domain or IP target.
    ///
    /// # Errors
    ///
    /// Returns [`RuleError::InvalidRule`] when `target` is neither a valid IP
    /// address nor a normalized domain.
    pub fn decide(&self, target: &str) -> Result<RouteDecision, RuleError> {
        if let Ok(address) = target.parse::<IpAddr>() {
            return Ok(self.decide_ip(address));
        }
        let domain = normalize_domain(target)?;
        if self.custom_domains.contains(&domain) {
            return Ok(direct(DecisionReason::CustomRule, Some(domain)));
        }
        if let Some(rule) = self
            .iran_domains
            .iter()
            .find(|rule| domain == **rule || domain.ends_with(&format!(".{rule}")))
        {
            return Ok(direct(DecisionReason::IranDomain, Some(rule.clone())));
        }
        Ok(RouteDecision {
            outbound: Outbound::Vpn,
            reason: DecisionReason::DefaultProxy,
            matched_rule: Some("MATCH".into()),
        })
    }

    fn decide_ip(&self, address: IpAddr) -> RouteDecision {
        if self.custom_ips.contains(&address) {
            return direct(DecisionReason::CustomRule, Some(address.to_string()));
        }
        if is_private_or_local(address) {
            return direct(DecisionReason::PrivateOrLocal, Some(address.to_string()));
        }
        if let Some(network) = self
            .iran_cidrs
            .iter()
            .find(|network| network.contains(&address))
        {
            return direct(DecisionReason::IranCidr, Some(network.to_string()));
        }
        RouteDecision {
            outbound: Outbound::Vpn,
            reason: DecisionReason::DefaultProxy,
            matched_rule: Some("MATCH".into()),
        }
    }
}

fn direct(reason: DecisionReason, matched_rule: Option<String>) -> RouteDecision {
    RouteDecision {
        outbound: Outbound::Direct,
        reason,
        matched_rule,
    }
}

fn is_private_or_local(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => {
            address.is_private()
                || address.is_loopback()
                || address.is_link_local()
                || u32::from(address) & 0xffc0_0000 == u32::from_be_bytes([100, 64, 0, 0])
        }
        IpAddr::V6(address) => {
            address.is_loopback() || address.is_unique_local() || address.is_unicast_link_local()
        }
    }
}

#[allow(dead_code)]
fn _socket_address_example(address: SocketAddr) -> IpAddr {
    address.ip()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct FixedResolver;

    #[async_trait]
    impl Resolver for FixedResolver {
        async fn resolve(&self, _domain: &str) -> Result<Vec<IpAddr>, RuleError> {
            Ok(vec!["203.0.113.9".parse().expect("IP")])
        }
    }

    #[test]
    fn normalizes_unicode_and_rejects_urls() {
        assert_eq!(
            normalize_domain("Example.COM.").expect("domain"),
            "example.com"
        );
        assert!(normalize_domain("https://example.com/path").is_err());
        assert!(normalize_domain("*.example.com").is_err());
    }

    #[tokio::test]
    async fn mutations_are_revisioned_and_atomic() {
        let directory = tempfile::tempdir().expect("tempdir");
        let manager = RuleManager::load(
            directory.path().join("direct-rules.json"),
            Arc::new(FixedResolver),
        )
        .expect("manager");
        let added = manager.add("example.com", 0).await.expect("add");
        assert_eq!(added.revision, 1);
        assert_eq!(added.rules[0].resolved_ips.len(), 1);
        assert!(manager.remove("example.com", 0).await.is_err());
        let removed = manager.remove("example.com", 1).await.expect("remove");
        assert!(removed.rules.is_empty());
    }

    #[test]
    fn precedence_is_custom_then_private_then_iran_then_proxy() {
        let custom = DirectRulesDocument {
            revision: 1,
            rules: vec![DirectRule {
                target: DirectTarget::Domain("example.com".into()),
                resolved_ips: vec![],
                created_at: Utc::now(),
                refreshed_at: None,
            }],
        };
        let set = RuleSet::from_sources(
            &custom,
            ["digikala.com".into()],
            ["5.22.0.0/16".parse().expect("CIDR")],
        );
        assert_eq!(
            set.decide("example.com").expect("decision").reason,
            DecisionReason::CustomRule
        );
        assert_eq!(
            set.decide("192.168.1.1").expect("decision").reason,
            DecisionReason::PrivateOrLocal
        );
        assert_eq!(
            set.decide("100.64.1.1").expect("decision").reason,
            DecisionReason::PrivateOrLocal
        );
        assert_eq!(
            set.decide("cdn.digikala.com").expect("decision").reason,
            DecisionReason::IranDomain
        );
        assert_eq!(
            set.decide("5.22.12.1").expect("decision").reason,
            DecisionReason::IranCidr
        );
        assert_eq!(
            set.decide("openai.com").expect("decision").outbound,
            Outbound::Vpn
        );
    }
}
