use super::*;
use crate::graph::GraphStore;
use crate::storage::{StateStore, VerbatimStats};
use std::sync::Arc;
use tempfile::tempdir;

const PAGE: &str = "Sam Example profile page. Phone: +1 555 010 0199. \
    Passport number: X0000000. Mailing address: 1 Example Street, Sampleton.";
const NOW: i64 = 1_800_000_000_000;
const DAY: i64 = 86_400_000;

fn on() -> Config {
    let mut config = Config::default();
    config.verbatim.enabled = true;
    config
}

fn record(id: &str, text: &str) -> MemoryRecord {
    MemoryRecord {
        id: id.into(),
        timestamp: NOW,
        app_name: "Safari".into(),
        window_title: format!("Sam Example profile {id}"),
        text: String::new(),
        clean_text: text.into(),
        snippet: "Sam Example's profile page with contact details".into(),
        display_summary: "Profile page with contact details for Sam Example".into(),
        summary_source: "llm".into(),
        source_type: "screen".into(),
        ..Default::default()
    }
}

/// `Store::new` runs its own runtime, so a test already inside one steps out.
fn open_store(dir: &std::path::Path) -> Store {
    tokio::task::block_in_place(|| Store::new(dir).expect("store"))
}

fn store() -> (tempfile::TempDir, Arc<Store>) {
    let dir = tempdir().unwrap();
    let store = Arc::new(open_store(dir.path()));
    (dir, store)
}

async fn land(store: &Store, records: &[MemoryRecord]) {
    store.add_batch_preserving_ids(records).await.unwrap();
}

async fn text_of(store: &Store, id: &str) -> Option<String> {
    store.verbatim_text_unchecked(id).await.unwrap()
}

fn state(dir: &std::path::Path, config: Config) -> Arc<AppState> {
    let store = Arc::new(open_store(dir));
    let state_store =
        Arc::new(tokio::task::block_in_place(|| StateStore::new(dir)).expect("state store"));
    let graph = GraphStore::new(store.clone());
    Arc::new(AppState::new(
        dir.to_path_buf(),
        config,
        store,
        state_store,
        graph,
        None,
    ))
}

#[tokio::test(flavor = "multi_thread")]
async fn the_store_drops_the_page_text_but_verbatim_keeps_it() {
    let (_dir, store) = store();
    let rec = record("m1", PAGE);
    let landed = store.add_batch_and_get_count(&[rec.clone()]).await.unwrap();
    assert_eq!(landed, 1);
    let stored = store.get_memory_by_id("m1").await.unwrap().unwrap();
    assert!(
        !stored.clean_text.contains("Passport number"),
        "compaction replaces the page text with the summary"
    );
    assert_eq!(stored.text, "");

    let kept = keep_batch(&store, &[rec], &on(), false).await.unwrap();
    assert_eq!(kept, 1);
    let text = text_of(&store, "m1").await.unwrap();
    assert!(text.contains("Phone: +1 555 010 0199"));
    assert!(text.contains("Passport number: X0000000"));
}

#[tokio::test(flavor = "multi_thread")]
async fn nothing_is_kept_when_the_switch_is_off() {
    let (_dir, store) = store();
    let rec = record("m1", PAGE);
    land(&store, &[rec.clone()]).await;
    let mut off = Config::default();
    off.verbatim.enabled = false;
    let kept = keep_batch(&store, &[rec], &off, false).await.unwrap();
    assert_eq!(kept, 0);
    assert_eq!(text_of(&store, "m1").await, None);
}

#[tokio::test(flavor = "multi_thread")]
async fn private_mode_keeps_nothing() {
    let (_dir, store) = store();
    let rec = record("m1", PAGE);
    land(&store, &[rec.clone()]).await;
    assert_eq!(keep_batch(&store, &[rec], &on(), true).await.unwrap(), 0);
    assert_eq!(text_of(&store, "m1").await, None);
}

