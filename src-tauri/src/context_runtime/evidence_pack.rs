//! Phase 3 evidence collection: join fused hits to their underlying
//! `MemoryRecord` rows and aggregate file / command / decision / error / todo /
//! URL refs, deduplicated and back-pointing to source memories.

use crate::context_runtime::context_pack::{
    CommandRef, DecisionRef, ErrorRef, EvidencePack, FileRef, FusedHit, SourceStatementRef,
    TaskRef, UrlRef,
};
use crate::inference::extraction_evidence::{has_source_evidence, source_evidence_sets_from_raw};
use crate::storage::{MemoryRecord, Store};
use crate::telemetry::runtime_metrics;
use std::collections::HashMap;

const MAX_SOURCE_STATEMENTS: usize = 12;
const MAX_SOURCE_QUOTE_CHARS: usize = 6000;

pub async fn collect_evidence(
    hits: &[FusedHit],
    store: &Store,
    blocklist: &[String],
) -> EvidencePack {
    let mut files: HashMap<String, Vec<String>> = HashMap::new();
    let mut commands: HashMap<String, Vec<String>> = HashMap::new();
    let mut decisions: HashMap<String, Vec<String>> = HashMap::new();
    let mut errors: HashMap<String, Vec<String>> = HashMap::new();
    let mut todos: HashMap<String, Vec<String>> = HashMap::new();
    let mut urls: HashMap<String, Vec<String>> = HashMap::new();
    let mut statements = Vec::new();
    let mut quote_chars = 0;

    for hit in hits {
        let Ok(Some(record)) = store.get_memory_by_id(&hit.memory_id).await else {
            continue;
        };
        if !crate::context_runtime::retrieve::memory_is_visible(&record, blocklist) {
            continue;
        }
        push_all(&mut files, &record.files_touched, &record.id);
        push_all(&mut commands, &record.commands, &record.id);
        push_all(&mut decisions, &record.decisions, &record.id);
        push_all(&mut errors, &record.errors, &record.id);
        if !has_source_evidence(&record.raw_evidence) {
            // Legacy task fields remain supported; source-backed observations
            // never inherit their pending-work interpretation.
            push_all(&mut todos, &record.next_steps, &record.id);
            push_all(&mut todos, &record.todos, &record.id);
        }
        for statement in source_statements_for_record(&record) {
            push_source_statement(&mut statements, &mut quote_chars, statement);
        }
        if let Some(url) = record.url.as_ref().filter(|u| !u.trim().is_empty()) {
            push_one(&mut urls, url, &record.id);
        }
    }

    let pack = EvidencePack {
        files: into_refs(files, |key, memory_ids| FileRef {
            path: key,
            memory_ids,
        }),
        commands: into_refs(commands, |key, memory_ids| CommandRef {
            command: key,
            memory_ids,
        }),
        decisions: into_refs(decisions, |key, memory_ids| DecisionRef {
            decision: key,
            memory_ids,
        }),
        errors: into_refs(errors, |key, memory_ids| ErrorRef {
            error: key,
            memory_ids,
        }),
        todos: into_refs(todos, |key, memory_ids| TaskRef {
            task: key,
            memory_ids,
        }),
        urls: into_refs(urls, |key, memory_ids| UrlRef {
            url: key,
            memory_ids,
        }),
        source_statements: statements,
    };

    for _ in &pack.files {
        runtime_metrics::bump("fndr.retrieval.evidence.file.count");
    }
    for _ in &pack.decisions {
        runtime_metrics::bump("fndr.retrieval.evidence.decision.count");
    }
    for _ in &pack.commands {
        runtime_metrics::bump("fndr.retrieval.evidence.command.count");
    }

    pack
}

/// Validated observations, current snapshot first, bounded without cutting quotes.
/// Their line numbers refer to each cited snapshot, not the merged memory text.
pub fn source_statements_for_record(record: &MemoryRecord) -> Vec<SourceStatementRef> {
    let mut statements = Vec::new();
    let mut quote_chars = 0;
    for evidence in source_evidence_sets_from_raw(&record.raw_evidence) {
        for statement in evidence.statements {
            push_source_statement(
                &mut statements,
                &mut quote_chars,
                SourceStatementRef {
                    memory_ids: vec![record.id.clone()],
                    kind: statement.kind,
                    quote: statement.quote,
                    source_sha256: evidence.source_sha256.clone(),
                    line: statement.line,
                },
            );
        }
    }
    statements
}

