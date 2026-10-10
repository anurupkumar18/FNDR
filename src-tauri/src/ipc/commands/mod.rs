//! Tauri command handlers

pub(crate) mod common;
pub mod search;

pub use search::{
    list_memory_cards, list_needs_signal_memory_cards, search, search_memory_cards,
    search_raw_results, summarize_search, NeedsSignalCard,
};

pub(crate) mod memory;
pub use memory::*;

mod quality;
pub use quality::*;

mod meeting;
pub use meeting::*;

mod export;
pub use export::*;

mod privacy;
pub use privacy::*;

pub(crate) mod stats;
pub use stats::*;

mod autofill;
pub use autofill::*;

mod voice_output;
pub use voice_output::*;

mod omnibar;
pub use omnibar::*;

mod notch;
pub use notch::*;

mod clipboard;
pub use clipboard::*;

mod todos;
pub use todos::*;

mod maintenance;
pub use maintenance::*;

mod hermes_agent;
pub use hermes_agent::*;

mod agent_chats;
pub use agent_chats::*;

mod home;
pub use home::*;

mod proactive_signals;
pub use proactive_signals::*;

pub(crate) mod codex_account;
pub use codex_account::*;

mod speech_out;
pub use speech_out::*;

pub(crate) mod openclicky_bridge;
pub use openclicky_bridge::*;

mod computer_use;
mod hermes_codex;
mod setup_center;
mod work_set;
pub use computer_use::*;
pub use setup_center::*;
pub use work_set::*;

mod agent;
pub use agent::*;

mod peer;
pub use peer::*;

mod graph;
pub use graph::*;

mod glasses_import;
pub use glasses_import::*;

mod daily_review;
pub use daily_review::*;

pub mod debug;
pub use debug::{get_memory_timeline_thread, inspect_memory_pipeline};

pub mod retrieval;
pub use retrieval::{
    fndr_answer, fndr_build_context_pack, fndr_get_memory_source_statements,
    fndr_get_memory_subgraph, fndr_get_related_memories, fndr_quality_status, fndr_search,
    fndr_timeline,
};

mod companion;
pub use companion::*;

mod screen_guide;
pub use screen_guide::*;

mod screen_guide_diagnostics;
pub use screen_guide_diagnostics::*;

#[cfg(debug_assertions)]
mod memory_journey;
#[cfg(debug_assertions)]
pub use memory_journey::*;

#[cfg(debug_assertions)]
mod quality_lab;
#[cfg(debug_assertions)]
pub use quality_lab::*;

// Privacy proof IPC command
pub use crate::privacy_proof::*;

// Resume Work IPC command
pub use crate::resume::*;

#[cfg(test)]
mod daily_summary_tests {
    use crate::embedding::{Embedder, EmbeddingBackend};
    use crate::storage::{MemoryRecord, SearchResult};

    use super::autofill::{
        extract_candidates_from_result, field_aliases, needs_autofill_confirmation,
        AutofillCandidate,
    };
    use super::quality::classify_storage_outcome_with_config;
    use super::search::memory_card_from_result;
    use super::stats::{
        build_daily_activity_summary, build_focus_task_embedding, surfaceable_daily_records,
    };

    fn result(
        id: &str,
        timestamp: i64,
        app_name: &str,
        window_title: &str,
        snippet: &str,
        url: Option<&str>,
    ) -> SearchResult {
        SearchResult {
            id: id.to_string(),
            timestamp,
            app_name: app_name.to_string(),
            bundle_id: None,
            window_title: window_title.to_string(),
            session_id: "test-session".to_string(),
            text: snippet.to_string(),
            clean_text: snippet.to_string(),
            ocr_confidence: 0.95,
            ocr_block_count: 4,
            snippet: snippet.to_string(),
            summary_source: "fallback".to_string(),
            noise_score: 0.02,
            session_key: "test-session-key".to_string(),
            lexical_shadow: String::new(),
            score: 1.0,
            screenshot_path: None,
            url: url.map(str::to_string),
            decay_score: 1.0,
            ..Default::default()
        }
    }

    #[test]
    fn daily_summary_is_grounded_for_short_capture_span() {
        let start = chrono::Utc::now().timestamp_millis();
        let records = vec![
            result(
                "1",
                start,
                "VS Code",
                "MemoryCardsPanel.tsx",
                "Investigated why all memory cards were not loading.",
                None,
            ),
            result(
                "2",
                start + 10 * 60_000,
                "Discord",
                "FNDR team chat",
                "Checked a short team follow-up.",
                None,
            ),
            result(
                "3",
                start + 30 * 60_000,
                "VS Code",
                "MemoryCardsPanel.tsx",
                "Tested the all-app memory browse flow.",
                None,
            ),
        ];

        let summary = build_daily_activity_summary(&records, "today");
        let lines = summary.lines().collect::<Vec<_>>();

        assert!(summary.contains("about 30 minutes today"));
        assert!(summary.contains("VS Code"));
        assert!(summary.contains("Discord"));
        assert!(!summary.contains("GitLab"));
        assert!(
            lines.len() <= 4,
            "short spans should not force 6-8 bullets: {summary}"
        );
    }

