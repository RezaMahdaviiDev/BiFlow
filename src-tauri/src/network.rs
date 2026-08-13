use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{net::IpAddr, time::Duration};
use tracing::{info, warn};

const COUNTRY_ENDPOINT: &str = "https://api.country.is/?fields=city";
const IP_ENDPOINT: &str = "https://api.ipify.org?format=json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InternetState {
    Checking,
    Online,
    Offline,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NetworkStatus {
    pub state: InternetState,
    pub public_ip: Option<String>,
    pub country_code: Option<String>,
    pub city: Option<String>,
    pub checked_at: DateTime<Utc>,
    pub detail: Option<String>,
}

impl Default for NetworkStatus {
    fn default() -> Self {
        Self {
            state: InternetState::Checking,
            public_ip: None,
            country_code: None,
            city: None,
            checked_at: Utc::now(),
            detail: Some("Checking internet connectivity".into()),
        }
    }
}

#[derive(Debug, Deserialize)]
struct CountryResponse {
    ip: String,
    country: String,
    city: Option<String>,
}

#[derive(Debug, Deserialize)]
struct IpResponse {
    ip: String,
}

#[derive(Debug, Clone)]
pub struct NetworkMonitor {
    proxy_client: reqwest::Client,
    direct_client: reqwest::Client,
}

impl NetworkMonitor {
    /// Creates a network monitor with bounded request times.
    ///
    /// # Errors
    ///
    /// Returns an error when the HTTPS client cannot be constructed.
    pub fn new() -> Result<Self, reqwest::Error> {
        Ok(Self {
            proxy_client: client(false)?,
            direct_client: client(true)?,
        })
    }

    pub async fn check(&self) -> NetworkStatus {
        match Self::check_with(&self.proxy_client).await {
            Ok(status) => {
                info!(
                    event = "network.check_completed",
                    section = "network",
                    initiator = "network_monitor",
                    cause = "none",
                    trace_route = "network_monitor->proxy_aware_client->public_ip_services",
                    state = ?status.state,
                    "proxy-aware network check completed"
                );
                status
            }
            Err(proxy_error) => match Self::check_with(&self.direct_client).await {
                Ok(status) => {
                    warn!(
                        event = "network.proxy_check_failed",
                        section = "network",
                        initiator = "network_monitor",
                        cause = %proxy_error,
                        trace_route = "network_monitor->proxy_aware_client->direct_retry",
                        "proxy-aware check failed; direct retry succeeded"
                    );
                    status
                }
                Err(direct_error) => {
                    warn!(
                        event = "network.check_failed",
                        section = "network",
                        initiator = "network_monitor",
                        cause = %format!("proxy: {proxy_error}; direct: {direct_error}"),
                        trace_route = "network_monitor->proxy_aware_client->direct_client->offline",
                        "all bounded network checks failed"
                    );
                    NetworkStatus {
                        state: InternetState::Offline,
                        public_ip: None,
                        country_code: None,
                        city: None,
                        checked_at: Utc::now(),
                        detail: Some(format!(
                            "Proxy-aware connectivity checks failed ({proxy_error}); direct checks failed ({direct_error})"
                        )),
                    }
                }
            },
        }
    }

    async fn check_with(client: &reqwest::Client) -> Result<NetworkStatus, String> {
        let (country, ip) = tokio::join!(Self::country_status(client), Self::ip_status(client));
        match (country, ip) {
            (Ok(status), _) | (Err(_), Ok(status)) => Ok(status),
            (Err(country_error), Err(ip_error)) => {
                Err(format!("country: {country_error}; IP: {ip_error}"))
            }
        }
    }

    async fn country_status(client: &reqwest::Client) -> Result<NetworkStatus, String> {
        let response = client
            .get(COUNTRY_ENDPOINT)
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(|error| error.to_string())?
            .json::<CountryResponse>()
            .await
            .map_err(|error| error.to_string())?;
        status_from_country(response)
    }

    async fn ip_status(client: &reqwest::Client) -> Result<NetworkStatus, String> {
        let response = client
            .get(IP_ENDPOINT)
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(|error| error.to_string())?
            .json::<IpResponse>()
            .await
            .map_err(|error| error.to_string())?;
        response
            .ip
            .parse::<IpAddr>()
            .map_err(|_| "IP service returned an invalid address".to_owned())?;
        Ok(NetworkStatus {
            state: InternetState::Online,
            public_ip: Some(response.ip),
            country_code: None,
            city: None,
            checked_at: Utc::now(),
            detail: Some("Internet is reachable; country lookup is temporarily unavailable".into()),
        })
    }
}

fn client(direct: bool) -> Result<reqwest::Client, reqwest::Error> {
    let mut builder = reqwest::Client::builder()
        .user_agent(concat!("BiFlow/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(8))
        .redirect(reqwest::redirect::Policy::limited(4));
    if direct {
        builder = builder.no_proxy();
    }
    builder.build()
}

fn status_from_country(response: CountryResponse) -> Result<NetworkStatus, String> {
    response
        .ip
        .parse::<IpAddr>()
        .map_err(|_| "country service returned an invalid IP address".to_owned())?;
    let country = response.country.trim().to_ascii_uppercase();
    if country.len() != 2 || !country.bytes().all(|byte| byte.is_ascii_alphabetic()) {
        return Err("country service returned an invalid country code".into());
    }
    Ok(NetworkStatus {
        state: InternetState::Online,
        public_ip: Some(response.ip),
        country_code: Some(country),
        city: response.city.filter(|city| !city.trim().is_empty()),
        checked_at: Utc::now(),
        detail: Some("Internet is reachable".into()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn country_response_becomes_online_status() {
        let status = status_from_country(CountryResponse {
            ip: "203.0.113.8".into(),
            country: "ir".into(),
            city: Some("Tehran".into()),
        })
        .expect("status");

        assert_eq!(status.state, InternetState::Online);
        assert_eq!(status.public_ip.as_deref(), Some("203.0.113.8"));
        assert_eq!(status.country_code.as_deref(), Some("IR"));
        assert_eq!(status.city.as_deref(), Some("Tehran"));
    }

    #[test]
    fn invalid_country_payload_is_rejected() {
        let result = status_from_country(CountryResponse {
            ip: "not-an-ip".into(),
            country: "unknown".into(),
            city: None,
        });

        assert!(result.is_err());
    }
}
