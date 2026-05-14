use crate::memory::types::QualityDecision;
use crate::storage::MemoryRecord;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RepairDisposition {
    Clean,
    Repairable,
    Quarantine,
    DeleteCandidate,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RepairAudit {
    pub memory_id: String,
    pub disposition: String,
    pub old_title: String,
    pub new_title: String,
    pub old_topic: String,
    pub new_topic: String,
    pub old_embedding_text_hash: String,
    pub new_embedding_text_hash: String,
    pub repair_reason: String,
    pub confidence_delta: f32,
    pub quarantine_reason: String,
}

pub fn classify_repair_disposition(
    record: &MemoryRecord,
    quality: &QualityDecision,
) -> RepairDisposition {
    if quality.passed {
        return RepairDisposition::Clean;
    }

    if quality
        .reasons
        .iter()
        .any(|r| r == "display_template_noise" || r == "high_pollution")
    {
        if record.memory_context.trim().is_empty() && record.clean_text.trim().len() < 24 {
            RepairDisposition::Quarantine
        } else {
            RepairDisposition::Repairable
        }
    } else if quality
        .reasons
        .iter()
        .any(|r| r == "grounding_confidence_zero")
    {
        RepairDisposition::Quarantine
    } else {
        RepairDisposition::Repairable
    }
}
