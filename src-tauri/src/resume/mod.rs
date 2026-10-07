//! Resume Work (FEA-02): groups recent memories into threads a person can
//! pick back up without re-explaining themselves, each with cited,
//! token-budgeted evidence.

pub mod pack;

use crate::inference::extraction_evidence::{
    has_source_evidence, render_source_statements, source_evidence_sets_from_raw,
};
use crate::storage::{MemoryRecord, Store};
use crate::tasks::extract_from_memory::{extract_task_candidates, TaskCandidate};
use pack::{pack_within_budget, PackItem, PackResult};
use serde::Serialize;
use std::collections::{HashMap, HashSet};

/// A thread suggests at most this many next steps.
const MAX_SUGGESTED_NEXT_STEPS: usize = 3;

#[derive(Debug, Clone, Serialize)]
pub struct ResumeThread {
    pub title: String,
    /// App of the newest memory, shown as a label beside the title.
    pub app_name: String,
    pub last_state: String,
    pub age_minutes: i64,
    pub next_steps: Vec<String>,
    /// Steps from anywhere in the thread, newest memory first, one per step,
    /// each citing the memory it came from (VS-36).
    pub suggested_next_steps: Vec<TaskCandidate>,
    pub evidence: Vec<String>,
    pub pack: PackResult,
}

/// Groups a memory into a thread: its project, falling back to its
/// session_key, then to the domain of its url, then to its app name.
fn thread_key(record: &MemoryRecord) -> String {
    if !record.project.trim().is_empty() {
        return record.project.clone();
    }
    if !record.session_key.trim().is_empty() {
        return record.session_key.clone();
    }
    if let Some(domain) = record
        .url
        .as_deref()
        .and_then(crate::capture::extract_domain)
    {
        return domain;
    }
    record.app_name.clone()
}

/// What a person sees as the thread's name. The grouping key is an internal
/// identifier (`google_chrome:title:grades_for_...`) and is never shown.
fn thread_title(record: &MemoryRecord) -> String {
    let project = record.project.trim();
    if !project.is_empty() {
        return project.to_string();
    }
    let window = record.window_title.trim();
    if !window.is_empty() && !window.eq_ignore_ascii_case(record.app_name.trim()) {
        return window.chars().take(80).collect::<String>().trim().to_string();
    }
    record
        .url
        .as_deref()
        .and_then(crate::capture::extract_domain)
        .unwrap_or_else(|| record.app_name.trim().to_string())
}

const MAX_STATE_CHARS: usize = 160;

/// One line saying where the thread stands. It must add something to the
/// title: a line that only repeats the title or the app name is dropped, and
/// a long line is cut at a word, never mid-word.
fn thread_state(record: &MemoryRecord, title: &str) -> String {
    let first_sentence = record
        .memory_context
        .split_terminator(['.', '!', '?'])
        .next()
        .unwrap_or("");
    let topic = record.topic.trim();
    let outcome = record.outcome.trim().replace('_', " ");
    let topic_line = format!("{topic} {outcome}");
    let repeats = |line: &str| {
        let line = line.trim().trim_end_matches('.').to_lowercase();
        line.is_empty()
            || line == title.trim().to_lowercase()
            || line == record.app_name.trim().to_lowercase()
    };
    // A descriptive sentence wins when it is a real sentence; a two-word
    // summary says less than the topic and its outcome.
    let state = [
        (record.insight_what_happened.as_str(), 4),
        (record.display_summary.as_str(), 4),
        (first_sentence, 4),
        (topic_line.as_str(), 1),
    ]
    .into_iter()
    .filter(|(line, min_words)| line.split_whitespace().count() >= *min_words)
    .map(|(line, _)| crate::summariser::narration_filter::neutral_voice(line))
    .map(|line| line.trim().to_string())
    .find(|line| !repeats(line))
    .map(|line| clip_at_word(&line, MAX_STATE_CHARS))
    .unwrap_or_default();
    state
}

fn clip_at_word(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let clipped: String = text.chars().take(max_chars).collect();
    let cut = clipped.rfind(' ').unwrap_or(clipped.len());
    format!("{}…", clipped[..cut].trim_end_matches([',', ';', ':', ' ']))
}

