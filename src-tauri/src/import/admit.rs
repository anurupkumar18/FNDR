//! Admission for imported items: the one function every item passes before
//! it can reach the store.
//!
//! It composes three existing rules and adds none of its own policy: the
//! capture safety gate (`privacy::safety_gate::evaluate`: banking, medical,
//! password managers, private windows, sign-in pages, secrets), the user
//! blocklist the way retrieval applies it to leaf rows (app, URL, and title
//! plus body), and the capture surface policy (`capture::admission`) for
//! browser pages. Modeled on how `mcp::remember` admits an agent note.
//!
//! Refusals carry a stable reason and never repeat the item.

use super::{ImportItem, ImportKind};
use crate::capture::admission::{classify_capture_surface_policy, CaptureSurfacePolicy};
use crate::privacy::safety_gate::{self, SafetyDecision};
use crate::privacy::Blocklist;

const REDACTED_LINE: &str = "[redacted]";

/// Path pieces that mark a file as holding credentials. Connectors skip these
/// themselves; this is the second lock.
const SENSITIVE_PATH_MARKERS: &[&str] = &[
    "/.ssh/",
    "/.aws/",
    "/.gnupg/",
    "/.kube/",
    "/.docker/config",
    "/.netrc",
    "/.npmrc",
    "/.pypirc",
    "/library/keychains/",
    "/id_rsa",
    "/id_ed25519",
    "/.env",
    ".pem",
    ".p12",
    ".pfx",
    ".keychain",
    ".kdbx",
    ".1pux",
];

/// What admission reads from the person's settings.
#[derive(Debug, Clone, Default)]
pub struct AdmitSettings {
    blocklist: Vec<String>,
}

impl AdmitSettings {
    /// Blank entries are dropped: an empty string matches everything.
    pub fn new(blocklist: &[String]) -> Self {
        Self {
            blocklist: blocklist
                .iter()
                .map(|entry| entry.trim().to_string())
                .filter(|entry| !entry.is_empty())
                .collect(),
        }
    }

