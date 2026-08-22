use serde::Serialize;
use std::time::{Duration, Instant};
use tracing::info;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(8);
// Above this the site answered but a browser would feel it crawl; the UI turns
// the row yellow instead of green.
const SLOW_THRESHOLD: Duration = Duration::from_millis(2500);

/// Which network path a probe target is expected to take, mirroring the
/// generated Mihomo rules: `.ir` stays DIRECT, everything else is MATCH,VPN.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProbePath {
    Vpn,
    Direct,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReachabilityStatus {
    Ok,
    Slow,
    Unreachable,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReachabilityResult {
    pub id: &'static str,
    pub domain: &'static str,
    pub path: ProbePath,
    /// Whether the VPN-path probes actually went through the Hiddify proxy.
    /// When the stack is down they fall back to a direct request, and the UI
    /// explains failures differently.
    pub via_proxy: bool,
    pub status: ReachabilityStatus,
    pub latency_ms: Option<u64>,
    pub detail: Option<String>,
}

struct Target {
    id: &'static str,
    domain: &'static str,
    url: &'static str,
    path: ProbePath,
}

// Fixed, well-known probe hosts — never user content, so ids may be logged.
const TARGETS: [Target; 3] = [
    Target {
        id: "google",
        domain: "google.com",
        url: "https://www.google.com/generate_204",
        path: ProbePath::Vpn,
    },
    Target {
        id: "facebook",
        domain: "facebook.com",
        url: "https://www.facebook.com/favicon.ico",
        path: ProbePath::Vpn,
    },
    Target {
        id: "iran",
        domain: "iran.ir",
        url: "https://iran.ir/",
        path: ProbePath::Direct,
    },
];

/// Probes the fixed reachability targets concurrently.
///
/// VPN-path targets go through the Hiddify proxy when one is supplied (the
/// desktop process is PROCESS-NAME-bypassed in Mihomo, so probing the mixed
/// port would silently test the DIRECT path instead). DIRECT-path targets and
/// the no-proxy fallback use a plain client that ignores environment proxies.
pub async fn check_all(hiddify_proxy: Option<(String, u16)>) -> Vec<ReachabilityResult> {
    let direct = plain_client();
    let proxied = hiddify_proxy.as_ref().and_then(|(host, port)| {
        reqwest::Proxy::all(format!("socks5h://{host}:{port}"))
            .ok()
            .and_then(|proxy| base_client_builder().no_proxy().proxy(proxy).build().ok())
    });

    let probe = |target: &'static Target| {
        let client = match (target.path, &proxied, &direct) {
            (ProbePath::Vpn, Some(client), _) => Some((client.clone(), true)),
            (_, _, Some(client)) => Some((client.clone(), false)),
            _ => None,
        };
        async move {
            let Some((client, via_proxy)) = client else {
                return ReachabilityResult {
                    id: target.id,
                    domain: target.domain,
                    path: target.path,
                    via_proxy: false,
                    status: ReachabilityStatus::Unreachable,
                    latency_ms: None,
                    detail: Some("probe client could not be built".into()),
                };
            };
            probe_target(target, &client, via_proxy).await
        }
    };

    let [first, second, third] = [&TARGETS[0], &TARGETS[1], &TARGETS[2]];
    let (first, second, third) = tokio::join!(probe(first), probe(second), probe(third));
    let results = vec![first, second, third];
    for result in &results {
        info!(
            event = "reachability.probe_completed",
            section = "network",
            initiator = "tauri_command",
            cause = "user_requested",
            trace_route = "tauri_command->network->reachability_probe",
            target_id = result.id,
            path = ?result.path,
            via_proxy = result.via_proxy,
            status = ?result.status,
            latency_ms = result.latency_ms,
            "reachability probe completed"
        );
    }
    results
}

async fn probe_target(
    target: &Target,
    client: &reqwest::Client,
    via_proxy: bool,
) -> ReachabilityResult {
    let started = Instant::now();
    // Any HTTP status counts as reachable: the point is whether the TLS
    // handshake survives, which is exactly what SNI filtering kills.
    let outcome = client.get(target.url).send().await;
    let elapsed = started.elapsed();
    let (status, latency_ms, detail) = match outcome {
        Ok(_) => (
            classify(elapsed),
            Some(u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)),
            None,
        ),
        Err(error) => (
            ReachabilityStatus::Unreachable,
            None,
            Some(error.without_url().to_string()),
        ),
    };
    ReachabilityResult {
        id: target.id,
        domain: target.domain,
        path: target.path,
        via_proxy,
        status,
        latency_ms,
        detail,
    }
}

fn classify(elapsed: Duration) -> ReachabilityStatus {
    if elapsed > SLOW_THRESHOLD {
        ReachabilityStatus::Slow
    } else {
        ReachabilityStatus::Ok
    }
}

fn base_client_builder() -> reqwest::ClientBuilder {
    reqwest::Client::builder()
        .user_agent(concat!("BiFlow/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .redirect(reqwest::redirect::Policy::limited(4))
}

fn plain_client() -> Option<reqwest::Client> {
    base_client_builder().no_proxy().build().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fast_response_is_ok_and_slow_response_is_slow() {
        assert_eq!(classify(Duration::from_millis(300)), ReachabilityStatus::Ok);
        assert_eq!(
            classify(SLOW_THRESHOLD + Duration::from_millis(1)),
            ReachabilityStatus::Slow
        );
    }

    #[test]
    fn targets_cover_both_paths_with_fixed_domains() {
        assert_eq!(TARGETS.len(), 3);
        assert!(TARGETS
            .iter()
            .any(|target| target.domain == "iran.ir" && target.path == ProbePath::Direct));
        assert!(
            TARGETS
                .iter()
                .filter(|target| target.path == ProbePath::Vpn)
                .count()
                == 2
        );
    }

    #[test]
    fn result_serializes_snake_case_for_the_ui() {
        let value = serde_json::to_value(ReachabilityResult {
            id: "google",
            domain: "google.com",
            path: ProbePath::Vpn,
            via_proxy: true,
            status: ReachabilityStatus::Unreachable,
            latency_ms: None,
            detail: Some("tls closed".into()),
        })
        .expect("serialize");
        assert_eq!(value["path"], "vpn");
        assert_eq!(value["status"], "unreachable");
        assert_eq!(value["via_proxy"], true);
    }
}
