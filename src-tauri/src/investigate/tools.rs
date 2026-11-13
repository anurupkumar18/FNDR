//! The loop's tools over FNDR's store. Each one is a path another surface
//! already uses: Search's `retrieve`, Resume's threads, the Vault's stored
//! links, To-dos' own tasks. Every memory passes the one visibility rule
//! before it becomes a `CaseMemory`, and imported rows and assistant notes
//! are left out: a case file reports the person's own work.

use super::agent::{CaseMemory, Hit, Search, Thread, Todo, Tools};
use crate::context_runtime::retrieval_routes::memory_record_to_search_result;
use crate::context_runtime::retrieve::memory_is_visible;
use crate::context_runtime::{related_memories, retrieve, RetrieveRequest};
use crate::inference::extraction_evidence::has_source_evidence;
use crate::search::memory_cards::build_fallback_card;
use crate::storage::MemoryRecord;
use crate::AppState;

/// Hits read for one search.
const SEARCH_LIMIT: usize = 12;
/// How far back work threads are read.
const THREAD_WINDOW_HOURS: u32 = 30 * 24;
const MAX_LINKS: usize = 6;

pub struct StoreTools<'a> {
    state: &'a AppState,
}

impl<'a> StoreTools<'a> {
    pub fn new(state: &'a AppState) -> Self {
        Self { state }
    }

    fn blocklist(&self) -> Vec<String> {
        self.state.config.read().blocklist.clone()
    }

    /// The memories with these ids that may be shown now, in the order asked.
    async fn memories(&self, ids: &[String]) -> Result<Vec<CaseMemory>, String> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let blocklist = self.blocklist();
        let mut found = self
            .state
            .store
            .get_memories_by_ids(ids)
            .await
            .map_err(|error| error.to_string())?;
        Ok(ids
            .iter()
            .filter_map(|id| found.remove(id))
            .filter_map(|record| case_memory(&record, &blocklist))
            .collect())
    }
}

fn trimmed(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect()
}

/// A memory as a case holds it, or `None` when it may not be shown or is
/// not the person's own work.
pub(crate) fn case_memory(record: &MemoryRecord, blocklist: &[String]) -> Option<CaseMemory> {
    if !memory_is_visible(record, blocklist)
        || record.is_leaf_row()
        || record.is_agent_note()
        || record.is_import()
    {
        return None;
    }
    let card = build_fallback_card("", &memory_record_to_search_result(record, 0.0));
    // The whole stored sentence when it passes the voice rules, as the
    // briefing quotes it. A card's line is clipped and loses the punctuation
    // the done rule reads clause by clause (seen on the seeded store).
    let stored = if record.display_summary.trim().is_empty() {
        &record.snippet
    } else {
        &record.display_summary
    };
    let line = crate::briefing::quotable(stored)
        .map(|sentence| format!("{sentence}."))
        .unwrap_or_else(|| card.summary.trim().to_string());
    // A memory with source statements keeps what it observed; its older
    // next-step fields are not read as pending work (evidence_pack.rs).
    let next_steps = if has_source_evidence(&record.raw_evidence) {
        Vec::new()
    } else {
        let mut steps = trimmed(&record.next_steps);
        for todo in trimmed(&record.todos) {
            if !steps.contains(&todo) {
                steps.push(todo);
            }
        }
        steps
    };
    let mut problems = trimmed(&record.errors);
    for blocker in trimmed(&record.blockers) {
        if !problems.contains(&blocker) {
            problems.push(blocker);
        }
    }
    let place = crate::workset::eligible(record, blocklist)
        .then(|| crate::workset::item_of(record))
        .flatten();
    Some(CaseMemory {
        id: record.id.clone(),
        at_ms: record.timestamp,
        app: record.app_name.trim().to_string(),
        title: card.title.trim().to_string(),
        line,
        decisions: trimmed(&record.decisions),
        next_steps,
        problems,
        has_links: !record.related_memory_ids.is_empty(),
        place,
    })
}

impl Tools for StoreTools<'_> {
    async fn search(&self, query: &str, time: Option<&str>) -> Result<Search, String> {
        let request = RetrieveRequest {
            query: query.to_string(),
            time: time.map(str::to_string),
            limit: SEARCH_LIMIT,
            ..Default::default()
        };
        let result = retrieve(self.state, &request).await?;
        let ids: Vec<String> = result
            .hits
            .iter()
            .map(|hit| hit.memory_id.clone())
            .collect();
        let memories = self.memories(&ids).await?;
        let mut routes: Vec<String> = Vec::new();
        let mut hits = Vec::new();
        for hit in &result.hits {
            let Some(memory) = memories.iter().find(|memory| memory.id == hit.memory_id) else {
                continue;
            };
            for route in &hit.why.routes {
                if !routes.contains(route) {
                    routes.push(route.clone());
                }
            }
            hits.push(Hit {
                memory: memory.clone(),
                score: hit.score,
                matched_terms: hit.why.matched_terms.clone(),
            });
        }
        Ok(Search {
            // The best hit decided `strong_match`; if that row is not the
            // person's own work, nothing is strong.
            strong: result.strong_match
                && result.hits.first().is_some_and(|best| {
                    hits.first()
                        .is_some_and(|kept| kept.memory.id == best.memory_id)
                }),
            hits,
            routes,
        })
    }

    async fn threads(&self) -> Result<Vec<Thread>, String> {
        let blocklist = self.blocklist();
        Ok(crate::resume::build_resume_threads(
            &self.state.store,
            THREAD_WINDOW_HOURS,
            1,
            &blocklist,
        )
        .await?
        .into_iter()
        .map(|thread| Thread {
            key: thread.key,
            title: thread.title,
            member_ids: thread.evidence,
        })
        .collect())
    }

    async fn read(&self, ids: &[String]) -> Result<Vec<CaseMemory>, String> {
        self.memories(ids).await
    }

    async fn links(&self, memory_id: &str) -> Result<Vec<CaseMemory>, String> {
        // `related_memories` resolves stored links first and applies the
        // visibility rule; a card found by similarity is not a link.
        let ids: Vec<String> = related_memories(self.state, memory_id, MAX_LINKS)
            .await?
            .into_iter()
            .filter(|card| {
                card.surfacing_reason
                    .as_ref()
                    .is_some_and(|reason| reason.routes.iter().any(|route| route == "stored_link"))
            })
            .map(|card| card.id)
            .collect();
        self.memories(&ids).await
    }

    async fn todos(&self) -> Result<Vec<Todo>, String> {
        let blocklist = self.blocklist();
        Ok(self
            .state
            .store
            .list_tasks()
            .await
            .map_err(|error| error.to_string())?
            .into_iter()
            .filter(|task| {
                !task.is_completed
                    && !task.is_dismissed
                    && !crate::tasks::suggest::is_suggestion(task)
                    && !crate::tasks::suggest::suggested_from_blocked_app(task, &blocklist)
            })
            .map(|task| {
                let mut memory_ids: Vec<String> = task.source_memory_id.into_iter().collect();
                for id in task.linked_memory_ids {
                    if !memory_ids.contains(&id) {
                        memory_ids.push(id);
                    }
                }
                Todo {
                    id: task.id,
                    title: task.title,
                    memory_ids,
                }
            })
            .collect())
    }
}
