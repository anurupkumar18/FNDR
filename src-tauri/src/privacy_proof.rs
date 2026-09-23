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
