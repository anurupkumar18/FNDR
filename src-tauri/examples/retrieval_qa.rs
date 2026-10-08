//! Retrieval baseline on a seeded profile, through the Search screen
//! (`search_ranked_results`), Ask's context-runtime card path
//! (`context_runtime::run_query`), and the shared `context_runtime::retrieve`
//! every surface is moving to (VS-09). Refuses missing and real profiles.
//! Case kinds: keyword and paraphrase (the headline Recall@5), time and app
//! (reported per kind), and negative (no relevant memory; reported as a
//! no-match row with top scores, never counted in recall).
//! Usage: cargo run --example retrieval_qa -- --data-dir <profile> --cases <json>
//!        [--out <md>] [--json <json>]

use fndr_lib::config::Config;
use fndr_lib::context_runtime::{
    retrieve, run_query, ComposeMode, RetrieveRequest, STRONG_MATCH_SCORE,
    STRONG_MATCH_SCORE_WITH_CHUNKS,
};
use fndr_lib::graph::GraphStore;
use fndr_lib::ipc::commands::reindex_memories_v5_for_state;
use fndr_lib::ipc::commands::search::search_ranked_results;
use fndr_lib::storage::{StateStore, Store};
use fndr_lib::AppState;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

const SCHEMA_VERSION: u8 = 2;
const CASE_SET: &str = "knowledge-worker";
const CASE_KINDS: [&str; 5] = ["keyword", "paraphrase", "time", "app", "negative"];
/// Kinds behind the headline Recall@5 and MRR@10, unchanged since schema v1.
const CORE_KINDS: [&str; 2] = ["keyword", "paraphrase"];
const NEGATIVE_KIND: &str = "negative";
const SEARCH_LIMIT: usize = 20;
const ASK_LIMIT: usize = 10;

#[derive(Debug, Deserialize)]
struct Case {
    query: String,
    kind: String,
    relevant_ids: Vec<String>,
}

#[derive(Debug, Default, Serialize)]
struct LatencyMs {
    p50: u128,
    p95: u128,
}

/// How a path behaves on queries whose right answer is "nothing matches".
/// VS-12 picks its weak-result threshold from these medians.
#[derive(Debug, Default, Serialize)]
struct NoMatch {
    cases: usize,
    returned_nothing: usize,
    /// Negative cases whose best result is under `STRONG_MATCH_SCORE` (or
    /// that returned nothing): the screen says "No strong matches" (VS-12).
    no_strong_match: usize,
    /// Positive cases whose best result is under the bar: these would wrongly
    /// say "No strong matches".
    positive_without_strong_match: usize,
    /// The bar used: `STRONG_MATCH_SCORE`, or the chunk-route bar with
    /// `--chunks` (VS-18).
    bar: f64,
    top_score_median: Option<f64>,
    positive_top_score_median: Option<f64>,
}

#[derive(Debug, Default, Serialize)]
struct PathMetrics {
    recall_at_5: f64,
    mrr_at_10: f64,
    latency_ms: LatencyMs,
    recall_at_5_by_kind: BTreeMap<String, f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    no_match: Option<NoMatch>,
}

#[derive(Debug, Serialize)]
struct PathReports {
    search: PathMetrics,
    ask: PathMetrics,
    retrieve: PathMetrics,
}

#[derive(Debug, Default, Serialize)]
struct Top1Agreement {
    count: usize,
    total: usize,
    rate: f64,
}

#[derive(Debug, Serialize)]
struct QueryReport {
    query: String,
    kind: String,
    search_rank_at_10: Option<usize>,
    ask_rank_at_10: Option<usize>,
    retrieve_rank_at_10: Option<usize>,
    search_top_score: Option<f64>,
    ask_top_score: Option<f64>,
    retrieve_top_score: Option<f64>,
}

#[derive(Debug, Serialize)]
struct RetrievalReport {
    schema_version: u8,
    case_set: String,
    case_count: usize,
    case_count_by_kind: BTreeMap<String, usize>,
    paths: PathReports,
    top1_agreement: Top1Agreement,
    chunk_route: ChunkRouteReport,
    queries: Vec<QueryReport>,
}

/// Whether the chunk route ran (`--chunks`, VS-18) and how many chunk rows
/// the evaluation copy held.
#[derive(Debug, Default, Serialize)]
struct ChunkRouteReport {
    enabled: bool,
    chunks: usize,
}

#[derive(Default)]
struct Score {
    count: usize,
    hits_at_5: usize,
    reciprocal_rank_sum: f64,
}

impl Score {
    fn add(&mut self, rank: Option<usize>) {
        self.count += 1;
        if let Some(rank) = rank.filter(|rank| *rank <= 10) {
            if rank <= 5 {
                self.hits_at_5 += 1;
            }
            self.reciprocal_rank_sum += 1.0 / rank as f64;
        }
    }

    fn recall_at_5(&self) -> f64 {
        self.hits_at_5 as f64 / self.count.max(1) as f64
    }

    fn mrr_at_10(&self) -> f64 {
        self.reciprocal_rank_sum / self.count.max(1) as f64
    }
}

