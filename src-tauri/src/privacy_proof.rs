//! Privacy proof: evidence that sensitive content never entered storage.
//!
//! Exposes skip-count and egress-request telemetry via IPC so the UI can
//! prove to the user that sensitive content was never captured or stored.
//! Never carries app names, URLs, or other identifying content from skipped frames.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

use parking_lot::Mutex;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PrivacyProof {
    pub evaluated: u64,
    pub stored: u64,
    pub skipped_by_reason: BTreeMap<String, u64>,
    pub egress_requests: u64,
    pub egress_hosts: Vec<String>,
    /// Cloud model requests this session, newest last (feature, host, bytes).
    pub model_requests: Vec<ModelRequest>,
}

/// One request to a cloud model. Carries no content, only its size.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ModelRequest {
    pub at_ms: i64,
    /// `notch_do_plan`, `notch_do_step`, `notch_do_screen_text`, `hermes_chat`.
    pub feature: String,
    pub host: String,
    /// Bytes FNDR put into the request (text and memory snippets it supplied).
    pub bytes_sent: u64,
}

const MAX_MODEL_REQUESTS: usize = 200;
static MODEL_REQUESTS: OnceLock<Mutex<Vec<ModelRequest>>> = OnceLock::new();

fn model_requests() -> &'static Mutex<Vec<ModelRequest>> {
    MODEL_REQUESTS.get_or_init(|| Mutex::new(Vec::new()))
}

/// Log one cloud model request: what feature, to which host, how many bytes.
pub fn record_model_request(feature: &str, host: &str, bytes_sent: usize) {
    record_egress(host);
    let mut log = model_requests().lock();
    log.push(ModelRequest {
        at_ms: chrono::Utc::now().timestamp_millis(),
        feature: feature.to_string(),
        host: host.to_string(),
        bytes_sent: bytes_sent as u64,
    });
    let overflow = log.len().saturating_sub(MAX_MODEL_REQUESTS);
    log.drain(..overflow);
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
    }
}

#[tauri::command]
pub async fn get_privacy_proof(
    state: tauri::State<'_, std::sync::Arc<crate::AppState>>,
) -> Result<PrivacyProof, String> {
    Ok(build_privacy_proof(&state.capture_stats))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_requests_carry_size_and_host_but_no_content_and_stay_bounded() {
        for _ in 0..(MAX_MODEL_REQUESTS + 5) {
            record_model_request("notch_do_plan", "chatgpt.com", 1234);
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
