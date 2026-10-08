//! Privacy proof: evidence that sensitive content never entered storage.
//!
//! Exposes skip-count and egress-request telemetry via IPC so the UI can
//! prove to the user that sensitive content was never captured or stored.
//! Never carries app names, URLs, or other identifying content from skipped frames.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PrivacyProof {
    pub evaluated: u64,
    pub stored: u64,
    pub skipped_by_reason: BTreeMap<String, u64>,
    pub egress_requests: u64,
    pub egress_hosts: Vec<String>,
    /// Recent cloud model requests, newest last (feature, host, bytes). Kept
    /// across restarts, bounded by count and age.
    pub model_requests: Vec<ModelRequest>,
    /// Recent Notch Do runs, newest first, as counts of what became of their actions.
    pub operator_runs: Vec<crate::operator::journal::RunSummary>,
}

/// One request to a cloud model. Carries no content, only its size.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ModelRequest {
    pub at_ms: i64,
    /// `notch_do_plan`, `notch_do_step`, `notch_do_screen_text`, `hermes_chat`,
    /// `screen_guide_answer`.
    pub feature: String,
    pub host: String,
    /// Bytes FNDR put into the request (text and memory snippets it supplied).
    pub bytes_sent: u64,
    /// Kinds of context that went with it: `memories`, `screen_text`, `screenshot`.
    #[serde(default)]
    pub included: Vec<String>,
}

const MAX_MODEL_REQUESTS: usize = 200;
const MAX_MODEL_REQUEST_AGE_MS: i64 = 30 * 24 * 60 * 60 * 1000;
const MODEL_REQUESTS_FILE: &str = "model-requests.json";
static MODEL_REQUESTS: OnceLock<Mutex<Vec<ModelRequest>>> = OnceLock::new();
static MODEL_REQUESTS_PATH: OnceLock<PathBuf> = OnceLock::new();

/// Rows saved by an earlier session that are still inside the age and count bounds.
fn load_rows(path: &Path, now_ms: i64) -> Vec<ModelRequest> {
    let mut rows: Vec<ModelRequest> = std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default();
    rows.retain(|row| now_ms - row.at_ms <= MAX_MODEL_REQUEST_AGE_MS);
    let overflow = rows.len().saturating_sub(MAX_MODEL_REQUESTS);
    rows.drain(..overflow);
    rows
}

fn save_rows(path: &Path, rows: &[ModelRequest]) -> std::io::Result<()> {
    use std::io::Write;
    let tmp = path.with_extension("json.tmp");
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&tmp)?;
    file.write_all(&serde_json::to_vec(rows).unwrap_or_default())?;
    std::fs::rename(&tmp, path)
}

/// Restores the saved log and keeps it on disk from here on. Called once at
/// startup; without it the log lives in memory only (tests, tools).
pub fn init_model_request_log(app_data_dir: &Path) {
    let path = app_data_dir.join(MODEL_REQUESTS_FILE);
    let mut log = model_requests().lock();
    let mut rows = load_rows(&path, chrono::Utc::now().timestamp_millis());
    rows.append(&mut log);
    *log = rows;
    let _ = MODEL_REQUESTS_PATH.set(path);
}

fn model_requests() -> &'static Mutex<Vec<ModelRequest>> {
    MODEL_REQUESTS.get_or_init(|| Mutex::new(Vec::new()))
}

/// Every caller that sends to a cloud model. The name is what Privacy
/// Activity stores and labels, so a new caller is added here, not as a string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Feature {
    NotchDoPlan,
    NotchDoStep,
    NotchDoScreenText,
    HermesChat,
    ScreenGuideAnswer,
}

impl Feature {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotchDoPlan => "notch_do_plan",
            Self::NotchDoStep => "notch_do_step",
            Self::NotchDoScreenText => "notch_do_screen_text",
            Self::HermesChat => "hermes_chat",
            Self::ScreenGuideAnswer => "screen_guide_answer",
        }
    }
}

/// Log one cloud model request: what feature, to which host, how many bytes.
pub fn record_model_request(feature: Feature, host: &str, bytes_sent: usize) {
    record_model_request_including(feature, host, bytes_sent, &[]);
}

/// As [`record_model_request`], naming the kinds of context that went along.
pub fn record_model_request_including(
    feature: Feature,
    host: &str,
    bytes_sent: usize,
    included: &[&str],
) {
    record_egress(host);
    let mut log = model_requests().lock();
    log.push(ModelRequest {
        at_ms: chrono::Utc::now().timestamp_millis(),
        feature: feature.as_str().to_string(),
        host: host.to_string(),
        bytes_sent: bytes_sent as u64,
        included: included.iter().map(|kind| kind.to_string()).collect(),
    });
    let overflow = log.len().saturating_sub(MAX_MODEL_REQUESTS);
    log.drain(..overflow);
    if let Some(path) = MODEL_REQUESTS_PATH.get() {
        // A failed write never fails the request it describes.
        if let Err(error) = save_rows(path, &log) {
            tracing::warn!(%error, "privacy_proof:model_request_log_write_failed");
        }
    }
}

static EGRESS_REQUESTS: AtomicU64 = AtomicU64::new(0);
static EGRESS_HOSTS: OnceLock<Mutex<BTreeSet<String>>> = OnceLock::new();

