//! Debug-build-only evidence for one capture-to-answer journey.
//!
//! This module is compiled out of release builds. Its content-bearing bundles
//! are explicitly armed, bounded, owner-only, and never passed to Store.

use crate::storage::MemoryRecord;
use image::GenericImageView;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
#[cfg(test)]
use std::io::Read;
use std::path::{Path, PathBuf};

pub const MEMORY_JOURNEY_SCHEMA_VERSION: u32 = 1;
pub const DEFAULT_MAX_BUNDLES: usize = 6;
pub const DEFAULT_MAX_AGE_MS: i64 = 24 * 60 * 60 * 1000;
pub const DEFAULT_MAX_TOTAL_BYTES: u64 = 128 * 1024 * 1024;
pub const MEMORY_JOURNEY_HANDOFF_GRACE_MS: i64 = 8_000;
const MANIFEST_FILE: &str = "manifest.json";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MemoryJourneyMode {
    Live,
    Reconstructed,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MemoryJourneyState {
    Armed,
    Capturing,
    Stored,
    Querying,
    Complete,
    Failed,
    Expired,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MemoryJourneyStageStatus {
    Observed,
    Persisted,
    Skipped,
    Failed,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryJourneyArtifactRef {
    pub id: String,
    pub stage: String,
    pub relative_path: String,
    pub sha256: String,
    pub size_bytes: u64,
    pub available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryJourneyStageRecord {
    pub name: String,
    pub status: MemoryJourneyStageStatus,
    pub observed_at_ms: i64,
    pub duration_ms: Option<u64>,
    pub outcome: String,
    #[serde(default)]
    pub details: Value,
    #[serde(default)]
    pub artifact_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MemoryJourneyQueryPath {
    Search,
    Ask,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum MemoryJourneyQueryKind {
    Exact,
    Paraphrase,
    #[default]
    Grounded,
    Unsupported,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryJourneyQueryRun {
    pub id: String,
    pub path: MemoryJourneyQueryPath,
    #[serde(default)]
    pub kind: MemoryJourneyQueryKind,
    pub query: String,
    pub started_at_ms: i64,
    pub duration_ms: u64,
    #[serde(default)]
    pub result_ids: Vec<String>,
    #[serde(default)]
    pub scores: Vec<f32>,
    #[serde(default)]
    pub citation_ids: Vec<String>,
    pub refusal: Option<bool>,
    #[serde(default)]
    pub explanation: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PipelineIntegrityScorecardDraft {
    pub privacy_outcome: Option<String>,
    pub ocr_cer: Option<f32>,
    pub cleanup_preservation: Option<f32>,
    pub extraction_valid: Option<bool>,
    pub vector_contracts_valid: Option<bool>,
    pub storage_integrity_valid: Option<bool>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct HumanUsefulnessScorecardDraft {
    pub should_be_memory: Option<bool>,
    pub understandable: Option<bool>,
    pub facts_preserved: Option<bool>,
    pub recoverable: Option<bool>,
    pub exact_findable: Option<bool>,
    pub paraphrase_findable: Option<bool>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct AgentGroundingScorecardDraft {
    pub required_fact_recall: Option<f32>,
    pub unsupported_fact_rate: Option<f32>,
    pub citation_precision: Option<f32>,
    pub evidence_complete: Option<bool>,
    pub correct_refusal: Option<bool>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryJourneyManifestV1 {
    pub schema_version: u32,
    pub journey_id: String,
    pub label: String,
    pub mode: MemoryJourneyMode,
    pub state: MemoryJourneyState,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    pub memory_id: Option<String>,
    #[serde(default)]
    pub stages: Vec<MemoryJourneyStageRecord>,
    #[serde(default)]
    pub artifacts: Vec<MemoryJourneyArtifactRef>,
    #[serde(default)]
    pub query_runs: Vec<MemoryJourneyQueryRun>,
    #[serde(default)]
    pub pipeline_integrity: PipelineIntegrityScorecardDraft,
    #[serde(default)]
    pub human_usefulness: HumanUsefulnessScorecardDraft,
    #[serde(default)]
    pub agent_grounding: AgentGroundingScorecardDraft,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryJourneySummary {
    pub journey_id: String,
    pub label: String,
    pub mode: MemoryJourneyMode,
    pub state: MemoryJourneyState,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    pub memory_id: Option<String>,
    pub stage_count: usize,
    pub artifact_count: usize,
    pub query_count: usize,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryJourneyStatus {
    pub armed: bool,
    pub arm_ready_at_ms: Option<i64>,
    pub handoff_grace_ms: i64,
    pub active_journey_id: Option<String>,
    pub active_state: Option<MemoryJourneyState>,
    pub journeys: Vec<MemoryJourneySummary>,
    /// Debug-only manifests for the inspector. Artifact contents remain on
    /// disk and are read only through explicit export.
    pub manifests: Vec<MemoryJourneyManifestV1>,
    pub total_bytes: u64,
    pub max_bundles: usize,
    pub max_age_ms: i64,
    pub max_total_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryJourneyExportReceipt {
    pub journey_id: String,
    pub file_name: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArmPhase {
    Inactive,
    HandoffPending,
    Started,
}

#[derive(Debug, Clone)]
struct ArmedJourney {
    id: String,
    label: String,
    created_at_ms: i64,
    ready_at_ms: i64,
}

#[derive(Debug, Default)]
struct RecorderState {
    armed: Option<ArmedJourney>,
    active: Option<MemoryJourneyManifestV1>,
}

#[derive(Debug, Clone, Copy)]
struct Limits {
    max_bundles: usize,
    max_age_ms: i64,
    max_total_bytes: u64,
}

#[derive(Debug)]
pub struct MemoryJourneyRecorder {
    root: PathBuf,
    inner: Mutex<RecorderState>,
    limits: Limits,
}

impl MemoryJourneyRecorder {
    pub fn new(root: PathBuf) -> Self {
        Self::with_limits(
            root,
            DEFAULT_MAX_BUNDLES,
            DEFAULT_MAX_AGE_MS,
            DEFAULT_MAX_TOTAL_BYTES,
        )
    }

    fn with_limits(
        root: PathBuf,
        max_bundles: usize,
        max_age_ms: i64,
        max_total_bytes: u64,
    ) -> Self {
        let recorder = Self {
            root,
            inner: Mutex::new(RecorderState::default()),
            limits: Limits {
                max_bundles,
                max_age_ms,
                max_total_bytes,
            },
        };
        let _ = recorder.cleanup();
        recorder
    }

    /// Where the one-shot arm is: nothing armed, waiting for the person to
    /// bring the target forward, or ready to consume the next attempt.
    pub fn arm_phase(&self) -> ArmPhase {
        self.arm_phase_at(now_ms())
    }

    fn arm_phase_at(&self, now: i64) -> ArmPhase {
        let inner = self.inner.lock();
        match inner.armed.as_ref() {
            Some(armed) if now < armed.ready_at_ms => ArmPhase::HandoffPending,
            Some(_) => ArmPhase::Started,
            None if inner.active.is_some() => ArmPhase::Started,
            None => ArmPhase::Inactive,
        }
    }

    /// True while the capture loop must skip its ordinary tick: frames seen
    /// during the handoff would seed dedupe history with the target itself.
    pub fn defers_ordinary_capture(&self) -> bool {
        self.arm_phase() == ArmPhase::HandoffPending
    }

    pub fn is_inactive(&self) -> bool {
        let inner = self.inner.lock();
        inner.armed.is_none() && inner.active.is_none()
    }

    pub fn arm(&self, label: String) -> Result<String, String> {
        self.cleanup()?;
        let mut inner = self.inner.lock();
        if inner.armed.is_some() || inner.active.is_some() {
            return Err("A Memory Journey is already armed or recording".to_string());
        }
        let id = uuid::Uuid::new_v4().to_string();
        let created_at_ms = now_ms();
        inner.armed = Some(ArmedJourney {
            id: id.clone(),
            label: label.trim().chars().take(80).collect(),
            created_at_ms,
            ready_at_ms: created_at_ms + MEMORY_JOURNEY_HANDOFF_GRACE_MS,
        });
        Ok(id)
    }

    /// Consume the one-shot arm. Call only after the capture loop has decided
    /// to evaluate an attempt, but before any privacy gate can return.
    pub fn begin_capture_attempt(&self, target_app_class: &str) -> Result<Option<String>, String> {
        self.begin_capture_attempt_at(target_app_class, now_ms())
    }

    fn begin_capture_attempt_at(
        &self,
        target_app_class: &str,
        now: i64,
    ) -> Result<Option<String>, String> {
        let mut inner = self.inner.lock();
        let Some(armed) = inner.armed.as_ref() else {
            return Ok(None);
        };
        if now < armed.ready_at_ms {
            return Ok(None);
        }
        let Some(armed) = inner.armed.take() else {
            return Ok(None);
        };
        let manifest = MemoryJourneyManifestV1 {
            schema_version: MEMORY_JOURNEY_SCHEMA_VERSION,
            journey_id: armed.id.clone(),
            label: armed.label,
            mode: MemoryJourneyMode::Live,
            state: MemoryJourneyState::Capturing,
            created_at_ms: armed.created_at_ms,
            updated_at_ms: now,
            memory_id: None,
            stages: vec![MemoryJourneyStageRecord {
                name: "admission".to_string(),
                status: MemoryJourneyStageStatus::Observed,
                observed_at_ms: now,
                duration_ms: None,
                outcome: "attempt_started".to_string(),
                details: json!({ "target_app_class": target_app_class }),
                artifact_ids: Vec::new(),
            }],
            artifacts: Vec::new(),
            query_runs: Vec::new(),
            pipeline_integrity: PipelineIntegrityScorecardDraft::default(),
            human_usefulness: HumanUsefulnessScorecardDraft::default(),
            agent_grounding: AgentGroundingScorecardDraft::default(),
        };
        let dir = self.partial_dir(&armed.id);
        create_private_dir(&dir)?;
        self.write_manifest_to(&dir, &manifest)?;
        inner.active = Some(manifest);
        Ok(Some(armed.id))
    }

    pub fn active_journey_id(&self) -> Option<String> {
        self.inner
            .lock()
            .active
            .as_ref()
            .map(|manifest| manifest.journey_id.clone())
    }

    pub fn active_memory_id(&self) -> Option<String> {
        self.inner
            .lock()
            .active
            .as_ref()
            .and_then(|manifest| manifest.memory_id.clone())
    }

    pub fn record_stage(
        &self,
        journey_id: &str,
        mut stage: MemoryJourneyStageRecord,
    ) -> Result<(), String> {
        let mut inner = self.inner.lock();
        let manifest = active_manifest_mut(&mut inner, journey_id)?;
        if stage.observed_at_ms == 0 {
            stage.observed_at_ms = now_ms();
        }
        manifest.updated_at_ms = now_ms();
        match stage.name.as_str() {
            "admission" => {
                manifest.pipeline_integrity.privacy_outcome = Some(stage.outcome.clone());
            }
            "cleanup" => {
                manifest.pipeline_integrity.cleanup_preservation = stage
                    .details
                    .get("preservation_ratio")
                    .and_then(Value::as_f64)
                    .map(|value| value as f32);
            }
            "extraction" if stage.outcome == "parsed" => {
                manifest.pipeline_integrity.extraction_valid = Some(
                    stage
                        .details
                        .get("validator_result")
                        .and_then(Value::as_str)
                        == Some("ok"),
                );
            }
            _ => {}
        }
        if let Some(existing) = manifest
            .stages
            .iter_mut()
            .find(|item| item.name == stage.name)
        {
            merge_stage_details(&mut stage.details, &existing.details);
            for artifact_id in existing.artifact_ids.iter() {
                if !stage.artifact_ids.contains(artifact_id) {
                    stage.artifact_ids.push(artifact_id.clone());
                }
            }
            *existing = stage;
        } else {
            manifest.stages.push(stage);
        }
        self.write_manifest_to(&self.partial_dir(journey_id), manifest)
    }

    pub fn record_artifact(
        &self,
        journey_id: &str,
        stage: &str,
        file_name: &str,
        bytes: &[u8],
    ) -> Result<MemoryJourneyArtifactRef, String> {
        if file_name.contains('/') || file_name.contains('\\') || file_name.starts_with('.') {
            return Err("Memory Journey artifact name must be a simple relative name".to_string());
        }
        if self.disk_usage()?.saturating_add(bytes.len() as u64) > self.limits.max_total_bytes {
            self.discard_active(journey_id)?;
            return Err(
                "Memory Journey storage cap exceeded; partial journey was removed".to_string(),
            );
        }
        let mut inner = self.inner.lock();
        let manifest = active_manifest_mut(&mut inner, journey_id)?;
        let artifacts_dir = self.partial_dir(journey_id).join("artifacts");
        create_private_dir(&artifacts_dir)?;
        let path = artifacts_dir.join(file_name);
        write_private_atomic(&path, bytes)?;
        let artifact = MemoryJourneyArtifactRef {
            id: uuid::Uuid::new_v4().to_string(),
            stage: stage.to_string(),
            relative_path: format!("artifacts/{file_name}"),
            sha256: sha256_hex(bytes),
            size_bytes: bytes.len() as u64,
            available: true,
        };
        manifest.artifacts.push(artifact.clone());
        if let Some(stage_record) = manifest.stages.iter_mut().find(|item| item.name == stage) {
            stage_record.artifact_ids.push(artifact.id.clone());
        } else {
            manifest.stages.push(MemoryJourneyStageRecord {
                name: stage.to_string(),
                status: MemoryJourneyStageStatus::Observed,
                observed_at_ms: now_ms(),
                duration_ms: None,
                outcome: "artifact_recorded".to_string(),
                details: Value::Null,
                artifact_ids: vec![artifact.id.clone()],
            });
        }
        manifest.updated_at_ms = now_ms();
        self.write_manifest_to(&self.partial_dir(journey_id), manifest)?;
        Ok(artifact)
    }

    pub fn attach_memory_id(&self, journey_id: &str, memory_id: &str) -> Result<(), String> {
        let mut inner = self.inner.lock();
        let manifest = active_manifest_mut(&mut inner, journey_id)?;
        manifest.memory_id = Some(memory_id.to_string());
        manifest.updated_at_ms = now_ms();
        self.write_manifest_to(&self.partial_dir(journey_id), manifest)
    }

    pub fn record_frame(
        &self,
        journey_id: &str,
        png: &[u8],
        duration_ms: u64,
    ) -> Result<(), String> {
        let metrics = match image::load_from_memory(png) {
            Ok(image) => {
                let grayscale = image.to_luma8();
                let pixel_count = grayscale.len().max(1) as f64;
                let mean = grayscale.iter().map(|value| *value as f64).sum::<f64>() / pixel_count;
                let variance = grayscale
                    .iter()
                    .map(|value| {
                        let delta = *value as f64 - mean;
                        delta * delta
                    })
                    .sum::<f64>()
                    / pixel_count;
                json!({
                    "width": image.width(),
                    "height": image.height(),
                    "luma_mean": mean,
                    "luma_stddev": variance.sqrt(),
                    "near_black_ratio": grayscale.iter().filter(|value| **value <= 8).count() as f64 / pixel_count,
                    "near_white_ratio": grayscale.iter().filter(|value| **value >= 247).count() as f64 / pixel_count,
                    "pixel_sha256": sha256_hex(png),
                })
            }
            Err(error) => json!({
                "decode_error": error.to_string(),
                "pixel_sha256": sha256_hex(png),
            }),
        };
        self.record_stage(
            journey_id,
            MemoryJourneyStageRecord {
                name: "frame".to_string(),
                status: MemoryJourneyStageStatus::Observed,
                observed_at_ms: now_ms(),
                duration_ms: Some(duration_ms),
                outcome: "captured".to_string(),
                details: metrics,
                artifact_ids: Vec::new(),
            },
        )?;
        self.record_artifact(journey_id, "frame", "capture.png", png)?;
        Ok(())
    }

    pub fn record_vector_contracts(
        &self,
        journey_id: &str,
        model: &str,
        primary: (&[f32], &str),
        snippet: (&[f32], &str),
        support: (&[f32], &str),
        image: (&[f32], &str),
        duration_ms: u64,
    ) -> Result<(), String> {
        let primary_contract = vector_contract("primary", model, "text", primary.0, primary.1);
        let snippet_contract = vector_contract("snippet", model, "text", snippet.0, snippet.1);
        let support_contract = vector_contract("support", model, "text", support.0, support.1);
        let image_contract = vector_contract("image", "clip", "image", image.0, image.1);
        let contracts_valid = [
            &primary_contract,
            &snippet_contract,
            &support_contract,
            &image_contract,
        ]
        .iter()
        .all(|contract| contract.get("valid").and_then(Value::as_bool) == Some(true));
        let similarities = json!({
            "primary_snippet": cosine_if_same_dimension(primary.0, snippet.0),
            "primary_support": cosine_if_same_dimension(primary.0, support.0),
            "snippet_support": cosine_if_same_dimension(snippet.0, support.0),
            "text_image": Value::Null,
        });
        self.record_stage(
            journey_id,
            MemoryJourneyStageRecord {
                name: "vector_contracts".to_string(),
                status: MemoryJourneyStageStatus::Observed,
                observed_at_ms: now_ms(),
                duration_ms: Some(duration_ms),
                outcome: "checked".to_string(),
                details: json!({
                    "primary": primary_contract,
                    "snippet": snippet_contract,
                    "support": support_contract,
                    "image": image_contract,
                    "pairwise_similarity": similarities,
                }),
                artifact_ids: Vec::new(),
            },
        )?;
        let mut inner = self.inner.lock();
        let manifest = active_manifest_mut(&mut inner, journey_id)?;
        manifest.pipeline_integrity.vector_contracts_valid = Some(contracts_valid);
        self.write_manifest_to(&self.partial_dir(journey_id), manifest)
    }

    pub fn record_llm_trace(
        &self,
        journey_id: &str,
        trace: &crate::telemetry::llm_trace::LlmTrace,
    ) -> Result<(), String> {
        let bytes = serde_json::to_vec_pretty(trace).map_err(|error| error.to_string())?;
        let file_name = format!("llm-{}.json", uuid::Uuid::new_v4());
        self.record_artifact(journey_id, "extraction", &file_name, &bytes)?;
        let mut inner = self.inner.lock();
        let manifest = active_manifest_mut(&mut inner, journey_id)?;
        if let Some(stage) = manifest
            .stages
            .iter_mut()
            .find(|stage| stage.name == "extraction")
        {
            stage.details = json!({
                "model": trace.model_id,
                "task": trace.task,
                "prompt_version": trace.prompt_version,
                "prompt_tokens": trace.prompt_tokens,
                "output_tokens": trace.output_tokens,
                "latency_ms": trace.latency_ms,
                "validator": trace.validator,
            });
        }
        manifest.updated_at_ms = now_ms();
        self.write_manifest_to(&self.partial_dir(journey_id), manifest)
    }

    pub fn complete_storage(
        &self,
        record: &MemoryRecord,
        persisted: bool,
        chunk_count: usize,
        duration_ms: u64,
    ) -> Result<Option<MemoryJourneyManifestV1>, String> {
        let memory_id = record.id.as_str();
        let journey_id = {
            let inner = self.inner.lock();
            inner.active.as_ref().and_then(|manifest| {
                (manifest.memory_id.as_deref() == Some(memory_id))
                    .then(|| manifest.journey_id.clone())
            })
        };
        let Some(journey_id) = journey_id else {
            return Ok(None);
        };
        self.record_stage(
            &journey_id,
            MemoryJourneyStageRecord {
                name: "storage".to_string(),
                status: if persisted {
                    MemoryJourneyStageStatus::Observed
                } else {
                    MemoryJourneyStageStatus::Failed
                },
                observed_at_ms: now_ms(),
                duration_ms: Some(duration_ms),
                outcome: if persisted {
                    if record.storage_outcome.is_empty() {
                        "inserted_or_updated"
                    } else {
                        record.storage_outcome.as_str()
                    }
                } else {
                    "persistence_verification_failed"
                }
                .to_string(),
                details: json!({
                    "memory_id": memory_id,
                    "table": "memories",
                    "persisted": persisted,
                    "schema_version": record.schema_version,
                    "chunk_count": chunk_count,
                    "review_status": record.enrichment_status,
                    "embedding_manifest": crate::memory_embedding_document::read_embedding_manifest(&record.raw_evidence),
                }),
                artifact_ids: Vec::new(),
            },
        )?;
        {
            let mut inner = self.inner.lock();
            let manifest = active_manifest_mut(&mut inner, &journey_id)?;
            manifest.pipeline_integrity.storage_integrity_valid = Some(persisted);
            self.write_manifest_to(&self.partial_dir(&journey_id), manifest)?;
        }
        let manifest = self.finish_active(
            &journey_id,
            if persisted {
                MemoryJourneyState::Complete
            } else {
                MemoryJourneyState::Failed
            },
        )?;
        Ok(Some(manifest))
    }

    pub fn fail_active_storage(
        &self,
        outcome: &str,
        duration_ms: u64,
    ) -> Result<Option<MemoryJourneyManifestV1>, String> {
        let journey_id = self.active_journey_id();
        let Some(journey_id) = journey_id else {
            return Ok(None);
        };
        self.record_stage(
            &journey_id,
            MemoryJourneyStageRecord {
                name: "storage".to_string(),
                status: MemoryJourneyStageStatus::Failed,
                observed_at_ms: now_ms(),
                duration_ms: Some(duration_ms),
                outcome: outcome.to_string(),
                details: json!({ "table": "memories", "inserted": false }),
                artifact_ids: Vec::new(),
            },
        )?;
        self.finish_active(&journey_id, MemoryJourneyState::Failed)
            .map(Some)
    }

    pub fn finish_active(
        &self,
        journey_id: &str,
        state: MemoryJourneyState,
    ) -> Result<MemoryJourneyManifestV1, String> {
        if !matches!(
            state,
            MemoryJourneyState::Complete | MemoryJourneyState::Failed
        ) {
            return Err("A finished journey must be complete or failed".to_string());
        }
        let mut inner = self.inner.lock();
        let mut manifest = inner
            .active
            .take()
            .ok_or_else(|| "No active Memory Journey".to_string())?;
        if manifest.journey_id != journey_id {
            inner.active = Some(manifest);
            return Err(
                "Memory Journey identifier does not match the active recording".to_string(),
            );
        }
        manifest.state = state;
        manifest.updated_at_ms = now_ms();
        let partial = self.partial_dir(journey_id);
        self.write_manifest_to(&partial, &manifest)?;
        let complete = self.complete_dir(journey_id);
        if complete.exists() {
            return Err("Memory Journey bundle already exists".to_string());
        }
        fs::rename(&partial, &complete).map_err(io_error("publish Memory Journey bundle"))?;
        drop(inner);
        self.cleanup()?;
        Ok(manifest)
    }

    pub fn finish_skipped(
        &self,
        journey_id: &str,
        outcome: &str,
        privacy_blocked: bool,
    ) -> Result<MemoryJourneyManifestV1, String> {
        self.finish_skipped_with(
            journey_id,
            outcome,
            json!({ "privacy_blocked": privacy_blocked }),
        )
    }

    /// Like `finish_skipped`, with caller-supplied details (for example the
    /// dedupe evidence behind a `perceptual_duplicate`).
    pub fn finish_skipped_with(
        &self,
        journey_id: &str,
        outcome: &str,
        details: serde_json::Value,
    ) -> Result<MemoryJourneyManifestV1, String> {
        self.record_stage(
            journey_id,
            MemoryJourneyStageRecord {
                name: "admission".to_string(),
                status: MemoryJourneyStageStatus::Skipped,
                observed_at_ms: now_ms(),
                duration_ms: None,
                outcome: outcome.to_string(),
                details,
                artifact_ids: Vec::new(),
            },
        )?;
        self.finish_active(journey_id, MemoryJourneyState::Failed)
    }

    /// A transient-content privacy gate may reject after frame/OCR observation.
    /// Remove every content artifact and publish only the aggregate admission
    /// outcome so the debug exception never overrides the privacy decision.
    pub fn redact_and_finish_privacy_skip(
        &self,
        journey_id: &str,
        outcome: &str,
    ) -> Result<MemoryJourneyManifestV1, String> {
        let mut inner = self.inner.lock();
        let manifest = active_manifest_mut(&mut inner, journey_id)?;
        let partial = self.partial_dir(journey_id);
        let artifacts = partial.join("artifacts");
        if artifacts.exists() {
            fs::remove_dir_all(&artifacts).map_err(io_error("redact private journey artifacts"))?;
        }
        manifest.artifacts.clear();
        manifest.stages.clear();
        manifest.stages.push(MemoryJourneyStageRecord {
            name: "admission".to_string(),
            status: MemoryJourneyStageStatus::Skipped,
            observed_at_ms: now_ms(),
            duration_ms: None,
            outcome: outcome.to_string(),
            details: json!({ "privacy_blocked": true, "content_artifacts_saved": false }),
            artifact_ids: Vec::new(),
        });
        manifest.updated_at_ms = now_ms();
        self.write_manifest_to(&partial, manifest)?;
        drop(inner);
        self.finish_active(journey_id, MemoryJourneyState::Failed)
    }

    pub fn discard_active(&self, journey_id: &str) -> Result<(), String> {
        let mut inner = self.inner.lock();
        if inner.active.as_ref().map(|m| m.journey_id.as_str()) == Some(journey_id) {
            inner.active = None;
        }
        let partial = self.partial_dir(journey_id);
        if partial.exists() {
            fs::remove_dir_all(&partial).map_err(io_error("remove partial Memory Journey"))?;
        }
        Ok(())
    }

    pub fn create_reconstructed(
        &self,
        record: &MemoryRecord,
    ) -> Result<MemoryJourneyManifestV1, String> {
        self.cleanup()?;
        let id = uuid::Uuid::new_v4().to_string();
        let now = now_ms();
        let unavailable = |name: &str| MemoryJourneyStageRecord {
            name: name.to_string(),
            status: MemoryJourneyStageStatus::Unavailable,
            observed_at_ms: now,
            duration_ms: None,
            outcome: "not_persisted".to_string(),
            details: Value::Null,
            artifact_ids: Vec::new(),
        };
        let vector_details = json!({
            "text": vector_contract("primary", &record.embedding_model, "text", &record.embedding, &record.embedding_text),
            "snippet": vector_contract("snippet", &record.embedding_model, "text", &record.snippet_embedding, &record.snippet),
            "support": vector_contract("support", &record.embedding_model, "text", &record.support_embedding, &record.memory_context),
            "image": vector_contract("image", "clip", "image", &record.image_embedding, "persisted-image-source-unavailable"),
        });
        let mut manifest = MemoryJourneyManifestV1 {
            schema_version: MEMORY_JOURNEY_SCHEMA_VERSION,
            journey_id: id.clone(),
            label: format!("Reconstructed {}", record.id),
            mode: MemoryJourneyMode::Reconstructed,
            state: MemoryJourneyState::Complete,
            created_at_ms: now,
            updated_at_ms: now,
            memory_id: Some(record.id.clone()),
            stages: vec![
                unavailable("admission"),
                unavailable("frame"),
                unavailable("text_source"),
                MemoryJourneyStageRecord {
                    name: "ocr".into(),
                    status: MemoryJourneyStageStatus::Persisted,
                    observed_at_ms: now,
                    duration_ms: None,
                    outcome: "persisted_fields_only".into(),
                    details: json!({"confidence": record.ocr_confidence, "block_count": record.ocr_block_count, "raw_text_available": !record.text.is_empty()}),
                    artifact_ids: vec![],
                },
                MemoryJourneyStageRecord {
                    name: "cleanup".into(),
                    status: MemoryJourneyStageStatus::Persisted,
                    observed_at_ms: now,
                    duration_ms: None,
                    outcome: "persisted_fields_only".into(),
                    details: json!({"raw_chars": record.text.chars().count(), "clean_chars": record.clean_text.chars().count()}),
                    artifact_ids: vec![],
                },
                MemoryJourneyStageRecord {
                    name: "extraction".into(),
                    status: MemoryJourneyStageStatus::Persisted,
                    observed_at_ms: now,
                    duration_ms: None,
                    outcome: record.summary_source.clone(),
                    details: json!({"summary_source": record.summary_source, "synthesis_branch": record.synthesis_branch, "confidence": record.extraction_confidence, "raw_evidence_available": !record.raw_evidence.is_empty()}),
                    artifact_ids: vec![],
                },
                MemoryJourneyStageRecord {
                    name: "embedding_document".into(),
                    status: MemoryJourneyStageStatus::Persisted,
                    observed_at_ms: now,
                    duration_ms: None,
                    outcome: "persisted_manifest_only".into(),
                    details: json!({"embedding_model": record.embedding_model, "embedding_dim": record.embedding_dim, "embedding_text_chars": record.embedding_text.chars().count()}),
                    artifact_ids: vec![],
                },
                MemoryJourneyStageRecord {
                    name: "vector_contracts".into(),
                    status: MemoryJourneyStageStatus::Persisted,
                    observed_at_ms: now,
                    duration_ms: None,
                    outcome: "checked".into(),
                    details: vector_details,
                    artifact_ids: vec![],
                },
                MemoryJourneyStageRecord {
                    name: "storage".into(),
                    status: MemoryJourneyStageStatus::Persisted,
                    observed_at_ms: now,
                    duration_ms: None,
                    outcome: "existing_record".into(),
                    details: json!({"memory_id": record.id, "schema_version": record.schema_version, "review_status": record.enrichment_status, "source_type": record.source_type}),
                    artifact_ids: vec![],
                },
                unavailable("retrieval"),
                MemoryJourneyStageRecord {
                    name: "presentation".into(),
                    status: MemoryJourneyStageStatus::Persisted,
                    observed_at_ms: now,
                    duration_ms: None,
                    outcome: "persisted_card_fields".into(),
                    details: json!({"title_source": record.window_title, "summary_source": record.summary_source}),
                    artifact_ids: vec![],
                },
            ],
            artifacts: Vec::new(),
            query_runs: Vec::new(),
            pipeline_integrity: PipelineIntegrityScorecardDraft::default(),
            human_usefulness: HumanUsefulnessScorecardDraft::default(),
            agent_grounding: AgentGroundingScorecardDraft::default(),
        };
        let partial = self.partial_dir(&id);
        create_private_dir(&partial)?;
        let evidence = json!({
            "memory_id": record.id,
            "text": record.text,
            "clean_text": record.clean_text,
            "snippet": record.snippet,
            "display_summary": record.display_summary,
            "memory_context": record.memory_context,
            "topic": record.topic,
            "workflow": record.workflow,
            "user_intent": record.user_intent,
            "raw_evidence": record.raw_evidence,
            "embedding_text": record.embedding_text,
        });
        let bytes = serde_json::to_vec_pretty(&evidence).map_err(|e| e.to_string())?;
        let artifact_path = partial.join("artifacts");
        create_private_dir(&artifact_path)?;
        write_private_atomic(&artifact_path.join("persisted-memory.json"), &bytes)?;
        let artifact = MemoryJourneyArtifactRef {
            id: uuid::Uuid::new_v4().to_string(),
            stage: "storage".to_string(),
            relative_path: "artifacts/persisted-memory.json".to_string(),
            sha256: sha256_hex(&bytes),
            size_bytes: bytes.len() as u64,
            available: true,
        };
        if let Some(storage) = manifest
            .stages
            .iter_mut()
            .find(|stage| stage.name == "storage")
        {
            storage.artifact_ids.push(artifact.id.clone());
        }
        manifest.artifacts.push(artifact);
        self.write_manifest_to(&partial, &manifest)?;
        fs::rename(partial, self.complete_dir(&id))
            .map_err(io_error("publish reconstructed journey"))?;
        self.cleanup()?;
        Ok(manifest)
    }

    pub fn append_query_run(
        &self,
        journey_id: &str,
        run: MemoryJourneyQueryRun,
    ) -> Result<MemoryJourneyManifestV1, String> {
        let dir = self.complete_dir(journey_id);
        let mut manifest = self.load_manifest(journey_id)?;
        manifest.state = MemoryJourneyState::Querying;
        manifest.updated_at_ms = now_ms();
        self.write_manifest_to(&dir, &manifest)?;
        upsert_manifest_stage(
            &mut manifest,
            MemoryJourneyStageRecord {
                name: "retrieval".to_string(),
                status: MemoryJourneyStageStatus::Observed,
                observed_at_ms: run.started_at_ms,
                duration_ms: Some(run.duration_ms),
                outcome: match run.path {
                    MemoryJourneyQueryPath::Search => "production_search",
                    MemoryJourneyQueryPath::Ask => "context_runtime",
                }
                .to_string(),
                details: run
                    .explanation
                    .get("retrieval")
                    .cloned()
                    .unwrap_or(Value::Null),
                artifact_ids: Vec::new(),
            },
        );
        let found_selected_memory = manifest
            .memory_id
            .as_ref()
            .is_some_and(|memory_id| run.result_ids.contains(memory_id));
        match run.kind {
            MemoryJourneyQueryKind::Exact => {
                manifest.human_usefulness.exact_findable = Some(found_selected_memory);
            }
            MemoryJourneyQueryKind::Paraphrase => {
                manifest.human_usefulness.paraphrase_findable = Some(found_selected_memory);
            }
            MemoryJourneyQueryKind::Unsupported => {
                manifest.agent_grounding.correct_refusal = run.refusal;
            }
            MemoryJourneyQueryKind::Grounded => {
                manifest.agent_grounding.evidence_complete = manifest
                    .memory_id
                    .as_ref()
                    .map(|memory_id| run.citation_ids.contains(memory_id));
            }
        }
        upsert_manifest_stage(
            &mut manifest,
            MemoryJourneyStageRecord {
                name: "presentation".to_string(),
                status: MemoryJourneyStageStatus::Observed,
                observed_at_ms: run.started_at_ms.saturating_add(run.duration_ms as i64),
                duration_ms: Some(run.duration_ms),
                outcome: if run.refusal == Some(true) {
                    "refused"
                } else {
                    "presented"
                }
                .to_string(),
                details: run
                    .explanation
                    .get("presentation")
                    .cloned()
                    .unwrap_or(Value::Null),
                artifact_ids: Vec::new(),
            },
        );
        manifest.query_runs.push(run);
        manifest.state = MemoryJourneyState::Complete;
        manifest.updated_at_ms = now_ms();
        self.write_manifest_to(&dir, &manifest)?;
        Ok(manifest)
    }

    pub fn load_manifest(&self, journey_id: &str) -> Result<MemoryJourneyManifestV1, String> {
        validate_id(journey_id)?;
        let bytes = fs::read(self.complete_dir(journey_id).join(MANIFEST_FILE))
            .map_err(io_error("read Memory Journey manifest"))?;
        serde_json::from_slice(&bytes).map_err(|e| format!("Invalid Memory Journey manifest: {e}"))
    }

    pub fn status(&self) -> Result<MemoryJourneyStatus, String> {
        self.cleanup()?;
        let inner = self.inner.lock();
        let armed = inner.armed.is_some();
        let arm_ready_at_ms = inner.armed.as_ref().map(|item| item.ready_at_ms);
        let active_journey_id = inner
            .active
            .as_ref()
            .map(|item| item.journey_id.clone())
            .or_else(|| inner.armed.as_ref().map(|item| item.id.clone()));
        let active_state = inner
            .active
            .as_ref()
            .map(|item| item.state)
            .or_else(|| inner.armed.as_ref().map(|_| MemoryJourneyState::Armed));
        let active_manifest = inner.active.clone();
        drop(inner);
        let mut journeys = self.list_summaries()?;
        journeys.sort_by(|a, b| b.updated_at_ms.cmp(&a.updated_at_ms));
        let mut manifests = journeys
            .iter()
            .filter_map(|summary| self.load_manifest(&summary.journey_id).ok())
            .collect::<Vec<_>>();
        if let Some(active_manifest) = active_manifest {
            manifests.insert(0, active_manifest);
        }
        Ok(MemoryJourneyStatus {
            armed,
            arm_ready_at_ms,
            handoff_grace_ms: MEMORY_JOURNEY_HANDOFF_GRACE_MS,
            active_journey_id,
            active_state,
            journeys,
            manifests,
            total_bytes: directory_size(&self.root)?,
            max_bundles: self.limits.max_bundles,
            max_age_ms: self.limits.max_age_ms,
            max_total_bytes: self.limits.max_total_bytes,
        })
    }

    pub fn export_to(
        &self,
        journey_id: &str,
        destination: &Path,
    ) -> Result<MemoryJourneyExportReceipt, String> {
        validate_id(journey_id)?;
        let source = self.complete_dir(journey_id);
        if !source.is_dir() {
            return Err("Memory Journey bundle was not found".to_string());
        }
        let file = File::create(destination).map_err(io_error("create Memory Journey export"))?;
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        let mut files = collect_files(&source)?;
        files.sort();
        for path in files {
            let relative = path.strip_prefix(&source).map_err(|e| e.to_string())?;
            let name = relative.to_string_lossy().replace('\\', "/");
            zip.start_file(name, options)
                .map_err(|e| format!("Start export entry failed: {e}"))?;
            let mut input = File::open(&path).map_err(io_error("read Memory Journey artifact"))?;
            std::io::copy(&mut input, &mut zip).map_err(io_error("write Memory Journey export"))?;
        }
        zip.finish()
            .map_err(|e| format!("Finish Memory Journey export failed: {e}"))?;
        set_private_file_permissions(destination)?;
        let size_bytes = fs::metadata(destination)
            .map_err(io_error("inspect Memory Journey export"))?
            .len();
        Ok(MemoryJourneyExportReceipt {
            journey_id: journey_id.to_string(),
            file_name: destination
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("memory-journey.fndrjourney.zip")
                .to_string(),
            size_bytes,
        })
    }

    pub fn delete(&self, journey_id: &str) -> Result<(), String> {
        validate_id(journey_id)?;
        let mut inner = self.inner.lock();
        if inner.armed.as_ref().map(|armed| armed.id.as_str()) == Some(journey_id) {
            inner.armed = None;
        }
        if inner
            .active
            .as_ref()
            .map(|manifest| manifest.journey_id.as_str())
            == Some(journey_id)
        {
            inner.active = None;
        }
        for path in [self.partial_dir(journey_id), self.complete_dir(journey_id)] {
            if path.exists() {
                fs::remove_dir_all(path).map_err(io_error("delete Memory Journey"))?;
            }
        }
        Ok(())
    }

    pub fn delete_all(&self) -> Result<(), String> {
        let mut inner = self.inner.lock();
        inner.armed = None;
        inner.active = None;
        if self.root.exists() {
            fs::remove_dir_all(&self.root).map_err(io_error("delete Memory Journeys"))?;
        }
        Ok(())
    }

    pub fn cleanup(&self) -> Result<(), String> {
        if !self.root.exists() {
            return Ok(());
        }
        let active_id = self
            .inner
            .lock()
            .active
            .as_ref()
            .map(|item| item.journey_id.clone());
        let now = now_ms();
        let mut complete = Vec::new();
        for entry in fs::read_dir(&self.root).map_err(io_error("list Memory Journeys"))? {
            let entry = entry.map_err(io_error("read Memory Journey entry"))?;
            if !entry
                .file_type()
                .map_err(io_error("inspect Memory Journey entry"))?
                .is_dir()
            {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if name.ends_with(".partial") {
                let id = name.trim_end_matches(".partial");
                if active_id.as_deref() != Some(id) {
                    fs::remove_dir_all(entry.path())
                        .map_err(io_error("remove abandoned journey"))?;
                }
                continue;
            }
            let manifest = match read_manifest_path(&entry.path().join(MANIFEST_FILE)) {
                Ok(manifest) => manifest,
                Err(_) => {
                    fs::remove_dir_all(entry.path()).map_err(io_error("remove invalid journey"))?;
                    continue;
                }
            };
            if now.saturating_sub(manifest.updated_at_ms) > self.limits.max_age_ms {
                fs::remove_dir_all(entry.path()).map_err(io_error("expire Memory Journey"))?;
            } else {
                complete.push((manifest.updated_at_ms, entry.path()));
            }
        }
        complete.sort_by_key(|(updated, _)| *updated);
        while complete.len() > self.limits.max_bundles {
            let (_, path) = complete.remove(0);
            fs::remove_dir_all(path).map_err(io_error("enforce Memory Journey retention"))?;
        }
        while directory_size(&self.root)? > self.limits.max_total_bytes && !complete.is_empty() {
            let (_, path) = complete.remove(0);
            fs::remove_dir_all(path).map_err(io_error("enforce Memory Journey size cap"))?;
        }
        Ok(())
    }

    fn list_summaries(&self) -> Result<Vec<MemoryJourneySummary>, String> {
        if !self.root.exists() {
            return Ok(Vec::new());
        }
        let mut summaries = Vec::new();
        for entry in fs::read_dir(&self.root).map_err(io_error("list Memory Journeys"))? {
            let entry = entry.map_err(io_error("read Memory Journey entry"))?;
            if !entry
                .file_type()
                .map_err(io_error("inspect Memory Journey entry"))?
                .is_dir()
                || entry.file_name().to_string_lossy().ends_with(".partial")
            {
                continue;
            }
            if let Ok(manifest) = read_manifest_path(&entry.path().join(MANIFEST_FILE)) {
                summaries.push(MemoryJourneySummary {
                    journey_id: manifest.journey_id,
                    label: manifest.label,
                    mode: manifest.mode,
                    state: manifest.state,
                    created_at_ms: manifest.created_at_ms,
                    updated_at_ms: manifest.updated_at_ms,
                    memory_id: manifest.memory_id,
                    stage_count: manifest.stages.len(),
                    artifact_count: manifest.artifacts.len(),
                    query_count: manifest.query_runs.len(),
                    size_bytes: directory_size(&entry.path())?,
                });
            }
        }
        Ok(summaries)
    }

    fn write_manifest_to(
        &self,
        dir: &Path,
        manifest: &MemoryJourneyManifestV1,
    ) -> Result<(), String> {
        let bytes = serde_json::to_vec_pretty(manifest).map_err(|e| e.to_string())?;
        write_private_atomic(&dir.join(MANIFEST_FILE), &bytes)
    }

    fn partial_dir(&self, id: &str) -> PathBuf {
        self.root.join(format!("{id}.partial"))
    }

    fn complete_dir(&self, id: &str) -> PathBuf {
        self.root.join(id)
    }

    fn disk_usage(&self) -> Result<u64, String> {
        directory_size(&self.root)
    }
}

fn upsert_manifest_stage(manifest: &mut MemoryJourneyManifestV1, stage: MemoryJourneyStageRecord) {
    if let Some(existing) = manifest
        .stages
        .iter_mut()
        .find(|item| item.name == stage.name)
    {
        *existing = stage;
    } else {
        manifest.stages.push(stage);
    }
}

fn merge_stage_details(target: &mut Value, prior: &Value) {
    let (Some(target), Some(prior)) = (target.as_object_mut(), prior.as_object()) else {
        return;
    };
    for (key, value) in prior {
        target.entry(key.clone()).or_insert_with(|| value.clone());
    }
}

fn active_manifest_mut<'a>(
    inner: &'a mut RecorderState,
    journey_id: &str,
) -> Result<&'a mut MemoryJourneyManifestV1, String> {
    let manifest = inner
        .active
        .as_mut()
        .ok_or_else(|| "No active Memory Journey".to_string())?;
    if manifest.journey_id != journey_id {
        return Err("Memory Journey identifier does not match the active recording".to_string());
    }
    Ok(manifest)
}

fn vector_contract(role: &str, model: &str, space: &str, vector: &[f32], source: &str) -> Value {
    let expected_dimension = if space == "image" {
        crate::config::DEFAULT_IMAGE_EMBEDDING_DIM
    } else if model.to_ascii_lowercase().contains("bge") {
        crate::inference::model_config::BGE_V5_DIMENSIONS
    } else {
        crate::config::DEFAULT_TEXT_EMBEDDING_DIM
    };
    let non_finite_count = vector.iter().filter(|value| !value.is_finite()).count();
    let zero_count = vector.iter().filter(|value| **value == 0.0).count();
    let norm = vector
        .iter()
        .filter(|value| value.is_finite())
        .map(|value| value * value)
        .sum::<f32>()
        .sqrt();
    let mut bytes = Vec::with_capacity(vector.len() * 4);
    for value in vector {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    json!({
        "role": role,
        "model": model,
        "vector_space": space,
        "dimension": vector.len(),
        "expected_dimension": expected_dimension,
        "dimension_matches": vector.len() == expected_dimension,
        "source_sha256": sha256_hex(source.as_bytes()),
        "norm": norm,
        "zero_count": zero_count,
        "non_finite_count": non_finite_count,
        "vector_sha256": sha256_hex(&bytes),
        "valid": vector.len() == expected_dimension && non_finite_count == 0 && zero_count < vector.len(),
    })
}

fn cosine_if_same_dimension(left: &[f32], right: &[f32]) -> Value {
    if left.is_empty() || left.len() != right.len() {
        return Value::Null;
    }
    let mut dot = 0.0f64;
    let mut left_norm = 0.0f64;
    let mut right_norm = 0.0f64;
    for (left, right) in left.iter().zip(right.iter()) {
        if !left.is_finite() || !right.is_finite() {
            return Value::Null;
        }
        dot += *left as f64 * *right as f64;
        left_norm += (*left as f64).powi(2);
        right_norm += (*right as f64).powi(2);
    }
    if left_norm == 0.0 || right_norm == 0.0 {
        Value::Null
    } else {
        json!(dot / (left_norm.sqrt() * right_norm.sqrt()))
    }
}

fn validate_id(id: &str) -> Result<(), String> {
    uuid::Uuid::parse_str(id)
        .map(|_| ())
        .map_err(|_| "Invalid Memory Journey identifier".to_string())
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn create_private_dir(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path).map_err(io_error("create Memory Journey directory"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(io_error("secure Memory Journey directory"))?;
    }
    Ok(())
}

fn set_private_file_permissions(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .map_err(io_error("secure Memory Journey file"))?;
    }
    Ok(())
}

fn write_private_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        create_private_dir(parent)?;
    }
    let temp = path.with_extension("tmp");
    fs::write(&temp, bytes).map_err(io_error("write Memory Journey temporary file"))?;
    set_private_file_permissions(&temp)?;
    fs::rename(&temp, path).map_err(io_error("publish Memory Journey file"))?;
    set_private_file_permissions(path)
}

fn read_manifest_path(path: &Path) -> Result<MemoryJourneyManifestV1, String> {
    let bytes = fs::read(path).map_err(io_error("read Memory Journey manifest"))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("Invalid Memory Journey manifest: {e}"))
}

fn directory_size(path: &Path) -> Result<u64, String> {
    if !path.exists() {
        return Ok(0);
    }
    let metadata = fs::metadata(path).map_err(io_error("inspect Memory Journey path"))?;
    if metadata.is_file() {
        return Ok(metadata.len());
    }
    let mut total = 0u64;
    for entry in fs::read_dir(path).map_err(io_error("measure Memory Journey directory"))? {
        let entry = entry.map_err(io_error("read Memory Journey directory"))?;
        total = total.saturating_add(directory_size(&entry.path())?);
    }
    Ok(total)
}

fn collect_files(path: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    for entry in fs::read_dir(path).map_err(io_error("list Memory Journey export"))? {
        let entry = entry.map_err(io_error("read Memory Journey export entry"))?;
        if entry
            .file_type()
            .map_err(io_error("inspect Memory Journey export entry"))?
            .is_dir()
        {
            files.extend(collect_files(&entry.path())?);
        } else {
            files.push(entry.path());
        }
    }
    Ok(files)
}

fn io_error(action: &'static str) -> impl Fn(std::io::Error) -> String {
    move |error| format!("Could not {action}: {error}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn start_armed(recorder: &MemoryJourneyRecorder, target_app_class: &str) -> Option<String> {
        recorder
            .begin_capture_attempt_at(target_app_class, i64::MAX)
            .unwrap()
    }

    fn stage(name: &str) -> MemoryJourneyStageRecord {
        MemoryJourneyStageRecord {
            name: name.to_string(),
            status: MemoryJourneyStageStatus::Observed,
            observed_at_ms: 0,
            duration_ms: Some(3),
            outcome: "ok".to_string(),
            details: json!({"count": 1}),
            artifact_ids: Vec::new(),
        }
    }

    #[test]
    fn disarmed_attempt_creates_no_files() {
        let dir = tempfile::tempdir().unwrap();
        let recorder = MemoryJourneyRecorder::new(dir.path().join("journeys"));
        assert_eq!(recorder.begin_capture_attempt("browser").unwrap(), None);
        assert!(!dir.path().join("journeys").exists());
    }

    #[test]
    fn one_arm_is_consumed_by_one_attempt() {
        let dir = tempfile::tempdir().unwrap();
        let recorder = MemoryJourneyRecorder::new(dir.path().join("journeys"));
        let armed = recorder.arm("case 1".into()).unwrap();
        assert_eq!(start_armed(&recorder, "browser"), Some(armed.clone()));
        assert_eq!(recorder.begin_capture_attempt("editor").unwrap(), None);
        recorder.record_stage(&armed, stage("frame")).unwrap();
        recorder
            .finish_active(&armed, MemoryJourneyState::Complete)
            .unwrap();
        assert_eq!(recorder.status().unwrap().journeys.len(), 1);
    }

    fn flat_png(shade: u8) -> Vec<u8> {
        let mut buf = Vec::new();
        image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(64, 64, image::Rgb([shade; 3])))
            .write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png)
            .unwrap();
        buf
    }

    #[test]
    fn arm_phase_is_tri_state() {
        let dir = tempfile::tempdir().unwrap();
        let recorder = MemoryJourneyRecorder::new(dir.path().join("journeys"));
        assert_eq!(recorder.arm_phase(), ArmPhase::Inactive);
        recorder.arm("phases".into()).unwrap();
        let ready_at_ms = recorder.status().unwrap().arm_ready_at_ms.unwrap();
        assert_eq!(
            recorder.arm_phase_at(ready_at_ms - 1),
            ArmPhase::HandoffPending
        );
        assert_eq!(recorder.arm_phase_at(ready_at_ms), ArmPhase::Started);
        recorder
            .begin_capture_attempt_at("browser", ready_at_ms)
            .unwrap();
        assert_eq!(recorder.arm_phase_at(ready_at_ms + 1), ArmPhase::Started);
    }

    #[test]
    fn handoff_frames_cannot_make_the_armed_target_a_duplicate() {
        use crate::capture::PerceptualHasher;
        let target = flat_png(200);
        let threshold = 5;

        // Control: the old behavior. The ordinary tick keeps feeding the
        // hasher during the handoff, so the target is its own duplicate.
        let mut ungated = PerceptualHasher::new();
        assert!(!ungated.check(&target, threshold).is_duplicate);
        assert!(ungated.check(&target, threshold).is_duplicate);

        // Fixed behavior: the capture loop skips its tick while the phase is
        // HandoffPending, so the first armed attempt sees an empty history.
        let dir = tempfile::tempdir().unwrap();
        let recorder = MemoryJourneyRecorder::new(dir.path().join("journeys"));
        recorder.arm("handoff dedupe".into()).unwrap();
        let ready_at_ms = recorder.status().unwrap().arm_ready_at_ms.unwrap();
        let mut gated = PerceptualHasher::new();
        for tick in 0..4 {
            let now = ready_at_ms - 1 - tick;
            if recorder.arm_phase_at(now) == ArmPhase::HandoffPending {
                continue;
            }
            gated.check(&target, threshold);
        }
        let id = recorder
            .begin_capture_attempt_at("browser", ready_at_ms)
            .unwrap();
        assert!(id.is_some());
        let verdict = gated.check(&target, threshold);
        assert!(!verdict.is_duplicate);
        assert_eq!(verdict.match_kind, crate::capture::DedupeMatchKind::Novel);
    }

    #[test]
    fn skipped_attempt_records_the_dedupe_evidence() {
        let dir = tempfile::tempdir().unwrap();
        let recorder = MemoryJourneyRecorder::new(dir.path().join("journeys"));
        let id = recorder.arm("dedupe evidence".into()).unwrap();
        start_armed(&recorder, "browser");
        recorder
            .finish_skipped_with(
                &id,
                "perceptual_duplicate",
                json!({ "dedupe": { "threshold": 5, "match_kind": "consecutive_hash", "hash_distance": 0, "rgb_distance": 0 } }),
            )
            .unwrap();
        let manifest = &recorder.status().unwrap().manifests[0];
        let stage = manifest.stages.last().unwrap();
        assert_eq!(stage.outcome, "perceptual_duplicate");
        assert_eq!(stage.details["dedupe"]["match_kind"], "consecutive_hash");
    }

    #[test]
    fn armed_journey_waits_for_the_target_handoff_before_consuming_the_attempt() {
        let dir = tempfile::tempdir().unwrap();
        let recorder = MemoryJourneyRecorder::new(dir.path().join("journeys"));
        let id = recorder.arm("handoff".into()).unwrap();
        let status = recorder.status().unwrap();
        let ready_at_ms = status.arm_ready_at_ms.expect("armed ready timestamp");

        assert_eq!(
            recorder
                .begin_capture_attempt_at("fndr", ready_at_ms - 1)
                .unwrap(),
            None
        );
        assert!(recorder.status().unwrap().armed);
        assert_eq!(
            recorder
                .begin_capture_attempt_at("browser", ready_at_ms)
                .unwrap(),
            Some(id)
        );
        assert!(!recorder.status().unwrap().armed);
    }

    #[test]
    fn status_events_include_the_active_observed_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let recorder = MemoryJourneyRecorder::new(dir.path().join("journeys"));
        let id = recorder.arm("live evidence".into()).unwrap();
        start_armed(&recorder, "browser");
        recorder.record_stage(&id, stage("ocr")).unwrap();

        let status = recorder.status().unwrap();
        assert_eq!(status.active_state, Some(MemoryJourneyState::Capturing));
        assert_eq!(status.manifests.len(), 1);
        assert_eq!(status.manifests[0].journey_id, id);
        assert!(status.manifests[0]
            .stages
            .iter()
            .any(|stage| stage.name == "ocr"));
    }

    #[test]
    fn later_stage_updates_preserve_scoped_model_provenance() {
        let dir = tempfile::tempdir().unwrap();
        let recorder = MemoryJourneyRecorder::new(dir.path().join("journeys"));
        let id = recorder.arm("provenance".into()).unwrap();
        start_armed(&recorder, "browser");
        let mut model_stage = stage("extraction");
        model_stage.details = json!({ "model": "local-model", "prompt_tokens": 12 });
        recorder.record_stage(&id, model_stage).unwrap();
        let mut validator_stage = stage("extraction");
        validator_stage.details = json!({ "validator_result": "ok", "parse_result": "parsed" });
        recorder.record_stage(&id, validator_stage).unwrap();

        let manifest = recorder
            .finish_active(&id, MemoryJourneyState::Complete)
            .unwrap();
        let extraction = manifest
            .stages
            .iter()
            .find(|stage| stage.name == "extraction")
            .unwrap();
        assert_eq!(extraction.details["model"], "local-model");
        assert_eq!(extraction.details["validator_result"], "ok");
    }

    #[test]
    fn privacy_skip_publishes_no_content_artifacts() {
        let dir = tempfile::tempdir().unwrap();
        let recorder = MemoryJourneyRecorder::new(dir.path().join("journeys"));
        let id = recorder.arm("protected".into()).unwrap();
        start_armed(&recorder, "protected");
        let manifest = recorder
            .finish_skipped(&id, "sensitive_context", true)
            .unwrap();
        assert!(manifest.artifacts.is_empty());
        assert_eq!(manifest.stages[0].status, MemoryJourneyStageStatus::Skipped);
    }

    #[test]
    fn post_ocr_privacy_skip_redacts_already_written_artifacts() {
        let dir = tempfile::tempdir().unwrap();
        let recorder = MemoryJourneyRecorder::new(dir.path().join("journeys"));
        let id = recorder.arm("protected".into()).unwrap();
        start_armed(&recorder, "browser");
        recorder.record_stage(&id, stage("ocr")).unwrap();
        recorder
            .record_artifact(&id, "ocr", "recognized.txt", b"secret")
            .unwrap();

        let manifest = recorder
            .redact_and_finish_privacy_skip(&id, "sensitive_transient_text")
            .unwrap();
        assert!(manifest.artifacts.is_empty());
        assert_eq!(manifest.stages.len(), 1);
        assert_eq!(manifest.stages[0].name, "admission");
        assert!(!recorder.complete_dir(&id).join("artifacts").exists());
    }

    #[test]
    fn artifact_refs_are_relative_hashed_and_owner_only() {
        let dir = tempfile::tempdir().unwrap();
        let recorder = MemoryJourneyRecorder::new(dir.path().join("journeys"));
        let id = recorder.arm("artifact".into()).unwrap();
        start_armed(&recorder, "browser");
        recorder.record_stage(&id, stage("ocr")).unwrap();
        let artifact = recorder
            .record_artifact(&id, "ocr", "raw.txt", b"private text")
            .unwrap();
        assert_eq!(artifact.relative_path, "artifacts/raw.txt");
        assert!(!artifact.relative_path.starts_with('/'));
        assert_eq!(artifact.sha256.len(), 64);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(recorder.partial_dir(&id).join(&artifact.relative_path))
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600);
        }
    }

    #[test]
    fn size_cap_removes_partial_instead_of_publishing_incomplete_data() {
        let dir = tempfile::tempdir().unwrap();
        let recorder = MemoryJourneyRecorder::with_limits(
            dir.path().join("journeys"),
            6,
            DEFAULT_MAX_AGE_MS,
            1_200,
        );
        let id = recorder.arm("too large".into()).unwrap();
        start_armed(&recorder, "browser");
        let err = recorder
            .record_artifact(&id, "frame", "frame.png", &vec![7; 2_000])
            .unwrap_err();
        assert!(err.contains("storage cap"));
        assert!(!recorder.partial_dir(&id).exists());
    }

    #[test]
    fn restart_cleanup_removes_abandoned_partial_and_delete_is_deterministic() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("journeys");
        let id = {
            let recorder = MemoryJourneyRecorder::new(root.clone());
            let id = recorder.arm("interrupted".into()).unwrap();
            start_armed(&recorder, "browser");
            assert!(recorder.partial_dir(&id).exists());
            id
        };
        let restarted = MemoryJourneyRecorder::new(root);
        assert!(!restarted.partial_dir(&id).exists());
        restarted.delete(&id).unwrap();
        restarted.delete(&id).unwrap();
    }

    #[test]
    fn delete_selected_cancels_an_active_recording_and_removes_partial_content() {
        let dir = tempfile::tempdir().unwrap();
        let recorder = MemoryJourneyRecorder::new(dir.path().join("journeys"));
        let id = recorder.arm("cancel me".into()).unwrap();
        start_armed(&recorder, "browser");
        recorder
            .record_artifact(&id, "frame", "capture.png", b"private pixels")
            .unwrap();

        recorder.delete(&id).unwrap();

        assert!(recorder.is_inactive());
        assert!(!recorder.partial_dir(&id).exists());
        assert!(recorder.status().unwrap().journeys.is_empty());
    }

    #[test]
    fn retention_keeps_only_the_newest_bounded_set() {
        let dir = tempfile::tempdir().unwrap();
        let recorder = MemoryJourneyRecorder::with_limits(
            dir.path().join("journeys"),
            2,
            DEFAULT_MAX_AGE_MS,
            DEFAULT_MAX_TOTAL_BYTES,
        );
        for label in ["one", "two", "three"] {
            let id = recorder.arm(label.into()).unwrap();
            start_armed(&recorder, "browser");
            recorder
                .finish_active(&id, MemoryJourneyState::Complete)
                .unwrap();
        }
        assert_eq!(recorder.status().unwrap().journeys.len(), 2);
    }

    #[test]
    fn vector_contract_detects_zero_and_non_finite_vectors_without_serializing_floats() {
        let zero = vector_contract("primary", "minilm", "text", &[0.0, 0.0], "source");
        assert_eq!(zero["valid"], false);
        assert_eq!(zero["dimension_matches"], false);
        assert_eq!(zero["expected_dimension"], 384);
        let invalid = vector_contract("primary", "minilm", "text", &[1.0, f32::NAN], "source");
        assert_eq!(invalid["non_finite_count"], 1);
        assert!(invalid.get("vector").is_none());
    }

    #[test]
    fn export_contains_relative_entries_and_no_absolute_manifest_paths() {
        let dir = tempfile::tempdir().unwrap();
        let recorder = MemoryJourneyRecorder::new(dir.path().join("journeys"));
        let id = recorder.arm("export".into()).unwrap();
        start_armed(&recorder, "browser");
        recorder.record_stage(&id, stage("ocr")).unwrap();
        recorder
            .record_artifact(&id, "ocr", "raw.txt", b"text")
            .unwrap();
        recorder
            .finish_active(&id, MemoryJourneyState::Complete)
            .unwrap();
        let output = dir.path().join("case.fndrjourney.zip");
        recorder.export_to(&id, &output).unwrap();
        let file = File::open(output).unwrap();
        let mut archive = zip::ZipArchive::new(file).unwrap();
        assert!(archive.by_name("manifest.json").is_ok());
        assert!(archive.by_name("artifacts/raw.txt").is_ok());
        let mut manifest = String::new();
        archive
            .by_name("manifest.json")
            .unwrap()
            .read_to_string(&mut manifest)
            .unwrap();
        assert!(!manifest.contains(dir.path().to_string_lossy().as_ref()));
    }

    #[test]
    fn query_runs_update_only_the_matching_scorecard_fields() {
        let dir = tempfile::tempdir().unwrap();
        let recorder = MemoryJourneyRecorder::new(dir.path().join("journeys"));
        let id = recorder.arm("scorecard".into()).unwrap();
        start_armed(&recorder, "browser");
        recorder.attach_memory_id(&id, "memory-1").unwrap();
        recorder
            .finish_active(&id, MemoryJourneyState::Complete)
            .unwrap();

        let exact = recorder
            .append_query_run(
                &id,
                MemoryJourneyQueryRun {
                    id: "exact-run".into(),
                    path: MemoryJourneyQueryPath::Search,
                    kind: MemoryJourneyQueryKind::Exact,
                    query: "distinctive fact".into(),
                    started_at_ms: now_ms(),
                    duration_ms: 4,
                    result_ids: vec!["memory-1".into()],
                    scores: vec![0.9],
                    citation_ids: Vec::new(),
                    refusal: None,
                    explanation: json!({}),
                },
            )
            .unwrap();
        assert_eq!(exact.human_usefulness.exact_findable, Some(true));
        assert_eq!(exact.human_usefulness.paraphrase_findable, None);
        assert_eq!(exact.agent_grounding.correct_refusal, None);

        let unsupported = recorder
            .append_query_run(
                &id,
                MemoryJourneyQueryRun {
                    id: "unsupported-run".into(),
                    path: MemoryJourneyQueryPath::Ask,
                    kind: MemoryJourneyQueryKind::Unsupported,
                    query: "What is not supported?".into(),
                    started_at_ms: now_ms(),
                    duration_ms: 6,
                    result_ids: Vec::new(),
                    scores: Vec::new(),
                    citation_ids: Vec::new(),
                    refusal: Some(true),
                    explanation: json!({}),
                },
            )
            .unwrap();
        assert_eq!(unsupported.agent_grounding.correct_refusal, Some(true));
        assert_eq!(
            unsupported.query_runs[0].kind,
            MemoryJourneyQueryKind::Exact
        );
        assert_eq!(
            unsupported.query_runs[1].kind,
            MemoryJourneyQueryKind::Unsupported
        );
    }

    #[tokio::test]
    async fn six_synthetic_journeys_reconstruct_from_temporary_storage_and_search() {
        use crate::config::{Config, DEFAULT_IMAGE_EMBEDDING_DIM};
        use crate::embedding::EMBEDDING_DIM;
        use crate::graph::GraphStore;
        use crate::storage::{StateStore, Store};
        use crate::AppState;
        use std::sync::Arc;

        let dir = tempfile::tempdir().unwrap();
        let store_path = dir.path().to_path_buf();
        let store = Arc::new(
            tokio::task::spawn_blocking(move || Store::new(&store_path).unwrap())
                .await
                .unwrap(),
        );
        let state_path = dir.path().to_path_buf();
        let state_store = Arc::new(
            tokio::task::spawn_blocking(move || StateStore::new(&state_path).unwrap())
                .await
                .unwrap(),
        );
        let state = AppState::new(
            dir.path().to_path_buf(),
            Config::default(),
            store.clone(),
            state_store,
            GraphStore::new(store.clone()),
            None,
        );
        let cases = [
            ("research", "The distinctive Zephyr fact is forty two."),
            (
                "code",
                "Terminal error EPIPE was fixed by closing the stale worker.",
            ),
            (
                "project",
                "Owner Mina chose Friday as the deadline and review as next step.",
            ),
            (
                "conversation",
                "Kai committed to draft the proposal; budget remains unresolved.",
            ),
            (
                "dashboard",
                "The north region table reports 19 completed items.",
            ),
            (
                "low-signal",
                "Unsupported placeholder with deliberately weak evidence.",
            ),
        ];
        let now = now_ms();
        let records = cases
            .iter()
            .enumerate()
            .map(|(index, (id, text))| MemoryRecord {
                id: id.to_string(),
                timestamp: now + index as i64,
                day_bucket: "2026-09-30".to_string(),
                app_name: "Synthetic QA".to_string(),
                window_title: format!("Synthetic {id}"),
                session_id: "memory-journey-suite".to_string(),
                text: text.to_string(),
                clean_text: text.to_string(),
                snippet: text.to_string(),
                display_summary: text.to_string(),
                memory_context: text.to_string(),
                embedding_text: text.to_string(),
                embedding_model: "all-MiniLM-L6-v2".to_string(),
                embedding_dim: EMBEDDING_DIM as u32,
                embedding: vec![0.1; EMBEDDING_DIM],
                snippet_embedding: vec![0.1; EMBEDDING_DIM],
                support_embedding: vec![0.1; EMBEDDING_DIM],
                image_embedding: vec![0.1; DEFAULT_IMAGE_EMBEDDING_DIM],
                schema_version: 2,
                storage_outcome: "primary_memory_card".to_string(),
                enrichment_status: "pending".to_string(),
                ..Default::default()
            })
            .collect::<Vec<_>>();
        assert_eq!(store.add_batch_and_get_count(&records).await.unwrap(), 6);

        for record in &records {
            let persisted = store.get_memory_by_id(&record.id).await.unwrap().unwrap();
            let manifest = state
                .memory_journey
                .create_reconstructed(&persisted)
                .unwrap();
            assert_eq!(manifest.mode, MemoryJourneyMode::Reconstructed);
            assert!(manifest.stages.iter().any(|stage| stage.name == "frame"
                && stage.status == MemoryJourneyStageStatus::Unavailable));
        }

        let results = crate::ipc::commands::search::search_ranked_results(
            &state,
            "distinctive Zephyr fact",
            None,
            None,
            10,
        )
        .await
        .unwrap();
        assert!(results.iter().any(|result| result.id == "research"));
        let (explained, explanation) =
            crate::ipc::commands::search::search_ranked_results_explained(
                &state,
                "distinctive Zephyr fact",
                None,
                None,
                10,
            )
            .await
            .unwrap();
        assert_eq!(
            results
                .iter()
                .map(|result| (&result.id, result.score))
                .collect::<Vec<_>>(),
            explained
                .iter()
                .map(|result| (&result.id, result.score))
                .collect::<Vec<_>>()
        );
        assert!(explanation.get("production_retrieval").is_some());
    }
}