fn build_thread(mut group: Vec<MemoryRecord>, now_ms: i64, budget_tokens: usize) -> ResumeThread {
    group.sort_by(|a, b| a.timestamp.cmp(&b.timestamp).then_with(|| a.id.cmp(&b.id)));
    let newest = group
        .last()
        .expect("build_thread requires a non-empty group");

    let mut title = thread_title(newest);
    let age_minutes = (now_ms - newest.timestamp).max(0) / 60_000;
    let mut last_state = thread_state(newest, &title);
    // When the only name available is the app, the sentence about the work is
    // the better headline; the app stays visible through the source.
    if title.eq_ignore_ascii_case(newest.app_name.trim()) && !last_state.is_empty() {
        title = clip_at_word(last_state.trim_end_matches('.'), 80);
        last_state = String::new();
    }
    let newest_source_backed = has_source_evidence(&newest.raw_evidence);
    let next_steps: Vec<String> = newest
        .next_steps
        .iter()
        .filter(|_| !newest_source_backed)
        .map(|step| step.trim().to_string())
        .filter(|step| !step.is_empty())
        .collect();
    let mut seen_steps = HashSet::new();
    let suggested_next_steps = group
        .iter()
        .rev()
        .flat_map(extract_task_candidates)
        .filter(|step| seen_steps.insert(crate::tasks::normalize_task_text(&step.title)))
        .take(MAX_SUGGESTED_NEXT_STEPS)
        .collect();

    let mut candidates = Vec::new();
    let mut evidence = Vec::new();
    for record in &group {
        evidence.push(record.id.clone());
        let source_backed = has_source_evidence(&record.raw_evidence);
        for snapshot in source_evidence_sets_from_raw(&record.raw_evidence) {
            let text = render_source_statements(&snapshot);
            if !text.trim().is_empty() {
                candidates.push(PackItem {
                    memory_id: record.id.clone(),
                    text,
                    ts_ms: record.timestamp,
                });
            }
        }
        for text in record
            .next_steps
            .iter()
            .filter(|_| !source_backed)
            .chain(record.decisions.iter())
            .chain(record.errors.iter())
            .chain(std::iter::once(&record.memory_context))
        {
            if text.trim().is_empty() {
                continue;
            }
            candidates.push(PackItem {
                memory_id: record.id.clone(),
                text: if source_backed {
                    format!("Generated context (unverified): {text}")
                } else {
                    text.clone()
                },
                ts_ms: record.timestamp,
            });
        }
    }
    // Newest evidence first: the most recent update to a thread is the most
    // useful thing to keep inside a tight token budget.
    candidates.sort_by(|a, b| b.ts_ms.cmp(&a.ts_ms));
    let pack = pack_within_budget(candidates, budget_tokens);

    ResumeThread {
        title,
        app_name: newest.app_name.trim().to_string(),
        last_state,
        age_minutes,
        next_steps,
        suggested_next_steps,
        evidence,
        pack,
    }
}

/// Builds Resume Work threads from memories captured in the last `hours`
/// hours, each packed within `budget_tokens`. Threads are ordered most
/// recently active first.
pub async fn build_resume_threads(
    store: &Store,
    hours: u32,
    budget_tokens: usize,
    blocklist: &[String],
) -> Result<Vec<ResumeThread>, String> {
    let now_ms = chrono::Utc::now().timestamp_millis();
    let window_ms = i64::from(hours) * 60 * 60 * 1000;
    let start_ms = now_ms - window_ms;

    let records = store
        .get_memories_in_range(start_ms, now_ms)
        .await
        .map_err(|e: Box<dyn std::error::Error>| e.to_string())?;

    let mut by_key: HashMap<String, Vec<MemoryRecord>> = HashMap::new();
    for record in records {
        // Match existing read-side admission before a row can influence
        // thread state, suggestions or citations. Agent notes are not
        // observed work and remain excluded by the remember contract.
        // A downloaded file is a fact for search, not a piece of work to resume.
        if !crate::context_runtime::retrieve::memory_is_visible(&record, blocklist)
            || record.source_type.trim().eq_ignore_ascii_case("agent")
            || record.snippet.starts_with("Downloaded: ")
        {
            continue;
        }
        by_key.entry(thread_key(&record)).or_default().push(record);
    }

    let mut threads: Vec<ResumeThread> = by_key
        .into_values()
        .map(|group| build_thread(group, now_ms, budget_tokens))
        .collect();
    threads.sort_by_key(|thread| thread.age_minutes);
    Ok(threads)
}

#[tauri::command]
pub async fn resume_work(
    state: tauri::State<'_, std::sync::Arc<crate::AppState>>,
    hours: u32,
    budget_tokens: usize,
) -> Result<Vec<ResumeThread>, String> {
    let blocklist = state.inner().config.read().blocklist.clone();
    build_resume_threads(&state.inner().store, hours, budget_tokens, &blocklist).await
}