#[derive(Default)]
struct PathScore {
    core: Score,
    by_kind: BTreeMap<String, Score>,
    positive_top_scores: Vec<f64>,
    positive_without_strong_match: usize,
    negative_top_scores: Vec<Option<f64>>,
    latency_ms: Vec<u128>,
    /// The strong-match bar; `None` means `STRONG_MATCH_SCORE`.
    strong_bar: Option<f64>,
}

impl PathScore {
    fn bar(&self) -> f64 {
        self.strong_bar
            .unwrap_or_else(|| f64::from(STRONG_MATCH_SCORE))
    }

    fn is_strong(&self, top_score: Option<f64>) -> bool {
        top_score.is_some_and(|score| score >= self.bar())
    }

    /// `top_score` is the score of the path's first result, `None` when it
    /// returned nothing.
    fn add(&mut self, kind: &str, rank: Option<usize>, top_score: Option<f64>, latency_ms: u128) {
        self.latency_ms.push(latency_ms);
        if kind == NEGATIVE_KIND {
            self.negative_top_scores.push(top_score);
            return;
        }
        if CORE_KINDS.contains(&kind) {
            self.core.add(rank);
        }
        self.by_kind.entry(kind.to_string()).or_default().add(rank);
        self.positive_top_scores.extend(top_score);
        if !self.is_strong(top_score) {
            self.positive_without_strong_match += 1;
        }
    }

    fn metrics(&self) -> PathMetrics {
        let no_match = (!self.negative_top_scores.is_empty()).then(|| {
            let returned = self
                .negative_top_scores
                .iter()
                .flatten()
                .copied()
                .collect::<Vec<_>>();
            NoMatch {
                cases: self.negative_top_scores.len(),
                returned_nothing: self.negative_top_scores.len() - returned.len(),
                no_strong_match: self
                    .negative_top_scores
                    .iter()
                    .filter(|score| !self.is_strong(**score))
                    .count(),
                positive_without_strong_match: self.positive_without_strong_match,
                bar: self.bar(),
                top_score_median: median(&returned),
                positive_top_score_median: median(&self.positive_top_scores),
            }
        });
        PathMetrics {
            recall_at_5: self.core.recall_at_5(),
            mrr_at_10: self.core.mrr_at_10(),
            latency_ms: LatencyMs {
                p50: percentile(&self.latency_ms, 50.0),
                p95: percentile(&self.latency_ms, 95.0),
            },
            recall_at_5_by_kind: self
                .by_kind
                .iter()
                .map(|(kind, score)| (kind.clone(), score.recall_at_5()))
                .collect(),
            no_match,
        }
    }
}

fn median(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let mid = sorted.len() / 2;
    Some(if sorted.len() % 2 == 0 {
        (sorted[mid - 1] + sorted[mid]) / 2.0
    } else {
        sorted[mid]
    })
}

/// 1-based rank of the first result, within `k`, that cites a relevant memory id.
fn first_relevant_rank(
    ranked: &[Vec<String>],
    relevant: &HashSet<String>,
    k: usize,
) -> Option<usize> {
    ranked
        .iter()
        .take(k)
        .position(|ids| ids.iter().any(|id| relevant.contains(id)))
        .map(|index| index + 1)
}

fn percentile(values: &[u128], p: f64) -> u128 {
    if values.is_empty() {
        return 0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let index = ((p / 100.0) * (sorted.len() - 1) as f64).round() as usize;
    sorted[index.min(sorted.len() - 1)]
}

fn rank_label(rank: Option<usize>) -> String {
    rank.map_or_else(|| "miss".to_string(), |rank| rank.to_string())
}

fn arg(name: &str) -> Option<String> {
    let args = std::env::args().collect::<Vec<_>>();
    args.iter()
        .position(|arg| arg == name)
        .and_then(|index| args.get(index + 1).cloned())
}

fn load_cases(path: impl AsRef<Path>) -> Result<Vec<Case>, Box<dyn std::error::Error>> {
    Ok(serde_json::from_slice(&std::fs::read(path)?)?)
}

fn case_set_name(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .and_then(|stem| stem.strip_suffix("-queries").or(Some(stem)))
        .filter(|stem| !stem.is_empty())
        .unwrap_or(CASE_SET)
        .to_string()
}

fn validate_cases(cases: &[Case]) -> Result<(), String> {
    if cases.is_empty() {
        return Err("the case set is empty".to_string());
    }
    let mut seen = HashSet::new();
    for case in cases {
        if case.query.trim().is_empty() {
            return Err("every case needs a query".to_string());
        }
        if (case.kind == NEGATIVE_KIND) != case.relevant_ids.is_empty() {
            return Err(format!(
                "negative cases need empty relevant_ids and every other case needs at least one; invalid query: {:?}",
                case.query
            ));
        }
        if !CASE_KINDS.contains(&case.kind.as_str()) {
            return Err(format!(
                "case {:?} has kind {:?}; expected one of {CASE_KINDS:?}",
                case.query, case.kind
            ));
        }
        if !seen.insert(case.query.as_str()) {
            return Err(format!("duplicate query {:?}", case.query));
        }
    }
    Ok(())
}

fn kind_counts(cases: &[Case]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for case in cases {
        *counts.entry(case.kind.clone()).or_default() += 1;
    }
    counts
}

/// Production config with the retrieval routes' time budgets raised to the
/// largest values `SearchConfig::normalized` allows. A route that runs past its
/// budget silently drops its hits, so with production budgets the ranks would
/// depend on how loaded the machine is (an unoptimized build measured a 354 ms
/// median keyword variant against a 320 ms budget). The gate measures ranking
/// quality; the latency columns still report how long each path took.
fn evaluation_config() -> Config {
    let mut config = Config::default();
    config.search.semantic_timeout_ms = 10_000;
    config.search.snippet_timeout_ms = 10_000;
    config.search.keyword_timeout_ms = 10_000;
    config.search.keyword_variant_timeout_ms = 5_000;
    config
}

fn validate_profile_path(data_dir: &Path, real_profile: Option<&Path>) -> Result<PathBuf, String> {
    if !data_dir.is_dir() {
        return Err("seeded FNDR profile does not exist".to_string());
    }
    let canonical = data_dir
        .canonicalize()
        .map_err(|_| "seeded FNDR profile does not exist".to_string())?;
    if let Some(real_profile) = real_profile {
        let canonical_real = real_profile
            .canonicalize()
            .unwrap_or_else(|_| real_profile.to_path_buf());
        if canonical == canonical_real || canonical.starts_with(&canonical_real) {
            return Err("refusing to evaluate against the real FNDR profile".to_string());
        }
    }
    if !canonical.join("lancedb").is_dir() {
        return Err("seeded FNDR profile is missing its lancedb store".to_string());
    }
    Ok(canonical)
}

fn copy_directory(source: &Path, destination: &Path) -> std::io::Result<()> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let target = destination.join(entry.file_name());
        if file_type.is_dir() {
            copy_directory(&entry.path(), &target)?;
        } else if file_type.is_file() {
            fs::copy(entry.path(), target)?;
        } else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "seeded lancedb contains an unsupported filesystem entry",
            ));
        }
    }
    Ok(())
}

