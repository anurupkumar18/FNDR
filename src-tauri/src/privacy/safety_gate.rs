#[derive(Debug, Clone, PartialEq)]
pub enum SafetyDecision {
    Allow,
    Redact,
    SkipStorage,
}

impl SafetyDecision {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Redact => "redact",
            Self::SkipStorage => "skip_storage",
        }
    }
}

/// Evaluate whether to allow, redact, or skip a capture frame.
/// Checks are purely deterministic (no model).
pub fn evaluate(
    app_name: Option<&str>,
    bundle_id: Option<&str>,
    url: Option<&str>,
    window_title: Option<&str>,
    ocr_text: Option<&str>,
    user_blocklist: &[String],
) -> SafetyDecision {
    let app = app_name.unwrap_or("").to_ascii_lowercase();
    let bundle = bundle_id.unwrap_or("").to_ascii_lowercase();
    let title = window_title.unwrap_or("").to_ascii_lowercase();
    let url_lower = url.unwrap_or("").to_ascii_lowercase();
    let text_lower = ocr_text.unwrap_or("").to_ascii_lowercase();

    for blocked in user_blocklist {
        let b = blocked.to_ascii_lowercase();
        if app.contains(&b) || title.contains(&b) || url_lower.contains(&b) {
            return SafetyDecision::SkipStorage;
        }
    }

    if let Some(id) = bundle_id {
        let id_lower = id.to_ascii_lowercase();
        if (id_lower.starts_with("com.fndr") || id_lower.contains(".fndr."))
            && !app.contains("fndr meeting")
        {
            return SafetyDecision::SkipStorage;
        }
    }

    const PASSWORD_MANAGERS: &[&str] = &[
        "1password",
        "bitwarden",
        "keychain",
        "lastpass",
        "dashlane",
        "keepass",
    ];
    for pm in PASSWORD_MANAGERS {
        if app.contains(pm) || bundle.contains(pm) {
            return SafetyDecision::SkipStorage;
        }
    }

    if title.contains("private") && (title.contains("browsing") || title.contains("window")) {
        return SafetyDecision::SkipStorage;
    }
    if title.contains("incognito") {
        return SafetyDecision::SkipStorage;
    }

    const BANKING_DOMAINS: &[&str] = &[
        "chase.com",
        "bankofamerica",
        "wellsfargo",
        "citibank",
        "capitalone",
        "usbank",
        "fidelity",
        "vanguard",
        "schwab",
        "americanexpress",
        "discover.com",
        "paypal.com",
        "venmo.com",
        "robinhood.com",
    ];
    for domain in BANKING_DOMAINS {
        if url_lower.contains(domain) {
            return SafetyDecision::SkipStorage;
        }
    }
    const BANKING_TITLES: &[&str] = &["online banking", "mobile banking", "bank account portal"];
    if BANKING_TITLES.iter().any(|marker| title.contains(marker)) {
        return SafetyDecision::SkipStorage;
    }

    const MEDICAL_DOMAINS: &[&str] = &["epic.com", "mychart", "healthportal", "patientportal"];
    for domain in MEDICAL_DOMAINS {
        if url_lower.contains(domain) {
            return SafetyDecision::SkipStorage;
        }
    }
    const MEDICAL_TITLES: &[&str] = &[
        "mychart",
        "patient portal",
        "health portal",
        "medical record",
    ];
    if MEDICAL_TITLES.iter().any(|marker| title.contains(marker)) {
        return SafetyDecision::SkipStorage;
    }

    const AUTH_INDICATORS: &[&str] = &[
        "sign in",
        "log in",
        "login",
        "authenticate",
        "authorization",
        "oauth",
        "saml",
        "two-factor",
        "2fa",
    ];
    for indicator in AUTH_INDICATORS {
        if title.contains(indicator) || url_lower.contains(indicator) {
            return SafetyDecision::SkipStorage;
        }
    }

    const SECRET_PATTERNS: &[&str] = &[
        "api_key",
        "apikey",
        "secret_key",
        "private_key",
        "access_token",
        "password:",
        "passwd:",
        "token:",
        "-----begin rsa",
        "-----begin ec",
    ];
    for pattern in SECRET_PATTERNS {
        if text_lower.contains(pattern) {
            return SafetyDecision::Redact;
        }
    }
    const SECRET_TOKEN_PREFIXES: &[&str] = &["ghp_", "sk-", "xoxb-"];
    if SECRET_TOKEN_PREFIXES
        .iter()
        .any(|prefix| contains_prefixed_secret(&text_lower, prefix, 12))
    {
        return SafetyDecision::Redact;
    }

    SafetyDecision::Allow
}

