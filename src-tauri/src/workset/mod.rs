//! Work sets (ADR 027): "pull up everything related to the assignment I was
//! working on" becomes the remembered places of one thread of work, resolved
//! on this Mac from Resume threads, open tasks and hybrid search.
//!
//! - `rank` is pure: fixture `MemoryRecord`s in, a `Resolution` out.
//! - `resolve` gathers that evidence from the store.
//! - `open_items` opens what a set holds through the same reopen core as the
//!   Vault, checking privacy again at open time.
//! - `named` keeps sets saved under a name, `routines` mines when sets are
//!   opened, and `arrange` puts opened windows into a layout.

pub mod arrange;
pub mod named;
mod open;
mod rank;
pub mod routines;

pub use open::{open_ids, open_items, open_items_with, ItemOutcome};
pub use rank::{rank, topic_words, Hit, Inputs, ThreadInput};

use serde::{Deserialize, Serialize};

use crate::context_runtime::{retrieve_search_results, RetrieveRequest};
use crate::AppState;

/// A set never holds more than this many items.
pub const MAX_ITEMS: usize = 6;

/// How far back Resume threads are read for candidates.
const THREAD_WINDOW_HOURS: u32 = 7 * 24;

/// Search hits read for one request.
const SEARCH_LIMIT: usize = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    Url,
    File,
    PdfPage,
    App,
    Folder,
}

/// One place a work set reopens, always backed by a memory FNDR captured.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkItem {
    pub memory_id: String,
    /// What the plan card shows: a page title, a file name, an app.
    pub label: String,
    pub kind: ItemKind,
    /// How precisely the target reopens what was seen (`memory::reopen::reopen_rank`).
    pub reopen_rank: u8,
    pub app_name: String,
    /// The site of a link, without `www.`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
    /// Capture time of the memory, in milliseconds.
    pub captured_at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkSet {
    pub id: String,
    pub title: String,
    /// Why this set was chosen, composed from the evidence.
    pub reason: String,
    pub score: f32,
    pub items: Vec<WorkItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "camelCase")]
pub enum Resolution {
    Best(WorkSet),
    /// The best sets scored too close to pick one; the person chooses.
    Ambiguous(Vec<WorkSet>),
    None {
        why: String,
    },
}

/// How long after a set opened "save this as" still means that set.
const SAVE_WINDOW: std::time::Duration = std::time::Duration::from_secs(30 * 60);

/// What the last work set opened in this session held, for "save this as".
/// Kept in memory only; it is gone when FNDR quits.
static LAST_OPENED: parking_lot::Mutex<Option<(Vec<String>, std::time::Instant)>> =
    parking_lot::Mutex::new(None);

/// The memories the last work set opened, if that was recent.
pub fn last_opened() -> Option<Vec<String>> {
    LAST_OPENED
        .lock()
        .as_ref()
        .filter(|(_, at)| at.elapsed() <= SAVE_WINDOW)
        .map(|(ids, _)| ids.clone())
}

fn private(state: &AppState) -> bool {
    state.is_incognito.load(std::sync::atomic::Ordering::SeqCst)
}

/// After a set opened: remembers it for "save this as", notes the time on
/// the saved set holding `requested`, and logs the opening for routines.
/// Nothing is remembered or logged while FNDR is private.
pub fn note_opened(
    state: &AppState,
    requested: &[String],
    opened: &[String],
    label: &str,
    source: routines::Source,
) {
    if opened.is_empty() || private(state) {
        return;
    }
    *LAST_OPENED.lock() = Some((opened.to_vec(), std::time::Instant::now()));
    let now = chrono::Utc::now().timestamp_millis();
    let set_id = named::mark_opened(&state.state_store, requested, now);
    routines::record(state, set_id.as_deref(), requested, label, source);
}

/// Opens memories by id, then arranges their windows when `layout` is
/// given. The arrangement rides on each opened item; it never turns the
/// opens into an error.
pub async fn open_and_arrange(
    state: &AppState,
    memory_ids: &[String],
    layout: Option<crate::operator::layout::Layout>,
    source: routines::Source,
) -> Vec<ItemOutcome> {
    let mut outcomes = open_ids(state, memory_ids).await;
    let opened: Vec<String> = outcomes
        .iter()
        .filter(|outcome| outcome.ok)
        .map(|outcome| outcome.memory_id.clone())
        .collect();
    if let (Some(layout), false) = (layout, opened.is_empty()) {
        let apps = state
            .store
            .get_memories_by_ids(&opened)
            .await
            .unwrap_or_default();
        let windows: Vec<arrange::Opened> = outcomes
            .iter()
            .filter(|outcome| outcome.ok)
            .map(|outcome| arrange::Opened {
                app: apps
                    .get(&outcome.memory_id)
                    .and_then(rank::item_of)
                    .map(|(_, item)| item.app_name)
                    .unwrap_or_default(),
                label: outcome.label.clone(),
            })
            .collect();
        let arrangement = arrange::after_open(state, windows, layout).await;
        for outcome in outcomes.iter_mut().filter(|outcome| outcome.ok) {
            outcome.arrangement = Some(arrangement.clone());
        }
    }
    let label = outcomes
        .iter()
        .find(|outcome| outcome.ok)
        .map(|outcome| outcome.label.clone())
        .unwrap_or_default();
    note_opened(state, memory_ids, &opened, &label, source);
    outcomes
}