fn hosts() -> &'static Mutex<BTreeSet<String>> {
    EGRESS_HOSTS.get_or_init(|| Mutex::new(BTreeSet::new()))
}

/// Count one outbound HTTP request. Records the host only, never the path or query.
pub fn record_egress(host: &str) {
    EGRESS_REQUESTS.fetch_add(1, Ordering::Relaxed);
    hosts().lock().insert(host.to_string());
}

pub fn build_privacy_proof(stats: &crate::CapturePipelineStats) -> PrivacyProof {
    PrivacyProof {
        evaluated: stats.evaluated_total(),
        stored: stats.total_stored(),
        skipped_by_reason: stats
            .skip_counts()
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect(),
        egress_requests: EGRESS_REQUESTS.load(Ordering::Relaxed),
        egress_hosts: hosts().lock().iter().cloned().collect(),
        model_requests: model_requests().lock().clone(),
        operator_runs: Vec::new(),
    }
}

#[tauri::command]
pub async fn get_privacy_proof(
    state: tauri::State<'_, std::sync::Arc<crate::AppState>>,
) -> Result<PrivacyProof, String> {
    let mut proof = build_privacy_proof(&state.capture_stats);
    proof.operator_runs = crate::operator::journal::Journal::recent_runs(
        &state.app_data_dir.join("operator").join("journal.jsonl"),
        10,
    );
    Ok(proof)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(at_ms: i64) -> ModelRequest {
        ModelRequest {
            at_ms,
            feature: "screen_guide_answer".to_string(),
            host: "chatgpt.com".to_string(),
            bytes_sent: 10,
            included: vec!["screen_text".to_string(), "screenshot".to_string()],
        }
    }

    #[test]
    fn saved_rows_survive_a_restart_within_the_age_and_count_bounds() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(MODEL_REQUESTS_FILE);
        let now = 100 * MAX_MODEL_REQUEST_AGE_MS;
        let mut rows = vec![row(now - MAX_MODEL_REQUEST_AGE_MS - 1)];
        rows.extend((0..(MAX_MODEL_REQUESTS as i64 + 3)).map(|n| row(now - 1_000 + n)));
        save_rows(&path, &rows).unwrap();

        let restored = load_rows(&path, now);
        assert_eq!(restored.len(), MAX_MODEL_REQUESTS);
        assert_eq!(restored.last(), rows.last());
        assert!(restored
            .iter()
            .all(|r| now - r.at_ms <= MAX_MODEL_REQUEST_AGE_MS));
        assert_eq!(restored[0].included, ["screen_text", "screenshot"]);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }

    #[test]
    fn a_missing_or_unreadable_log_restores_as_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(MODEL_REQUESTS_FILE);
        assert!(load_rows(&path, 0).is_empty());
        std::fs::write(&path, "not json").unwrap();
        assert!(load_rows(&path, 0).is_empty());
    }

    #[test]
    fn rows_written_before_the_included_field_still_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(MODEL_REQUESTS_FILE);
        std::fs::write(
            &path,
            r#"[{"atMs":5,"feature":"hermes_chat","host":"chatgpt.com","bytesSent":9}]"#,
        )
        .unwrap();
        let restored = load_rows(&path, 6);
        assert_eq!(restored.len(), 1);
        assert!(restored[0].included.is_empty());
    }

    #[test]
    fn model_requests_carry_size_and_host_but_no_content_and_stay_bounded() {
        for _ in 0..(MAX_MODEL_REQUESTS + 5) {
            record_model_request(Feature::NotchDoPlan, "chatgpt.com", 1234);
        }
        let proof = build_privacy_proof(&crate::CapturePipelineStats::default());
        assert!(proof.model_requests.len() <= MAX_MODEL_REQUESTS);
        let last = proof.model_requests.last().unwrap();
        assert_eq!(
            (last.feature.as_str(), last.host.as_str(), last.bytes_sent),
            ("notch_do_plan", "chatgpt.com", 1234)
        );
        assert!(proof.egress_hosts.contains(&"chatgpt.com".to_string()));
    }
    use crate::{CapturePipelineStats, SkipReason, StoreOutcome};

    #[test]
    fn proof_reports_skips_by_reason_and_never_carries_content() {
        let stats = CapturePipelineStats::default();
        stats.record_evaluated();
        stats.record_evaluated();
        stats.record_skip(SkipReason::SensitiveContext, "Example Bank");
        stats.record_store(StoreOutcome::OcrPath);
        let proof = build_privacy_proof(&stats);
        assert_eq!(proof.evaluated, 2);
        assert_eq!(proof.stored, 1);
        assert_eq!(proof.skipped_by_reason["sensitive_context"], 1);
        let json = serde_json::to_string(&proof).unwrap();
        assert!(
            !json.contains("Example Bank"),
            "app names must not leak into the proof"
        );
    }

    #[test]
    fn egress_counts_requests_and_keeps_only_hosts() {
        let before = build_privacy_proof(&CapturePipelineStats::default()).egress_requests;
        record_egress("huggingface.co");
        record_egress("huggingface.co");
        let proof = build_privacy_proof(&CapturePipelineStats::default());
        assert!(proof.egress_requests >= before + 2);
        assert!(proof.egress_hosts.contains(&"huggingface.co".to_string()));
    }
}
