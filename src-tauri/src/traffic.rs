use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TrafficTotals {
    pub sent: u64,
    pub received: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PersistedTraffic {
    pub lifetime_sent: u64,
    pub lifetime_received: u64,
    pub last_session_sent: u64,
    pub last_session_received: u64,
}

/// Folds a Mihomo session into lifetime totals so disconnect does not zero the bar.
#[must_use]
pub fn accumulate(
    store: &mut PersistedTraffic,
    session_sent: u64,
    session_received: u64,
    connected: bool,
) -> TrafficTotals {
    if connected {
        if session_sent < store.last_session_sent || session_received < store.last_session_received
        {
            store.lifetime_sent = store.lifetime_sent.saturating_add(store.last_session_sent);
            store.lifetime_received = store
                .lifetime_received
                .saturating_add(store.last_session_received);
        }
        store.last_session_sent = session_sent;
        store.last_session_received = session_received;
    } else if store.last_session_sent > 0 || store.last_session_received > 0 {
        store.lifetime_sent = store.lifetime_sent.saturating_add(store.last_session_sent);
        store.lifetime_received = store
            .lifetime_received
            .saturating_add(store.last_session_received);
        store.last_session_sent = 0;
        store.last_session_received = 0;
    }
    TrafficTotals {
        sent: store.lifetime_sent.saturating_add(store.last_session_sent),
        received: store
            .lifetime_received
            .saturating_add(store.last_session_received),
    }
}

pub fn load(path: &Path) -> PersistedTraffic {
    fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// Writes persisted totals. Failures are logged by the caller.
///
/// # Errors
///
/// Returns an I/O or encode error when the file cannot be replaced.
pub fn save(path: &Path, store: &PersistedTraffic) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    fs::write(
        path,
        serde_json::to_vec_pretty(store).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disconnect_keeps_the_displayed_total() {
        let mut store = PersistedTraffic::default();
        let connected = accumulate(&mut store, 1_000, 2_000, true);
        assert_eq!(
            connected,
            TrafficTotals {
                sent: 1_000,
                received: 2_000
            }
        );
        let disconnected = accumulate(&mut store, 0, 0, false);
        assert_eq!(disconnected, connected);
        let reconnected = accumulate(&mut store, 50, 75, true);
        assert_eq!(
            reconnected,
            TrafficTotals {
                sent: 1_050,
                received: 2_075
            }
        );
    }

    #[test]
    fn a_mihomo_restart_folds_the_previous_session() {
        let mut store = PersistedTraffic::default();
        let first = accumulate(&mut store, 500, 500, true);
        assert_eq!(
            first,
            TrafficTotals {
                sent: 500,
                received: 500
            }
        );
        let after_restart = accumulate(&mut store, 10, 10, true);
        assert_eq!(
            after_restart,
            TrafficTotals {
                sent: 510,
                received: 510
            }
        );
    }

    #[test]
    fn save_replaces_the_totals_file_atomically_enough_to_reload() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("traffic-totals.json");
        let store = PersistedTraffic {
            lifetime_sent: 9_000,
            lifetime_received: 8_000,
            last_session_sent: 100,
            last_session_received: 200,
        };
        save(&path, &store).expect("save");
        assert_eq!(load(&path), store);
        assert_eq!(
            load(directory.path().join("missing.json").as_path()),
            PersistedTraffic::default()
        );
    }
}