/// The saved set a request names, resolved now: `None` when it names none.
/// A named set is planned from what FNDR saved, with no search and no model.
pub async fn resolve_named(state: &AppState, request: &str) -> Option<Resolution> {
    let sets = match named::load(&state.state_store) {
        Ok(sets) => sets,
        Err(error) => {
            tracing::warn!(%error, "workset:named_load_failed");
            return None;
        }
    };
    let names: Vec<String> = sets.iter().map(|set| set.name.clone()).collect();
    let set = &sets[crate::operator::plan::named_set_asked_for(request, &names)?];
    if private(state) {
        return Some(Resolution::None {
            why: "FNDR is private right now, so it does not open saved sets.".to_string(),
        });
    }
    let records = match state.store.get_memories_by_ids(&set.memory_ids).await {
        Ok(records) => records,
        Err(error) => {
            return Some(Resolution::None {
                why: format!("FNDR could not read its memories: {error}"),
            })
        }
    };
    let blocklist = state.config.read().blocklist.clone();
    let shown = named::present(set, &records, &blocklist);
    Some(if shown.items.is_empty() {
        Resolution::None {
            why: format!(
                "Nothing in \u{201c}{}\u{201d} can be reopened now: its memories were deleted or are private.",
                shown.name
            ),
        }
    } else {
        Resolution::Best(named::as_work_set(&shown))
    })
}

/// Words in a request that say when, for the search's time filter.
fn time_filter(query: &str) -> Option<String> {
    let lower = query.to_lowercase();
    if lower.contains("yesterday") {
        Some("yesterday".to_string())
    } else if lower.contains("today") || lower.contains("this morning") {
        Some("today".to_string())
    } else if lower.contains("last week") || lower.contains("this week") {
        Some("7d".to_string())
    } else {
        None
    }
}

/// Resolves a request to a work set from what FNDR holds on this Mac.
/// Nothing leaves the Mac and no model is asked.
pub async fn resolve(state: &AppState, query: &str) -> Resolution {
    if state.is_incognito.load(std::sync::atomic::Ordering::SeqCst) {
        return Resolution::None {
            why: "FNDR is private right now, so it does not look through memories.".to_string(),
        };
    }
    match gather(state, query).await {
        Ok(inputs) => rank(&inputs),
        Err(error) => {
            tracing::warn!(%error, "workset:gather_failed");
            Resolution::None {
                why: format!("FNDR could not read its memories: {error}"),
            }
        }
    }
}