#[cfg(test)]
mod tests {
    #[test]
    fn thread_state_adds_to_the_title_and_never_cuts_a_word() {
        let empty = MemoryRecord {
            app_name: "Claude".into(),
            topic: "claude".into(),
            ..Default::default()
        };
        assert_eq!(super::thread_state(&empty, "Claude"), "");

        let described = MemoryRecord {
            app_name: "Claude".into(),
            topic: "claude".into(),
            memory_context: "You reviewed the prompt catalog and removed dead model code. More.".into(),
            ..Default::default()
        };
        assert_eq!(
            super::thread_state(&described, "Claude"),
            "Reviewed the prompt catalog and removed dead model code"
        );

        let long = MemoryRecord {
            display_summary: "word ".repeat(60),
            ..Default::default()
        };
        let state = super::thread_state(&long, "ChatGPT");
        assert!(state.ends_with("word…"), "{state}");
        assert!(state.chars().count() <= super::MAX_STATE_CHARS + 1);
    }

    #[test]
    fn thread_title_is_readable_when_the_grouping_key_is_not() {
        let record = MemoryRecord {
            app_name: "Google Chrome".into(),
            window_title: "Grades for Anurup Kumar: 4500-001 Fall 2026".into(),
            session_key: "google_chrome:title:grades_for_anurup_kumar_4500_001_fall_2026".into(),
            ..Default::default()
        };
        assert_eq!(super::thread_key(&record), record.session_key);
        assert_eq!(
            super::thread_title(&record),
            "Grades for Anurup Kumar: 4500-001 Fall 2026"
        );

        let bare = MemoryRecord {
            app_name: "Terminal".into(),
            window_title: "Terminal".into(),
            session_key: "terminal:title:terminal".into(),
            ..Default::default()
        };
        assert_eq!(super::thread_title(&bare), "Terminal");
    }

    use super::*;