#[tokio::test(flavor = "multi_thread")]
async fn blocked_and_sensitive_frames_keep_nothing() {
    let (_dir, store) = store();
    let mut config = on();
    config.blocklist = vec!["Vaultly".into(), "intranet.example".into()];

    let mut blocked_app = record("app", PAGE);
    blocked_app.app_name = "Vaultly".into();
    let mut blocked_site = record("site", PAGE);
    blocked_site.url = Some("https://intranet.example/people".into());
    let secret = record(
        "secret",
        "Deploy notes. api_key = abc123 for the staging box.",
    );
    let mut bank = record("bank", PAGE);
    bank.window_title = "Sam Example - Online Banking".into();
    let mut notes = record("note", PAGE);
    notes.source_type = "agent".into();
    let mut imported = record("import", PAGE);
    imported.source_type = format!("{}mail", crate::storage::IMPORT_SOURCE_TYPE_PREFIX);
    let mut deleted = record("deleted", PAGE);
    deleted.is_soft_deleted = true;
    let all = [
        blocked_app,
        blocked_site,
        secret,
        bank,
        notes,
        imported,
        deleted,
    ];
    land(&store, &all).await;

    assert_eq!(keep_batch(&store, &all, &config, false).await.unwrap(), 0);
    for rec in &all {
        assert_eq!(text_of(&store, &rec.id).await, None, "{}", rec.id);
    }
    assert_eq!(
        admit(&all[2], source_text(&all[2]), &config, false),
        Err(Skip::Sensitive)
    );
    assert_eq!(admit(&all[0], PAGE, &config, false), Err(Skip::Blocked));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_record_the_store_dropped_leaves_no_orphan_text() {
    let (_dir, store) = store();
    let kept_rec = record("landed", PAGE);
    land(&store, &[kept_rec.clone()]).await;
    let never_landed = record("ghost", PAGE);
    let kept = keep_batch(&store, &[kept_rec, never_landed], &on(), false)
        .await
        .unwrap();
    assert_eq!(kept, 1);
    assert!(text_of(&store, "landed").await.is_some());
    assert_eq!(text_of(&store, "ghost").await, None);
}

#[tokio::test(flavor = "multi_thread")]
async fn text_is_capped_on_a_character_boundary() {
    let (_dir, store) = store();
    let mut config = on();
    config.verbatim.max_bytes_per_memory = 600;
    let long = "caf\u{e9} na\u{ef}ve r\u{e9}sum\u{e9} ".repeat(200);
    let rec = record("long", &long);
    land(&store, &[rec.clone()]).await;
    keep_batch(&store, &[rec], &config, false).await.unwrap();
    let text = text_of(&store, "long").await.unwrap();
    assert!(text.len() <= 600, "{} bytes", text.len());
    assert!(text.len() > 550);
    assert!(long.starts_with(&text));
    assert_eq!(cap_text("  short  ", 100), "short");
    assert_eq!(cap_text("\u{e9}\u{e9}\u{e9}", 3), "\u{e9}");
}

#[tokio::test(flavor = "multi_thread")]
async fn deleting_a_memory_deletes_its_text_on_every_path() {
    let (_dir, store) = store();
    let mut by_domain = record("domain", PAGE);
    by_domain.url = Some("https://tracker.example/me".into());
    let mut old = record("old", PAGE);
    old.timestamp = 1_000;
    let ids = ["one", "domain", "old", "soft", "keep"];
    let mut rows = vec![
        record("one", PAGE),
        by_domain,
        old,
        record("soft", PAGE),
        record("keep", PAGE),
    ];
    land(&store, &rows).await;
    keep_batch(&store, &rows, &on(), false).await.unwrap();
    for id in ids {
        assert!(text_of(&store, id).await.is_some(), "{id} stored");
    }

    store.delete_memory_by_id("one").await.unwrap();
    assert_eq!(text_of(&store, "one").await, None);

    store
        .delete_memories_by_domain("tracker.example")
        .await
        .unwrap();
    assert_eq!(text_of(&store, "domain").await, None);

    store.delete_older_than(30).await.unwrap();
    assert_eq!(text_of(&store, "old").await, None);

    rows[3].is_soft_deleted = true;
    store
        .replace_memory_preserving_chunks(&rows[3])
        .await
        .unwrap();
    assert_eq!(text_of(&store, "soft").await, None);

    assert!(text_of(&store, "keep").await.is_some());
    store.delete_all().await.unwrap();
    assert_eq!(text_of(&store, "keep").await, None);
    assert_eq!(store.verbatim_stats().await.unwrap().rows, 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn retention_removes_text_and_leaves_the_memory() {
    let (_dir, store) = store();
    let mut stale = record("stale", PAGE);
    stale.timestamp = NOW - 31 * DAY;
    let mut fresh = record("fresh", PAGE);
    fresh.timestamp = NOW - 2 * DAY;
    let rows = [stale, fresh];
    land(&store, &rows).await;
    keep_batch(&store, &rows, &on(), false).await.unwrap();

    let report = sweep(&store, &on().verbatim, NOW).await.unwrap();
    assert_eq!(report.expired, 1);
    assert_eq!(text_of(&store, "stale").await, None);
    assert!(text_of(&store, "fresh").await.is_some());
    let memory = store.get_memory_by_id("stale").await.unwrap().unwrap();
    assert!(memory.snippet.contains("Sam Example"));
}

#[tokio::test(flavor = "multi_thread")]
async fn the_budget_evicts_the_oldest_text_first() {
    let (_dir, store) = store();
    let rows: Vec<MemoryRecord> = (0..4)
        .map(|i| {
            let mut rec = record(&format!("m{i}"), &"x ".repeat(1_000));
            rec.timestamp = NOW - (4 - i) * DAY;
            rec
        })
        .collect();
    land(&store, &rows).await;
    keep_batch(&store, &rows, &on(), false).await.unwrap();
    let per_row = store.verbatim_stats().await.unwrap().bytes / 4;
    let removed = store
        .enforce_verbatim_budget(per_row * 2 + 10)
        .await
        .unwrap();
    assert_eq!(removed, 2);
    assert_eq!(text_of(&store, "m0").await, None);
    assert_eq!(text_of(&store, "m1").await, None);
    assert!(text_of(&store, "m2").await.is_some());
    assert!(text_of(&store, "m3").await.is_some());
}

#[tokio::test(flavor = "multi_thread")]
async fn turning_the_switch_off_purges_everything() {
    let (_dir, store) = store();
    let rows = [record("a", PAGE), record("b", PAGE)];
    land(&store, &rows).await;
    keep_batch(&store, &rows, &on(), false).await.unwrap();
    let mut off = Config::default().verbatim;
    off.enabled = false;
    let report = sweep(&store, &off, NOW).await.unwrap();
    assert!(report.switched_off);
    assert_eq!(store.verbatim_stats().await.unwrap().rows, 0);
    assert!(store.get_memory_by_id("a").await.unwrap().is_some());
}

#[tokio::test(flavor = "multi_thread")]
async fn the_api_serves_text_only_for_visible_memories() {
    let dir = tempdir().unwrap();
    let mut config = on();
    config.verbatim.retention_days = 30;
    let now = chrono::Utc::now().timestamp_millis();
    let mut rows = vec![
        record("shown", PAGE),
        record("hidden_app", PAGE),
        record("soft", PAGE),
        record("expired", PAGE),
    ];
    for rec in &mut rows {
        rec.timestamp = now - DAY;
    }
    rows[1].app_name = "Vaultly".into();
    rows[3].timestamp = now - 45 * DAY;
    let state = state(dir.path(), config.clone());
    land(&state.store, &rows).await;
    keep_batch(&state.store, &rows, &config, false)
        .await
        .unwrap();
    // After capture: the app is blocked, and one memory is forgotten. The
    // forget purges its text; a stale row put back must still not be served.
    state.config.write().blocklist = vec!["Vaultly".into()];
    rows[2].is_soft_deleted = true;
    state
        .store
        .replace_memory_preserving_chunks(&rows[2])
        .await
        .unwrap();
    assert_eq!(text_of(&state.store, "soft").await, None);
    state
        .store
        .put_verbatim(&[VerbatimEntry {
            memory_id: "soft".into(),
            text: PAGE.into(),
            captured_at: now,
        }])
        .await
        .unwrap();

    let shown = text_for_memory(&state, "shown", 4_000)
        .await
        .unwrap()
        .unwrap();
    assert!(shown.contains("Phone: +1 555 010 0199"));
    assert_eq!(
        text_for_memory(&state, "hidden_app", 4_000).await.unwrap(),
        None
    );
    assert_eq!(text_for_memory(&state, "soft", 4_000).await.unwrap(), None);
    assert_eq!(
        text_for_memory(&state, "expired", 4_000).await.unwrap(),
        None
    );
    assert_eq!(
        text_for_memory(&state, "missing", 4_000).await.unwrap(),
        None
    );

    let limited = text_for_memory(&state, "shown", 20).await.unwrap().unwrap();
    assert!(limited.len() <= 20);

    state.config.write().verbatim.enabled = false;
    assert_eq!(text_for_memory(&state, "shown", 4_000).await.unwrap(), None);
}

#[test]
fn a_merge_keeps_older_text_then_the_new_capture() {
    assert_eq!(join_texts(None, " new "), "new");
    assert_eq!(join_texts(Some("old"), "new"), "old\n\nnew");
    assert_eq!(join_texts(Some("old and new"), "new"), "old and new");
    assert_eq!(join_texts(Some("old"), "  "), "old");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_existing_profile_opens_and_memories_are_unchanged() {
    let dir = tempdir().unwrap();
    {
        let store = open_store(dir.path());
        land(&store, &[record("old", PAGE)]).await;
    }
    let reopened = open_store(dir.path());
    assert!(reopened.get_memory_by_id("old").await.unwrap().is_some());
    assert_eq!(
        reopened.verbatim_stats().await.unwrap(),
        VerbatimStats::default()
    );
}

/// Disk use for synthetic rows written the way capture writes them: a few
/// per flush, each flush its own table version. Run with
/// `cargo test --lib verbatim::tests::measure_disk_growth -- --ignored --nocapture`.
#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn measure_disk_growth() {
    fn dir_bytes(path: &std::path::Path) -> u64 {
        std::fs::read_dir(path)
            .map(|rd| {
                rd.flatten()
                    .map(|e| match e.metadata() {
                        Ok(m) if m.is_dir() => dir_bytes(&e.path()),
                        Ok(m) => m.len(),
                        Err(_) => 0,
                    })
                    .sum()
            })
            .unwrap_or(0)
    }
    let (dir, store) = store();
    let table = dir.path().join("lancedb").join("memory_verbatim_v1.lance");
    // Pseudo-random words, so the table cannot win by compressing repeats.
    let mut seed: u64 = 7;
    let mut page_for = |n: usize| -> String {
        (0..n)
            .map(|_| {
                seed = seed
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                format!("w{:x}", (seed >> 33) % 20_000)
            })
            .collect::<Vec<_>>()
            .join(" ")
    };
    for flush in 0..250 {
        let entries: Vec<VerbatimEntry> = (0..8)
            .map(|i| VerbatimEntry {
                memory_id: format!("m{flush}-{i}"),
                text: cap_text(&page_for(900), 4_096),
                captured_at: NOW,
            })
            .collect();
        store.put_verbatim(&entries).await.unwrap();
    }
    let stats = store.verbatim_stats().await.unwrap();
    let before = dir_bytes(&table);
    store.reclaim_verbatim_space().await.unwrap();
    let after = dir_bytes(&table);
    println!(
        "rows={} text_bytes={} disk_before_compact={} disk_after_compact={}",
        stats.rows, stats.bytes, before, after
    );
}
