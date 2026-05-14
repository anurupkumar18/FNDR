use crate::memory::types::{
    DistilledMemory, DiagnosticObservation, MemoryDecision, QualityDecision, QualityScores,
    SkipReason, ValidatedMemory,
};
use crate::storage::MemoryRecord;

fn has_display_template_noise(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    ["reopen:", "find similar", "delete", "topic:", "continues from"]
        .iter()
        .any(|needle| lower.contains(needle))
}

fn is_url_only_topic(topic: &str) -> bool {
    let trimmed = topic.trim().to_ascii_lowercase();
    trimmed.starts_with("http://")
        || trimmed.starts_with("https://")
        || trimmed.starts_with("www.")
        || trimmed.contains('/') && !trimmed.contains(' ')
}

pub fn quality_decision_for_record(record: &MemoryRecord) -> QualityDecision {
    let mut reasons = Vec::new();
    let topic_clarity = crate::storage::topic_clarity_score(record).clamp(0.0, 1.0);
    let pollution_ratio = crate::storage::pollution_ratio_score(record).clamp(0.0, 1.0);
    let retrieval_value = record.retrieval_value_score.clamp(0.0, 1.0);
    let graph_readiness = record.graph_readiness_score.clamp(0.0, 1.0);
    let evidence_quality = (1.0 - record.ocr_noise_score).clamp(0.0, 1.0);
    let grounding = record.extraction_confidence.clamp(0.0, 1.0);

    if record.topic.trim().is_empty() || record.topic.eq_ignore_ascii_case("unknown") {
        reasons.push("missing_topic".to_string());
    }
    if is_url_only_topic(&record.topic) {
        reasons.push("url_only_topic".to_string());
    }
    if has_display_template_noise(&record.memory_context)
        || has_display_template_noise(&record.embedding_text)
    {
        reasons.push("display_template_noise".to_string());
    }
    if grounding <= 0.0 {
        reasons.push("grounding_confidence_zero".to_string());
    }

    if evidence_quality < 0.35 {
        reasons.push("weak_evidence_quality".to_string());
    }
    if pollution_ratio > 0.70 {
        reasons.push("high_pollution".to_string());
    }

    let passed = reasons.is_empty()
        && grounding >= 0.30
        && evidence_quality >= 0.40
        && pollution_ratio <= 0.65
        && topic_clarity >= 0.20;

    QualityDecision {
        decision: if passed {
            "store".to_string()
        } else if reasons.iter().any(|r| r == "high_pollution" || r == "display_template_noise") {
            "quarantine".to_string()
        } else {
            "skip".to_string()
        },
        passed,
        reasons,
        scores: QualityScores {
            grounding_confidence: grounding,
            evidence_quality,
            contamination_score: pollution_ratio,
            topic_clarity,
            pollution_ratio,
            retrieval_value,
            graph_readiness,
        },
    }
}

pub fn decide_memory(distilled: DistilledMemory, quality: &QualityDecision) -> MemoryDecision {
    if quality.passed {
        return MemoryDecision::Store(ValidatedMemory {
            title: distilled.title,
            topic: distilled.topic,
            summary_short: distilled.summary_short,
            memory_context: distilled.memory_context,
            activity_type: distilled.activity_type,
            workflow: distilled.workflow,
            project: distilled.project,
            entities: distilled.entities,
            actions: distilled.actions,
            user_intent: distilled.user_intent,
            confidence: distilled.confidence,
            grounding_confidence: quality.scores.grounding_confidence,
            evidence_quality: quality.scores.evidence_quality,
            contamination_score: quality.scores.contamination_score,
            quality_flags: distilled.quality_flags,
        });
    }

    if quality
        .reasons
        .iter()
        .any(|reason| reason == "high_pollution" || reason == "display_template_noise")
    {
        return MemoryDecision::Quarantine(DiagnosticObservation {
            reason: "polluted_memory".to_string(),
            details: quality.reasons.clone(),
        });
    }

    MemoryDecision::Skip(if quality
        .reasons
        .iter()
        .any(|reason| reason == "grounding_confidence_zero")
    {
        SkipReason::LowGrounding
    } else if quality
        .reasons
        .iter()
        .any(|reason| reason == "missing_topic")
    {
        SkipReason::MissingCoreFields
    } else {
        SkipReason::WeakEvidence
    })
}

pub fn can_merge_into_continuity(record: &MemoryRecord) -> bool {
    let quality = quality_decision_for_record(record);
    quality.passed && quality.scores.graph_readiness >= 0.35
}

pub fn should_queue_graph(record: &MemoryRecord) -> bool {
    let quality = quality_decision_for_record(record);
    quality.passed && quality.scores.graph_readiness >= 0.45
}
