//! FEA-02 integration test: Resume Work groups recent memories into threads
//! with citations that all resolve back to real stored memories.

use fndr_lib::embedding::EMBEDDING_DIM;
use fndr_lib::resume::build_resume_threads;
use fndr_lib::storage::{MemoryRecord, Store};

fn record(id: &str, project: &str, minutes_ago: i64, now_ms: i64) -> MemoryRecord {
    MemoryRecord {
        id: id.to_string(),
        timestamp: now_ms - minutes_ago * 60_000,
        app_name: "Terminal".to_string(),
        window_title: format!("{project} work"),
        session_id: format!("session-{project}"),
        project: project.to_string(),
        topic: format!("working on {project}"),
        outcome: "in_progress".to_string(),
        next_steps: vec![format!("Next step for {id}")],
        memory_context: format!("Memory context for {id}"),
        embedding: vec![0.0; EMBEDDING_DIM],
        ..Default::default()
    }
}

#[test]
fn groups_recent_memories_into_cited_threads_and_excludes_stale_ones() {
    std::env::set_var("FNDR_ALLOW_MOCK_EMBEDDER", "1");
    let dir = tempfile::tempdir().expect("tempdir");
    let store = Store::new(dir.path()).expect("store");
    let now_ms = chrono::Utc::now().timestamp_millis();

    let records = vec![
        record("alpha-1", "Alpha", 200, now_ms),
        record("alpha-2", "Alpha", 30, now_ms),
        record("beta-1", "Beta", 180, now_ms),
        record("beta-2", "Beta", 10, now_ms),
        // Older than the 4-hour window: must be excluded entirely.
        record("stale-1", "Alpha", 10 * 60, now_ms),
    ];

    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    runtime
        .block_on(store.add_batch(&records))
        .expect("add records");

    let threads = runtime
        .block_on(build_resume_threads(&store, 4, 2000))
        .expect("build resume threads");

    assert_eq!(threads.len(), 2, "expected exactly two threads, got {threads:?}");

    let stored_ids: std::collections::HashSet<&str> =
        records.iter().map(|r| r.id.as_str()).collect();
    for thread in &threads {
        for id in &thread.evidence {
            assert!(
                stored_ids.contains(id.as_str()),
                "thread {} cited {id}, which is not in the store",
                thread.title
            );
        }
        for item in &thread.pack.items {
            assert!(
                stored_ids.contains(item.memory_id.as_str()),
                "thread {} pack cited {}, which is not in the store",
                thread.title,
                item.memory_id
            );
        }
    }

    let alpha = threads
        .iter()
        .find(|t| t.title == "Alpha")
        .expect("an Alpha thread");
    let mut alpha_evidence = alpha.evidence.clone();
    alpha_evidence.sort();
    assert_eq!(
        alpha_evidence,
        vec!["alpha-1".to_string(), "alpha-2".to_string()],
        "stale-1 must be excluded from the Alpha thread"
    );
    // Newest Alpha memory (alpha-2) is 30 minutes old.
    assert_eq!(alpha.age_minutes, 30);

    let beta = threads
        .iter()
        .find(|t| t.title == "Beta")
        .expect("a Beta thread");
    let mut beta_evidence = beta.evidence.clone();
    beta_evidence.sort();
    assert_eq!(
        beta_evidence,
        vec!["beta-1".to_string(), "beta-2".to_string()]
    );
    assert_eq!(beta.age_minutes, 10);

    // Most recently active thread (Beta, 10 min) sorts before Alpha (30 min).
    assert_eq!(threads[0].title, "Beta");
    assert_eq!(threads[1].title, "Alpha");
}

#[test]
fn resume_work_p95_latency_over_50_calls() {
    // Demo-week-scale volume: 10 projects x 20 memories each, spread over a
    // week, all inside the default 24-hour resume window's neighborhood so
    // the query has real work to filter and group.
    std::env::set_var("FNDR_ALLOW_MOCK_EMBEDDER", "1");
    let dir = tempfile::tempdir().expect("tempdir");
    let store = Store::new(dir.path()).expect("store");
    let now_ms = chrono::Utc::now().timestamp_millis();

    let mut records = Vec::new();
    for project_idx in 0..10 {
        let project = format!("Project-{project_idx}");
        for memory_idx in 0..20 {
            let minutes_ago = (project_idx * 20 + memory_idx) as i64;
            records.push(record(
                &format!("p{project_idx}-m{memory_idx}"),
                &project,
                minutes_ago,
                now_ms,
            ));
        }
    }

    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    runtime
        .block_on(store.add_batch(&records))
        .expect("add records");

    let mut durations_ms: Vec<f64> = Vec::with_capacity(50);
    for _ in 0..50 {
        let started = std::time::Instant::now();
        let threads = runtime
            .block_on(build_resume_threads(&store, 24, 2000))
            .expect("build resume threads");
        durations_ms.push(started.elapsed().as_secs_f64() * 1000.0);
        assert!(!threads.is_empty());
    }

    durations_ms.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let p95_index = ((durations_ms.len() as f64) * 0.95).ceil() as usize - 1;
    let p95 = durations_ms[p95_index.min(durations_ms.len() - 1)];
    let mean = durations_ms.iter().sum::<f64>() / durations_ms.len() as f64;
    println!(
        "resume_work latency over {} calls on {} records: mean {:.2}ms, p95 {:.2}ms",
        durations_ms.len(),
        records.len(),
        mean,
        p95
    );
    assert!(
        p95 <= 2000.0,
        "resume_work p95 {p95:.2}ms exceeded the 2 second budget"
    );
}
