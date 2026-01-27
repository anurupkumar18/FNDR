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
}
