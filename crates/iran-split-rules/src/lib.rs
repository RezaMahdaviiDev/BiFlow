mod canonical;
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

pub use canonical::{canonical_target, domain_matches_pin, registrable_domain};
pub use cloud::{
    bundled_snapshot_is_complete, ensure_bundled_snapshot, provider_entry_count,
    resolve_provider_path, CloudRuleSetStatus, CloudRuleStore, CloudRulesStatus, CloudSyncError,
    RuleFetcher,
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
        canonical_target(input)
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
    /// Hosts forced onto the VPN even when the Iran list would keep them
    /// direct. `default` so documents written before exclusions still load.
    #[serde(default)]
    pub vpn_rules: Vec<DirectRule>,
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
            let bytes = fs::read(&path)?;
            let original: DirectRulesDocument = serde_json::from_slice(&bytes)?;
            let migrated = canonicalize_document(original.clone());
            if migrated != original {
                backup_last_good(&path)?;
                publish(&path, &migrated)?;
            }
            migrated
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

    /// Adds an exact domain or IP rule to the DIRECT list.
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
        self.pin(input, Outbound::Direct, expected_revision).await
    }

    /// Pins an exact domain or IP to one outbound.
    ///
    /// A host belongs to at most one user list, so pinning it to an outbound
    /// drops any pin it had on the other one.
    ///
    /// # Errors
    ///
    /// Returns [`RuleError`] for invalid input, a private or local address
    /// pinned to the VPN, DNS resolution failure, a revision conflict, or an
    /// atomic persistence failure.
    pub async fn pin(
        &self,
        input: &str,
        outbound: Outbound,
        expected_revision: u64,
    ) -> Result<DirectRulesDocument, RuleError> {
        let target = DirectTarget::parse(input)?;
        if outbound == Outbound::Vpn {
            // Loopback, LAN, and CGNAT have to stay direct or the machine
            // loses its own network while the tunnel is up.
            if let DirectTarget::Ip(address) = &target {
                if is_private_or_local(*address) {
                    return Err(RuleError::InvalidRule(
                        "private, loopback, and carrier-grade NAT addresses cannot be sent through the VPN".into(),
                    ));
                }
            }
        }
        let resolved_ips = match &target {
            DirectTarget::Domain(_) => Vec::new(),
            DirectTarget::Ip(address) => vec![*address],
        };
        let mut document = self.document.lock().await;
        ensure_revision(&document, expected_revision)?;

        let already_pinned = match outbound {
            Outbound::Direct => &document.rules,
            Outbound::Vpn => &document.vpn_rules,
        }
        .iter()
        .any(|rule| rule.target == target);
        let other = match outbound {
            Outbound::Direct => &mut document.vpn_rules,
            Outbound::Vpn => &mut document.rules,
        };
        let dropped = other.len();
        other.retain(|rule| rule.target != target);
        let moved = other.len() != dropped;
        if already_pinned && !moved {
            return Ok(document.clone());
        }
        if !already_pinned {
            let now = Utc::now();
            let list = match outbound {
                Outbound::Direct => &mut document.rules,
                Outbound::Vpn => &mut document.vpn_rules,
            };
            list.push(DirectRule {
                target,
                resolved_ips,
                created_at: now,
                refreshed_at: Some(now),
            });
            list.sort_by_key(|rule| rule.target.display_value());
        }
        document.revision = document.revision.saturating_add(1);
        publish(&self.path, &document)?;
        Ok(document.clone())
    }

    /// Removes an exact domain or IP pin from whichever list holds it.
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
        let before = document.rules.len() + document.vpn_rules.len();
        document.rules.retain(|rule| rule.target != target);
        document.vpn_rules.retain(|rule| rule.target != target);
        if document.rules.len() + document.vpn_rules.len() != before {
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
                .chain(document.vpn_rules.iter())
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
        let DirectRulesDocument {
            rules, vpn_rules, ..
        } = &mut *document;
        for rule in rules.iter_mut().chain(vpn_rules.iter_mut()) {
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

    /// Replaces the in-memory document and publishes it. Used to roll a failed
    /// live apply back to the last-good pins.
    ///
    /// # Errors
    ///
    /// Returns [`RuleError`] when the document cannot be written atomically.
    pub async fn restore(
        &self,
        document: DirectRulesDocument,
    ) -> Result<DirectRulesDocument, RuleError> {
        let mut current = self.document.lock().await;
        *current = document;
        publish(&self.path, &current)?;
        Ok(current.clone())
    }
}

fn canonicalize_document(mut document: DirectRulesDocument) -> DirectRulesDocument {
    let before = document.clone();
    document.rules = merge_canonical_rules(document.rules);
    document.vpn_rules = merge_canonical_rules(document.vpn_rules);
    if document.rules != before.rules || document.vpn_rules != before.vpn_rules {
        document.revision = document.revision.saturating_add(1);
    }
    document
}

fn merge_canonical_rules(rules: Vec<DirectRule>) -> Vec<DirectRule> {
    let mut merged: Vec<DirectRule> = Vec::new();
    for rule in rules {
        let target = match &rule.target {
            DirectTarget::Domain(domain) => {
                canonical_target(domain).unwrap_or_else(|_| rule.target.clone())
            }
            DirectTarget::Ip(_) => rule.target.clone(),
        };
        if let Some(existing) = merged.iter_mut().find(|item| item.target == target) {
            if rule.created_at < existing.created_at {
                existing.created_at = rule.created_at;
            }
            continue;
        }
        merged.push(DirectRule {
            target,
            resolved_ips: Vec::new(),
            created_at: rule.created_at,
            refreshed_at: rule.refreshed_at,
        });
    }
    merged.sort_by_key(|rule| rule.target.display_value());
    merged
}

fn backup_last_good(path: &Path) -> Result<(), RuleError> {
    let backup = path.with_extension("json.last-good");
    fs::copy(path, backup)?;
    Ok(())
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
    VpnRule,
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
    vpn_domains: HashSet<String>,
    vpn_ips: HashSet<IpAddr>,
    custom_domains: HashSet<String>,
    custom_ips: HashSet<IpAddr>,
    iran_domains: HashSet<String>,
    business_domains: HashSet<String>,
    iran_cidrs: Vec<IpNet>,
}

impl RuleSet {
    pub fn from_sources(
        custom: &DirectRulesDocument,
        iran_domains: impl IntoIterator<Item = String>,
        iran_cidrs: impl IntoIterator<Item = IpNet>,
        business_domains: impl IntoIterator<Item = String>,
    ) -> Self {
        let mut set = Self {
            iran_domains: iran_domains.into_iter().collect(),
            business_domains: business_domains.into_iter().collect(),
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
        }
        for rule in &custom.vpn_rules {
            match &rule.target {
                DirectTarget::Domain(domain) => {
                    set.vpn_domains.insert(domain.clone());
                }
                DirectTarget::Ip(address) => {
                    set.vpn_ips.insert(*address);
                }
            }
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
        let canonical = registrable_domain(&domain).unwrap_or_else(|_| domain.clone());
        if let Some(pin) = self
            .vpn_domains
            .iter()
            .find(|pin| domain_matches_pin(&domain, pin) || *pin == &canonical)
        {
            return Ok(RouteDecision {
                outbound: Outbound::Vpn,
                reason: DecisionReason::VpnRule,
                matched_rule: Some(pin.clone()),
            });
        }
        if let Some(pin) = self
            .custom_domains
            .iter()
            .find(|pin| domain_matches_pin(&domain, pin) || *pin == &canonical)
        {
            return Ok(direct(DecisionReason::CustomRule, Some(pin.clone())));
        }
        if let Some(rule) = self
            .iran_domains
            .iter()
            .find(|rule| domain == **rule || domain.ends_with(&format!(".{rule}")))
        {
            return Ok(direct(DecisionReason::IranDomain, Some(rule.clone())));
        }
        if let Some(rule) = self
            .business_domains
            .iter()
            .find(|pin| domain_matches_pin(&domain, pin) || *pin == &canonical)
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
        // Loopback and LAN stay direct even under an exclusion; the generated
        // config keeps private-networks ahead of the VPN rule sets too.
        if is_private_or_local(address) {
            return direct(DecisionReason::PrivateOrLocal, Some(address.to_string()));
        }
        if self.vpn_ips.contains(&address) {
            return RouteDecision {
                outbound: Outbound::Vpn,
                reason: DecisionReason::VpnRule,
                matched_rule: Some(address.to_string()),
            };
        }
        if self.custom_ips.contains(&address) {
            return direct(DecisionReason::CustomRule, Some(address.to_string()));
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

    fn iran_rule_set(custom: &DirectRulesDocument) -> RuleSet {
        RuleSet::from_sources(custom, ["ir".to_owned()], [], [])
    }

    #[tokio::test]
    async fn a_vpn_pin_overrides_the_bundled_iran_list_and_survives_removal() {
        let directory = tempfile::tempdir().expect("tempdir");
        let manager = RuleManager::load(
            directory.path().join("direct-rules.json"),
            Arc::new(FixedResolver),
        )
        .expect("manager");

        // iran.ir is DIRECT only because the bundled list carries `ir`.
        let decision = iran_rule_set(&manager.list().await)
            .decide("iran.ir")
            .expect("decide");
        assert_eq!(decision.outbound, Outbound::Direct);
        assert_eq!(decision.reason, DecisionReason::IranDomain);

        let pinned = manager
            .pin("iran.ir", Outbound::Vpn, 0)
            .await
            .expect("pin vpn");
        assert_eq!(pinned.vpn_rules.len(), 1);
        let decision = iran_rule_set(&pinned).decide("iran.ir").expect("decide");
        assert_eq!(decision.outbound, Outbound::Vpn);
        assert_eq!(decision.reason, DecisionReason::VpnRule);

        // Removing the pin must restore the bundled decision, not delete `ir`.
        let cleared = manager
            .remove("iran.ir", pinned.revision)
            .await
            .expect("remove");
        assert!(cleared.vpn_rules.is_empty());
        assert_eq!(
            iran_rule_set(&cleared)
                .decide("iran.ir")
                .expect("decide")
                .reason,
            DecisionReason::IranDomain
        );
    }

    #[tokio::test]
    async fn a_host_lives_in_exactly_one_user_list() {
        let directory = tempfile::tempdir().expect("tempdir");
        let manager = RuleManager::load(
            directory.path().join("direct-rules.json"),
            Arc::new(FixedResolver),
        )
        .expect("manager");

        let direct = manager.add("example.com", 0).await.expect("direct");
        assert_eq!(direct.rules.len(), 1);
        assert_eq!(
            iran_rule_set(&direct)
                .decide("example.com")
                .expect("decide")
                .reason,
            DecisionReason::CustomRule
        );

        let moved = manager
            .pin("example.com", Outbound::Vpn, direct.revision)
            .await
            .expect("vpn");
        assert!(moved.rules.is_empty(), "the direct pin must be dropped");
        assert_eq!(moved.vpn_rules.len(), 1);

        let back = manager
            .pin("example.com", Outbound::Direct, moved.revision)
            .await
            .expect("direct again");
        assert_eq!(back.rules.len(), 1);
        assert!(back.vpn_rules.is_empty());
    }

    #[tokio::test]
    async fn private_and_loopback_addresses_cannot_be_pinned_to_the_vpn() {
        let directory = tempfile::tempdir().expect("tempdir");
        let manager = RuleManager::load(
            directory.path().join("direct-rules.json"),
            Arc::new(FixedResolver),
        )
        .expect("manager");

        // The rejection happens before the revision check, so revision 0 holds
        // for every address here.
        for address in ["192.168.1.1", "127.0.0.1", "100.64.0.1", "::1"] {
            let error = manager
                .pin(address, Outbound::Vpn, 0)
                .await
                .expect_err(address);
            assert!(matches!(error, RuleError::InvalidRule(_)), "{address}");
        }
        // The same address is still allowed on the direct list.
        let pinned = manager
            .pin("192.168.1.1", Outbound::Direct, 0)
            .await
            .expect("direct");
        assert_eq!(pinned.rules.len(), 1);
    }

    #[tokio::test]
    async fn refresh_resolves_domains_on_both_lists() {
        let directory = tempfile::tempdir().expect("tempdir");
        let manager = RuleManager::load(
            directory.path().join("direct-rules.json"),
            Arc::new(FixedResolver),
        )
        .expect("manager");
        let direct = manager.add("direct.example", 0).await.expect("direct");
        let pinned = manager
            .pin("vpn.example", Outbound::Vpn, direct.revision)
            .await
            .expect("vpn");

        let refreshed = manager.refresh().await.expect("refresh");
        assert_eq!(refreshed.rules.len(), 1);
        assert_eq!(refreshed.vpn_rules.len(), 1);
        for rule in refreshed.rules.iter().chain(refreshed.vpn_rules.iter()) {
            assert!(rule.refreshed_at.is_some());
            assert_eq!(
                rule.resolved_ips,
                vec!["203.0.113.9".parse::<IpAddr>().expect("ip")]
            );
        }
        let _ = pinned;
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
        assert!(added.rules[0].resolved_ips.is_empty());
        assert!(manager.remove("example.com", 0).await.is_err());
        let removed = manager.remove("example.com", 1).await.expect("remove");
        assert!(removed.rules.is_empty());
    }

    #[test]
    fn precedence_is_custom_then_private_then_iran_then_proxy() {
        let custom = DirectRulesDocument {
            revision: 1,
            vpn_rules: Vec::new(),
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
            ["technolife.com".into()],
        );
        assert_eq!(
            set.decide("www.technolife.com").expect("catalog").reason,
            DecisionReason::IranDomain
        );
        assert_eq!(
            set.decide("www.technolife.com")
                .expect("catalog")
                .matched_rule
                .as_deref(),
            Some("technolife.com")
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

    #[test]
    fn bundled_catalog_sends_kavenegar_console_direct() {
        let catalog = include_str!("../../../resources/rules/iran-business-domains.txt")
            .lines()
            .filter_map(|line| line.strip_prefix("+.").map(str::to_owned));
        let set = RuleSet::from_sources(&DirectRulesDocument::default(), [], [], catalog);
        let decision = set.decide("console.kavenegar.com").expect("decide");
        assert_eq!(decision.outbound, Outbound::Direct);
        assert_eq!(decision.reason, DecisionReason::IranDomain);
        assert_eq!(decision.matched_rule.as_deref(), Some("kavenegar.com"));
    }

    #[tokio::test]
    async fn a_subdomain_pin_stores_the_registrable_root_and_covers_siblings() {
        let directory = tempfile::tempdir().expect("tempdir");
        let manager = RuleManager::load(
            directory.path().join("direct-rules.json"),
            Arc::new(FixedResolver),
        )
        .expect("manager");
        let added = manager.add("api.shop.example.com", 0).await.expect("add");
        assert_eq!(added.rules.len(), 1);
        assert_eq!(
            added.rules[0].target,
            DirectTarget::Domain("example.com".into())
        );
        let set = iran_rule_set(&added);
        assert_eq!(
            set.decide("www.example.com").expect("www").reason,
            DecisionReason::CustomRule
        );
        assert_eq!(
            set.decide("api.shop.example.com").expect("nested").reason,
            DecisionReason::CustomRule
        );
        assert_eq!(
            set.decide("notexample.com").expect("sibling").outbound,
            Outbound::Vpn
        );
        let moved = manager
            .pin("www.example.com", Outbound::Vpn, added.revision)
            .await
            .expect("move");
        assert!(moved.rules.is_empty());
        assert_eq!(moved.vpn_rules.len(), 1);
        assert_eq!(
            iran_rule_set(&moved)
                .decide("cdn.example.com")
                .expect("cdn")
                .outbound,
            Outbound::Vpn
        );
    }

    #[tokio::test]
    async fn a_private_suffix_tenant_does_not_pin_the_whole_suffix() {
        let directory = tempfile::tempdir().expect("tempdir");
        let manager = RuleManager::load(
            directory.path().join("direct-rules.json"),
            Arc::new(FixedResolver),
        )
        .expect("manager");
        let added = manager.add("user.github.io", 0).await.expect("pages");
        let set = iran_rule_set(&added);
        assert_eq!(
            set.decide("user.github.io").expect("self").reason,
            DecisionReason::CustomRule
        );
        assert_eq!(
            set.decide("other.github.io").expect("other").outbound,
            Outbound::Vpn
        );
    }

    #[tokio::test]
    async fn pin_does_not_wait_for_dns() {
        struct SlowResolver;
        #[async_trait]
        impl Resolver for SlowResolver {
            async fn resolve(&self, _domain: &str) -> Result<Vec<IpAddr>, RuleError> {
                tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                Ok(Vec::new())
            }
        }
        let directory = tempfile::tempdir().expect("tempdir");
        let manager = RuleManager::load(
            directory.path().join("direct-rules.json"),
            Arc::new(SlowResolver),
        )
        .expect("manager");
        let started = std::time::Instant::now();
        manager.add("example.com", 0).await.expect("add");
        assert!(
            started.elapsed() < std::time::Duration::from_millis(500),
            "pin must not wait on DNS"
        );
    }

    #[tokio::test]
    async fn load_migrates_exact_hosts_to_the_registrable_root() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("direct-rules.json");
        let original = DirectRulesDocument {
            revision: 4,
            rules: vec![
                DirectRule {
                    target: DirectTarget::Domain("www.example.com".into()),
                    resolved_ips: vec!["203.0.113.9".parse().expect("ip")],
                    created_at: Utc::now(),
                    refreshed_at: None,
                },
                DirectRule {
                    target: DirectTarget::Domain("api.example.com".into()),
                    resolved_ips: vec![],
                    created_at: Utc::now(),
                    refreshed_at: None,
                },
            ],
            vpn_rules: Vec::new(),
        };
        fs::write(&path, serde_json::to_vec_pretty(&original).expect("json")).expect("write");
        let manager = RuleManager::load(&path, Arc::new(FixedResolver)).expect("load");
        let loaded = manager.list().await;
        assert_eq!(loaded.rules.len(), 1);
        assert_eq!(
            loaded.rules[0].target,
            DirectTarget::Domain("example.com".into())
        );
        assert_eq!(loaded.revision, 5);
        assert!(path.with_extension("json.last-good").is_file());
    }
}
