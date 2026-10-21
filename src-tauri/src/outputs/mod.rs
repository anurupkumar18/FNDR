//! Daily outputs: the standup, weekly status and timesheet a person would
//! otherwise write by hand, composed in code from memories and activity
//! events.
//!
//! Every line cites the memory or activity event it came from, and a line
//! with no citation is dropped (`OutputDoc::enforce_citations`). The on-device
//! 2B model does not write these (AGENTS.md: it extracts, it does not
//! synthesize); at most it rewords one finished line at a time, and a
//! rewording that adds a fact is discarded (`polish`). With no model loaded
//! the templated text is the output.
//!
//! Imported rows (`source_type` starting `import:`) and assistant notes are
//! untrusted text, not the person's own work: `gather` keeps them out.
//!
//! Nothing here sends anything on its own. Sending is `send` behind `tools`,
//! one approval at a time.

pub mod build;
pub mod gather;
pub mod polish;
pub mod render;
pub mod send;
pub mod state;
pub mod timesheet;
pub mod tools;

use crate::storage::ActivityEvent;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputKind {
    Standup,
    WeeklyStatus,
    Timesheet,
}

impl OutputKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Standup => "standup",
            Self::WeeklyStatus => "weekly-status",
            Self::Timesheet => "timesheet",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputFormat {
    Markdown,
    PlainText,
    Html,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CitationKind {
    Memory,
    ActivityEvent,
}

/// Where a line came from. `memory_id` is the memory the person can open for
/// it: the memory itself, or the one an activity event was built from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Citation {
    pub kind: CitationKind,
    pub id: String,
    pub memory_id: Option<String>,
}

impl Citation {
    pub fn memory(id: &str) -> Self {
        Self {
            kind: CitationKind::Memory,
            id: id.to_string(),
            memory_id: Some(id.to_string()),
        }
    }

    pub fn event(event: &ActivityEvent) -> Self {
        Self {
            kind: CitationKind::ActivityEvent,
            id: event.id.clone(),
            memory_id: Some(event.memory_id.clone()).filter(|id| !id.trim().is_empty()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Line {
    pub text: String,
    pub project: Option<String>,
    pub citations: Vec<Citation>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Section {
    pub heading: String,
    pub lines: Vec<Line>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutputDoc {
    pub id: String,
    pub kind: OutputKind,
    pub title: String,
    pub start_ms: i64,
    pub end_ms: i64,
    pub generated_at_ms: i64,
    /// True when the local model reworded at least one line.
    pub model_polished: bool,
    pub sections: Vec<Section>,
}

impl OutputDoc {
    /// Drops every line that cites nothing. Returns how many were dropped.
    pub fn enforce_citations(&mut self) -> usize {
        let mut dropped = 0;
        for section in &mut self.sections {
            let before = section.lines.len();
            section.lines.retain(|line| {
                !line.text.trim().is_empty()
                    && line.citations.iter().any(|citation| {
                        !citation.id.trim().is_empty()
                            && citation
                                .memory_id
                                .as_deref()
                                .is_some_and(|id| !id.trim().is_empty())
                    })
            });
            dropped += before - section.lines.len();
        }
        dropped
    }

    pub fn line_count(&self) -> usize {
        self.sections
            .iter()
            .map(|section| section.lines.len())
            .sum()
    }
}

/// A capture the outputs may quote. Already filtered by the one visibility
/// rule (`gather`).
#[derive(Debug, Clone, PartialEq)]
pub struct MemoryEvidence {
    pub id: String,
    pub timestamp_ms: i64,
    pub app: String,
    pub window_title: String,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TaskEvidence {
    pub title: String,
    pub due_ms: Option<i64>,
    pub created_at_ms: i64,
    /// The memories the task came from. A task with none is not quoted.
    pub memory_ids: Vec<String>,
}

/// Everything a builder may read. Builders are pure over this.
#[derive(Debug, Clone, Default)]
pub struct Evidence {
    pub events: Vec<ActivityEvent>,
    pub memories: Vec<MemoryEvidence>,
    pub tasks: Vec<TaskEvidence>,
}

impl Evidence {
    /// The app each memory was captured in, for the timesheet.
    pub fn app_by_memory(&self) -> HashMap<String, String> {
        self.memories
            .iter()
            .map(|memory| (memory.id.clone(), memory.app.clone()))
            .collect()
    }
}

pub(crate) fn has_narrator(text: &str) -> bool {
    let lower = text.trim().to_lowercase();
    ["the user ", "user ", "you ", "i ", "we "]
        .iter()
        .any(|prefix| lower.starts_with(prefix))
}

/// The document for `kind` over `[start_ms, end_ms]`, with uncited lines
/// already dropped.
pub fn build(
    kind: OutputKind,
    evidence: &Evidence,
    start_ms: i64,
    end_ms: i64,
    now_ms: i64,
) -> OutputDoc {
    let mut doc = match kind {
        OutputKind::Standup => build::standup(evidence, start_ms, end_ms, now_ms),
        OutputKind::WeeklyStatus => build::weekly_status(evidence, start_ms, end_ms, now_ms),
        OutputKind::Timesheet => build::timesheet_doc(evidence, start_ms, end_ms, now_ms),
    };
    doc.enforce_citations();
    doc
}

#[cfg(test)]
mod citation_tests {
    use super::*;

    #[test]
    fn a_line_without_a_memory_id_is_not_an_output_line() {
        let mut doc = OutputDoc {
            id: "d".to_string(),
            kind: OutputKind::Standup,
            title: "Standup".to_string(),
            start_ms: 0,
            end_ms: 1,
            generated_at_ms: 0,
            model_polished: false,
            sections: vec![Section {
                heading: "Yesterday".to_string(),
                lines: vec![Line {
                    text: "Reviewed the export plan".to_string(),
                    project: None,
                    citations: vec![Citation {
                        kind: CitationKind::ActivityEvent,
                        id: "event-1".to_string(),
                        memory_id: None,
                    }],
                }],
            }],
        };
        assert_eq!(doc.enforce_citations(), 1);
        assert_eq!(doc.line_count(), 0);
    }
}