fn push_source_statement(
    statements: &mut Vec<SourceStatementRef>,
    quote_chars: &mut usize,
    statement: SourceStatementRef,
) {
    if let Some(existing) = statements.iter_mut().find(|existing| {
        existing.source_sha256 == statement.source_sha256
            && existing.line == statement.line
            && existing.kind == statement.kind
            && existing.quote == statement.quote
    }) {
        existing.memory_ids.extend(statement.memory_ids);
        existing.memory_ids.sort();
        existing.memory_ids.dedup();
        return;
    }
    let chars = statement.quote.chars().count();
    if statements.len() < MAX_SOURCE_STATEMENTS && *quote_chars + chars <= MAX_SOURCE_QUOTE_CHARS {
        *quote_chars += chars;
        statements.push(statement);
    }
}

fn push_all(map: &mut HashMap<String, Vec<String>>, values: &[String], memory_id: &str) {
    for value in values {
        push_one(map, value, memory_id);
    }
}

fn push_one(map: &mut HashMap<String, Vec<String>>, raw: &str, memory_id: &str) {
    let key = raw.trim();
    if key.is_empty() {
        return;
    }
    let entry = map.entry(key.to_string()).or_default();
    if !entry.iter().any(|id| id == memory_id) {
        entry.push(memory_id.to_string());
    }
}