    #[test]
    fn daily_summary_excludes_low_signal_visual_fallbacks() {
        let mut fallback = result(
            "fallback",
            chrono::Utc::now().timestamp_millis(),
            "ChatGPT",
            "ChatGPT_1789740602743.png",
            "ChatGPT",
            None,
        );
        fallback.storage_outcome = "low_quality_evidence".to_string();
        fallback.synthesis_branch = "llm_ocr_grounded_visual_fallback".to_string();
        fallback.ocr_block_count = 1;

        let retained = surfaceable_daily_records(vec![fallback], &[]);

        assert!(retained.is_empty());
    }

    #[test]
    fn autofill_aliases_expand_common_synonyms() {
        let aliases = field_aliases("Tax ID");

        assert!(aliases
            .iter()
            .any(|alias| alias.eq_ignore_ascii_case("ein")));
        assert!(aliases
            .iter()
            .any(|alias| alias.eq_ignore_ascii_case("employer identification number")));
    }

    #[test]
    fn autofill_extracts_inline_and_table_values() {
        let record = result(
            "autofill-1",
            chrono::Utc::now().timestamp_millis(),
            "Preview",
            "StateFarm_Statement.pdf",
            "Policy Number: POL-88291-X\nGroup Number  8821\nMember Name  Jane Doe",
            None,
        );

        let inline = extract_candidates_from_result(
            "Policy Number",
            &field_aliases("Policy Number"),
            &record,
            1.0,
        );
        assert!(
            inline
                .iter()
                .any(|candidate| candidate.value == "POL-88291-X"),
            "expected inline label-value extraction, got: {inline:?}"
        );

        let table = extract_candidates_from_result(
            "Group Number",
            &field_aliases("Group Number"),
            &record,
            1.0,
        );
        assert!(
            table.iter().any(|candidate| candidate.value == "8821"),
            "expected table-style extraction, got: {table:?}"
        );
    }

    #[test]
    fn autofill_requires_confirmation_when_candidates_are_close() {
        let candidates = vec![
            AutofillCandidate {
                value: "POL-111".to_string(),
                confidence: 0.94,
                match_reason: "Top".to_string(),
                source_snippet: "".to_string(),
                source_app: "Preview".to_string(),
                source_window_title: "A.pdf".to_string(),
                timestamp: 1,
                memory_id: "1".to_string(),
            },
            AutofillCandidate {
                value: "POL-222".to_string(),
                confidence: 0.91,
                match_reason: "Close".to_string(),
                source_snippet: "".to_string(),
                source_app: "Preview".to_string(),
                source_window_title: "B.pdf".to_string(),
                timestamp: 2,
                memory_id: "2".to_string(),
            },
        ];

        assert!(needs_autofill_confirmation(&candidates, 0.90));
    }

    #[test]
    fn focus_task_embedding_is_disabled_without_semantic_backend() {
        assert_eq!(
            build_focus_task_embedding("Finish quarterly planning", None).expect("focus embedding"),
            None
        );
    }

    #[test]
    fn focus_task_embedding_is_present_for_real_backend_when_available() {
        let Ok(embedder) = Embedder::new() else {
            return;
        };
        if !matches!(embedder.backend(), EmbeddingBackend::Real) {
            return;
        }

        let embedding = build_focus_task_embedding("Finish quarterly planning", Some(&embedder))
            .expect("focus embedding");
        assert!(embedding.is_some());
        assert!(embedding
            .as_ref()
            .is_some_and(|vector| vector.iter().any(|value| *value != 0.0)));
    }

    #[test]
    fn list_card_builder_preserves_activity_files_and_duration() {
        let mut source = result(
            "list-1",
            chrono::Utc::now().timestamp_millis(),
            "VS Code",
            "capture/mod.rs",
            "Updated structured extraction validation",
            None,
        );
        source.activity_type = "coding".to_string();
        source.files_touched = vec![
            "src-tauri/src/capture/mod.rs".to_string(),
            "src-tauri/src/ipc/commands.rs".to_string(),
        ];
        source.session_duration_mins = 37;

        let card = memory_card_from_result(source);
        assert_eq!(card.activity_type, "coding");
        assert_eq!(card.files_touched.len(), 2);
        assert_eq!(card.session_duration_mins, 37);
    }

    #[test]
    fn a_vault_card_counts_what_its_memory_records() {
        let mut card = memory_card_from_result(result(
            "list-2",
            chrono::Utc::now().timestamp_millis(),
            "Figma",
            "Vault cards",
            "Chose the smaller cards for the Vault",
            None,
        ));
        assert_eq!(
            (card.decision_count, card.error_count, card.next_step_count),
            (0, 0, 0)
        );

        let record = MemoryRecord {
            decisions: vec!["Use the smaller cards".to_string(), "  ".to_string()],
            errors: vec!["build failed on lint".to_string()],
            next_steps: Vec::new(),
            ..Default::default()
        };
        super::search::set_fact_counts(&mut card, &record);
        assert_eq!(
            (card.decision_count, card.error_count, card.next_step_count),
            (1, 1, 0)
        );
    }

    #[test]
    fn api_storage_classifier_matches_shared_classifier() {
        let config = crate::memory_quality::default_memory_quality_config();
        let record = MemoryRecord {
            specificity_score: 0.33,
            intent_score: 0.78,
            agent_usefulness_score: 0.64,
            evidence_confidence: 0.28,
            ocr_noise_score: 0.25,
            dedup_fingerprint: "fndr:debug:ocr".to_string(),
            ..Default::default()
        };

        let api_outcome = classify_storage_outcome_with_config(&record, &config);
        let shared_outcome = crate::memory_quality::classify_storage_outcome(&record, &config);
        assert_eq!(api_outcome, shared_outcome);
    }
}