    pub fn from_config(config: &crate::config::Config) -> Self {
        Self::new(&config.blocklist)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Admission {
    /// Store the item as it is.
    Allow,
    /// Store the item with this text instead of its own (secret lines
    /// replaced, hidden characters removed, or a listing page reduced to its
    /// title and address).
    Redact(String),
    Skip(SkipReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SkipReason {
    Empty,
    InternalApp,
    UserBlocklist,
    /// Banking, medical, password manager, private window, sign-in page.
    SensitiveContext,
    SensitivePath,
    /// A search, feed, or start page: navigation, not content.
    NavigationPage,
    SecretInTitle,
    /// Redaction could not make the text safe.
    UnredactableSecret,
}

impl SkipReason {
    pub fn as_str(self) -> &'static str {
        match self {
            SkipReason::Empty => "empty",
            SkipReason::InternalApp => "internal_app",
            SkipReason::UserBlocklist => "user_blocklist",
            SkipReason::SensitiveContext => "sensitive_context",
            SkipReason::SensitivePath => "sensitive_path",
            SkipReason::NavigationPage => "navigation_page",
            SkipReason::SecretInTitle => "secret_in_title",
            SkipReason::UnredactableSecret => "unredactable_secret",
        }
    }
}

/// Decides what happens to one item. Pure and deterministic: no model, no I/O.
pub fn admit(item: &ImportItem, settings: &AdmitSettings) -> Admission {
    let title = item.title.trim();
    let app = item.app_label.trim();
    if title.is_empty() && item.text.trim().is_empty() {
        return Admission::Skip(SkipReason::Empty);
    }

    let reopen = item.reopen_target.as_deref().map(str::trim);
    let url = item
        .url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| reopen.filter(|value| is_web_url(value)));

    if Blocklist::is_internal_app(app, None) {
        return Admission::Skip(SkipReason::InternalApp);
    }
    if reopen.is_some_and(is_sensitive_path) || url.is_some_and(is_sensitive_path) {
        return Admission::Skip(SkipReason::SensitivePath);
    }

    // The words retrieval will test against the row later: title and body.
    let hidden_later = format!("{}\n{}\n{}", title, item.text, item.people.join(" "));
    if (!app.is_empty() && Blocklist::is_blocked(app, &settings.blocklist))
        || Blocklist::is_context_blocked(url, Some(&hidden_later), &settings.blocklist)
        || reopen.is_some_and(|target| {
            Blocklist::is_context_blocked(Some(target), None, &settings.blocklist)
        })
    {
        return Admission::Skip(SkipReason::UserBlocklist);
    }

    if safety_gate::evaluate(Some(app), None, url, Some(title), None, &settings.blocklist)
        != SafetyDecision::Allow
    {
        return Admission::Skip(SkipReason::SensitiveContext);
    }
    if safety_gate::evaluate(None, None, None, None, Some(title), &[]) != SafetyDecision::Allow {
        return Admission::Skip(SkipReason::SecretInTitle);
    }

    let mut listing_page = false;
    if let Some(url) = url.filter(|_| item.kind == ImportKind::Browser) {
        match classify_capture_surface_policy(&browser_name(app), title, Some(url)) {
            CaptureSurfacePolicy::SkipFrame => {
                return Admission::Skip(SkipReason::NavigationPage);
            }
            CaptureSurfacePolicy::UrlOnly => listing_page = true,
            CaptureSurfacePolicy::Normal => {}
        }
    }
    if listing_page {
        return Admission::Redact(String::new());
    }

    let cleaned = strip_hidden_characters(&item.text);
    let redacted = redact_secret_lines(&cleaned);
    if safety_gate::evaluate(None, None, None, None, Some(&redacted), &[]) != SafetyDecision::Allow
    {
        return Admission::Skip(SkipReason::UnredactableSecret);
    }
    if redacted == item.text {
        Admission::Allow
    } else {
        Admission::Redact(redacted)
    }
}

/// A browser import's label ("Chrome", "Arc history") is not always a name
/// `browsers::is_browser` knows; the surface policy only applies to browsers,
/// and every item of the browser kind is a page.
fn browser_name(app: &str) -> String {
    if crate::browsers::is_browser(app) {
        app.to_string()
    } else {
        "Safari".to_string()
    }
}

fn is_web_url(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

fn is_sensitive_path(value: &str) -> bool {
    if is_web_url(value) {
        return false;
    }
    let lower = format!("/{}", value.to_ascii_lowercase().replace('\\', "/"));
    SENSITIVE_PATH_MARKERS
        .iter()
        .any(|marker| lower.contains(marker))
}

/// Invisible text a model can read and a person cannot, and control
/// characters. Tab, line feed and carriage return stay.
fn strip_hidden_characters(text: &str) -> String {
    text.chars()
        .filter(|c| {
            let code = *c as u32;
            let hidden = (code < 0x20 && !matches!(c, '\t' | '\n' | '\r'))
                || code == 0x7F
                || (0x80..=0x9F).contains(&code)
                || matches!(code, 0x200B | 0x200E | 0x200F | 0x202A..=0x202E | 0x2060..=0x2069 | 0xFEFF)
                || (0xE0000..=0xE007F).contains(&code);
            !hidden
        })
        .collect()
}

/// Replaces every line the safety gate would flag as a secret. The gate is
/// the only definition of "secret", so the redactor asks it line by line.
fn redact_secret_lines(text: &str) -> String {
    text.split('\n')
        .map(|line| {
            if safety_gate::evaluate(None, None, None, None, Some(line), &[])
                == SafetyDecision::Allow
            {
                line.to_string()
            } else {
                REDACTED_LINE.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(kind: ImportKind) -> ImportItem {
        ImportItem {
            kind,
            external_id: "ext-1".into(),
            timestamp_ms: 1_800_000_000_000,
            title: "Quarterly planning notes".into(),
            text: "We agreed to ship the importer in October.".into(),
            reopen_target: None,
            app_label: "Notes".into(),
            people: Vec::new(),
            url: None,
        }
    }

    fn settings() -> AdmitSettings {
        AdmitSettings::new(&["Blocked Co".to_string(), "  ".to_string()])
    }

    #[test]
    fn plain_content_is_allowed() {
        assert_eq!(
            admit(&item(ImportKind::File), &settings()),
            Admission::Allow
        );
    }

    #[test]
    fn banking_url_is_skipped() {
        let mut banking = item(ImportKind::Browser);
        banking.app_label = "Chrome".into();
        banking.url = Some("https://secure.chase.com/account/summary".into());
        assert_eq!(
            admit(&banking, &settings()),
            Admission::Skip(SkipReason::SensitiveContext)
        );
    }

    #[test]
    fn password_manager_app_is_skipped() {
        let mut vault = item(ImportKind::File);
        vault.app_label = "1Password 8".into();
        assert_eq!(
            admit(&vault, &settings()),
            Admission::Skip(SkipReason::SensitiveContext)
        );
    }

    #[test]
    fn private_window_title_is_skipped() {
        let mut private = item(ImportKind::Browser);
        private.app_label = "Safari".into();
        private.title = "Example - Private Browsing".into();
        private.url = Some("https://example.com/page".into());
        assert_eq!(
            admit(&private, &settings()),
            Admission::Skip(SkipReason::SensitiveContext)
        );
    }

    #[test]
    fn secret_lines_are_redacted_and_the_rest_is_kept() {
        let mut leaky = item(ImportKind::Mail);
        leaky.text =
            "Deploy notes\napi_key = abc123abc123\nghp_abcdefghijklmnop1234 leaked\nDone.".into();
        let Admission::Redact(text) = admit(&leaky, &settings()) else {
            panic!("expected redaction");
        };
        assert!(!text.contains("abc123abc123"));
        assert!(!text.contains("ghp_"));
        assert!(text.contains("Deploy notes") && text.contains("Done."));
        assert_eq!(text.matches(REDACTED_LINE).count(), 2);
    }

    #[test]
    fn secret_in_title_is_skipped_without_echo() {
        let mut leaky = item(ImportKind::File);
        leaky.title = "password: hunter2hunter2".into();
        assert_eq!(
            admit(&leaky, &settings()),
            Admission::Skip(SkipReason::SecretInTitle)
        );
    }

    #[test]
    fn user_blocklist_matches_app_url_title_and_body() {
        let mut by_app = item(ImportKind::File);
        by_app.app_label = "Blocked Co Mail".into();
        let mut by_url = item(ImportKind::Browser);
        by_url.app_label = "Chrome".into();
        by_url.url = Some("https://intranet.blocked-co.example/wiki".into());
        by_url.title = "wiki".into();
        let mut by_body = item(ImportKind::Mail);
        by_body.text = "forwarded from blocked co finance".into();
        for blocked in [by_app, by_body] {
            assert_eq!(
                admit(&blocked, &settings()),
                Admission::Skip(SkipReason::UserBlocklist)
            );
        }
        let host_rule = AdmitSettings::new(&["blocked-co.example".to_string()]);
        assert_eq!(
            admit(&by_url, &host_rule),
            Admission::Skip(SkipReason::UserBlocklist)
        );
    }

    #[test]
    fn blank_blocklist_entries_block_nothing() {
        let blank = AdmitSettings::new(&["".to_string(), "   ".to_string()]);
        assert_eq!(admit(&item(ImportKind::File), &blank), Admission::Allow);
    }

    #[test]
    fn search_and_feed_pages_are_navigation_and_listings_keep_only_the_title() {
        let mut search = item(ImportKind::Browser);
        search.app_label = "Chrome".into();
        search.url = Some("https://www.example.com/search?q=rust".into());
        assert_eq!(
            admit(&search, &settings()),
            Admission::Skip(SkipReason::NavigationPage)
        );
        let mut listing = item(ImportKind::Browser);
        listing.app_label = "Arc history".into();
        listing.url = Some("https://video.example.com/@someone".into());
        listing.text = "body text of a profile listing".into();
        assert_eq!(
            admit(&listing, &settings()),
            Admission::Redact(String::new())
        );
        let mut article = item(ImportKind::Browser);
        article.app_label = "Chrome".into();
        article.url = Some("https://docs.example.com/guide/install".into());
        assert_eq!(admit(&article, &settings()), Admission::Allow);
    }

    #[test]
    fn credential_paths_are_skipped() {
        for path in [
            "/Users/someone/.ssh/config",
            "/Users/someone/projects/app/.env",
            "/Users/someone/Library/Keychains/login.keychain-db",
            "C:\\keys\\server.pem",
        ] {
            let mut file = item(ImportKind::File);
            file.reopen_target = Some(path.into());
            assert_eq!(
                admit(&file, &settings()),
                Admission::Skip(SkipReason::SensitivePath),
                "{path}"
            );
        }
        let mut ordinary = item(ImportKind::File);
        ordinary.reopen_target = Some("/Users/someone/Documents/plan.md".into());
        assert_eq!(admit(&ordinary, &settings()), Admission::Allow);
    }

    #[test]
    fn hidden_characters_are_removed_and_reported_as_a_redaction() {
        let mut hidden = item(ImportKind::Mail);
        hidden.text = "visible\u{E0049}\u{E0047}\u{202E} words\u{0007}".into();
        assert_eq!(
            admit(&hidden, &settings()),
            Admission::Redact("visible words".into())
        );
    }

    #[test]
    fn empty_items_and_fndr_itself_are_skipped() {
        let mut empty = item(ImportKind::File);
        empty.title = "  ".into();
        empty.text = String::new();
        assert_eq!(
            admit(&empty, &settings()),
            Admission::Skip(SkipReason::Empty)
        );
        let mut own = item(ImportKind::File);
        own.app_label = "FNDR".into();
        assert_eq!(
            admit(&own, &settings()),
            Admission::Skip(SkipReason::InternalApp)
        );
    }
}
