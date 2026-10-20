//! The one row shape the recall tools return, and the one gate that decides
//! whether a memory may leave FNDR for an assistant.
//!
//! A row says where a memory came from (`source_type`, `source_label`,
//! `app`), when (`timestamp_ms`, `timestamp_iso`) and how to get back to it
//! (`reopen`). The shape is generic on purpose: an imported record
//! (`source_type = "import:<kind>"`) is a row like any other, so a new import
//! source appears in every tool with no change here.
//!
//! The gate is stricter than the in-app search. The text goes to a cloud
//! model, so a memory that the blocklist, the sensitive-site list or the
//! privacy class rules out is withheld, and only a count says so.

use crate::context_runtime::{context_privacy_is_visible, privacy_class_of, retrieve};
use crate::privacy::Blocklist;
use crate::storage::SearchResult;
use serde_json::{json, Value};

const SNIPPET_MAX_CHARS: usize = 400;

/// Whether one memory may be returned by a recall tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Gate {
    Open,
    /// Gone, blocked or internal: not mentioned at all.
    Hidden,
    /// Ruled out by privacy class or a sensitive site: counted, never shown.
    Withheld,
}

pub(super) fn gate(result: &SearchResult, blocklist: &[String], hide_more: bool) -> Gate {
    if !retrieve::result_is_permitted(result, blocklist) {
        return Gate::Hidden;
    }
    let class = privacy_class_of(&result.app_name, &result.window_title, &result.clean_text);
    let broad_secret_word = hide_more
        && ["password", "secret", "token"].iter().any(|word| {
            result.app_name.to_lowercase().contains(word)
                || result.window_title.to_lowercase().contains(word)
                || result.clean_text.to_lowercase().contains(word)
        });
    if broad_secret_word
        || !context_privacy_is_visible(&class)
        || Blocklist::is_sensitive_context(result.url.as_deref(), Some(&result.window_title))
    {
        return Gate::Withheld;
    }
    Gate::Open
}

/// Rows that pass the gate, in input order, and how many were withheld.
pub(super) fn open_rows(
    results: Vec<SearchResult>,
    blocklist: &[String],
    hide_more: bool,
) -> (Vec<SearchResult>, usize) {
    let mut withheld = 0;
    let open = results
        .into_iter()
        .filter(|result| match gate(result, blocklist, hide_more) {
            Gate::Open => true,
            Gate::Withheld => {
                withheld += 1;
                false
            }
            Gate::Hidden => false,
        })
        .collect();
    (open, withheld)
}

/// A person-readable name for a `source_type`. Unknown and imported kinds get
/// a label from the type itself, so nothing needs registering.
pub(super) fn source_label(source_type: &str) -> String {
    match source_type {
        "screen" => "Screen capture".to_string(),
        "agent" => "Note added by an assistant".to_string(),
        other => match other.strip_prefix("import:") {
            Some(kind) if !kind.is_empty() => format!("Imported {kind}"),
            _ => other.to_string(),
        },
    }
}

pub(super) fn matches_source(result: &SearchResult, wanted: Option<&str>) -> bool {
    match wanted.map(str::trim).filter(|value| !value.is_empty()) {
        None => true,
        // "import" or "import:" selects every imported kind.
        Some(wanted) if wanted == "import" || wanted == "import:" => {
            result.source_type.starts_with("import:")
        }
        Some(wanted) => result.source_type.eq_ignore_ascii_case(wanted),
    }
}

pub(super) fn matches_app(result: &SearchResult, wanted: Option<&str>) -> bool {
    match wanted.map(str::trim).filter(|value| !value.is_empty()) {
        None => true,
        Some(wanted) => result.app_name.eq_ignore_ascii_case(wanted),
    }
}

pub(super) fn clip(text: &str, max: usize) -> String {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= max {
        return collapsed;
    }
    let mut clipped: String = collapsed.chars().take(max).collect();
    clipped.push_str("...");
    clipped
}

/// The words that stand for a memory: its summary, else its snippet, else the
/// start of its cleaned text. Never the raw capture text.
pub(super) fn snippet(result: &SearchResult) -> String {
    [&result.display_summary, &result.snippet, &result.clean_text]
        .into_iter()
        .map(|text| text.trim())
        .find(|text| !text.is_empty())
        .map(|text| clip(text, SNIPPET_MAX_CHARS))
        .unwrap_or_default()
}

