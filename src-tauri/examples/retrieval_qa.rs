//! Retrieval baseline on a seeded profile, through the two paths the app uses:
//! the Search screen (`search_ranked_results`) and Ask's context-runtime card
//! path (`context_runtime::run_query`). Refuses missing and real profiles.
//! Usage: cargo run --example retrieval_qa -- --data-dir <profile> --cases <json>
//!        [--out <md>] [--json <json>]

use fndr_lib::config::Config;
use fndr_lib::context_runtime::{run_query, ComposeMode};
use fndr_lib::graph::GraphStore;
use fndr_lib::ipc::commands::search::search_ranked_results;
use fndr_lib::storage::{StateStore, Store};
use fndr_lib::AppState;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

const SCHEMA_VERSION: u8 = 1;
const CASE_SET: &str = "knowledge-worker";
const EXPECTED_CASES: usize = 22;
const EXPECTED_KEYWORD_CASES: usize = 14;
const EXPECTED_PARAPHRASE_CASES: usize = 8;
const SEARCH_LIMIT: usize = 20;
const ASK_LIMIT: usize = 10;

#[derive(Debug, Deserialize)]
struct Case {
    query: String,
    kind: String,
    relevant_ids: Vec<String>,
}

#[derive(Debug, Default, Serialize)]
struct KindCounts {
    keyword: usize,
    paraphrase: usize,
}

#[derive(Debug, Default, Serialize)]
struct LatencyMs {
    p50: u128,
    p95: u128,
}

#[derive(Debug, Default, Serialize)]
struct RecallAt5ByKind {
    keyword: f64,
    paraphrase: f64,
}

#[derive(Debug, Default, Serialize)]
struct PathMetrics {
    recall_at_5: f64,
    mrr_at_10: f64,
    latency_ms: LatencyMs,
    recall_at_5_by_kind: RecallAt5ByKind,
}

#[derive(Debug, Serialize)]
struct PathReports {
    search: PathMetrics,
    ask: PathMetrics,
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
}

#[derive(Debug, Serialize)]
struct RetrievalReportV1 {
    schema_version: u8,
    case_set: String,
    case_count: usize,
    case_count_by_kind: KindCounts,
    paths: PathReports,
    top1_agreement: Top1Agreement,
    queries: Vec<QueryReport>,
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
    overall: Score,
    keyword: Score,
    paraphrase: Score,
    latency_ms: Vec<u128>,
}

impl PathScore {
    fn add(&mut self, kind: &str, rank: Option<usize>, latency_ms: u128) {
        self.overall.add(rank);
        match kind {
            "keyword" => self.keyword.add(rank),
            "paraphrase" => self.paraphrase.add(rank),
            _ => {}
        }
        self.latency_ms.push(latency_ms);
    }