/// Reads the evidence `rank` needs: Resume threads of the last week, open
/// tasks, the search hits for the request's topic words, and every memory
/// any of them names.
async fn gather(state: &AppState, query: &str) -> Result<Inputs, String> {
    let blocklist = state.config.read().blocklist.clone();
    let now_ms = chrono::Utc::now().timestamp_millis();
    let threads =
        crate::resume::build_resume_threads(&state.store, THREAD_WINDOW_HOURS, 1, &blocklist)
            .await?
            .into_iter()
            .map(|thread| ThreadInput {
                title: thread.title,
                member_ids: thread.evidence,
            })
            .collect::<Vec<_>>();
    let tasks = state
        .store
        .list_tasks()
        .await
        .map_err(|e: Box<dyn std::error::Error>| e.to_string())?
        .into_iter()
        .filter(|task| !task.is_completed && !task.is_dismissed)
        .collect::<Vec<_>>();
    let words = topic_words(query);
    let hits = if words.is_empty() {
        Vec::new()
    } else {
        let request = RetrieveRequest {
            query: words.join(" "),
            time: time_filter(query),
            limit: SEARCH_LIMIT,
            ..Default::default()
        };
        match retrieve_search_results(state, &request).await {
            Ok((_, results)) => results
                .into_iter()
                .map(|result| Hit {
                    memory_id: result.id,
                    score: result.score,
                })
                .collect(),
            Err(error) => {
                tracing::warn!(%error, "workset:search_failed");
                Vec::new()
            }
        }
    };
    let mut ids: Vec<String> = threads
        .iter()
        .flat_map(|thread| thread.member_ids.iter().cloned())
        .chain(hits.iter().map(|hit| hit.memory_id.clone()))
        .chain(tasks.iter().flat_map(|task| {
            task.source_memory_id
                .iter()
                .chain(task.linked_memory_ids.iter())
                .cloned()
                .collect::<Vec<_>>()
        }))
        .collect();
    ids.sort();
    ids.dedup();
    let records = state
        .store
        .get_memories_by_ids(&ids)
        .await
        .map_err(|e: Box<dyn std::error::Error>| e.to_string())?;
    Ok(Inputs {
        query: query.to_string(),
        now_ms,
        threads,
        hits,
        tasks,
        records,
        blocklist,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_resolution_serializes_with_its_kind_beside_its_value() {
        let none = serde_json::to_value(Resolution::None { why: "x".into() }).unwrap();
        assert_eq!(
            none,
            serde_json::json!({ "kind": "none", "value": { "why": "x" } })
        );
        let ambiguous = serde_json::to_value(Resolution::Ambiguous(Vec::new())).unwrap();
        assert_eq!(
            ambiguous,
            serde_json::json!({ "kind": "ambiguous", "value": [] })
        );
        let item = WorkItem {
            memory_id: "m".into(),
            label: "Notes.pdf, page 4".into(),
            kind: ItemKind::PdfPage,
            reopen_rank: 7,
            app_name: "Preview".into(),
            host: None,
            page: Some(4),
            captured_at: 1,
        };
        let value = serde_json::to_value(&item).unwrap();
        assert_eq!(value["memoryId"], "m");
        assert_eq!(value["kind"], "pdf_page");
        assert_eq!(value["reopenRank"], 7);
        assert!(value.get("host").is_none());
    }

    fn page(id: &str, app: &str, title: &str, url: &str) -> crate::storage::MemoryRecord {
        crate::storage::MemoryRecord {
            id: id.into(),
            timestamp: 1_800_000_000_000,
            app_name: app.into(),
            window_title: title.into(),
            session_id: "s".into(),
            day_bucket: "2027-01-15".into(),
            clean_text: format!("{title} was open"),
            snippet: format!("{title} was open"),
            embedding: vec![0.01; crate::embedding::EMBEDDING_DIM],
            reopen_kind: crate::memory::reopen::ReopenKind::BrowserUrl,
            reopen_url: Some(url.into()),
            url: Some(url.into()),
            ..Default::default()
        }
    }

    /// A real store and state store: a saved set resolves on this Mac, an
    /// opening is remembered and logged, and Private Mode stops all of it.
    #[test]
    fn saved_sets_and_routines_stay_on_this_mac_and_private_mode_stops_them() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_path_buf();
        let store = std::sync::Arc::new(crate::storage::Store::new(&path).unwrap());
        let state_store = std::sync::Arc::new(crate::storage::StateStore::new(&path).unwrap());
        let graph = crate::graph::GraphStore::new(store.clone());
        let mut config = crate::config::Config::default();
        config.blocklist = vec!["Slack".to_string()];
        let state = AppState::new(path, config, store, state_store, graph, None);
        let ids: Vec<String> = ["canvas", "chat"].iter().map(|id| id.to_string()).collect();
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            state
                .store
                .add_batch_preserving_ids(&[
                    page(
                        "canvas",
                        "Google Chrome",
                        "Capstone",
                        "https://canvas.example/c",
                    ),
                    page("chat", "Slack", "Team chat", "https://slack.example/c"),
                ])
                .await
                .unwrap();
            named::save(&state.state_store, "Capstone demo prep", &ids).unwrap();
            let listed = named::list(&state).await.unwrap();
            assert_eq!(
                listed[0].memory_ids,
                ["canvas"],
                "the blocklisted chat drops out"
            );

            match resolve_named(&state, "set up capstone demo prep side by side").await {
                Some(Resolution::Best(set)) => {
                    assert_eq!(set.title, "Capstone demo prep");
                    assert_eq!(set.items.len(), 1);
                }
                other => panic!("expected the saved set, got {other:?}"),
            }
            assert!(resolve_named(&state, "open Safari").await.is_none());

            note_opened(
                &state,
                &ids,
                &["canvas".to_string()],
                "Capstone",
                routines::Source::Notch,
            );
            assert_eq!(last_opened(), Some(vec!["canvas".to_string()]));
            let saved = named::load(&state.state_store).unwrap();
            assert!(saved[0].last_opened_at.is_some());
            let logged = routines::log(&state.state_store).unwrap();
            assert_eq!(logged.len(), 1);
            assert_eq!(logged[0].set_id.as_deref(), Some(saved[0].id.as_str()));

            state
                .is_incognito
                .store(true, std::sync::atomic::Ordering::SeqCst);
            note_opened(
                &state,
                &ids,
                &["canvas".to_string()],
                "Capstone",
                routines::Source::Home,
            );
            assert_eq!(routines::log(&state.state_store).unwrap().len(), 1);
            assert!(routines::current(&state).await.unwrap().is_empty());
            match resolve_named(&state, "set up capstone demo prep").await {
                Some(Resolution::None { why }) => assert!(why.contains("private"), "{why}"),
                other => panic!("expected a refusal, got {other:?}"),
            }
        });
    }

    #[test]
    fn the_search_is_narrowed_by_when_the_request_says() {
        assert_eq!(
            time_filter("open what I had yesterday").as_deref(),
            Some("yesterday")
        );
        assert_eq!(
            time_filter("pull up the essay from this morning").as_deref(),
            Some("today")
        );
        assert_eq!(time_filter("pull up the essay"), None);
    }

    /// Read-only evaluation on a COPY of a profile. Refuses the real one.
    /// `FNDR_WORKSET_EVAL_DIR=<copy> cargo test --lib work_set_eval -- --ignored --nocapture`
    #[test]
    #[ignore = "reads a copy of a real profile"]
    fn work_set_eval_on_a_vault_copy() {
        let Ok(dir) = std::env::var("FNDR_WORKSET_EVAL_DIR") else {
            return;
        };
        let dir = std::path::PathBuf::from(dir).canonicalize().unwrap();
        let real = dirs::data_dir().unwrap().join("com.fndr.app");
        assert!(
            !real.canonicalize().is_ok_and(|real| dir.starts_with(real)),
            "refusing the real FNDR profile; pass a copy"
        );
        let store = std::sync::Arc::new(crate::storage::Store::new(&dir).unwrap());
        let state_store = std::sync::Arc::new(crate::storage::StateStore::new(&dir).unwrap());
        let graph = crate::graph::GraphStore::new(store.clone());
        let mut config = crate::config::Config::default();
        config.search.semantic_timeout_ms = 10_000;
        config.search.snippet_timeout_ms = 10_000;
        config.search.keyword_timeout_ms = 10_000;
        let state = AppState::new(dir.clone(), config, store, state_store, graph, None);
        let queries: Vec<String> = std::env::var("FNDR_WORKSET_QUERIES")
            .map(|q| q.split('|').map(str::to_string).collect())
            .unwrap_or_default();
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            let now = chrono::Utc::now().timestamp_millis();
            let week = state
                .store
                .get_memories_in_range(now - 7 * 24 * 3_600_000, now)
                .await
                .unwrap();
            println!("memories in the last 7 days: {}", week.len());
            let all = state.store.list_all_memories().await.unwrap();
            let mut kinds = std::collections::BTreeMap::new();
            for record in &all {
                let kind = rank::item_of(record).map(|(_, item)| format!("{:?}", item.kind));
                *kinds
                    .entry(kind.unwrap_or_else(|| "none".into()))
                    .or_insert(0) += 1;
            }
            println!("memories: {}; reopen items by kind: {kinds:?}", all.len());
            if std::env::var("FNDR_WORKSET_LIST").is_ok() {
                for record in &all {
                    println!(
                        "  row {} | {} | {} | project {:?} | session {}",
                        &record.id[..8.min(record.id.len())],
                        record.app_name,
                        record.window_title.chars().take(70).collect::<String>(),
                        record.project,
                        record.session_key.chars().take(40).collect::<String>()
                    );
                }
            }
            for query in queries {
                let started = std::time::Instant::now();
                let resolution = resolve(&state, &query).await;
                println!("\n### {query} ({} ms)", started.elapsed().as_millis());
                let show = |set: &WorkSet| {
                    println!(
                        "- set \"{}\" score {:.3}: {}",
                        set.title, set.score, set.reason
                    );
                    for item in &set.items {
                        println!(
                            "    - {:?} {} | {} | {} | rank {}",
                            item.kind,
                            item.label,
                            item.app_name,
                            item.host.as_deref().unwrap_or(""),
                            item.reopen_rank
                        );
                    }
                };
                match &resolution {
                    Resolution::Best(set) => show(set),
                    Resolution::Ambiguous(sets) => {
                        println!("AMBIGUOUS");
                        sets.iter().for_each(show);
                    }
                    Resolution::None { why } => println!("NONE: {why}"),
                }
            }
        });
    }
}
