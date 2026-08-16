use iran_split_core::StackPhase;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrayLabels {
    pub connection_id: &'static str,
    pub connection_label: &'static str,
    pub pause_id: &'static str,
    pub pause_label: &'static str,
}

/// One item from each pair: Connect/Disconnect and Pause/Resume.
#[must_use]
pub fn labels_for(phase: StackPhase) -> TrayLabels {
    let connected = matches!(
        phase,
        StackPhase::Running | StackPhase::Degraded | StackPhase::Paused
    );
    let paused = matches!(phase, StackPhase::Paused);
    TrayLabels {
        connection_id: if connected { "disconnect" } else { "connect" },
        connection_label: if connected { "Disconnect" } else { "Connect" },
        pause_id: if paused { "resume" } else { "pause" },
        pause_label: if paused { "Resume" } else { "Pause" },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stopped_shows_connect_and_pause() {
        let labels = labels_for(StackPhase::Stopped);
        assert_eq!(labels.connection_id, "connect");
        assert_eq!(labels.connection_label, "Connect");
        assert_eq!(labels.pause_id, "pause");
        assert_eq!(labels.pause_label, "Pause");
    }

    #[test]
    fn running_shows_disconnect_and_pause() {
        let labels = labels_for(StackPhase::Running);
        assert_eq!(labels.connection_id, "disconnect");
        assert_eq!(labels.pause_id, "pause");
    }

    #[test]
    fn paused_shows_disconnect_and_resume() {
        let labels = labels_for(StackPhase::Paused);
        assert_eq!(labels.connection_id, "disconnect");
        assert_eq!(labels.connection_label, "Disconnect");
        assert_eq!(labels.pause_id, "resume");
        assert_eq!(labels.pause_label, "Resume");
    }

    #[test]
    fn never_emits_both_options_from_the_same_pair() {
        for phase in [
            StackPhase::Uninitialized,
            StackPhase::Stopped,
            StackPhase::StartingHiddify,
            StackPhase::PreparingRuntime,
            StackPhase::ValidatingConfig,
            StackPhase::StartingCore,
            StackPhase::CheckingReadiness,
            StackPhase::Running,
            StackPhase::Paused,
            StackPhase::Degraded,
            StackPhase::Stopping,
            StackPhase::Recovering,
            StackPhase::Error,
        ] {
            let labels = labels_for(phase);
            assert!((labels.connection_id == "connect") ^ (labels.connection_id == "disconnect"));
            assert!((labels.pause_id == "pause") ^ (labels.pause_id == "resume"));
        }
    }
}