    fn metrics(&self) -> PathMetrics {
        PathMetrics {
            recall_at_5: self.overall.recall_at_5(),
            mrr_at_10: self.overall.mrr_at_10(),
            latency_ms: LatencyMs {
                p50: percentile(&self.latency_ms, 50.0),
                p95: percentile(&self.latency_ms, 95.0),
            },
            recall_at_5_by_kind: RecallAt5ByKind {
                keyword: self.keyword.recall_at_5(),
                paraphrase: self.paraphrase.recall_at_5(),
            },
        }
    }
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
    let keyword = cases.iter().filter(|case| case.kind == "keyword").count();
    let paraphrase = cases
        .iter()
        .filter(|case| case.kind == "paraphrase")
        .count();
    if cases.len() != EXPECTED_CASES
        || keyword != EXPECTED_KEYWORD_CASES
        || paraphrase != EXPECTED_PARAPHRASE_CASES
    {
        return Err(format!(
            "expected {EXPECTED_CASES} cases ({EXPECTED_KEYWORD_CASES} keyword, {EXPECTED_PARAPHRASE_CASES} paraphrase); found {} ({keyword} keyword, {paraphrase} paraphrase)",
            cases.len()
        ));
    }
    if let Some(case) = cases
        .iter()
        .find(|case| case.query.trim().is_empty() || case.relevant_ids.is_empty())
    {
        return Err(format!(
            "every case needs a query and relevant_ids; invalid query: {:?}",
            case.query
        ));
    }
    Ok(())
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

fn render_markdown(report: &RetrievalReportV1) -> String {
    let mut lines = vec![
        format!("# Retrieval baseline: {}", report.case_set),
        String::new(),
        format!(
            "Schema v{}; {} queries ({} keyword, {} paraphrase).",
            report.schema_version,
            report.case_count,
            report.case_count_by_kind.keyword,
            report.case_count_by_kind.paraphrase
        ),
        "Search is the pre-card-synthesis ranked retrieval path used by Search; Ask is the context_runtime card path."
            .to_string(),
        "Recall@5 is case-level: a case is recalled when at least one accepted relevant ID appears in its top five results."
            .to_string(),
        String::new(),
        "| Path | Recall@5 | MRR@10 | Keyword Recall@5 | Paraphrase Recall@5 | p50 ms | p95 ms |"
            .to_string(),
        "|---|---:|---:|---:|---:|---:|---:|".to_string(),
        format!(
            "| Search | {:.3} | {:.3} | {:.3} | {:.3} | {} | {} |",
            report.paths.search.recall_at_5,
            report.paths.search.mrr_at_10,
            report.paths.search.recall_at_5_by_kind.keyword,
            report.paths.search.recall_at_5_by_kind.paraphrase,
            report.paths.search.latency_ms.p50,
            report.paths.search.latency_ms.p95
        ),
        format!(
            "| Ask | {:.3} | {:.3} | {:.3} | {:.3} | {} | {} |",
            report.paths.ask.recall_at_5,
            report.paths.ask.mrr_at_10,
            report.paths.ask.recall_at_5_by_kind.keyword,
            report.paths.ask.recall_at_5_by_kind.paraphrase,
            report.paths.ask.latency_ms.p50,
            report.paths.ask.latency_ms.p95
        ),
        String::new(),
        format!(
            "Top-1 agreement: {}/{} ({:.3}).",
            report.top1_agreement.count, report.top1_agreement.total, report.top1_agreement.rate
        ),
        String::new(),
        "| Query | Kind | Search rank@10 | Ask rank@10 |".to_string(),
        "|---|---|---:|---:|".to_string(),
    ];
    lines.extend(report.queries.iter().map(|query| {
        format!(
            "| {} | {} | {} | {} |",
            markdown_cell(&query.query),
            query.kind,
            rank_label(query.search_rank_at_10),
            rank_label(query.ask_rank_at_10)
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
    let state = AppState::new(
        evaluation_dir,
        Config::default(),
        store,
        state_store,
        graph,
        None,
        None,
    );
    let runtime = tokio::runtime::Runtime::new()?;

    let mut search_score = PathScore::default();
    let mut ask_score = PathScore::default();
    let mut top1_agreement = 0usize;
    let mut queries = Vec::with_capacity(cases.len());
    for case in &cases {
        let relevant = case.relevant_ids.iter().cloned().collect::<HashSet<_>>();

        let started = Instant::now();
        let search_ranked = runtime
            .block_on(search_ranked_results(
                &state,
                &case.query,
                None,
                None,
                SEARCH_LIMIT,
            ))?
            .into_iter()
            .map(|result| vec![result.id])
            .collect::<Vec<_>>();
        let search_latency_ms = started.elapsed().as_millis();

        let started = Instant::now();
        let answer = runtime.block_on(run_query(
            &state,
            &case.query,
            ASK_LIMIT,
            ComposeMode::Cards,
        ))?;
        let ask_latency_ms = started.elapsed().as_millis();
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

        let search_rank = first_relevant_rank(&search_ranked, &relevant, 10);
        let ask_rank = first_relevant_rank(&ask_ranked, &relevant, 10);
        search_score.add(&case.kind, search_rank, search_latency_ms);
        ask_score.add(&case.kind, ask_rank, ask_latency_ms);
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
        });
    }

    let case_count = cases.len();
    let report = RetrievalReportV1 {
        schema_version: SCHEMA_VERSION,
        case_set: case_set_name(&cases_path),
        case_count,
        case_count_by_kind: KindCounts {
            keyword: EXPECTED_KEYWORD_CASES,
            paraphrase: EXPECTED_PARAPHRASE_CASES,
        },
        paths: PathReports {
            search: search_score.metrics(),
            ask: ask_score.metrics(),
        },
        top1_agreement: Top1Agreement {
            count: top1_agreement,
            total: case_count,
            rate: top1_agreement as f64 / case_count.max(1) as f64,
        },
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
        score.add("keyword", Some(1), 10);
        score.add("keyword", None, 20);
        score.add("paraphrase", Some(4), 30);
        score.add("paraphrase", Some(7), 40);

        let metrics = score.metrics();
        assert!((metrics.recall_at_5 - 0.5).abs() < 1e-6);
        assert!((metrics.mrr_at_10 - (1.0 + 0.25 + 1.0 / 7.0) / 4.0).abs() < 1e-6);
        assert!((metrics.recall_at_5_by_kind.keyword - 0.5).abs() < 1e-6);
        assert!((metrics.recall_at_5_by_kind.paraphrase - 0.5).abs() < 1e-6);
        assert_eq!(metrics.latency_ms.p50, 30);
        assert_eq!(metrics.latency_ms.p95, 40);
    }

    #[test]
    fn percentile_uses_nearest_rank_and_handles_empty() {
        assert_eq!(percentile(&[], 95.0), 0);
        assert_eq!(percentile(&[10, 20, 30, 40, 50], 50.0), 30);
        assert_eq!(percentile(&[10, 20, 30, 40, 50], 95.0), 50);
    }

    #[test]
    fn schema_v1_serializes_only_the_accepted_portable_fields() {
        let report = RetrievalReportV1 {
            schema_version: 1,
            case_set: "knowledge-worker".to_string(),
            case_count: 1,
            case_count_by_kind: KindCounts {
                keyword: 1,
                paraphrase: 0,
            },
            paths: PathReports {
                search: PathMetrics::default(),
                ask: PathMetrics::default(),
            },
            top1_agreement: Top1Agreement {
                count: 0,
                total: 1,
                rate: 0.0,
            },
            queries: vec![QueryReport {
                query: "missing example".to_string(),
                kind: "keyword".to_string(),
                search_rank_at_10: None,
                ask_rank_at_10: Some(2),
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
            "queries",
        ]
        .into();
        assert_eq!(json_keys(&value), expected);
        assert_eq!(
            json_keys(&value["case_count_by_kind"]),
            ["keyword", "paraphrase"].into()
        );
        assert_eq!(json_keys(&value["paths"]), ["search", "ask"].into());
        assert_eq!(
            json_keys(&value["paths"]["search"]),
            [
                "recall_at_5",
                "mrr_at_10",
                "latency_ms",
                "recall_at_5_by_kind",
            ]
            .into()
        );
        assert_eq!(
            json_keys(&value["paths"]["search"]["latency_ms"]),
            ["p50", "p95"].into()
        );
        assert_eq!(
            json_keys(&value["paths"]["search"]["recall_at_5_by_kind"]),
            ["keyword", "paraphrase"].into()
        );
        assert_eq!(
            json_keys(&value["top1_agreement"]),
            ["count", "total", "rate"].into()
        );
        assert_eq!(
            json_keys(&value["queries"][0]),
            ["query", "kind", "search_rank_at_10", "ask_rank_at_10",].into()
        );
        assert_eq!(value["schema_version"], 1);
        assert_eq!(value["case_set"], "knowledge-worker");
        assert!(value["queries"][0]["search_rank_at_10"].is_null());
        assert_eq!(value["queries"][0]["ask_rank_at_10"], 2);
        assert!(value.get("generated_at").is_none());
        assert!(value.get("cases_path").is_none());
        assert!(value.get("data_dir").is_none());
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
    fn every_expected_id_exists_and_case_mix_is_exact() {
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../scripts/demo/");
        let cases = load_cases(format!("{root}knowledge-worker-queries.json")).expect("cases");
        let corpus: Vec<serde_json::Value> = serde_json::from_slice(
            &std::fs::read(format!("{root}knowledge-worker-week.json")).expect("corpus"),
        )
        .expect("json");
        assert_eq!(corpus.len(), 20, "knowledge-worker corpus size changed");
        let known: HashSet<&str> = corpus
            .iter()
            .filter_map(|entry| entry["id"].as_str())
            .collect();

        validate_cases(&cases).expect("valid case set");
        assert_eq!(cases.len(), 22);
        assert_eq!(
            cases.iter().filter(|case| case.kind == "keyword").count(),
            14
        );
        assert_eq!(
            cases
                .iter()
                .filter(|case| case.kind == "paraphrase")
                .count(),
            8
        );
        for case in &cases {
            for id in &case.relevant_ids {
                assert!(
                    known.contains(id.as_str()),
                    "{} expects unknown id {id}",
                    case.query
                );
            }
        }
    }
}