fn into_refs<T>(
    map: HashMap<String, Vec<String>>,
    build: impl Fn(String, Vec<String>) -> T,
) -> Vec<T> {
    let mut sorted: Vec<(String, Vec<String>)> = map.into_iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    sorted
        .into_iter()
        .map(|(key, ids)| build(key, ids))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context_runtime::context_pack::{FusionSignals, SurfacingReason};
    use crate::embedding::EMBEDDING_DIM;
    use crate::storage::MemoryRecord;

    fn fused(memory_id: &str) -> FusedHit {
        FusedHit {
            memory_id: memory_id.to_string(),
            score: 1.0,
            signals: FusionSignals::default(),
            surfacing_reason: SurfacingReason::default(),
            contributing_routes: Vec::new(),
        }
    }

    #[tokio::test]
    async fn source_backed_evidence_keeps_snapshot_citations_without_todos() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_path_buf();
        let store = tokio::task::spawn_blocking(move || Store::new(&path).unwrap())
            .await
            .unwrap();
        let raw = serde_json::json!({
            "source_evidence": {
                "version":1, "source_sha256":"a".repeat(64),
                "statements":[{"kind":"action", "line":2, "quote":"Mira: I will review the draft after approval."}],
                "issues":[]
            },
            "source_evidence_history":[{
                "version":1, "source_sha256":"b".repeat(64),
                "statements":[{"kind":"intent", "line":1, "quote":"Mira: The goal is to compare the two proposals."}],
                "issues":[]
            }]
        }).to_string();
        let records = ["observed-a", "observed-b"].map(|id| MemoryRecord {
            id: id.into(),
            app_name: "Chat".into(),
            clean_text:
                "Mira discussed reviewing a draft after approval and comparing two proposals."
                    .into(),
            raw_evidence: raw.clone(),
            next_steps: vec!["Review the draft".into()],
            todos: vec!["Compare the proposals".into()],
            ..Default::default()
        });
        store.add_batch_preserving_ids(&records).await.unwrap();
        let pack = collect_evidence(&[fused("observed-a"), fused("observed-b")], &store, &[]).await;
        let json = serde_json::to_value(&pack).unwrap();
        let statements = json["source_statements"]
            .as_array()
            .expect("source observations in evidence pack");
        assert_eq!(
            statements.len(),
            2,
            "identical snapshot statements share citations"
        );
        let action = statements
            .iter()
            .find(|statement| statement["kind"] == "action")
            .unwrap();
        assert_eq!(
            action["quote"],
            "Mira: I will review the draft after approval."
        );
        assert_eq!(action["line"], 2);
        assert_eq!(action["source_sha256"], "a".repeat(64));
        assert_eq!(
            action["memory_ids"],
            serde_json::json!(["observed-a", "observed-b"])
        );
        let intent = statements
            .iter()
            .find(|statement| statement["kind"] == "intent")
            .unwrap();
        assert_eq!(intent["line"], 1);
        assert_eq!(intent["source_sha256"], "b".repeat(64));
        assert!(
            pack.todos.is_empty(),
            "source observations have no pending task status"
        );
    }

    #[tokio::test]
    async fn source_backed_evidence_budget_preserves_whole_quotes_and_rank_priority() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_path_buf();
        let store = tokio::task::spawn_blocking(move || Store::new(&path).unwrap())
            .await
            .unwrap();
        let snapshot = |hash: &str, quotes: Vec<String>| {
            serde_json::json!({
                "version":1, "source_sha256":hash.repeat(64), "issues":[],
                "statements":quotes.into_iter().enumerate().map(|(i, quote)|
                    serde_json::json!({"kind":"action", "line":i+1, "quote":quote})
                ).collect::<Vec<_>>()
            })
        };
        let make =
            |id: &str, current: serde_json::Value, history: Vec<serde_json::Value>| MemoryRecord {
                id: id.into(),
                app_name: "Chat".into(),
                clean_text: "A discussion of conditional draft review.".into(),
                raw_evidence: serde_json::json!({"source_evidence":current,
                "source_evidence_history":history})
                .to_string(),
                ..Default::default()
            };
        let short = |prefix: &str| {
            (0..6)
                .map(|i| format!("{prefix} statement {i}: do not proceed."))
                .collect()
        };
        let long_quotes: Vec<String> = (0..6)
            .map(|i| {
                let suffix = format!(" {i}: do not proceed.");
                format!("{}{}", "界".repeat(1000 - suffix.chars().count()), suffix)
            })
            .collect();
        store
            .add_batch_preserving_ids(&[
                make(
                    "rank-first",
                    snapshot("f", short("Current")),
                    vec![
                        snapshot("d", short("History")),
                        snapshot("c", short("Older")),
                    ],
                ),
                make("rank-later", snapshot("a", short("Later")), vec![]),
                make(
                    "large-first",
                    snapshot("e", long_quotes.clone()),
                    vec![snapshot("b", short("Short history"))],
                ),
                make("large-copy", snapshot("e", long_quotes.clone()), vec![]),
            ])
            .await
            .unwrap();

        let first = store.get_memory_by_id("rank-first").await.unwrap().unwrap();
        assert_eq!(
            source_statements_for_record(&first).len(),
            12,
            "selected-memory budget"
        );
        let large = store
            .get_memory_by_id("large-first")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            source_statements_for_record(&large).len(),
            6,
            "selected-memory character budget"
        );

        let ranked =
            collect_evidence(&[fused("rank-first"), fused("rank-later")], &store, &[]).await;
        assert_eq!(
            ranked.source_statements.len(),
            12,
            "aggregate statement cap"
        );
        assert!(ranked
            .source_statements
            .iter()
            .all(|s| s.memory_ids == ["rank-first"]));
        assert!(ranked
            .source_statements
            .iter()
            .any(|s| s.quote.starts_with("Current")));
        assert!(ranked
            .source_statements
            .iter()
            .any(|s| s.quote.starts_with("History")));
        let repeated =
            collect_evidence(&[fused("rank-first"), fused("rank-later")], &store, &[]).await;
        assert_eq!(ranked.source_statements, repeated.source_statements);

        let bounded = collect_evidence(
            &[
                fused("large-first"),
                fused("rank-later"),
                fused("large-copy"),
            ],
            &store,
            &[],
        )
        .await;
        assert_eq!(
            bounded.source_statements.len(),
            6,
            "6000-character aggregate quote cap"
        );
        assert!(
            bounded
                .source_statements
                .iter()
                .all(|s| s.memory_ids == ["large-copy", "large-first"]),
            "duplicates retain citations after the budget is full"
        );
        assert_eq!(
            bounded
                .source_statements
                .iter()
                .map(|s| s.quote.chars().count())
                .sum::<usize>(),
            6000
        );
        assert_eq!(
            bounded
                .source_statements
                .iter()
                .map(|s| s.quote.clone())
                .collect::<Vec<_>>(),
            long_quotes
        );
    }

    #[tokio::test]
    async fn source_backed_evidence_rechecks_current_visibility_before_collecting() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_path_buf();
        let store = tokio::task::spawn_blocking(move || Store::new(&path).unwrap())
            .await
            .unwrap();
        let ids = [
            "visible",
            "blocked-app",
            "blocked-title",
            "blocked-url",
            "deleted",
            "internal",
            "low-signal",
        ];
        let rows = ids.iter().map(|id| {
            let mut record = MemoryRecord {
                id: (*id).into(), app_name: "Editor".into(), window_title: "Release checklist".into(),
                clean_text: "Reviewed release verification evidence and discussed a conditional rollout.".into(),
                snippet: "Reviewed release verification evidence and discussed a conditional rollout.".into(),
                files_touched: vec![format!("{id}.md")],
                decisions: vec![format!("{id} decision")],
                raw_evidence: serde_json::json!({"source_evidence":{
                    "version":1,"source_sha256":"a".repeat(64),"issues":[],
                    "statements":[{"kind":"action","line":1,"quote":"Mira: Do not release before approval."}]
                }}).to_string(),
                ..Default::default()
            };
            match *id {
                "blocked-app" => record.app_name = "PrivateWorkspace".into(),
                "blocked-title" => record.window_title = "PrivateWorkspace release checklist".into(),
                "blocked-url" => record.url = Some("https://privateworkspace.example/checklist".into()),
                "deleted" => record.is_soft_deleted = true,
                "internal" => record.bundle_id = Some("com.fndr.app".into()),
                "low-signal" => record.storage_outcome = "visual_semantics_failed".into(),
                _ => {},
            }
            record
        }).collect::<Vec<_>>();
        store.add_batch_preserving_ids(&rows).await.unwrap();
        let hits = ids.iter().map(|id| fused(id)).collect::<Vec<_>>();
        // This rule is applied after the rows were stored and the hit IDs obtained.
        let pack = collect_evidence(&hits, &store, &["privateworkspace".into()]).await;
        assert_eq!(pack.source_statements.len(), 1);
        assert_eq!(pack.source_statements[0].memory_ids, ["visible"]);
        assert_eq!(
            pack.source_statements[0].quote,
            "Mira: Do not release before approval."
        );
        assert_eq!(
            pack.files
                .iter()
                .map(|r| r.path.as_str())
                .collect::<Vec<_>>(),
            ["visible.md"]
        );
        assert_eq!(
            pack.decisions
                .iter()
                .map(|r| r.decision.as_str())
                .collect::<Vec<_>>(),
            ["visible decision"]
        );
        assert!(pack.urls.is_empty());
    }

    #[tokio::test]
    async fn evidence_dedupes_across_memories() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().to_path_buf();
        let store = tokio::task::spawn_blocking(move || Store::new(&path).expect("store"))
            .await
            .expect("blocking");

        let now = chrono::Utc::now().timestamp_millis();
        let make = |id: &str, text: &str, ts: i64, files: Vec<String>| MemoryRecord {
            id: id.to_string(),
            text: text.to_string(),
            clean_text: text.to_string(),
            snippet: text.to_string(),
            app_name: "Terminal".to_string(),
            window_title: format!("FNDR {id}"),
            project: "FNDR".to_string(),
            timestamp: ts,
            embedding: vec![0.0; EMBEDDING_DIM],
            snippet_embedding: vec![0.0; EMBEDDING_DIM],
            support_embedding: vec![0.0; EMBEDDING_DIM],
            confidence_score: 0.8,
            files_touched: files,
            ..Default::default()
        };
        let mut rec_a = make(
            "m-1",
            "FNDR planner debounce fix landed in plan.ts after the alpha bug",
            now,
            vec!["plan.ts".to_string()],
        );
        rec_a.commands = vec!["cargo test".to_string()];
        rec_a.decisions = vec!["use debounce".to_string()];
        let rec_b = make(
            "m-2",
            "FNDR follow-up: refactor plan.ts and fix.ts with the new debounce semantics",
            now + 1_000,
            vec!["plan.ts".to_string(), "fix.ts".to_string()],
        );

        store.add_batch(&[rec_a, rec_b]).await.expect("add");

        let pack = collect_evidence(&[fused("m-1"), fused("m-2")], &store, &[]).await;
        let plan_ref = pack.files.iter().find(|r| r.path == "plan.ts").unwrap();
        assert_eq!(plan_ref.memory_ids.len(), 2);
        assert!(pack.files.iter().any(|r| r.path == "fix.ts"));
        assert!(pack.commands.iter().any(|r| r.command == "cargo test"));
        assert!(pack.decisions.iter().any(|r| r.decision == "use debounce"));
    }
}