fn non_empty(value: &Option<String>) -> Option<&str> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

/// How to get back to the memory, as `fndr.open_target` would hand it over.
/// `target` is the single best choice; the parts stay available.
pub(super) fn reopen(result: &SearchResult) -> Value {
    let target = non_empty(&result.reopen_url)
        .or_else(|| non_empty(&result.reopen_app_deep_link))
        .or_else(|| non_empty(&result.reopen_file_path))
        .or_else(|| non_empty(&result.url));
    json!({
        "target": target,
        "kind": serde_json::to_value(&result.reopen_kind).unwrap_or(Value::Null),
        "url": non_empty(&result.reopen_url).or_else(|| non_empty(&result.url)),
        "file_path": non_empty(&result.reopen_file_path),
        "app_deep_link": non_empty(&result.reopen_app_deep_link),
    })
}

fn iso(timestamp_ms: i64) -> Option<String> {
    chrono::DateTime::from_timestamp_millis(timestamp_ms).map(|time| time.to_rfc3339())
}

/// One memory as a tool result row.
pub(super) fn row(result: &SearchResult) -> Value {
    let mut row = json!({
        "memory_id": result.id,
        "title": result.window_title,
        "snippet": snippet(result),
        "source_type": result.source_type,
        "source_label": source_label(&result.source_type),
        "app": result.app_name,
        "timestamp_ms": result.timestamp,
        "timestamp_iso": iso(result.timestamp),
        "project": (!result.project.trim().is_empty()).then(|| result.project.clone()),
        "reopen": reopen(result),
    });
    if let Some(client) = &result.added_by {
        row["added_by"] = json!(client);
    }
    row
}

