//! An admitted [`ImportItem`] as a leaf [`MemoryRecord`].

use super::ImportItem;
use crate::memory::reopen::ReopenKind;
use crate::storage::{MemoryRecord, IMPORT_SESSION_PREFIX};
use chrono::TimeZone;
use serde_json::json;

const SUMMARY_MAX_CHARS: usize = 240;
const EMBEDDING_TEXT_MAX_CHARS: usize = 2_000;

/// The row for `item`. `item.text` is the admitted text. Vectors are filled
/// by the runner.
pub(super) fn build_record(item: &ImportItem, now_ms: i64) -> MemoryRecord {
    let title = item.title.trim();
    let summary = {
        let first = first_sentence(&item.text);
        if first.is_empty() {
            title.chars().take(SUMMARY_MAX_CHARS).collect()
        } else {
            first
        }
    };
    let provenance = json!({
        "import_provenance": {
            "version": 1,
            "kind": item.kind.as_str(),
            "source_label": item.app_label,
            "external_id": item.external_id,
            "item_timestamp_ms": item.timestamp_ms,
            "imported_at_ms": now_ms,
        }
    });
    let day_bucket = chrono::Local
        .timestamp_millis_opt(item.timestamp_ms)
        .single()
        .map(|moment| moment.format("%Y-%m-%d").to_string())
        .unwrap_or_default();
    let mut record = MemoryRecord {
        id: item.record_id(),
        timestamp: item.timestamp_ms,
        timestamp_start: item.timestamp_ms,
        timestamp_end: item.timestamp_ms,
        day_bucket,
        source_type: item.kind.source_type(),
        raw_evidence: provenance.to_string(),
        storage_outcome: "import".to_string(),
        activity_type: "import".to_string(),
        tags: vec!["import".to_string(), item.kind.as_str().to_string()],
        entities: item.people.clone(),
        app_name: item.app_label.trim().to_string(),
        window_title: title.to_string(),
        text: item.text.clone(),
        clean_text: item.text.clone(),
        memory_context: item.text.clone(),
        snippet: summary.clone(),
        display_summary: summary,
        url: item.url.clone().filter(|value| !value.trim().is_empty()),
        session_key: format!(
            "{IMPORT_SESSION_PREFIX}{}:{}",
            item.kind.as_str(),
            item.external_id
        ),
        content_hash: item.content_hash(),
        enrichment_status: "import".to_string(),
        confidence_score: 1.0,
        importance_score: 0.5,
        summary_source: "fallback".to_string(),
        ..MemoryRecord::default()
    };
    record.embedding_text = embedding_text(item);
    apply_reopen_target(&mut record, item.reopen_target.as_deref());
    record
}

/// What the vectors describe: the title, where it came from, and the body,
/// so a search for "the Slack thread about X" has the source to match.
fn embedding_text(item: &ImportItem) -> String {
    let joined = format!(
        "{}\n{} ({})\n{}",
        item.title.trim(),
        item.app_label.trim(),
        item.kind.as_str(),
        item.text.trim()
    );
    joined.chars().take(EMBEDDING_TEXT_MAX_CHARS).collect()
}

fn first_sentence(text: &str) -> String {
    text.trim()
        .split_inclusive(['.', '!', '?', '\n'])
        .next()
        .unwrap_or_default()
        .trim()
        .chars()
        .take(SUMMARY_MAX_CHARS)
        .collect()
}

/// Typed reopen fields only. The legacy `Reopen:` marker in body text is not
/// read for imports (see `resolve_reopen_target`), because the body is
/// someone else's text.
fn apply_reopen_target(record: &mut MemoryRecord, target: Option<&str>) {
    let Some(target) = target.map(str::trim).filter(|value| !value.is_empty()) else {
        return;
    };
    let lower = target.to_ascii_lowercase();
    if lower.starts_with("http://") {
        return;
    }
    if lower.starts_with("https://") {
        record.reopen_kind = ReopenKind::BrowserUrl;
        record.reopen_url = Some(target.to_string());
    } else if lower.starts_with("file://") || target.starts_with('/') || target.starts_with('~') {
        record.reopen_kind = ReopenKind::FilePath;
        record.reopen_file_path =
            Some(target.strip_prefix("file://").unwrap_or(target).to_string());
    } else if target.contains("://") {
        record.reopen_kind = ReopenKind::AppDeepLink;
        record.reopen_app_deep_link = Some(target.to_string());
    } else {
        return;
    }
    record.reopen_confidence = 1.0;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::ImportKind;

    fn item(target: Option<&str>) -> ImportItem {
        ImportItem {
            kind: ImportKind::Mail,
            external_id: "uid-77".into(),
            timestamp_ms: 1_800_000_000_000,
            title: "Budget review".into(),
            text: "Numbers attached. Please confirm by Friday.".into(),
            reopen_target: target.map(str::to_string),
            app_label: "Mail".into(),
            people: vec!["Ada".into()],
            url: None,
        }
    }

    #[test]
    fn a_row_is_a_leaf_with_provenance_and_a_stable_id() {
        let first = build_record(&item(None), 1);
        let again = build_record(&item(None), 2);
        assert_eq!(first.id, again.id);
        assert!(first.id.starts_with("imp_"));
        assert_eq!(first.source_type, "import:mail");
        assert_eq!(first.session_key, "import:mail:uid-77");
        assert!(first.is_import() && first.is_leaf_row() && !first.is_agent_note());
        assert_eq!(first.snippet, "Numbers attached.");
        assert!(first.embedding_text.contains("Budget review"));
        assert!(first.embedding_text.contains("Mail (mail)"));
        assert!(first.raw_evidence.contains("import_provenance"));
        assert_eq!(first.entities, vec!["Ada".to_string()]);
    }

    #[test]
    fn reopen_targets_become_typed_fields() {
        let web = build_record(&item(Some("https://example.com/a")), 1);
        assert_eq!(web.reopen_kind, ReopenKind::BrowserUrl);
        let file = build_record(&item(Some("/Users/x/Documents/plan.md")), 1);
        assert_eq!(file.reopen_kind, ReopenKind::FilePath);
        assert_eq!(
            file.reopen_file_path.as_deref(),
            Some("/Users/x/Documents/plan.md")
        );
        let message = build_record(&item(Some("message://%3Cid@example%3E")), 1);
        assert_eq!(message.reopen_kind, ReopenKind::AppDeepLink);
        let none = build_record(&item(Some("not a target")), 1);
        assert_eq!(none.reopen_kind, ReopenKind::Unknown);
    }

    #[test]
    fn non_https_web_targets_do_not_become_reopen_provenance() {
        let record = build_record(&item(Some("http://untrusted.example/path")), 1);
        assert_eq!(record.reopen_kind, ReopenKind::Unknown);
        assert_eq!(record.reopen_url, None);
    }

    #[test]
    fn a_new_hash_means_changed_content_and_the_same_id() {
        let mut edited = item(None);
        edited.text.push_str(" Updated.");
        assert_eq!(item(None).record_id(), edited.record_id());
        assert_ne!(item(None).content_hash(), edited.content_hash());
    }
}
