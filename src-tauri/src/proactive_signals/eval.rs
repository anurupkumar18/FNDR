//! Read-only evaluation on a COPY of a profile: how many stuck episodes and
//! what-changed digests the stored memories produce. Prints counts and ids,
//! never screen text. Refuses the real profile.
//!
//! FNDR_SIGNALS_EVAL_DIR=<copy> cargo test --lib proactive_signals::eval -- --ignored --nocapture

use super::stuck::{detect_stuck, past_match_for, StuckKind, LOOKBACK_MS};
use super::thread_digest::{compute_digest, worth_a_nudge};
use crate::config::Config;
use crate::graph::GraphStore;
use crate::resume::{admits, thread_key};
use crate::storage::{StateStore, Store};
use crate::AppState;
use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

const DAY_MS: i64 = 24 * 3_600_000;

#[test]
#[ignore = "needs FNDR_SIGNALS_EVAL_DIR pointing at a profile copy"]
fn evaluate_signals_on_a_profile_copy() {
    let dir = std::path::PathBuf::from(
        std::env::var("FNDR_SIGNALS_EVAL_DIR").expect("FNDR_SIGNALS_EVAL_DIR"),
    )
    .canonicalize()
    .unwrap();
    let real = dirs::data_dir().unwrap().join("com.fndr.app");
    assert!(
        !real.canonicalize().is_ok_and(|real| dir.starts_with(real)),
        "refusing the real FNDR profile; pass a copy"
    );
    let store = Arc::new(Store::new(&dir).unwrap());
    let state_store = Arc::new(StateStore::new(&dir).unwrap());
    let config = Config::default();
    let blocklist = config.blocklist.clone();
    let state = AppState::new(
        dir.clone(),
        config,
        store.clone(),
        state_store,
        GraphStore::new(store.clone()),
        None,
    );
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let mut rows = store.list_all_memories().await.unwrap();
        rows.sort_by_key(|row| row.timestamp);
        let admitted = rows.iter().filter(|r| admits(r, &blocklist)).count();
        let span = |ts: i64| super::stuck::date_label(ts);
        let gaps: Vec<i64> = rows
            .windows(2)
            .map(|pair| (pair[1].timestamp - pair[0].timestamp) / 60_000)
            .collect();
        println!(
            "memories={} admitted={admitted} first={} last={} apps={} gaps_minutes={gaps:?}",
            rows.len(),
            rows.first().map_or(String::new(), |r| span(r.timestamp)),
            rows.last().map_or(String::new(), |r| span(r.timestamp)),
            rows.iter()
                .map(|r| r.app_name.as_str())
                .collect::<HashSet<_>>()
                .len(),
        );

        // Stuck: replay every capture as "now"; one episode per issue per day.
        let mut seen: HashSet<(String, i64)> = HashSet::new();
        let mut episodes = Vec::new();
        let mut start = 0;
        for (end, row) in rows.iter().enumerate() {
            while rows[start].timestamp < row.timestamp - LOOKBACK_MS {
                start += 1;
            }
            if let Some(episode) = detect_stuck(&rows[start..=end], row.timestamp, &blocklist) {
                if seen.insert((episode.issue_key(), row.timestamp / DAY_MS)) {
                    episodes.push((episode, row.app_name.clone()));
                }
            }
        }
        let errors = episodes
            .iter()
            .filter(|(e, _)| e.kind == StuckKind::Error)
            .count();
        let with_error_lines = rows
            .iter()
            .filter(|r| !super::stuck::error_lines(&r.clean_text).is_empty())
            .count();
        println!("captures_with_error_lines={with_error_lines}");
        println!(
            "stuck_episodes={} error={errors} page={}",
            episodes.len(),
            episodes.len() - errors
        );
        let mut with_past = 0;
        for (episode, app) in &episodes {
            let past = past_match_for(&state, episode).await;
            with_past += usize::from(past.is_some());
            println!(
                "EPISODE kind={:?} app={app} minutes={} captures={} first={} past={:?} sample_line={:?}",
                episode.kind,
                episode.minutes(),
                episode.captures,
                episode.memory_ids[0],
                past.map(|p| (p.memory_id, (episode.started_at - p.timestamp) / DAY_MS)),
                std::env::var("FNDR_SIGNALS_EVAL_SHOW_TEXT")
                    .is_ok()
                    .then(|| episode.query.chars().take(90).collect::<String>()),
            );
        }
        println!("episodes_with_past_match={with_past}");

        // What changed: the thread marked seen at its first capture, compared
        // with everything stored after it.
        let tasks = store.list_tasks().await.unwrap_or_default();
        let admitted_rows: Vec<_> = rows
            .iter()
            .filter(|r| admits(r, &blocklist))
            .cloned()
            .collect();
        let mut tally: BTreeMap<&str, usize> = BTreeMap::new();
        let mut first_seen: BTreeMap<String, i64> = BTreeMap::new();
        for row in &admitted_rows {
            first_seen.entry(thread_key(row)).or_insert(row.timestamp);
        }
        for (key, marker) in &first_seen {
            let digest = compute_digest(key, *marker, false, &admitted_rows, &tasks);
            *tally.entry("threads").or_default() += 1;
            if digest.new_memories > 0 {
                *tally.entry("with_new").or_default() += 1;
                println!(
                    "DIGEST new={} pages={} files={} tasks={} commits={} nudge={} newest={:?} title={:?}",
                    digest.new_memories,
                    digest.page_count,
                    digest.file_count,
                    digest.task_count,
                    digest.commit_count,
                    worth_a_nudge(&digest),
                    digest.newest_memory_id,
                    std::env::var("FNDR_SIGNALS_EVAL_SHOW_TEXT")
                        .is_ok()
                        .then_some(&digest.title),
                );
            }
            if worth_a_nudge(&digest) {
                *tally.entry("nudge").or_default() += 1;
            }
            if digest.task_count > 0 {
                let ids: HashSet<&str> = admitted_rows
                    .iter()
                    .filter(|r| r.timestamp > *marker && thread_key(r) == *key)
                    .map(|r| r.id.as_str())
                    .collect();
                let from_source = tasks
                    .iter()
                    .filter(|t| t.created_at > *marker && !t.is_dismissed)
                    .filter(|t| t.source_memory_id.as_deref().is_some_and(|id| ids.contains(id)))
                    .count();
                let completed = tasks
                    .iter()
                    .filter(|t| t.created_at > *marker && !t.is_dismissed && t.is_completed)
                    .filter(|t| digest.tasks.contains(&t.title))
                    .count();
                println!(
                    "TASKS thread_tasks={} from_source_memory={from_source} completed_among_listed={completed} total_tasks={}",
                    digest.task_count,
                    tasks.len()
                );
            }
        }
        println!("digests={tally:?}");
    });
}
