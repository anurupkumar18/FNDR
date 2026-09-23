//! Resume Work (FEA-02): groups recent memories into threads a person can
//! pick back up without re-explaining themselves, each with cited,
//! token-budgeted evidence.

pub mod pack;

use crate::storage::{MemoryRecord, Store};
use pack::{pack_within_budget, PackItem, PackResult};
use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize)]
pub struct ResumeThread {
    pub title: String,
    pub last_state: String,
    pub age_minutes: i64,
    pub next_steps: Vec<String>,
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

fn build_thread(mut group: Vec<MemoryRecord>, now_ms: i64, budget_tokens: usize) -> ResumeThread {
    group.sort_by_key(|record| record.timestamp);
    let newest = group.last().expect("build_thread requires a non-empty group");

    let title = thread_key(newest);
    let age_minutes = (now_ms - newest.timestamp).max(0) / 60_000;
    let last_state = {
        let topic = newest.topic.trim();
        let outcome = newest.outcome.trim();
        if !outcome.is_empty() {
            format!("{topic} {outcome}").trim().to_string()
        } else {
            topic.to_string()
        }
    };
    let next_steps: Vec<String> = newest
        .next_steps
        .iter()
        .map(|step| step.trim().to_string())
        .filter(|step| !step.is_empty())
        .collect();

    let mut candidates = Vec::new();
    let mut evidence = Vec::new();
    for record in &group {
        evidence.push(record.id.clone());
        for text in record
            .next_steps
            .iter()
            .chain(record.decisions.iter())
            .chain(record.errors.iter())
            .chain(std::iter::once(&record.memory_context))
        {
            if text.trim().is_empty() {
                continue;
            }
            candidates.push(PackItem {
                memory_id: record.id.clone(),
                text: text.clone(),
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
        last_state,
        age_minutes,
        next_steps,
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
    build_resume_threads(&state.inner().store, hours, budget_tokens).await
}

#[cfg(test)]
mod tests {
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
        assert_eq!(thread.last_state, "shipping the resume feature done");
        assert_eq!(thread.age_minutes, 1);
        assert_eq!(thread.next_steps, vec!["Measure p95 latency".to_string()]);
        assert_eq!(thread.evidence, vec!["m-1".to_string(), "m-2".to_string()]);
    }
}
