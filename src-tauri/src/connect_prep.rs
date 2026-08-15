use iran_split_core::ComponentPhase;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectRequirement {
    Helper,
    Hiddify,
    Mihomo,
}

impl ConnectRequirement {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Helper => "helper",
            Self::Hiddify => "hiddify",
            Self::Mihomo => "mihomo",
        }
    }
}

#[must_use]
pub fn helper_is_ready(phase: ComponentPhase) -> bool {
    !matches!(phase, ComponentPhase::Unavailable | ComponentPhase::Error)
}

#[must_use]
pub fn missing_requirements(
    helper_ready: bool,
    hiddify_installed: bool,
    mihomo_installed: bool,
) -> Vec<ConnectRequirement> {
    let mut missing = Vec::new();
    if !helper_ready {
        missing.push(ConnectRequirement::Helper);
    }
    if !hiddify_installed {
        missing.push(ConnectRequirement::Hiddify);
    }
    if !mihomo_installed {
        missing.push(ConnectRequirement::Mihomo);
    }
    missing
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_helper_then_hiddify_then_mihomo() {
        assert_eq!(
            missing_requirements(false, false, false),
            [
                ConnectRequirement::Helper,
                ConnectRequirement::Hiddify,
                ConnectRequirement::Mihomo
            ]
        );
        assert_eq!(
            missing_requirements(true, true, true),
            [] as [ConnectRequirement; 0]
        );
    }

    #[test]
    fn treats_only_unavailable_or_error_helpers_as_missing() {
        assert!(!helper_is_ready(ComponentPhase::Unavailable));
        assert!(!helper_is_ready(ComponentPhase::Error));
        assert!(helper_is_ready(ComponentPhase::Running));
        assert!(helper_is_ready(ComponentPhase::Stopped));
        assert!(helper_is_ready(ComponentPhase::Degraded));
    }
}
