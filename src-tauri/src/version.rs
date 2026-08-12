#[must_use]
pub fn app_version() -> &'static str {
    include_str!("../../version").trim()
}

#[cfg(test)]
mod tests {
    use super::app_version;

    #[test]
    fn version_file_is_semver() {
        let version = app_version();
        let parts: Vec<_> = version.split('.').collect();
        assert_eq!(parts.len(), 3);
        assert!(parts.iter().all(|part| part.chars().all(|ch| ch.is_ascii_digit())));
    }
}