fn evaluation_copy(data_dir: &Path) -> Result<tempfile::TempDir, Box<dyn std::error::Error>> {
    let scratch = tempfile::tempdir()?;
    copy_directory(&data_dir.join("lancedb"), &scratch.path().join("lancedb"))?;
    Ok(scratch)
}

fn markdown_cell(value: &str) -> String {
    value.replace('|', "\\|").replace(['\n', '\r'], " ")
}

fn kind_label(kind: &str) -> String {
    let mut chars = kind.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
        .unwrap_or_default()
}

fn score_label(score: Option<f64>) -> String {
    score.map_or_else(|| "none".to_string(), |score| format!("{score:.3}"))
}

fn render_markdown(report: &RetrievalReport) -> String {
    let kinds = CASE_KINDS
        .iter()
        .copied()
        .filter(|kind| report.case_count_by_kind.contains_key(*kind))
        .collect::<Vec<_>>();
    let positive_kinds = kinds
        .iter()
        .filter(|kind| **kind != NEGATIVE_KIND)
        .copied()
        .collect::<Vec<_>>();
    let paths = [
        ("Search", &report.paths.search),
        ("Ask", &report.paths.ask),
        ("Retrieve", &report.paths.retrieve),
    ];

    let mut lines = vec![
        format!("# Retrieval baseline: {}", report.case_set),
        String::new(),
        if report.chunk_route.enabled {
            format!(
                "Chunk route: on, {} chunks indexed.",
                report.chunk_route.chunks
            )
        } else {
            "Chunk route: off.".to_string()
        },
        String::new(),
        format!(
            "Schema v{}; {} queries ({}).",
            report.schema_version,
            report.case_count,
            kinds
                .iter()
                .map(|kind| format!("{} {kind}", report.case_count_by_kind[*kind]))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        "Search is the pre-card-synthesis ranked retrieval path used by Search; Ask is the context_runtime card path; Retrieve is the shared `retrieve` function (VS-09)."
            .to_string(),
        "Recall@5 is case-level: a case is recalled when at least one accepted relevant ID appears in its top five results."
            .to_string(),
        "Recall@5 and MRR@10 cover keyword and paraphrase cases; other kinds have their own columns, and negative cases are scored only in the no-match table."
            .to_string(),
        "Route time budgets are raised to their configured maximums so ranks do not depend on machine load; latency is still measured."
            .to_string(),
        String::new(),
        format!(
            "| Path | Recall@5 | MRR@10 | {} | p50 ms | p95 ms |",
            positive_kinds
                .iter()
                .map(|kind| format!("{} Recall@5", kind_label(kind)))
                .collect::<Vec<_>>()
                .join(" | ")
        ),
        format!("|---|---:|---:|{}---:|---:|", "---:|".repeat(positive_kinds.len())),
    ];
    for (name, metrics) in paths {
        lines.push(format!(
            "| {name} | {:.3} | {:.3} | {} | {} | {} |",
            metrics.recall_at_5,
            metrics.mrr_at_10,
            positive_kinds
                .iter()
                .map(|kind| metrics
                    .recall_at_5_by_kind
                    .get(*kind)
                    .map_or_else(|| "n/a".to_string(), |recall| format!("{recall:.3}")))
                .collect::<Vec<_>>()
                .join(" | "),
            metrics.latency_ms.p50,
            metrics.latency_ms.p95
        ));
    }
    if paths.iter().any(|(_, metrics)| metrics.no_match.is_some()) {
        lines.extend([
            String::new(),
            "| Path | Negative cases | Returned nothing | Bar | Negatives under the bar | Positives under the bar | Median top score, negative | Median top score, positive |"
                .to_string(),
            "|---|---:|---:|---:|---:|---:|---:|---:|".to_string(),
        ]);
        for (name, metrics) in paths {
            if let Some(no_match) = &metrics.no_match {
                lines.push(format!(
                    "| {name} | {} | {} | {:.2} | {} | {} | {} | {} |",
                    no_match.cases,
                    no_match.returned_nothing,
                    no_match.bar,
                    no_match.no_strong_match,
                    no_match.positive_without_strong_match,
                    score_label(no_match.top_score_median),
                    score_label(no_match.positive_top_score_median)
                ));
            }
        }
    }
    lines.extend([
        String::new(),
        format!(
            "Top-1 agreement: {}/{} ({:.3}).",
            report.top1_agreement.count, report.top1_agreement.total, report.top1_agreement.rate
        ),
        String::new(),
        "| Query | Kind | Search rank@10 | Ask rank@10 | Retrieve rank@10 | Search top score | Ask top score | Retrieve top score |".to_string(),
        "|---|---|---:|---:|---:|---:|---:|---:|".to_string(),
    ]);
    lines.extend(report.queries.iter().map(|query| {
        format!(
            "| {} | {} | {} | {} | {} | {} | {} | {} |",
            markdown_cell(&query.query),
            query.kind,
            rank_label(query.search_rank_at_10),
            rank_label(query.ask_rank_at_10),
            rank_label(query.retrieve_rank_at_10),
            score_label(query.search_top_score),
            score_label(query.ask_top_score),
            score_label(query.retrieve_top_score)
        )
    }));
    lines.join("\n") + "\n"
}

fn write_output(path: &Path, contents: &str) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, contents)?;
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let data_dir = PathBuf::from(arg("--data-dir").ok_or("--data-dir required")?);
    let cases_path = PathBuf::from(arg("--cases").ok_or("--cases required")?);
    let real_profile = dirs::data_dir().map(|dir| dir.join("com.fndr.app"));
    let data_dir = validate_profile_path(&data_dir, real_profile.as_deref())
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidInput, error))?;
    let cases = load_cases(&cases_path)?;
    validate_cases(&cases)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidInput, error))?;

    // Store construction creates tables and may migrate legacy files. Evaluate
    // a disposable lancedb copy so recording a baseline cannot mutate its source.
    let evaluation = evaluation_copy(&data_dir)?;
    let evaluation_dir = evaluation.path().to_path_buf();

    // Store and StateStore each initialize a current-thread runtime internally,
    // so construct them before the evaluator's Tokio runtime.
    let store = Arc::new(Store::new(&evaluation_dir)?);
    let state_store = Arc::new(StateStore::new(&evaluation_dir)?);
    let graph = GraphStore::new(store.clone());
    // `--chunks` (VS-18): index the copy with BGE parent and chunk rows,
    // then measure with the chunk route on.
    let with_chunks = std::env::args().any(|arg| arg == "--chunks");
    let mut config = evaluation_config();
    config.search.use_chunk_first_retrieval = with_chunks;
    let state = Arc::new(AppState::new(
        evaluation_dir,
        config,
        store,
        state_store,
        graph,
        None,
    ));
    let runtime = tokio::runtime::Runtime::new()?;
    let chunk_route = if with_chunks {
        let summary = runtime
            .block_on(reindex_memories_v5_for_state(state.clone()))
            .map_err(|error| std::io::Error::other(format!("--chunks: {error}")))?;
        eprintln!(
            "chunk route: {} memories and {} chunks indexed with {}",
            summary.reindexed, summary.chunks_reindexed, summary.model_name
        );
        ChunkRouteReport {
            enabled: true,
            chunks: summary.chunks_reindexed,
        }
    } else {
        ChunkRouteReport::default()
    };

    let strong_bar = with_chunks.then(|| f64::from(STRONG_MATCH_SCORE_WITH_CHUNKS));
    let new_score = || PathScore {
        strong_bar,
        ..PathScore::default()
    };
    let mut search_score = new_score();
    let mut ask_score = new_score();
    let mut retrieve_score = new_score();
    let mut top1_agreement = 0usize;
    let mut queries = Vec::with_capacity(cases.len());
    // (negatives, negatives marked strong, positives, positives marked weak)
    let mut product_strength = (0usize, 0usize, 0usize, 0usize);
    for case in &cases {
        let relevant = case.relevant_ids.iter().cloned().collect::<HashSet<_>>();

        let started = Instant::now();
        let search_results = runtime.block_on(search_ranked_results(
            &state,
            &case.query,
            None,
            None,
            SEARCH_LIMIT,
        ))?;
        let search_latency_ms = started.elapsed().as_millis();
        let search_top_score = search_results.first().map(|result| f64::from(result.score));
        let search_ranked = search_results
            .into_iter()
            .map(|result| vec![result.id])
            .collect::<Vec<_>>();

        let started = Instant::now();
        let answer = runtime.block_on(run_query(
            &state,
            &case.query,
            ASK_LIMIT,
            ComposeMode::Cards,
        ))?;
        let ask_latency_ms = started.elapsed().as_millis();
        let ask_top_score = answer.cards.first().map(|card| f64::from(card.score));
        let ask_ranked = answer
            .cards
            .into_iter()
            .map(|card| {
                let mut ids = Vec::with_capacity(card.evidence_ids.len() + 1);
                ids.push(card.id);
                ids.extend(card.evidence_ids);
                ids
            })
            .collect::<Vec<_>>();

        let started = Instant::now();
        let retrieved = runtime.block_on(retrieve(
            &state,
            &RetrieveRequest {
                query: case.query.clone(),
                limit: SEARCH_LIMIT,
                ..Default::default()
            },
        ))?;
        let retrieve_latency_ms = started.elapsed().as_millis();
        let retrieve_top_score = retrieved.hits.first().map(|hit| f64::from(hit.score));
        // What the product itself decided, which also weighs matched words
        // and closeness in meaning, not only the score against the bar.
        if case.kind == NEGATIVE_KIND {
            product_strength.0 += 1;
            product_strength.1 += usize::from(retrieved.strong_match);
            if retrieved.strong_match {
                let why = retrieved.hits.first().map(|hit| hit.why.routes.join("+"));
                eprintln!(
                    "no-match query marked strong: {:?} (routes of the top result: {})",
                    case.query,
                    why.unwrap_or_default()
                );
            }
        } else {
            product_strength.2 += 1;
            product_strength.3 += usize::from(!retrieved.strong_match);
        }
        let retrieve_ranked = retrieved
            .hits
            .into_iter()
            .map(|hit| vec![hit.memory_id])
            .collect::<Vec<_>>();

        let search_rank = first_relevant_rank(&search_ranked, &relevant, 10);
        let ask_rank = first_relevant_rank(&ask_ranked, &relevant, 10);
        search_score.add(&case.kind, search_rank, search_top_score, search_latency_ms);
        ask_score.add(&case.kind, ask_rank, ask_top_score, ask_latency_ms);
        let retrieve_rank = first_relevant_rank(&retrieve_ranked, &relevant, 10);
        retrieve_score.add(
            &case.kind,
            retrieve_rank,
            retrieve_top_score,
            retrieve_latency_ms,
        );
        if let (Some(search_first), Some(ask_first)) = (search_ranked.first(), ask_ranked.first()) {
            if search_first.iter().any(|id| ask_first.contains(id)) {
                top1_agreement += 1;
            }
        }
        queries.push(QueryReport {
            query: case.query.clone(),
            kind: case.kind.clone(),
            search_rank_at_10: search_rank,
            ask_rank_at_10: ask_rank,
            retrieve_rank_at_10: retrieve_rank,
            search_top_score,
            ask_top_score,
            retrieve_top_score,
        });
    }

    eprintln!(
        "strong-match flag as the product sets it: {} of {} no-match queries marked strong, {} of {} real queries marked weak",
        product_strength.1, product_strength.0, product_strength.3, product_strength.2
    );
    let case_count = cases.len();
    let report = RetrievalReport {
        schema_version: SCHEMA_VERSION,
        case_set: case_set_name(&cases_path),
        case_count,
        case_count_by_kind: kind_counts(&cases),
        paths: PathReports {
            search: search_score.metrics(),
            ask: ask_score.metrics(),
            retrieve: retrieve_score.metrics(),
        },
        top1_agreement: Top1Agreement {
            count: top1_agreement,
            total: case_count,
            rate: top1_agreement as f64 / case_count.max(1) as f64,
        },
        chunk_route,
        queries,
    };

    let markdown = render_markdown(&report);
    if let Some(path) = arg("--out") {
        write_output(Path::new(&path), &markdown)?;
    }
    if let Some(path) = arg("--json") {
        let json = serde_json::to_string_pretty(&report)? + "\n";
        write_output(Path::new(&path), &json)?;
    }
    print!("{markdown}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    fn json_keys(value: &serde_json::Value) -> HashSet<&str> {
        value
            .as_object()
            .expect("json object")
            .keys()
            .map(String::as_str)
            .collect()
    }

    #[test]
    fn rank_counts_a_card_as_relevant_when_any_cited_id_is_relevant() {
        let relevant: HashSet<String> = ["b".to_string()].into();
        let ranked = vec![ids(&["x"]), ids(&["y", "b"]), ids(&["b"])];
        assert_eq!(first_relevant_rank(&ranked, &relevant, 10), Some(2));
        assert_eq!(first_relevant_rank(&ranked, &relevant, 1), None);
    }

    #[test]
    fn path_score_reports_overall_and_per_kind_metrics() {
        let mut score = PathScore::default();
        score.add("keyword", Some(1), Some(0.9), 10);
        score.add("keyword", None, Some(0.4), 20);
        score.add("paraphrase", Some(4), Some(0.7), 30);
        score.add("paraphrase", Some(7), Some(0.6), 40);

        let metrics = score.metrics();
        assert!((metrics.recall_at_5 - 0.5).abs() < 1e-6);
        assert!((metrics.mrr_at_10 - (1.0 + 0.25 + 1.0 / 7.0) / 4.0).abs() < 1e-6);
        assert!((metrics.recall_at_5_by_kind["keyword"] - 0.5).abs() < 1e-6);
        assert!((metrics.recall_at_5_by_kind["paraphrase"] - 0.5).abs() < 1e-6);
        assert_eq!(metrics.latency_ms.p50, 30);
        assert_eq!(metrics.latency_ms.p95, 40);
        assert!(metrics.no_match.is_none());
    }

    #[test]
    fn time_and_app_cases_get_their_own_recall_but_not_the_headline() {
        let mut score = PathScore::default();
        score.add("keyword", Some(1), Some(0.9), 10);
        score.add("time", None, Some(0.5), 10);
        score.add("app", Some(2), Some(0.8), 10);

        let metrics = score.metrics();
        assert!((metrics.recall_at_5 - 1.0).abs() < 1e-6);
        assert!((metrics.mrr_at_10 - 1.0).abs() < 1e-6);
        assert!((metrics.recall_at_5_by_kind["time"] - 0.0).abs() < 1e-6);
        assert!((metrics.recall_at_5_by_kind["app"] - 1.0).abs() < 1e-6);
        assert!(!metrics.recall_at_5_by_kind.contains_key("paraphrase"));
    }

    #[test]
    fn no_match_counts_queries_whose_best_result_is_weak() {
        let mut score = PathScore::default();
        score.add("keyword", Some(1), Some(0.9), 10);
        score.add("time", Some(3), Some(0.2), 10);
        score.add("negative", None, Some(0.5), 10);
        score.add("negative", None, Some(0.1), 10);
        score.add("negative", None, None, 10);

        let no_match = score.metrics().no_match.expect("negative cases reported");
        // A negative is right when nothing clears the bar; a positive whose
        // best result is under it would show "No strong matches" (VS-12).
        assert_eq!(no_match.no_strong_match, 2);
        assert_eq!(no_match.positive_without_strong_match, 1);
    }

    #[test]
    fn negative_cases_are_scored_as_no_match_and_never_count_as_misses() {
        let mut score = PathScore::default();
        score.add("keyword", Some(1), Some(0.9), 10);
        score.add("paraphrase", Some(2), Some(0.7), 10);
        score.add("negative", None, Some(0.3), 10);
        score.add("negative", None, Some(0.5), 10);
        score.add("negative", None, None, 10);

        let metrics = score.metrics();
        assert!((metrics.recall_at_5 - 1.0).abs() < 1e-6);
        assert!(!metrics.recall_at_5_by_kind.contains_key("negative"));
        let no_match = metrics.no_match.expect("negative cases reported");
        assert_eq!(no_match.cases, 3);
        assert_eq!(no_match.returned_nothing, 1);
        assert_eq!(no_match.top_score_median, Some(0.4));
        assert_eq!(no_match.positive_top_score_median, Some(0.8));
        assert_eq!(metrics.latency_ms.p50, 10);
    }

    #[test]
    fn median_handles_even_odd_and_empty() {
        assert_eq!(median(&[]), None);
        assert_eq!(median(&[3.0, 1.0, 2.0]), Some(2.0));
        assert_eq!(median(&[4.0, 1.0, 2.0, 3.0]), Some(2.5));
    }

    #[test]
    fn percentile_uses_nearest_rank_and_handles_empty() {
        assert_eq!(percentile(&[], 95.0), 0);
        assert_eq!(percentile(&[10, 20, 30, 40, 50], 50.0), 30);
        assert_eq!(percentile(&[10, 20, 30, 40, 50], 95.0), 50);
    }

    #[test]
    fn schema_v2_serializes_only_the_accepted_portable_fields() {
        let cases = vec![
            Case {
                query: "missing example".to_string(),
                kind: "keyword".to_string(),
                relevant_ids: ids(&["x"]),
            },
            Case {
                query: "nothing like this".to_string(),
                kind: "negative".to_string(),
                relevant_ids: Vec::new(),
            },
        ];
        let mut search = PathScore::default();
        search.add("keyword", None, Some(0.4), 10);
        search.add("negative", None, None, 10);
        let report = RetrievalReport {
            schema_version: SCHEMA_VERSION,
            case_set: "knowledge-worker".to_string(),
            case_count: cases.len(),
            case_count_by_kind: kind_counts(&cases),
            paths: PathReports {
                search: search.metrics(),
                ask: PathMetrics::default(),
                retrieve: PathMetrics::default(),
            },
            top1_agreement: Top1Agreement {
                count: 0,
                total: 2,
                rate: 0.0,
            },
            chunk_route: ChunkRouteReport::default(),
            queries: vec![QueryReport {
                query: "missing example".to_string(),
                kind: "keyword".to_string(),
                search_rank_at_10: None,
                ask_rank_at_10: Some(2),
                retrieve_rank_at_10: Some(1),
                search_top_score: Some(0.4),
                ask_top_score: None,
                retrieve_top_score: Some(0.3),
            }],
        };

        let value = serde_json::to_value(report).expect("serialize report");
        let expected: HashSet<&str> = [
            "schema_version",
            "case_set",
            "case_count",
            "case_count_by_kind",
            "paths",
            "top1_agreement",
            "chunk_route",
            "queries",
        ]
        .into();
        assert_eq!(json_keys(&value), expected);
        assert_eq!(
            json_keys(&value["chunk_route"]),
            ["enabled", "chunks"].into()
        );
        assert_eq!(
            json_keys(&value["case_count_by_kind"]),
            ["keyword", "negative"].into()
        );
        assert_eq!(
            json_keys(&value["paths"]),
            ["search", "ask", "retrieve"].into()
        );
        assert_eq!(
            json_keys(&value["paths"]["search"]),
            [
                "recall_at_5",
                "mrr_at_10",
                "latency_ms",
                "recall_at_5_by_kind",
                "no_match",
            ]
            .into()
        );
        // A path without negative cases has no no_match block.
        assert!(value["paths"]["ask"].get("no_match").is_none());
        assert_eq!(
            json_keys(&value["paths"]["search"]["latency_ms"]),
            ["p50", "p95"].into()
        );
        assert_eq!(
            json_keys(&value["paths"]["search"]["recall_at_5_by_kind"]),
            ["keyword"].into()
        );
        assert_eq!(
            json_keys(&value["paths"]["search"]["no_match"]),
            [
                "cases",
                "returned_nothing",
                "no_strong_match",
                "positive_without_strong_match",
                "bar",
                "top_score_median",
                "positive_top_score_median",
            ]
            .into()
        );
        assert_eq!(
            json_keys(&value["top1_agreement"]),
            ["count", "total", "rate"].into()
        );
        assert_eq!(
            json_keys(&value["queries"][0]),
            [
                "query",
                "kind",
                "search_rank_at_10",
                "ask_rank_at_10",
                "retrieve_rank_at_10",
                "search_top_score",
                "ask_top_score",
                "retrieve_top_score",
            ]
            .into()
        );
        assert_eq!(value["schema_version"], 2);
        assert_eq!(value["case_set"], "knowledge-worker");
        assert!(value["queries"][0]["search_rank_at_10"].is_null());
        assert_eq!(value["queries"][0]["ask_rank_at_10"], 2);
        assert!(value["queries"][0]["ask_top_score"].is_null());
        assert!(value.get("generated_at").is_none());
        assert!(value.get("cases_path").is_none());
        assert!(value.get("data_dir").is_none());
    }

    #[test]
    fn markdown_shows_kind_columns_and_the_no_match_table() {
        let cases = vec![
            Case {
                query: "a".to_string(),
                kind: "keyword".to_string(),
                relevant_ids: ids(&["x"]),
            },
            Case {
                query: "b".to_string(),
                kind: "time".to_string(),
                relevant_ids: ids(&["y"]),
            },
            Case {
                query: "c".to_string(),
                kind: "negative".to_string(),
                relevant_ids: Vec::new(),
            },
        ];
        let mut path = PathScore::default();
        path.add("keyword", Some(1), Some(0.9), 10);
        path.add("time", None, Some(0.5), 10);
        path.add("negative", None, Some(0.2), 10);
        let report = RetrievalReport {
            schema_version: SCHEMA_VERSION,
            case_set: "demo".to_string(),
            case_count: 3,
            case_count_by_kind: kind_counts(&cases),
            paths: PathReports {
                search: path.metrics(),
                ask: path.metrics(),
                retrieve: path.metrics(),
            },
            top1_agreement: Top1Agreement::default(),
            chunk_route: ChunkRouteReport::default(),
            queries: Vec::new(),
        };
        let markdown = render_markdown(&report);
        assert!(markdown.contains("Chunk route: off."));
        let markdown = render_markdown(&RetrievalReport {
            chunk_route: ChunkRouteReport {
                enabled: true,
                chunks: 57,
            },
            ..report
        });
        assert!(markdown.contains("Chunk route: on, 57 chunks indexed."));
        assert!(markdown.contains("3 queries (1 keyword, 1 time, 1 negative)"));
        assert!(markdown.contains("| Keyword Recall@5 | Time Recall@5 |"));
        assert!(markdown.contains("| Search | 1 | 0 | 0.25 | 1 | 0 | 0.200 | 0.700 |"));
    }

    #[test]
    fn profile_validation_refuses_missing_and_real_profiles() {
        let root = tempfile::tempdir().expect("temp root");
        let missing = root.path().join("missing");
        assert!(validate_profile_path(&missing, None).is_err());

        let real = root.path().join("com.fndr.app");
        std::fs::create_dir_all(real.join("lancedb")).expect("real profile");
        let error = validate_profile_path(&real, Some(&real)).expect_err("real profile refused");
        assert!(error.contains("real FNDR profile"));

        let descendant = real.join("nested-profile");
        std::fs::create_dir_all(descendant.join("lancedb")).expect("nested profile");
        let error = validate_profile_path(&descendant, Some(&real))
            .expect_err("real profile descendant refused");
        assert!(error.contains("real FNDR profile"));
    }

    #[test]
    fn evaluation_copy_is_disposable_and_case_set_comes_from_fixture_name() {
        let source = tempfile::tempdir().expect("source profile");
        let nested = source.path().join("lancedb/table.lance");
        std::fs::create_dir_all(&nested).expect("table directory");
        std::fs::write(nested.join("data.bin"), b"baseline").expect("source data");

        let copy = evaluation_copy(source.path()).expect("evaluation copy");
        let copied = copy.path().join("lancedb/table.lance/data.bin");
        assert_eq!(std::fs::read(&copied).expect("copied data"), b"baseline");
        std::fs::write(copy.path().join("lancedb/evaluator-only"), b"state")
            .expect("scratch mutation");
        assert!(!source.path().join("lancedb/evaluator-only").exists());

        assert_eq!(
            case_set_name(Path::new("fixtures/knowledge-worker-queries.json")),
            "knowledge-worker"
        );
        assert_eq!(
            case_set_name(Path::new("fixtures/designer-queries.json")),
            "designer"
        );
    }

    #[test]
    fn evaluation_lifts_route_time_budgets_to_their_maximums() {
        let search = evaluation_config().search.normalized();
        assert_eq!(search.semantic_timeout_ms, 10_000);
        assert_eq!(search.snippet_timeout_ms, 10_000);
        assert_eq!(search.keyword_timeout_ms, 10_000);
        assert_eq!(search.keyword_variant_timeout_ms, 5_000);
        // Everything that is not a time budget stays at the production default.
        let production = Config::default().search;
        assert_eq!(search.max_keyword_variants, production.max_keyword_variants);
        assert_eq!(
            search.max_keyword_branch_limit,
            production.max_keyword_branch_limit
        );
    }

    fn demo_fixture(name: &str) -> String {
        concat!(env!("CARGO_MANIFEST_DIR"), "/../scripts/demo/").to_string() + name
    }

    /// Every relevant id must exist in the persona's corpus and must not be
    /// the low-signal control, so a miss always means retrieval missed.
    fn assert_persona_fixture(persona: &str, corpus_size: usize, kinds: &[(&str, usize)]) {
        let cases = load_cases(demo_fixture(&format!("{persona}-queries.json"))).expect("cases");
        let corpus: Vec<serde_json::Value> = serde_json::from_slice(
            &std::fs::read(demo_fixture(&format!("{persona}-week.json"))).expect("corpus"),
        )
        .expect("json");
        assert_eq!(corpus.len(), corpus_size, "{persona} corpus size changed");
        let known: HashSet<&str> = corpus
            .iter()
            .filter_map(|entry| entry["id"].as_str())
            .collect();
        assert_eq!(known.len(), corpus.len(), "{persona} corpus ids are unique");
        let low_signal: HashSet<&str> = corpus
            .iter()
            .filter(|entry| entry["low_signal"].as_bool().unwrap_or(false))
            .filter_map(|entry| entry["id"].as_str())
            .collect();

        validate_cases(&cases).expect("valid case set");
        for (kind, count) in kinds {
            assert_eq!(
                cases.iter().filter(|case| case.kind == *kind).count(),
                *count,
                "{persona} {kind} case count changed"
            );
        }
        assert_eq!(
            cases.len(),
            kinds.iter().map(|(_, count)| count).sum::<usize>()
        );
        for case in &cases {
            for id in &case.relevant_ids {
                assert!(
                    known.contains(id.as_str()),
                    "{} expects unknown id {id}",
                    case.query
                );
                assert!(
                    !low_signal.contains(id.as_str()),
                    "{} expects the low-signal control {id}",
                    case.query
                );
            }
        }
    }

    #[test]
    fn knowledge_worker_fixture_ids_exist_and_case_mix_is_exact() {
        assert_persona_fixture(
            "knowledge-worker",
            20,
            &[
                ("keyword", 14),
                ("paraphrase", 8),
                ("time", 8),
                ("app", 5),
                ("negative", 4),
            ],
        );
    }

    #[test]
    fn office_pm_fixture_ids_exist_and_case_mix_is_exact() {
        assert_persona_fixture(
            "office-pm",
            40,
            &[
                ("keyword", 11),
                ("paraphrase", 9),
                ("time", 8),
                ("app", 5),
                ("negative", 4),
            ],
        );
    }

    #[test]
    fn case_validation_rejects_unknown_kinds_duplicates_and_empty_answers() {
        let case = |query: &str, kind: &str, relevant: &[&str]| Case {
            query: query.to_string(),
            kind: kind.to_string(),
            relevant_ids: ids(relevant),
        };
        assert!(validate_cases(&[case("a", "keyword", &["x"])]).is_ok());
        assert!(validate_cases(&[]).is_err());
        assert!(validate_cases(&[case("a", "vibes", &["x"])]).is_err());
        assert!(validate_cases(&[case("a", "keyword", &[])]).is_err());
        assert!(validate_cases(&[case("a", "time", &[])]).is_err());
        assert!(validate_cases(&[case("a", "negative", &[])]).is_ok());
        assert!(validate_cases(&[case("a", "negative", &["x"])]).is_err());
        assert!(validate_cases(&[case(" ", "keyword", &["x"])]).is_err());
        assert!(validate_cases(&[
            case("a", "keyword", &["x"]),
            case("a", "paraphrase", &["y"])
        ])
        .is_err());
    }

    #[test]
    fn kind_counts_come_from_the_cases_not_constants() {
        let cases = vec![
            Case {
                query: "a".to_string(),
                kind: "keyword".to_string(),
                relevant_ids: ids(&["x"]),
            },
            Case {
                query: "b".to_string(),
                kind: "paraphrase".to_string(),
                relevant_ids: ids(&["y"]),
            },
            Case {
                query: "c".to_string(),
                kind: "paraphrase".to_string(),
                relevant_ids: ids(&["z"]),
            },
        ];
        let counts = kind_counts(&cases);
        assert_eq!(counts["keyword"], 1);
        assert_eq!(counts["paraphrase"], 2);
        assert_eq!(counts.len(), 2);
    }
}
