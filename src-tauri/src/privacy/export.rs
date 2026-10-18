//! Portable memory export and restore.

use crate::privacy::vault::{self, KeyProvider};
use crate::storage::{MemoryRecord, Store};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt::Write;
use std::path::Path;

const FORMAT: &str = "fndr-memory-jsonl";
const VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
struct Header {
    format: String,
    version: u32,
}

/// Keep full memory fields, including provenance and search vectors, but never
/// export a local screenshot path that would not resolve on another machine.
pub fn encode_jsonl(records: &[MemoryRecord]) -> Result<Vec<u8>, String> {
    let mut output = serde_json::to_vec(&Header {
        format: FORMAT.to_string(),
        version: VERSION,
    })
    .map_err(|error| error.to_string())?;
    output.push(b'\n');
    for record in records {
        let mut portable = record.clone();
        portable.screenshot_path = None;
        serde_json::to_writer(&mut output, &portable).map_err(|error| error.to_string())?;
        output.push(b'\n');
    }
    Ok(output)
}

pub fn decode_jsonl(bytes: &[u8]) -> Result<Vec<MemoryRecord>, String> {
    let text = std::str::from_utf8(bytes).map_err(|_| "Export is not UTF-8".to_string())?;
    let mut lines = text.lines();
    let header: Header = serde_json::from_str(lines.next().unwrap_or(""))
        .map_err(|_| "Export header is invalid".to_string())?;
    if header.format != FORMAT || header.version != VERSION {
        return Err("Export format or version is unsupported".to_string());
    }
    let mut ids = HashSet::new();
    let mut records = Vec::new();
    for (index, line) in lines.enumerate() {
        if line.is_empty() {
            continue;
        }
        let mut record: MemoryRecord = serde_json::from_str(line)
            .map_err(|_| format!("Export row {} is invalid", index + 1))?;
        if record.id.is_empty() || !ids.insert(record.id.clone()) {
            return Err(format!(
                "Export row {} has an empty or duplicate id",
                index + 1
            ));
        }
        record.screenshot_path = None;
        records.push(record);
    }
    Ok(records)
}

