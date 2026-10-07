//! Quality scorecard for a COPY of a profile: how good the stored summaries
//! are, how healthy the vectors are, and whether a memory can be found again
//! from its own title or summary (known-item search), through the hybrid
//! path and through the vector branch alone. Also prints the stage-by-stage
//! record of one real capture. Refuses the real profile; pass a copy.
//! Usage: cargo run --example vault_qa -- --data-dir <profile copy> [--sample N]

use fndr_lib::config::Config;
use fndr_lib::embedding::Embedder;
use fndr_lib::graph::GraphStore;
use fndr_lib::ipc::commands::search::search_ranked_results_explained;
use fndr_lib::memory_review::repair_record;
use fndr_lib::storage::{MemoryRecord, StateStore, Store};
use fndr_lib::summariser::narration_filter::{is_placeholder_summary, narration_filter_hits, neutral_voice};
use fndr_lib::AppState;
use serde_json::json;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

fn arg(name: &str) -> Option<String> {
    let mut args = std::env::args();
    while let Some(current) = args.next() {
        if current == name {
            return args.next();
        }
    }
    None
}

static EARLIER: std::sync::OnceLock<BTreeMap<String, String>> = std::sync::OnceLock::new();

fn norm(vector: &[f32]) -> f32 {
    vector.iter().map(|x| x * x).sum::<f32>().sqrt()
}

fn first_words(text: &str, count: usize) -> String {
    text.split_whitespace().take(count).collect::<Vec<_>>().join(" ")
}

