use crate::storage::MemoryRecord;
use serde::Serialize;

/// A step to take next, suggested from what one memory recorded.
#[derive(Debug, Clone, Serialize)]
pub struct TaskCandidate {
    pub title: String,
    pub source_memory_id: String,
    pub confidence: f32,
}

/// Extract task candidates from a memory's next steps, its decisions that
/// state work to do, and its errors. Does NOT call any model.
pub fn extract_task_candidates(record: &MemoryRecord) -> Vec<TaskCandidate> {
    let candidate = |title: String, weight: f32| TaskCandidate {
        title,
        source_memory_id: record.id.clone(),
        confidence: record.confidence_score * weight,
    };
    let mut candidates = Vec::new();

    for step in &record.next_steps {
        if step.trim().is_empty() {
            continue;
        }
        candidates.push(candidate(step.trim().to_string(), 0.85));
    }

    for decision in &record.decisions {
        if decision.trim().is_empty() {
            continue;
        }
        let lower = decision.to_ascii_lowercase();
        if lower.contains("todo") || lower.contains("need to") || lower.contains("will ") {
            candidates.push(candidate(decision.trim().to_string(), 0.70));
        }
    }

    for error in &record.errors {
        if error.trim().is_empty() {
            continue;
        }
        candidates.push(candidate(format!("Fix: {}", error.trim()), 0.65));
    }

    candidates
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_next_steps_as_tasks() {
        let record = MemoryRecord {
            id: "mem-001".to_string(),
            next_steps: vec!["merge PR #42".to_string(), "update docs".to_string()],
            confidence_score: 0.8,
            ..Default::default()
        };
        let tasks = extract_task_candidates(&record);
        assert!(tasks.iter().any(|t| t.title == "merge PR #42"));
        assert!(tasks.iter().any(|t| t.title == "update docs"));
        assert!(tasks.iter().all(|t| t.source_memory_id == "mem-001"));
    }

    #[test]
    fn extracts_errors_as_fix_tasks() {
        let record = MemoryRecord {
            id: "mem-002".to_string(),
            errors: vec!["connection refused on port 5432".to_string()],
            confidence_score: 0.7,
            ..Default::default()
        };
        let tasks = extract_task_candidates(&record);
        assert!(tasks.iter().any(|t| t.title.starts_with("Fix:")));
    }

    #[test]
    fn empty_record_produces_no_tasks() {
        let tasks = extract_task_candidates(&MemoryRecord::default());
        assert!(tasks.is_empty());
    }
}