    fn record(id: &str, project: &str, timestamp: i64) -> MemoryRecord {
        MemoryRecord {
            id: id.to_string(),
            timestamp,
            project: project.to_string(),
            topic: "reviewing capture pipeline".to_string(),
            outcome: "in_progress".to_string(),
            next_steps: vec!["Write the integration test".to_string()],
            memory_context: "Looked at the merge decision path.".to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn source_backed_resume_packs_observations_without_suggesting_tasks() {
        let mut observed = record("observed-chat", "Draft", 1_800_000_000_000);
        let quote = "Mira: I will review the draft after approval.";
        observed.next_steps = vec!["INVENTED_PENDING_TASK".into()];
        observed.errors = vec!["The preview reported a validation failure.".into()];
        observed.memory_context = "A chat discussed a draft review requiring approval.".into();
        observed.raw_evidence = serde_json::json!({
            "source_evidence": {
                "version":1, "source_sha256":"a".repeat(64),
                "statements":[{"kind":"action", "line":2, "quote":quote}],
                "issues":[]
            }
        })
        .to_string();
        let thread = build_thread(vec![observed], 1_800_000_060_000, 1000);
        assert!(thread.next_steps.is_empty());
        assert!(thread.suggested_next_steps.is_empty());
        let packed = serde_json::to_string(&thread.pack).unwrap();
        assert!(packed.contains(quote));
        assert!(
            packed.contains("observed-chat"),
            "pack retains its memory citation"
        );
        assert!(
            packed.contains("A chat discussed a draft review requiring approval."),
            "useful descriptive context remains available"
        );
        assert!(
            !packed.contains("INVENTED_PENDING_TASK"),
            "stale next steps must not re-enter the resume pack"
        );
    }

    #[test]
    fn thread_key_prefers_project_then_session_then_domain_then_app() {
        assert_eq!(
            thread_key(&MemoryRecord {
                project: "FNDR".to_string(),
                session_key: "should-be-ignored".to_string(),
                ..Default::default()
            }),
            "FNDR"
        );
        assert_eq!(
            thread_key(&MemoryRecord {
                session_key: "chrome:news.ycombinator.com".to_string(),
                url: Some("https://news.ycombinator.com/item?id=1".to_string()),
                ..Default::default()
            }),
            "chrome:news.ycombinator.com"
        );
        assert_eq!(
            thread_key(&MemoryRecord {
                url: Some("https://example.com/path".to_string()),
                app_name: "Chrome".to_string(),
                ..Default::default()
            }),
            "example.com"
        );
        assert_eq!(
            thread_key(&MemoryRecord {
                app_name: "Terminal".to_string(),
                ..Default::default()
            }),
            "Terminal"
        );
    }

    #[test]
    fn build_thread_uses_the_newest_record_for_title_state_and_next_steps() {
        let now = 1_700_000_600_000;
        let older = record("m-1", "FNDR", now - 600_000);
        let mut newer = record("m-2", "FNDR", now - 60_000);
        newer.topic = "shipping the resume feature".to_string();
        newer.outcome = "done".to_string();
        newer.next_steps = vec!["Measure p95 latency".to_string()];

        let thread = build_thread(vec![older, newer], now, 10_000);

        assert_eq!(thread.title, "FNDR");
        assert_eq!(thread.last_state, "Looked at the merge decision path");
        assert_eq!(thread.age_minutes, 1);
        assert_eq!(thread.next_steps, vec!["Measure p95 latency".to_string()]);
        assert_eq!(thread.evidence, vec!["m-1".to_string(), "m-2".to_string()]);
    }

    #[tokio::test]
    async fn resume_excludes_hidden_and_agent_rows_before_building_threads() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().to_path_buf();
        let store =
            tokio::task::spawn_blocking(move || Store::new(&path).map_err(|e| e.to_string()))
                .await
                .expect("join")
                .expect("store");
        let now = chrono::Utc::now().timestamp_millis();
        let mut eligible = record("eligible", "Parser", now - 600_000);
        eligible.app_name = "Cursor".into();
        eligible.clean_text = "Reviewed the parser's query filters and fixed weekday aliases hiding the requested app.".into();
        eligible.ocr_block_count = 5;
        eligible.ocr_confidence = 0.9;

        let mut low_quality = eligible.clone();
        low_quality.id = "low-quality".into();
        low_quality.timestamp = now - 1_200_000;
        low_quality.storage_outcome = "low_quality_evidence".into();
        let mut grounded = low_quality.clone();
        grounded.id = "grounded".into();
        grounded.timestamp = now - 900_000;
        grounded.synthesis_branch = "llm_ocr_grounded_visual_fallback".into();

        let mut hidden = Vec::new();
        for kind in [
            "quarantine",
            "failed",
            "image",
            "internal-name",
            "internal-bundle",
            "agent",
        ] {
            let mut row = eligible.clone();
            row.id = format!("EXCLUDED_{kind}");
            row.timestamp = now - 60_000;
            row.topic = row.id.clone();
            row.memory_context = row.id.clone();
            row.next_steps = vec![row.id.clone()];
            match kind {
                "quarantine" => row.storage_outcome = "quarantine_low_grounding".into(),
                "failed" => {
                    row.raw_evidence = r#"{"extraction_issues":["visual_semantics_failed"]}"#.into()
                }
                "image" => {
                    row.enrichment_status = "visual_metadata_fallback".into();
                    row.clean_text.clear();
                    row.ocr_block_count = 0;
                    row.ocr_confidence = 0.0;
                }
                "internal-name" => row.app_name = "FNDR".into(),
                "internal-bundle" => row.bundle_id = Some("com.fndr.app".into()),
                "agent" => row.source_type = " Agent ".into(),
                _ => unreachable!(),
            }
            hidden.push(row);
        }
        let mut hidden_project = hidden[0].clone();
        hidden_project.id = "EXCLUDED_project".into();
        hidden_project.project = "Excluded project".into();
        hidden.push(hidden_project);
        let excluded_ids = hidden.iter().map(|row| row.id.clone()).collect::<Vec<_>>();
        let mut records = vec![low_quality, grounded, eligible];
        records.extend(hidden);
        store
            .add_batch_preserving_ids(&records)
            .await
            .expect("insert");

        let threads = build_resume_threads(&store, 24, 2000, &[])
            .await
            .expect("resume");
        assert_eq!(threads.len(), 1, "hidden-only projects must not appear");
        let thread = &threads[0];
        assert_eq!(thread.title, "Parser");
        assert_eq!(thread.age_minutes, 10);
        assert_eq!(thread.last_state, "Looked at the merge decision path");
        assert_eq!(thread.evidence, vec!["low-quality", "grounded", "eligible"]);
        let serialized = serde_json::to_string(&threads).expect("serialize threads");
        for id in excluded_ids {
            assert!(!serialized.contains(&id), "excluded source leaked: {id}");
        }
    }

    #[tokio::test]
    async fn resume_rechecks_stored_eligibility_and_deletions() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().to_path_buf();
        let store =
            tokio::task::spawn_blocking(move || Store::new(&path).map_err(|e| e.to_string()))
                .await
                .expect("join")
                .expect("store");
        let now = chrono::Utc::now().timestamp_millis();
        let mut reviewed = record("reviewed", "Parser", now - 600_000);
        let deleted = record("deleted", "Parser", now - 300_000);
        store
            .add_batch_preserving_ids(&[reviewed.clone(), deleted])
            .await
            .expect("insert");
        let before = build_resume_threads(&store, 24, 2000, &[])
            .await
            .expect("initial resume");
        assert_eq!(before[0].evidence, vec!["reviewed", "deleted"]);

        reviewed.storage_outcome = "quarantine_low_grounding".into();
        store
            .replace_memory_preserving_chunks(&reviewed)
            .await
            .expect("update eligibility");
        store
            .delete_memory_by_id("deleted")
            .await
            .expect("delete memory");

        let after = build_resume_threads(&store, 24, 2000, &[])
            .await
            .expect("refresh resume");
        assert!(
            after.is_empty(),
            "hidden and deleted records must not return"
        );
    }
}
