#![allow(clippy::module_name_repetitions)]

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tempfile::NamedTempFile;
use thiserror::Error;
use tracing::{info, warn};

const GITHUB_RAW: &str = "https://raw.githubusercontent.com/Chocolate4U/Iran-clash-rules/release";
const JSDELIVR: &str = "https://cdn.jsdelivr.net/gh/chocolate4u/Iran-clash-rules@release";
const JSDELIVR_FASTLY: &str = "https://fastly.jsdelivr.net/gh/chocolate4u/Iran-clash-rules@release";
const GITHUB_RELEASE: &str =
    "https://github.com/Chocolate4U/Iran-clash-rules/releases/latest/download";
const META_FILE: &str = "sync-meta.json";
const MAX_BYTES: usize = 20 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum CloudSyncError {
    #[error("rule I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("rule metadata is invalid: {0}")]
    Json(#[from] serde_json::Error),
    #[error("rule publication failed: {0}")]
    Persist(#[from] tempfile::PersistError),
    #[error("cloud rule download failed: {0}")]
    Fetch(String),
    #[error("downloaded rule set {0} is empty or truncated")]
    InvalidSet(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    Domain,
    IpCidr,
}

#[derive(Debug, Clone, Copy)]
struct CatalogEntry {
    local_name: &'static str,
    remote_name: &'static str,
    kind: ProviderKind,
    min_entries: u64,
}

const CATALOG: [CatalogEntry; 3] = [
    CatalogEntry {
        local_name: "iran-domains.txt",
        remote_name: "ir.txt",
        kind: ProviderKind::Domain,
        min_entries: 1_000,
    },
    CatalogEntry {
        local_name: "iran-networks.txt",
        remote_name: "ircidr.txt",
        kind: ProviderKind::IpCidr,
        min_entries: 100,
    },
    CatalogEntry {
        local_name: "private.txt",
        remote_name: "private.txt",
        kind: ProviderKind::IpCidr,
        min_entries: 5,
    },
];

#[must_use]
pub fn fail_safe_urls(remote_name: &str) -> Vec<String> {
    vec![
        format!("{GITHUB_RAW}/{remote_name}"),
        format!("{JSDELIVR}/{remote_name}"),
        format!("{JSDELIVR_FASTLY}/{remote_name}"),
        format!("{GITHUB_RELEASE}/{remote_name}"),
    ]
}

#[must_use]
pub fn provider_entry_count(text: &str) -> u64 {
    u64::try_from(provider_lines(text).count()).unwrap_or(u64::MAX)
}

#[must_use]
pub fn resolve_provider_path(cache_dir: &Path, bundled_dir: &Path, name: &str) -> PathBuf {
    let cached = cache_dir.join(name);
    if cached.is_file() {
        cached
    } else {
        bundled_dir.join(name)
    }
}

fn provider_lines(text: &str) -> impl Iterator<Item = &str> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#') && !line.starts_with("payload:"))
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

#[async_trait]
pub trait RuleFetcher: Send + Sync {
    async fn fetch(&self, url: &str) -> Result<Vec<u8>, CloudSyncError>;
}

#[derive(Debug)]
pub struct ReqwestFetcher {
    client: reqwest::Client,
}

impl ReqwestFetcher {
    fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .user_agent("BiFlow/0.1.0")
                .connect_timeout(Duration::from_secs(15))
                .timeout(Duration::from_secs(60))
                .redirect(reqwest::redirect::Policy::limited(8))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
        }
    }
}

impl Default for ReqwestFetcher {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl RuleFetcher for ReqwestFetcher {
    async fn fetch(&self, url: &str) -> Result<Vec<u8>, CloudSyncError> {
        let response = self
            .client
            .get(url)
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(|error| CloudSyncError::Fetch(error.to_string()))?;
        let bytes = response
            .bytes()
            .await
            .map_err(|error| CloudSyncError::Fetch(error.to_string()))?;
        if bytes.len() > MAX_BYTES {
            return Err(CloudSyncError::Fetch(format!(
                "response exceeded {MAX_BYTES} bytes"
            )));
        }
        Ok(bytes.to_vec())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloudRuleSetStatus {
    pub id: String,
    pub kind: ProviderKind,
    pub entry_count: u64,
    pub source: String,
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloudRulesStatus {
    pub domain_count: u64,
    pub ip_count: u64,
    pub last_synced_at: Option<DateTime<Utc>>,
    pub source: String,
    pub sets: Vec<CloudRuleSetStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct SyncMeta {
    last_synced_at: Option<DateTime<Utc>>,
    source: String,
    sets: Vec<CloudRuleSetStatus>,
}

#[derive(Clone)]
pub struct CloudRuleStore {
    bundled_dir: PathBuf,
    cache_dir: PathBuf,
    fetcher: Arc<dyn RuleFetcher>,
}

impl std::fmt::Debug for CloudRuleStore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CloudRuleStore")
            .field("bundled_dir", &self.bundled_dir)
            .field("cache_dir", &self.cache_dir)
            .finish_non_exhaustive()
    }
}

impl CloudRuleStore {
    #[must_use]
    pub fn load(bundled_dir: impl Into<PathBuf>, cache_dir: impl Into<PathBuf>) -> Self {
        Self::with_fetcher(bundled_dir, cache_dir, Arc::new(ReqwestFetcher::new()))
    }

    #[must_use]
    pub fn with_fetcher(
        bundled_dir: impl Into<PathBuf>,
        cache_dir: impl Into<PathBuf>,
        fetcher: Arc<dyn RuleFetcher>,
    ) -> Self {
        Self {
            bundled_dir: bundled_dir.into(),
            cache_dir: cache_dir.into(),
            fetcher,
        }
    }

    #[must_use]
    pub fn resolve(&self, name: &str) -> PathBuf {
        resolve_provider_path(&self.cache_dir, &self.bundled_dir, name)
    }

    /// Returns status from a complete cache, falling back to bundled rules.
    ///
    /// # Errors
    ///
    /// Returns [`CloudSyncError`] when cached metadata cannot be read or decoded.
    pub fn status(&self) -> Result<CloudRulesStatus, CloudSyncError> {
        let meta = self.read_meta()?;
        let cache_complete = CATALOG
            .iter()
            .all(|entry| self.cache_dir.join(entry.local_name).is_file());
        if cache_complete {
            if let Some(status) = status_from_meta(&meta) {
                return Ok(status);
            }
        }
        self.bundled_status()
    }

    /// Downloads, validates, and atomically publishes every cloud rule set.
    ///
    /// # Errors
    ///
    /// Returns [`CloudSyncError`] for download, encoding, validation, metadata,
    /// or atomic persistence failures. Existing cached rules remain available
    /// when a replacement cannot be published.
    pub async fn sync(&self) -> Result<CloudRulesStatus, CloudSyncError> {
        info!(
            event = "cloud_rules.sync_started",
            section = "cloud_rules",
            initiator = "cloud_rule_store",
            cause = "user_request",
            trace_route = "tauri_command->cloud_rule_store->fail_safe_sources",
            provider_count = CATALOG.len(),
            "cloud rule synchronization started"
        );
        fs::create_dir_all(&self.cache_dir)?;
        let mut sets = Vec::with_capacity(CATALOG.len());
        let mut used_source = String::from("bundled");
        for entry in CATALOG {
            let (bytes, source) = self.download_entry(entry).await?;
            let text = std::str::from_utf8(&bytes)
                .map_err(|_| CloudSyncError::InvalidSet(entry.local_name.into()))?;
            let count = provider_entry_count(text);
            if count < entry.min_entries {
                return Err(CloudSyncError::InvalidSet(entry.local_name.into()));
            }
            write_atomic(&self.cache_dir.join(entry.local_name), &bytes)?;
            used_source.clone_from(&source);
            sets.push(CloudRuleSetStatus {
                id: entry.local_name.trim_end_matches(".txt").to_owned(),
                kind: entry.kind,
                entry_count: count,
                source,
                sha256: Some(sha256_hex(&bytes)),
            });
        }
        let meta = SyncMeta {
            last_synced_at: Some(Utc::now()),
            source: used_source,
            sets,
        };
        write_atomic(
            &self.cache_dir.join(META_FILE),
            &serde_json::to_vec_pretty(&meta)?,
        )?;
        let status = status_from_meta(&meta).unwrap_or(CloudRulesStatus {
            domain_count: 0,
            ip_count: 0,
            last_synced_at: meta.last_synced_at,
            source: meta.source,
            sets: meta.sets,
        });
        info!(
            event = "cloud_rules.sync_completed",
            section = "cloud_rules",
            initiator = "cloud_rule_store",
            cause = "none",
            trace_route = "tauri_command->cloud_rule_store->atomic_cache",
            domain_count = status.domain_count,
            ip_count = status.ip_count,
            source = status.source,
            "cloud rule synchronization completed"
        );
        Ok(status)
    }

    fn bundled_status(&self) -> Result<CloudRulesStatus, CloudSyncError> {
        let mut sets = Vec::new();
        for entry in CATALOG {
            let path = self.bundled_dir.join(entry.local_name);
            let bytes = fs::read(&path)?;
            let text = std::str::from_utf8(&bytes)
                .map_err(|_| CloudSyncError::InvalidSet(entry.local_name.into()))?;
            let count = provider_entry_count(text);
            if count < entry.min_entries {
                return Err(CloudSyncError::InvalidSet(entry.local_name.into()));
            }
            sets.push(CloudRuleSetStatus {
                id: entry.local_name.trim_end_matches(".txt").to_owned(),
                kind: entry.kind,
                entry_count: count,
                source: "bundled".into(),
                sha256: Some(sha256_hex(&bytes)),
            });
        }
        Ok(summarize(sets, None, "bundled"))
    }

    fn read_meta(&self) -> Result<SyncMeta, CloudSyncError> {
        let path = self.cache_dir.join(META_FILE);
        if !path.is_file() {
            return Ok(SyncMeta::default());
        }
        Ok(serde_json::from_slice(&fs::read(path)?)?)
    }

    async fn download_entry(
        &self,
        entry: CatalogEntry,
    ) -> Result<(Vec<u8>, String), CloudSyncError> {
        let mut last_error = CloudSyncError::Fetch("no fail-safe URL succeeded".into());
        for url in fail_safe_urls(entry.remote_name) {
            match self.fetcher.fetch(&url).await {
                Ok(bytes) => {
                    let text = String::from_utf8_lossy(&bytes);
                    if provider_entry_count(&text) >= entry.min_entries {
                        return Ok((bytes, source_label(&url)));
                    }
                    last_error = CloudSyncError::InvalidSet(entry.local_name.into());
                    warn!(
                        event = "cloud_rules.source_rejected",
                        section = "cloud_rules",
                        initiator = "fail_safe_fetcher",
                        cause = "provider_below_minimum_entries",
                        trace_route = "cloud_rule_store->fail_safe_source->validation",
                        provider = entry.local_name,
                        source = source_label(&url),
                        "cloud rule source returned an incomplete provider"
                    );
                }
                Err(error) => {
                    warn!(
                        event = "cloud_rules.source_failed",
                        section = "cloud_rules",
                        initiator = "fail_safe_fetcher",
                        cause = %error,
                        trace_route = "cloud_rule_store->fail_safe_source->next_source",
                        provider = entry.local_name,
                        source = source_label(&url),
                        "cloud rule source failed; trying the next source"
                    );
                    last_error = error;
                }
            }
        }
        Err(last_error)
    }
}

fn status_from_meta(meta: &SyncMeta) -> Option<CloudRulesStatus> {
    if meta.sets.is_empty() {
        return None;
    }
    Some(summarize(
        meta.sets.clone(),
        meta.last_synced_at,
        &meta.source,
    ))
}

fn summarize(
    sets: Vec<CloudRuleSetStatus>,
    last_synced_at: Option<DateTime<Utc>>,
    source: &str,
) -> CloudRulesStatus {
    let domain_count = sets
        .iter()
        .filter(|set| set.kind == ProviderKind::Domain)
        .map(|set| set.entry_count)
        .sum();
    let ip_count = sets
        .iter()
        .filter(|set| set.kind == ProviderKind::IpCidr)
        .map(|set| set.entry_count)
        .sum();
    CloudRulesStatus {
        domain_count,
        ip_count,
        last_synced_at,
        source: source.to_owned(),
        sets,
    }
}

fn source_label(url: &str) -> String {
    if url.contains("jsdelivr.net") {
        "jsdelivr".into()
    } else if url.contains("releases/latest") {
        "github-release".into()
    } else {
        "github".into()
    }
}

fn write_atomic(path: &Path, content: &[u8]) -> Result<(), CloudSyncError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let mut temporary = NamedTempFile::new_in(parent)?;
    temporary.write_all(content)?;
    temporary.as_file().sync_all()?;
    temporary.persist(path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::fmt::Write as _;

    struct MapFetcher {
        responses: HashMap<String, Result<Vec<u8>, String>>,
    }

    #[async_trait]
    impl RuleFetcher for MapFetcher {
        async fn fetch(&self, url: &str) -> Result<Vec<u8>, CloudSyncError> {
            match self.responses.get(url) {
                Some(Ok(bytes)) => Ok(bytes.clone()),
                Some(Err(message)) => Err(CloudSyncError::Fetch(message.clone())),
                None => Err(CloudSyncError::Fetch("missing".into())),
            }
        }
    }

    fn domain_payload(count: usize) -> Vec<u8> {
        (0..count)
            .fold(String::new(), |mut payload, index| {
                writeln!(payload, "+.example{index}.ir").expect("write to string");
                payload
            })
            .into_bytes()
    }

    fn cidr_payload(count: usize) -> Vec<u8> {
        (0..count)
            .fold(String::new(), |mut payload, index| {
                writeln!(payload, "203.0.{index}.0/24").expect("write to string");
                payload
            })
            .into_bytes()
    }

    #[test]
    fn fail_safe_chain_prefers_github_then_jsdelivr() {
        let urls = fail_safe_urls("ir.txt");
        assert!(urls[0].starts_with(
            "https://raw.githubusercontent.com/Chocolate4U/Iran-clash-rules/release/ir.txt"
        ));
        assert!(urls[1].contains("cdn.jsdelivr.net"));
        assert!(urls[2].contains("fastly.jsdelivr.net"));
        assert!(urls[3].contains("releases/latest/download/ir.txt"));
    }

    #[test]
    fn counts_ignore_comments_and_payload_headers() {
        let text = "# comment\npayload:\n+.digikala.com\n\n5.22.0.0/16\n";
        assert_eq!(provider_entry_count(text), 2);
    }

    #[test]
    fn incomplete_bundled_snapshot_is_rejected() {
        let directory = tempfile::tempdir().expect("tempdir");
        let store = CloudRuleStore::load(directory.path(), directory.path().join("cache"));

        assert!(matches!(store.status(), Err(CloudSyncError::Io(_))));
    }

    #[tokio::test]
    async fn sync_uses_next_fail_safe_url_and_keeps_last_good_on_later_failure() {
        let directory = tempfile::tempdir().expect("tempdir");
        let bundled = directory.path().join("bundled");
        let cache = directory.path().join("cache");
        fs::create_dir_all(&bundled).expect("bundled");
        fs::write(bundled.join("iran-domains.txt"), "+.old.ir\n").expect("domains");
        fs::write(bundled.join("iran-networks.txt"), "1.2.3.0/24\n").expect("networks");
        fs::write(bundled.join("private.txt"), "10.0.0.0/8\n").expect("private");

        let mut responses = HashMap::new();
        for (remote, body) in [
            ("ir.txt", domain_payload(1_000)),
            ("ircidr.txt", cidr_payload(100)),
            ("private.txt", cidr_payload(8)),
        ] {
            let urls = fail_safe_urls(remote);
            responses.insert(urls[0].clone(), Err("blocked".into()));
            responses.insert(urls[1].clone(), Ok(body));
        }
        let store = CloudRuleStore::with_fetcher(
            bundled,
            cache.clone(),
            Arc::new(MapFetcher { responses }),
        );
        let status = store.sync().await.expect("sync");
        assert_eq!(status.domain_count, 1_000);
        assert_eq!(status.ip_count, 108);
        assert_eq!(status.source, "jsdelivr");
        assert!(status.last_synced_at.is_some());
        assert!(cache.join("iran-domains.txt").is_file());

        let failing = CloudRuleStore::with_fetcher(
            directory.path().join("bundled"),
            cache.clone(),
            Arc::new(MapFetcher {
                responses: HashMap::new(),
            }),
        );
        assert!(failing.sync().await.is_err());
        let kept = failing.status().expect("status");
        assert_eq!(kept.domain_count, 1_000);
        assert_eq!(kept.source, "jsdelivr");
    }
}
