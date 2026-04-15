//! Application blocklist management

/// Blocklist for applications that should not be captured
pub struct Blocklist;

impl Blocklist {
    /// Check if an application is blocked
    pub fn is_blocked(app_name: &str, blocklist: &[String]) -> bool {
        let app_lower = app_name.to_lowercase();
        blocklist.iter().any(|blocked| {
            let blocked_lower = blocked.to_lowercase();
            app_lower.contains(&blocked_lower) || blocked_lower.contains(&app_lower)
        })
    }

    /// Check if the frontmost app belongs to FNDR itself and should never be captured.
    pub fn is_internal_app(app_name: &str, bundle_id: Option<&str>) -> bool {
        let normalized_name = app_name.trim().to_lowercase();
        if normalized_name.starts_with("fndr") && !normalized_name.contains("meeting") {
            return true;
        }

        bundle_id.is_some_and(|bundle| {
            let normalized_bundle = bundle.trim().to_lowercase();
            normalized_bundle == "com.fndr"
                || normalized_bundle.starts_with("com.fndr.")
                || normalized_bundle.ends_with(".fndr")
                || normalized_bundle.contains(".fndr.")
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exact_match() {
        let blocklist = vec!["1Password".to_string()];
        assert!(Blocklist::is_blocked("1Password", &blocklist));
    }

    #[test]
    fn test_case_insensitive() {
        let blocklist = vec!["1Password".to_string()];
        assert!(Blocklist::is_blocked("1password", &blocklist));
    }

    #[test]
    fn test_partial_match() {
        let blocklist = vec!["Keychain".to_string()];
        assert!(Blocklist::is_blocked("Keychain Access", &blocklist));
    }

    #[test]
    fn test_not_blocked() {
        let blocklist = vec!["1Password".to_string()];
        assert!(!Blocklist::is_blocked("Safari", &blocklist));
    }

    #[test]
    fn test_detects_internal_app_by_name() {
        assert!(Blocklist::is_internal_app("FNDR", None));
        assert!(!Blocklist::is_internal_app("FNDR Meetings", None));
    }

    #[test]
    fn test_detects_internal_app_by_bundle() {
        assert!(Blocklist::is_internal_app(
            "Anything",
            Some("com.fndr.desktop")
        ));
        assert!(!Blocklist::is_internal_app(
            "Finder",
            Some("com.apple.finder")
        ));
    }
}