fn tally(map: &mut BTreeMap<String, usize>, key: &str) {
    let key = if key.trim().is_empty() { "(empty)" } else { key.trim() };
    *map.entry(key.to_string()).or_default() += 1;
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let data_dir = PathBuf::from(arg("--data-dir").ok_or("--data-dir required")?).canonicalize()?;
    let sample: usize = arg("--sample").and_then(|value| value.parse().ok()).unwrap_or(40);
    let real = dirs::data_dir().ok_or("no data dir")?.join("com.fndr.app");
    if real.canonicalize().is_ok_and(|real| data_dir.starts_with(&real)) {
        return Err("refusing the real FNDR profile; pass a copy".into());
    }

    let store = Arc::new(Store::new(&data_dir)?);
    let state_store = Arc::new(StateStore::new(&data_dir)?);
    let graph = GraphStore::new(store.clone());
    let mut config = Config::default();
    config.search.semantic_timeout_ms = 10_000;
    config.search.snippet_timeout_ms = 10_000;
    config.search.keyword_timeout_ms = 10_000;
    let state = Arc::new(AppState::new(data_dir.clone(), config, store.clone(), state_store, graph, None));
    let embedder = Embedder::new().ok();
    let runtime = tokio::runtime::Runtime::new()?;

    let report = runtime.block_on(async {
        let mut rows: Vec<MemoryRecord> = store.list_all_memories().await.map_err(|e| e.to_string())?;
        rows.sort_by_key(|row| std::cmp::Reverse(row.timestamp));
        let total = rows.len().max(1);

        let (mut placeholder, mut narrated, mut cut, mut no_why, mut zero_vec, mut same_vec, mut session_noise) = (0, 0, 0, 0, 0, 0, 0);
        let (mut by_source, mut by_status, mut by_intent, mut by_model, mut by_activity) =
            (BTreeMap::new(), BTreeMap::new(), BTreeMap::new(), BTreeMap::new(), BTreeMap::new());
        for row in &rows {
            let lower = row.display_summary.trim().to_lowercase();
            placeholder += usize::from(is_placeholder_summary(&row.display_summary));
            narrated += usize::from(
                narration_filter_hits(&row.display_summary) || lower.starts_with("the user") || lower.starts_with("you "),
            );
            cut += usize::from(repair_record(&mut row.clone()).is_some());
            no_why += usize::from(row.insight_why_mattered.trim().is_empty());
            zero_vec += usize::from(norm(&row.embedding) < 1e-6);
            same_vec += usize::from(row.embedding == row.snippet_embedding);
            session_noise += usize::from(row.embedding_text.contains("context_thread: session"));
            tally(&mut by_source, &row.summary_source);
            tally(&mut by_status, &row.enrichment_status);
            tally(&mut by_intent, &row.intent_analysis.intent_label);
            tally(&mut by_model, &format!("{} / {}", row.embedding_model, row.embedding_dim));
            tally(&mut by_activity, &row.activity_type);
        }

        // Search keeps one result per content hash. Memories sharing a hash
        // can never appear together, whatever they say.
        let mut by_hash: BTreeMap<String, usize> = BTreeMap::new();
        for row in &rows {
            tally(&mut by_hash, &row.content_hash);
        }
        let shared: usize = by_hash.values().filter(|count| **count > 1).sum();
        let hidden_low_signal = rows
            .iter()
            .filter(|row| fndr_lib::memory_quality::record_low_signal_reason(row).is_some())
            .count();
        let dedup = json!({
            "hidden_from_search_as_low_signal": hidden_low_signal,
            "memories": rows.len(),
            "distinct_content_hashes": by_hash.len(),
            "memories_sharing_a_hash": shared,
            "largest_group": by_hash.values().max().copied().unwrap_or(0),
            "empty_hash": by_hash.get("(empty)").copied().unwrap_or(0),
        });

        // Do the stored vectors still describe the stored text? Re-embed each
        // row's current primary and snippet text the way capture does and
        // compare with what is stored.
        let mut freshness = BTreeMap::new();
        let fast = std::env::args().any(|current| current == "--fast");
        if let Some(embedder) = embedder.as_ref().filter(|_| !fast) {
            for row in &rows {
                let document = fndr_lib::memory_embedding_document::compose_memory_embedding_document(row, None);
                let mut current = (*row).clone();
                if !fndr_lib::memory_embedding_document::refresh_text_vectors(&mut current, Some(embedder)) {
                    continue;
                }
                let fresh = [current.embedding, current.snippet_embedding];
                let cos = |a: &[f32], b: &[f32]| a.iter().zip(b).map(|(x, y)| x * y).sum::<f32>();
                let bucket = |value: f32| if value >= 0.98 { "matches (0.98+)" } else if value >= 0.85 { "drifted (0.85 to 0.98)" } else { "stale (under 0.85)" };
                tally(&mut freshness, &format!("primary vector {}", bucket(cos(&fresh[0], &row.embedding))));
                tally(&mut freshness, &format!("snippet vector {}", bucket(cos(&fresh[1], &row.snippet_embedding))));
                if cos(&fresh[1], &row.snippet_embedding) < 0.85 {
                    tally(&mut freshness, &format!("stale snippet vector, stored as: {}", row.storage_outcome));
                    tally(&mut freshness, &format!("stale snippet vector, status: {}", row.enrichment_status));
                }
                if document.primary_text != row.embedding_text {
                    tally(&mut freshness, "stored embedding_text differs from the text composed now");
                }
            }
        }

        // Known-item search: can a memory be found again from its own words?
        let candidates: Vec<&MemoryRecord> = rows
            .iter()
            .filter(|row| !row.is_agent_note() && !is_placeholder_summary(&row.display_summary))
            .filter(|row| row.display_summary.split_whitespace().count() >= 5)
            // Search hides low-signal memories on purpose; they are not misses.
            .filter(|row| fndr_lib::memory_quality::record_low_signal_reason(row).is_none())
            .take(sample)
            .collect();
        if let Some(path) = arg("--dump-summaries") {
            let dump: BTreeMap<&str, &str> = rows.iter().map(|row| (row.id.as_str(), row.display_summary.as_str())).collect();
            std::fs::write(path, serde_json::to_string(&dump).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        }
        if let Some(path) = arg("--queries") {
            let earlier: BTreeMap<String, String> = serde_json::from_str(&std::fs::read_to_string(path).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
            let _ = EARLIER.set(earlier);
        }
        let mut search = json!({});
        for (label, query_of) in [
            ("summary_words", (|row: &MemoryRecord| first_words(&row.display_summary, 7)) as fn(&MemoryRecord) -> String),
            // The same summary without a narrator opener ("The user is ..."),
            // which is shared by many memories and says nothing about this one.
            ("summary_gist", |row: &MemoryRecord| first_words(&neutral_voice(&row.display_summary), 8)),
            // The summary a memory had before a rewrite (`--queries`), to check
            // that the words a person remembers still find it afterwards.
            ("earlier_summary_gist", |row: &MemoryRecord| {
                EARLIER.get().and_then(|map| map.get(&row.id)).map_or_else(String::new, |old| first_words(&neutral_voice(old), 8))
            }),
            ("window_title", |row: &MemoryRecord| first_words(&row.window_title, 7)),
        ] {
            let (mut asked, mut h1, mut h5, mut v1, mut v5, mut top_score) = (0usize, 0usize, 0usize, 0usize, 0usize, 0f64);
            let mut same_session_first = 0usize;
            for row in &candidates {
                let query = query_of(row);
                if query.split_whitespace().count() < 2 {
                    continue;
                }
                asked += 1;
                let (results, _) = search_ranked_results_explained(&state, &query, None, None, 10).await?;
                let rank = results.iter().position(|result| result.id == row.id);
                h1 += usize::from(rank == Some(0));
                h5 += usize::from(rank.is_some_and(|rank| rank < 5));
                // Not first, but the top hit is another capture of the same
                // app within 30 minutes: the session was found, not the moment.
                same_session_first += usize::from(rank != Some(0) && results.first().is_some_and(|top| {
                    top.app_name == row.app_name && (top.timestamp - row.timestamp).abs() <= 30 * 60 * 1000
                }));
                top_score += results.first().map_or(0.0, |result| f64::from(result.score));
                if let Some(embedder) = embedder.as_ref() {
                    let vector = embedder.embed_query(&query)?;
                    let hits = store.vector_search(&vector, 10, None, None).await.map_err(|e| e.to_string())?;
                    let rank = hits.iter().position(|hit| hit.id == row.id);
                    v1 += usize::from(rank == Some(0));
                    v5 += usize::from(rank.is_some_and(|rank| rank < 5));
                }
            }
            let rate = |hits: usize| format!("{hits}/{asked}");
            search[label] = json!({
                "hybrid_first": rate(h1), "hybrid_top5": rate(h5),
                "not_first_but_same_session_first": rate(same_session_first),
                "vector_first": rate(v1), "vector_top5": rate(v5),
                "mean_top_score": if asked > 0 { top_score / asked as f64 } else { 0.0 },
            });
        }

        // VS-85: why a memory is not in the top five for its own summary.
        let mut misses = BTreeMap::new();
        let mut miss_count = 0usize;
        for row in candidates.iter().filter(|_| !fast) {
            let query = first_words(&neutral_voice(&row.display_summary), 8);
            let (results, _) = search_ranked_results_explained(&state, &query, None, None, 50).await?;
            let rank = results.iter().position(|result| result.id == row.id);
            if rank.is_some_and(|rank| rank < 5) {
                continue;
            }
            miss_count += 1;
            let summary_embedded = row
                .embedding_text
                .to_lowercase()
                .contains(&query.to_lowercase());
            let crowd = results
                .iter()
                .take(5)
                .filter(|top| top.app_name == row.app_name && (top.timestamp - row.timestamp).abs() <= 30 * 60 * 1000)
                .count();
            let same_summary_above = results
                .iter()
                .take(5)
                .filter(|top| first_words(&top.snippet, 7).eq_ignore_ascii_case(&query))
                .count();
            let vector_rank = match embedder.as_ref() {
                Some(embedder) => {
                    let vector = embedder.embed_query(&query)?;
                    store
                        .vector_search(&vector, 50, None, None)
                        .await
                        .map_err(|e| e.to_string())?
                        .iter()
                        .position(|hit| hit.id == row.id)
                }
                None => None,
            };
            // Exact ranks from the stored vectors, with no dedup and no
            // filters, to separate "the vector is far away" from "a sibling
            // with the same content hash took its place".
            if let Some(embedder) = embedder.as_ref() {
                let q = embedder.embed_query(&query)?;
                let cos = |v: &[f32]| q.iter().zip(v).map(|(a, b)| a * b).sum::<f32>();
                let own_primary = cos(&row.embedding);
                let own_snippet = cos(&row.snippet_embedding);
                let primary_rank = rows.iter().filter(|other| cos(&other.embedding) > own_primary).count();
                let snippet_rank = rows.iter().filter(|other| cos(&other.snippet_embedding) > own_snippet).count();
                let beaten_by_sibling = rows.iter().any(|other| {
                    other.id != row.id && other.content_hash == row.content_hash && cos(&other.embedding) > own_primary
                });
                tally(&mut misses, if primary_rank < 5 { "exact primary-vector rank: top 5" } else if primary_rank < 20 { "exact primary-vector rank: 6 to 20" } else { "exact primary-vector rank: beyond 20" });
                tally(&mut misses, if snippet_rank < 5 { "exact snippet-vector rank: top 5" } else if snippet_rank < 20 { "exact snippet-vector rank: 6 to 20" } else { "exact snippet-vector rank: beyond 20" });
                tally(&mut misses, if beaten_by_sibling { "a same-hash sibling has a closer primary vector" } else { "no same-hash sibling is closer" });
            }
            let sibling_rank = results
                .iter()
                .position(|top| !row.content_hash.is_empty() && top.content_hash == row.content_hash);
            tally(&mut misses, match sibling_rank {
                Some(rank) if rank < 5 => "a same-page sibling (same content hash) is in the top five",
                Some(_) => "a same-page sibling ranks 6 to 50",
                None => "no same-page sibling returned",
            });
            tally(&mut misses, if summary_embedded { "summary is in the embedded text" } else { "summary is NOT in the embedded text" });
            tally(&mut misses, match rank { Some(_) => "hybrid: ranked 6 to 50", None => "hybrid: not in top 50" });
            tally(&mut misses, match vector_rank { Some(rank) if rank < 5 => "vector alone: top 5", Some(_) => "vector alone: ranked 6 to 50", None => "vector alone: not in top 50" });
            tally(&mut misses, if crowd >= 3 { "top five crowded by the same session (3 or more)" } else { "top five not crowded by the same session" });
            tally(&mut misses, if same_summary_above > 0 { "another memory with the same opening words ranks above" } else { "no memory with the same opening words above" });
            tally(&mut misses, &format!("status: {}", row.enrichment_status));
            tally(&mut misses, &format!("stored as: {}", row.storage_outcome));
        }

        // One real capture, stage by stage, as it is stored.
        let traced = rows
            .iter()
            .find(|row| row.summary_source == "llm" && !row.clean_text.trim().is_empty() && !is_placeholder_summary(&row.display_summary));
        let trace = match traced {
            Some(row) => {
                let query = first_words(&row.display_summary, 6);
                let (results, explanation) = search_ranked_results_explained(&state, &query, None, None, 5).await?;
                json!({
                    "1_capture": { "app": row.app_name, "window_title": row.window_title, "url": row.url, "timestamp_ms": row.timestamp, "source_type": row.source_type },
                    "2_text": { "raw_text_chars": row.text.chars().count(), "clean_text_chars": row.clean_text.chars().count(), "ocr_confidence": row.ocr_confidence, "ocr_blocks": row.ocr_block_count, "ocr_noise": row.ocr_noise_score },
                    "3_extraction": { "summary_source": row.summary_source, "synthesis_branch": row.synthesis_branch, "activity_type": row.activity_type, "topic": row.topic, "has_source_evidence": row.raw_evidence.contains("source_evidence"), "entities": row.entities.len(), "decisions": row.decisions.len(), "errors": row.errors.len(), "files": row.files_touched.len() },
                    "4_summary": { "memory_context": row.memory_context, "display_summary": row.display_summary, "snippet": row.snippet },
                    "5_insight": { "what_happened": row.insight_what_happened, "why_mattered": row.insight_why_mattered, "what_changed": row.insight_what_changed, "thread": row.insight_context_thread, "intent": row.intent_analysis.intent_label },
                    "6_embedding": { "model": row.embedding_model, "dim": row.embedding_dim, "embedding_text": row.embedding_text, "primary_norm": norm(&row.embedding), "snippet_norm": norm(&row.snippet_embedding), "support_norm": norm(&row.support_embedding) },
                    "7_storage": { "storage_outcome": row.storage_outcome, "enrichment_status": row.enrichment_status, "reviewer_generation": row.reviewer_generation, "schema_version": row.schema_version },
                    "8_retrieval": { "query": query, "rank_of_this_memory": results.iter().position(|result| result.id == row.id).map(|rank| rank + 1), "top": results.iter().take(3).map(|result| json!({"title": result.window_title, "score": result.score, "is_this_memory": result.id == row.id})).collect::<Vec<_>>(), "routes": explanation.get("production_retrieval") },
                })
            }
            None => json!(null),
        };

        let pct = |count: usize| format!("{count} ({:.0}%)", 100.0 * count as f64 / total as f64);
        Ok::<_, String>(json!({
            "memories": rows.len(),
            "summary_quality": { "placeholder": pct(placeholder), "narrated": pct(narrated), "cut_inside_token": pct(cut), "no_why_it_mattered": pct(no_why) },
            "vector_health": { "zero_primary_vector": pct(zero_vec), "primary_equals_snippet_vector": pct(same_vec), "embedding_text_carries_session_id": pct(session_noise), "model_and_dim": by_model },
            "labels": { "summary_source": by_source, "enrichment_status": by_status, "intent": by_intent, "activity_type": by_activity },
            "search_dedup": dedup,
            "vector_freshness": freshness,
            "known_item_search": { "sampled": candidates.len(), "by_query": search, "misses_by_summary_words": miss_count, "miss_analysis": misses },
            "one_capture": trace,
        }))
    })?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