/// A synthetic memory for tests in this folder.
#[cfg(test)]
pub(super) fn sample(id: &str, source_type: &str, app: &str, title: &str) -> SearchResult {
    SearchResult {
        id: id.to_string(),
        timestamp: 1_800_000_000_000,
        app_name: app.to_string(),
        source_type: source_type.to_string(),
        window_title: title.to_string(),
        snippet: format!("Snippet of {title}"),
        clean_text: format!("Body of {title}"),
        ..SearchResult::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_row_names_where_a_memory_came_from_and_how_to_reopen_it() {
        let mut result = sample("m1", "screen", "Safari", "FNDR roadmap");
        result.reopen_url = Some("https://example.com/roadmap".into());
        result.project = "fndr".into();
        let row = row(&result);
        assert_eq!(row["memory_id"], "m1");
        assert_eq!(row["source_type"], "screen");
        assert_eq!(row["source_label"], "Screen capture");
        assert_eq!(row["app"], "Safari");
        assert_eq!(row["timestamp_ms"], 1_800_000_000_000_i64);
        assert!(row["timestamp_iso"]
            .as_str()
            .unwrap()
            .starts_with("2027-01-15"));
        assert_eq!(row["reopen"]["target"], "https://example.com/roadmap");
        assert_eq!(row["project"], "fndr");
        assert_eq!(row["snippet"], "Snippet of FNDR roadmap");
    }

    #[test]
    fn an_imported_row_has_the_same_shape_with_no_registration() {
        let mut mail = sample("imp-1", "import:mail", "Mail", "Re: lease renewal");
        mail.reopen_file_path = Some("/Users/x/Library/Mail/V10/1.emlx".into());
        let row = row(&mail);
        assert_eq!(row["source_type"], "import:mail");
        assert_eq!(row["source_label"], "Imported mail");
        assert_eq!(row["reopen"]["target"], "/Users/x/Library/Mail/V10/1.emlx");
        assert_eq!(
            row["reopen"]["file_path"],
            "/Users/x/Library/Mail/V10/1.emlx"
        );

        let unknown = sample("imp-2", "import:something_new", "Tool", "Item");
        assert_eq!(
            super::row(&unknown)["source_label"],
            "Imported something_new"
        );
    }

    #[test]
    fn the_import_prefix_selects_every_imported_kind() {
        let mail = sample("a", "import:mail", "Mail", "t");
        let slack = sample("b", "import:slack", "Slack export", "t");
        let screen = sample("c", "screen", "Safari", "t");
        for wanted in ["import", "import:"] {
            assert!(matches_source(&mail, Some(wanted)));
            assert!(matches_source(&slack, Some(wanted)));
            assert!(!matches_source(&screen, Some(wanted)));
        }
        assert!(matches_source(&mail, Some("import:mail")));
        assert!(!matches_source(&slack, Some("import:mail")));
        assert!(matches_source(&screen, None));
        assert!(matches_source(&screen, Some("  ")));
    }

    #[test]
    fn the_snippet_is_a_summary_never_the_raw_text() {
        let mut result = sample("m", "screen", "Notes", "t");
        result.snippet = String::new();
        result.display_summary = "A short summary".into();
        result.text = "RAW OCR TEXT".into();
        assert_eq!(snippet(&result), "A short summary");
        result.display_summary = String::new();
        result.clean_text = "x".repeat(900);
        let clipped = snippet(&result);
        assert!(clipped.chars().count() <= SNIPPET_MAX_CHARS + 3);
        assert!(!clipped.contains("RAW OCR"));
    }

    #[test]
    fn the_gate_hides_blocked_withholds_private_and_opens_the_rest() {
        let blocklist = vec!["Signal".to_string()];

        let ordinary = sample("ok", "screen", "Xcode", "main.rs");
        assert_eq!(gate(&ordinary, &blocklist, false), Gate::Open);

        let blocked = sample("b", "screen", "Signal", "chat");
        assert_eq!(gate(&blocked, &blocklist, false), Gate::Hidden);

        let mut deleted = sample("d", "screen", "Xcode", "main.rs");
        deleted.is_soft_deleted = true;
        assert_eq!(gate(&deleted, &blocklist, false), Gate::Hidden);

        let mut secret = sample("s", "screen", "Notes", "Passwords");
        secret.clean_text = "password: synthetic-hunter2".into();
        assert_eq!(gate(&secret, &blocklist, false), Gate::Withheld);

        let mut bank = sample("k", "screen", "Safari", "Account");
        bank.url = Some("https://www.chase.com/accounts".into());
        assert_eq!(gate(&bank, &blocklist, false), Gate::Withheld);

        let mut imported_secret = sample("i", "import:file", "Files", "notes.txt");
        imported_secret.clean_text = "access_token: synthetic-credential-value".into();
        assert_eq!(gate(&imported_secret, &blocklist, false), Gate::Withheld);
    }

    #[test]
    fn ordinary_development_words_stay_visible_but_real_secrets_and_login_pages_do_not() {
        let mut discussion = sample("discussion", "screen", "Xcode", "Token budget notes");
        discussion.clean_text = "Reviewed password manager tests and token budget.".into();
        assert_eq!(gate(&discussion, &[], false), Gate::Open);
        assert_eq!(gate(&discussion, &[], true), Gate::Withheld);

        let mut credential = sample("credential", "screen", "Xcode", "Build settings");
        credential.clean_text = "access_token: synthetic-credential-value".into();
        assert_eq!(gate(&credential, &[], false), Gate::Withheld);

        let manager = sample("manager", "screen", "1Password", "Item list");
        assert_eq!(gate(&manager, &[], false), Gate::Withheld);

        let login = sample("login", "screen", "Safari", "Sign in to workspace");
        assert_eq!(gate(&login, &[], false), Gate::Withheld);
    }

    #[test]
    fn open_rows_counts_withheld_and_says_nothing_of_hidden() {
        let blocklist = vec!["Signal".to_string()];
        let mut secret = sample("s", "screen", "Notes", "Passwords");
        secret.clean_text = "password: synthetic-hunter2".into();
        let (open, withheld) = open_rows(
            vec![
                sample("a", "screen", "Xcode", "one"),
                sample("b", "screen", "Signal", "hidden"),
                secret,
            ],
            &blocklist,
            false,
        );
        assert_eq!(open.len(), 1);
        assert_eq!(open[0].id, "a");
        assert_eq!(withheld, 1);
    }
}
