use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TrafficTotals {
    pub sent: u64,
    pub received: u64,
}

/// In-memory session counters. Lifetime is the desktop process, not a file.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SessionAccumulator {
    sent: u64,
    received: u64,
    last_generation_sent: Option<u64>,
    last_generation_received: Option<u64>,
}

impl SessionAccumulator {
    #[must_use]
    pub fn last_generation(&self) -> (u64, u64) {
        (
            self.last_generation_sent.unwrap_or(0),
            self.last_generation_received.unwrap_or(0),
        )
    }
}

/// Folds Mihomo `/connections` deltas into process-scoped totals.
///
/// A repeated poll of the same snapshot adds nothing. A counter decrease is a
/// new generation: the previous snapshot is already in the total, so only the
/// new baseline is added. Disconnect keeps the displayed total and clears the
/// generation cursor so the next connect starts a fresh delta.
#[must_use]
pub fn accumulate(
    store: &mut SessionAccumulator,
    session_sent: u64,
    session_received: u64,
    connected: bool,
) -> TrafficTotals {
    if !connected {
        store.last_generation_sent = None;
        store.last_generation_received = None;
        return TrafficTotals {
            sent: store.sent,
            received: store.received,
        };
    }
    match (store.last_generation_sent, store.last_generation_received) {
        (Some(previous_sent), Some(previous_received))
            if session_sent >= previous_sent && session_received >= previous_received =>
        {
            store.sent = store
                .sent
                .saturating_add(session_sent.saturating_sub(previous_sent));
            store.received = store
                .received
                .saturating_add(session_received.saturating_sub(previous_received));
        }
        _ => {
            store.sent = store.sent.saturating_add(session_sent);
            store.received = store.received.saturating_add(session_received);
        }
    }
    store.last_generation_sent = Some(session_sent);
    store.last_generation_received = Some(session_received);
    TrafficTotals {
        sent: store.sent,
        received: store.received,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disconnect_keeps_the_displayed_session_total() {
        let mut store = SessionAccumulator::default();
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
    fn a_repeated_poll_of_the_same_snapshot_adds_nothing() {
        let mut store = SessionAccumulator::default();
        let first = accumulate(&mut store, 500, 800, true);
        let again = accumulate(&mut store, 500, 800, true);
        assert_eq!(first, again);
        assert_eq!(
            first,
            TrafficTotals {
                sent: 500,
                received: 800
            }
        );
    }

    #[test]
    fn a_mihomo_restart_folds_only_the_new_generation() {
        let mut store = SessionAccumulator::default();
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
    fn a_legacy_totals_file_is_not_loaded() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("traffic-totals.json");
        std::fs::write(
            &path,
            br#"{"lifetime_sent":9000,"lifetime_received":8000,"last_session_sent":100,"last_session_received":200}"#,
        )
        .expect("legacy");
        assert!(path.is_file());
        let mut store = SessionAccumulator::default();
        assert_eq!(
            accumulate(&mut store, 0, 0, false),
            TrafficTotals::default()
        );
    }
}