/// Restore missing rows only. The current visibility policy also applies to
/// an archive created before the person's blocklist changed.
pub async fn restore_records(
    store: &Store,
    records: &[MemoryRecord],
    blocklist: &[String],
) -> Result<usize, String> {
    let eligible = records
        .iter()
        .filter(|record| record_is_exportable(record, blocklist))
        .cloned()
        .collect::<Vec<_>>();
    let ids = eligible
        .iter()
        .map(|record| record.id.clone())
        .collect::<Vec<_>>();
    let existing = store
        .get_memories_by_ids(&ids)
        .await
        .map_err(|error| error.to_string())?;
    let missing = eligible
        .into_iter()
        .filter(|record| !existing.contains_key(&record.id))
        .collect::<Vec<_>>();
    store
        .add_batch_preserving_ids(&missing)
        .await
        .map_err(|error| error.to_string())?;
    Ok(missing.len())
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportReport {
    pub exported: usize,
    pub jsonl_path: String,
    pub markdown_path: String,
}

pub async fn export_archive(
    store: &Store,
    blocklist: &[String],
    keys: &dyn KeyProvider,
    directory: &Path,
) -> Result<ExportReport, String> {
    let records = store
        .list_all_memories()
        .await
        .map_err(|error| error.to_string())?
        .into_iter()
        .filter(|record| record_is_exportable(record, blocklist))
        .collect::<Vec<_>>();
    std::fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    let jsonl = encode_jsonl(&records)?;
    let markdown = encode_markdown(&records);
    let jsonl_path = vault::write_sealed(keys, &directory.join("memories.jsonl"), &jsonl)
        .map_err(|error| error.to_string())?;
    let markdown_path =
        match vault::write_sealed(keys, &directory.join("memories.md"), markdown.as_bytes()) {
            Ok(path) => path,
            Err(error) => {
                let _ = std::fs::remove_file(&jsonl_path);
                return Err(error.to_string());
            }
        };
    Ok(ExportReport {
        exported: records.len(),
        jsonl_path: jsonl_path.to_string_lossy().into_owned(),
        markdown_path: markdown_path.to_string_lossy().into_owned(),
    })
}

/// Imported rows go back through the same admission as a scan, so an archive
/// cannot carry in what admission would refuse today.
fn import_row_is_admitted(record: &MemoryRecord, blocklist: &[String]) -> bool {
    use crate::import::admit::{admit, Admission, AdmitSettings};
    use crate::import::{ImportItem, ImportKind};
    let Some(kind) = record
        .source_type
        .strip_prefix(crate::storage::IMPORT_SOURCE_TYPE_PREFIX)
        .and_then(ImportKind::parse)
    else {
        return !record.is_import();
    };
    let item = ImportItem {
        kind,
        external_id: record.id.clone(),
        timestamp_ms: record.timestamp,
        title: record.window_title.clone(),
        text: if record.clean_text.is_empty() {
            record.text.clone()
        } else {
            record.clean_text.clone()
        },
        reopen_target: None,
        app_label: record.app_name.clone(),
        people: Vec::new(),
        url: record.url.clone(),
    };
    matches!(
        admit(&item, &AdmitSettings::new(blocklist)),
        Admission::Allow
    )
}

fn record_is_exportable(record: &MemoryRecord, blocklist: &[String]) -> bool {
    import_row_is_admitted(record, blocklist)
        && crate::context_runtime::retrieve::memory_is_visible(record, blocklist)
        && crate::privacy::safety_gate::evaluate(
            Some(&record.app_name),
            record.bundle_id.as_deref(),
            record.url.as_deref(),
            Some(&record.window_title),
            Some(&record.text),
            blocklist,
        ) != crate::privacy::safety_gate::SafetyDecision::SkipStorage
}

pub async fn import_archive(
    store: &Store,
    blocklist: &[String],
    keys: &dyn KeyProvider,
    path: &Path,
) -> Result<usize, String> {
    if !path
        .file_name()
        .is_some_and(|name| name.to_string_lossy().ends_with(".jsonl.fndrenc"))
    {
        return Err("Choose a sealed FNDR JSONL archive".to_string());
    }
    let bytes = vault::read_sealed(keys, path).map_err(|error| error.to_string())?;
    let records = decode_jsonl(&bytes)?;
    restore_records(store, &records, blocklist).await
}

/// A readable companion to the complete JSONL archive.
pub fn encode_markdown(records: &[MemoryRecord]) -> String {
    let mut output = String::from("# FNDR memory export\n\n");
    for record in records {
        let _ = writeln!(
            output,
            "## {}",
            escape_html(&record.window_title.replace('\n', " "))
        );
        let _ = writeln!(output, "\n- Memory ID: `{}`", record.id.replace('`', ""));
        let _ = writeln!(output, "- Captured at: {}", record.timestamp);
        let _ = writeln!(output, "- Source: {}", escape_html(&record.source_type));
        let _ = writeln!(
            output,
            "- App: {}\n",
            escape_html(&record.app_name.replace('\n', " "))
        );
        for line in record.text.lines() {
            let _ = writeln!(output, "> {}", escape_html(line));
        }
        output.push('\n');
    }
    output
}

fn escape_html(value: &str) -> String {
    value.replace('&', "&amp;").replace('<', "&lt;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::privacy::vault::{read_sealed, write_sealed, StaticKey};
    use crate::storage::MemoryRecord;

    fn memory() -> MemoryRecord {
        MemoryRecord {
            id: "synthetic-id".to_string(),
            timestamp: 1_760_000_000_000,
            app_name: "Synthetic Notes".to_string(),
            window_title: "Planning".to_string(),
            text: "A synthetic decision was recorded.".to_string(),
            clean_text: "A synthetic decision was recorded.".to_string(),
            snippet: "A synthetic decision was recorded.".to_string(),
            source_type: "screen".to_string(),
            screenshot_path: Some("/synthetic/shot.png".to_string()),
            ..Default::default()
        }
    }

    #[test]
    fn jsonl_and_markdown_round_trip_through_sealed_files() {
        let record = memory();
        let jsonl = encode_jsonl(&[record.clone()]).unwrap();
        let markdown = encode_markdown(&[record.clone()]);
        let dir = tempfile::tempdir().unwrap();
        let key = StaticKey([42; 32]);
        let jsonl_path = write_sealed(&key, &dir.path().join("memories.jsonl"), &jsonl).unwrap();
        let md_path =
            write_sealed(&key, &dir.path().join("memories.md"), markdown.as_bytes()).unwrap();

        assert!(!dir.path().join("memories.jsonl").exists());
        assert!(!dir.path().join("memories.md").exists());
        let restored = decode_jsonl(&read_sealed(&key, &jsonl_path).unwrap()).unwrap();
        assert_eq!(restored.len(), 1);
        assert_eq!(restored[0].id, record.id);
        assert_eq!(restored[0].source_type, record.source_type);
        assert_eq!(restored[0].text, record.text);
        assert_eq!(restored[0].screenshot_path, None);
        let readable = String::from_utf8(read_sealed(&key, &md_path).unwrap()).unwrap();
        assert!(readable.contains("synthetic-id"));
        assert!(readable.contains("A synthetic decision was recorded."));
    }

    #[test]
    fn restore_rejects_unknown_format_and_duplicate_ids() {
        let jsonl = encode_jsonl(&[memory(), memory()]).unwrap();
        assert!(decode_jsonl(&jsonl).is_err());
        assert!(decode_jsonl(b"{\"format\":\"other\",\"version\":1}\n").is_err());
    }

    #[test]
    fn markdown_escapes_html_from_captured_fields() {
        let mut record = memory();
        record.window_title = "<script>title</script>".to_string();
        record.app_name = "<b>app</b>".to_string();
        record.text = "<img src=x>".to_string();
        let markdown = encode_markdown(&[record]);
        assert!(!markdown.contains("<script>"));
        assert!(!markdown.contains("<b>"));
        assert!(!markdown.contains("<img"));
        assert!(markdown.contains("&lt;script>"));
    }

    #[tokio::test]
    async fn restore_is_idempotent_and_applies_current_visibility_rules() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_path_buf();
        let store = tokio::task::spawn_blocking(move || crate::storage::Store::new(&path).unwrap())
            .await
            .unwrap();
        let mut blocked = memory();
        blocked.id = "blocked-id".to_string();
        blocked.app_name = "Blocked App".to_string();
        let mut banking = memory();
        banking.id = "bank-id".to_string();
        banking.url = Some("https://www.chase.com/accounts".to_string());
        let mut manager = memory();
        manager.id = "manager-id".to_string();
        manager.app_name = "Bitwarden".to_string();
        let rows =
            decode_jsonl(&encode_jsonl(&[memory(), blocked, banking, manager]).unwrap()).unwrap();

        assert_eq!(
            restore_records(&store, &rows, &["Blocked App".to_string()])
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            restore_records(&store, &rows, &["Blocked App".to_string()])
                .await
                .unwrap(),
            0
        );
        let stored = store.list_all_memories().await.unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].id, "synthetic-id");
    }

    #[tokio::test]
    async fn archive_files_export_only_visible_rows_and_restore_from_jsonl() {
        let source_dir = tempfile::tempdir().unwrap();
        let source_path = source_dir.path().to_path_buf();
        let source =
            tokio::task::spawn_blocking(move || crate::storage::Store::new(&source_path).unwrap())
                .await
                .unwrap();
        let mut blocked = memory();
        blocked.id = "blocked-id".to_string();
        blocked.app_name = "Blocked App".to_string();
        let mut banking = memory();
        banking.id = "bank-id".to_string();
        banking.url = Some("https://www.chase.com/accounts".to_string());
        source
            .add_batch_preserving_ids(&[memory(), blocked, banking])
            .await
            .unwrap();
        let archive_dir = tempfile::tempdir().unwrap();
        let key = StaticKey([42; 32]);
        let report = export_archive(
            &source,
            &["Blocked App".to_string()],
            &key,
            archive_dir.path(),
        )
        .await
        .unwrap();
        assert_eq!(report.exported, 1);
        assert!(report.jsonl_path.ends_with(".fndrenc"));
        assert!(report.markdown_path.ends_with(".fndrenc"));

        let target_dir = tempfile::tempdir().unwrap();
        let target_path = target_dir.path().to_path_buf();
        let target =
            tokio::task::spawn_blocking(move || crate::storage::Store::new(&target_path).unwrap())
                .await
                .unwrap();
        let archive = std::path::Path::new(&report.jsonl_path);
        assert_eq!(
            import_archive(&target, &[], &key, archive).await.unwrap(),
            1
        );
        assert_eq!(
            import_archive(&target, &[], &key, archive).await.unwrap(),
            0
        );
        assert!(target
            .get_memory_by_id("synthetic-id")
            .await
            .unwrap()
            .is_some());
        assert!(target
            .get_memory_by_id("blocked-id")
            .await
            .unwrap()
            .is_none());
    }

    #[test]
    fn an_archived_import_row_that_admission_would_refuse_is_not_exportable() {
        let banking = MemoryRecord {
            id: "imp_banking".into(),
            source_type: "import:browser".into(),
            app_name: "Chrome".into(),
            window_title: "Account".into(),
            url: Some("https://www.chase.com/personal/accounts".into()),
            text: "Account overview".into(),
            clean_text: "Account overview".into(),
            ..Default::default()
        };
        let ordinary = MemoryRecord {
            id: "imp_docs".into(),
            source_type: "import:browser".into(),
            app_name: "Chrome".into(),
            window_title: "Rust book".into(),
            url: Some("https://doc.rust-lang.org/book/".into()),
            text: "The Rust Programming Language".into(),
            clean_text: "The Rust Programming Language".into(),
            ..Default::default()
        };
        assert!(!import_row_is_admitted(&banking, &[]));
        assert!(import_row_is_admitted(&ordinary, &[]));
    }
}