fn contains_prefixed_secret(text: &str, prefix: &str, min_suffix_len: usize) -> bool {
    let mut search_from = 0;
    while let Some(relative_start) = text[search_from..].find(prefix) {
        let start = search_from + relative_start;
        let starts_at_token_boundary = start == 0
            || text[..start]
                .chars()
                .next_back()
                .is_none_or(|ch| !ch.is_ascii_alphanumeric() && ch != '_');
        let suffix_len = text[start + prefix.len()..]
            .chars()
            .take_while(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-'))
            .count();
        if starts_at_token_boundary && suffix_len >= min_suffix_len {
            return true;
        }
        search_from = start + prefix.len();
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_normal_content() {
        assert_eq!(
            evaluate(
                Some("VS Code"),
                None,
                None,
                Some("main.rs"),
                Some("fn main() {}"),
                &[]
            ),
            SafetyDecision::Allow
        );
    }

    #[test]
    fn blocks_password_manager() {
        assert_eq!(
            evaluate(Some("1Password"), None, None, Some("Vault"), None, &[]),
            SafetyDecision::SkipStorage
        );
    }

    #[test]
    fn blocks_banking_url() {
        assert_eq!(
            evaluate(
                Some("Safari"),
                None,
                Some("https://chase.com/account"),
                Some("Chase Bank"),
                None,
                &[]
            ),
            SafetyDecision::SkipStorage
        );
    }

    #[test]
    fn blocks_incognito_window() {
        assert_eq!(
            evaluate(
                Some("Chrome"),
                None,
                None,
                Some("New Incognito Window"),
                None,
                &[]
            ),
            SafetyDecision::SkipStorage
        );
    }

    #[test]
    fn redacts_api_key_in_text() {
        assert_eq!(
            evaluate(
                Some("Terminal"),
                None,
                None,
                Some("bash"),
                Some("export api_key=abc123"),
                &[]
            ),
            SafetyDecision::Redact
        );
    }

    #[test]
    fn distinguishes_long_secret_tokens_from_ordinary_hyphenated_words() {
        assert_eq!(
            evaluate(
                Some("Terminal"),
                None,
                None,
                Some("bash"),
                Some("credential sk-1234567890abcdef"),
                &[]
            ),
            SafetyDecision::Redact
        );
        assert_eq!(
            evaluate(
                Some("VS Code"),
                None,
                None,
                Some("README.md"),
                Some("Run the flask-abcdefghijklmnop migration and inspect sk-short"),
                &[]
            ),
            SafetyDecision::Allow
        );
    }

    #[test]
    fn respects_user_blocklist() {
        assert_eq!(
            evaluate(
                Some("Figma"),
                None,
                None,
                Some("Client NDA Design"),
                None,
                &["nda".to_string()]
            ),
            SafetyDecision::SkipStorage
        );
    }

    #[test]
    fn blocks_auth_pages() {
        assert_eq!(
            evaluate(
                Some("Safari"),
                None,
                Some("https://app.example.com/login"),
                Some("Sign in"),
                None,
                &[]
            ),
            SafetyDecision::SkipStorage
        );
    }
}
