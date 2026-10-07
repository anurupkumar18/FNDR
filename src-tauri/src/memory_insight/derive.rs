//! Derive persisted insight columns from structured fields + salience ranking.
//!
//! When synthesis (VLM/LLM) has supplied `insight_what_happened` / `insight_why_mattered`
//! directly, those win. Otherwise we synthesize coherent fallback text from
//! structured metadata, deliberately avoiding template strings, filename
//! patterns, and raw OCR fragments that would just leak as fluff to the UI.

use crate::capture::text_cleanup::{rank_salient_spans, salience_concentration, SalientSpan};
use crate::storage::schema::MemoryRecord;

const TOP_SPAN_DEBUG: usize = 8;
const DROP_SCORE: f32 = 0.12;
const MAX_WHAT_CHARS: usize = 280;
const MAX_WHY_CHARS: usize = 320;
const MAX_CHANGED_CHARS: usize = 400;

fn pollution_for_insight(record: &MemoryRecord) -> f32 {
    let noise = record.ocr_noise_score.clamp(0.0, 1.0);
    let concentration = salience_concentration(&record.clean_text, &record.app_name);
    let diffusion = (1.0 - concentration).clamp(0.0, 1.0);
    ((noise * 0.55) + (diffusion * 0.45)).clamp(0.0, 1.0)
}

/// Returns true when `s` is a machine-generated placeholder rather than real
/// content — filenames with timestamps, "Screen capture (visual): X.png. App",
/// "Captured recent activity at HH:MM", etc.
fn is_template_summary(s: &str) -> bool {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return true;
    }
    let lower = trimmed.to_ascii_lowercase();

    // Filename with embedded timestamp (heuristic: contains .png AND 6+ digits)
    let has_png = lower.contains(".png");
    let digit_count = lower.chars().filter(|c| c.is_ascii_digit()).count();
    if has_png && digit_count >= 6 {
        return true;
    }

    lower.starts_with("screen capture (visual)")
        || lower.starts_with("captured recent")
        || lower.starts_with("viewed content on")
        || lower.starts_with("url-only surface capture")
        || (lower.starts_with("viewed ") && lower.contains(" at "))
}

/// Strip redundant repetition of app/project names from a candidate insight
/// string. Avoids fluff like "Google Chrome - Google Chrome - Google Chrome".
fn dedupe_repeating_phrases(s: &str) -> String {
    // Conservative pass: collapse runs of identical comma/dash/colon separated
    // tokens (case-insensitive). Keeps the first occurrence.
    let parts: Vec<&str> = s.split(['|', '·', '—']).collect();
    if parts.len() < 2 {
        return s.to_string();
    }
    let mut seen = std::collections::HashSet::new();
    let mut kept: Vec<&str> = Vec::new();
    for p in parts {
        let key = p.trim().to_ascii_lowercase();
        if key.is_empty() {
            continue;
        }
        if seen.insert(key) {
            kept.push(p.trim());
        }
    }
    kept.join(" — ")
}

/// Display label for an `activity_type`. Stored values are identifiers such as
/// `reviewing_agent_output` and must never reach a sentence as they are.
fn activity_label(activity: &str) -> Option<String> {
    let label = match activity.trim().to_ascii_lowercase().as_str() {
        "" | "unknown" | "other" => return None,
        "coding" => "Coding",
        "debugging" => "Debugging",
        "reviewing_agent_output" => "Reviewing agent output",
        "researching" => "Research",
        "planning" => "Planning",
        "writing" => "Writing",
        "studying" => "Studying",
        "watching_or_listening" => "Watching or listening",
        "configuring_tool" => "Tool setup",
        "testing_workflow" => "Testing",
        "reading_results" => "Reading results",
        "organizing_information" => "Organizing information",
        "communication" => "Communication",
        "job_or_career_work" => "Career work",
        "travel_or_logistics" => "Travel and logistics",
        "entertainment_or_personal_interest" => "Personal interest",
        other => return Some(capitalize_first(&other.replace('_', " "))),
    };
    Some(label.to_string())
}

/// Build a neutral what-happened line from structured metadata, without
/// touching OCR text or display_summary. The most specific subject wins.
fn coherent_what_happened_from_metadata(record: &MemoryRecord) -> String {
    let win = record.window_title.trim();
    let app = record.app_name.trim();
    let topic = record.topic.trim();
    let entities: Vec<&str> = record
        .entities
        .iter()
        .filter(|e| !e.trim().is_empty())
        .take(2)
        .map(|s| s.as_str())
        .collect();

    let win_is_meaningful = !win.is_empty() && !win.eq_ignore_ascii_case(app) && win.len() <= 120;

    // The app name is left out because the app chip already shows it.
    let subject = if win_is_meaningful {
        win.to_string()
    } else if !entities.is_empty() {
        entities.join(", ")
    } else if !topic.is_empty() && topic != "unknown" {
        topic.to_string()
    } else {
        String::new()
    };
    let subject = subject.trim_end_matches('.');

    match (activity_label(&record.activity_type), subject.is_empty()) {
        (Some(activity), false) => format!("{activity}: {subject}."),
        (Some(activity), true) => format!("{activity}."),
        (None, false) => format!("{subject}."),
        (None, true) => String::new(),
    }
}

/// True when two lines carry the same words, so showing both adds nothing.
fn says_the_same(a: &str, b: &str) -> bool {
    let normalize = |s: &str| {
        s.to_lowercase()
            .chars()
            .filter(|c| c.is_alphanumeric() || c.is_whitespace())
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    };
    let (a, b) = (normalize(a), normalize(b));
    !a.is_empty() && !b.is_empty() && (a.contains(&b) || b.contains(&a))
}

/// Build a why-mattered sentence from structured signals.
/// Order: decisions > errors > blockers > first sentence of memory_context
/// (only when it adds to what-happened) > a stated goal. Empty beats filler.
fn coherent_why_mattered_from_metadata(record: &MemoryRecord, what_happened: &str) -> String {
    // Prefer concrete structured signals that imply significance
    if let Some(d) = record
        .decisions
        .iter()
        .find(|s| !s.trim().is_empty() && !is_template_summary(s))
    {
        return d.chars().take(MAX_WHY_CHARS).collect();
    }
    if let Some(e) = record.errors.iter().find(|s| !s.trim().is_empty()) {
        return format!(
            "Encountered error: {}",
            e.chars().take(MAX_WHY_CHARS - 20).collect::<String>()
        );
    }
    if let Some(b) = record.blockers.iter().find(|s| !s.trim().is_empty()) {
        return format!(
            "Blocked on: {}",
            b.chars().take(MAX_WHY_CHARS - 12).collect::<String>()
        );
    }

    // Use memory_context first sentence ONLY if it's a real narrative
    let ctx_first = crate::summariser::sentences::first_sentence(&record.memory_context);
    if !ctx_first.is_empty()
        && !is_template_summary(ctx_first)
        && ctx_first.split_whitespace().count() >= 5
        && !says_the_same(ctx_first, what_happened)
    {
        return ctx_first.chars().take(MAX_WHY_CHARS).collect();
    }

    let intent = record.user_intent.trim().trim_end_matches('.');
    if !intent.is_empty() && intent != "unknown" {
        return format!("Goal: {intent}.");
    }

    String::new()
}

fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().chain(chars).collect(),
    }
}

fn clip_chars(s: String, max: usize) -> String {
    if s.chars().count() <= max {
        return s;
    }
    let trimmed: String = s.chars().take(max.saturating_sub(1)).collect();
    let trimmed = trimmed.trim_end_matches([',', ';', '—', '-', ' ']);
    format!("{}…", trimmed)
}

/// Populate `insight_*` when layers are still empty (idempotent for pre-filled rows).
pub fn derive_insight_for_record(record: &mut MemoryRecord) {
    if !record.insight_what_happened.trim().is_empty() {
        // Even when synthesis pre-filled the insight fields, strip fluff
        // (redundant app/project mentions, "the user is..." preambles).
        apply_fluff_strip(record);
        return;
    }

    let spans = rank_salient_spans(&record.clean_text, &record.app_name);
    let top: Vec<&SalientSpan> = spans.iter().take(TOP_SPAN_DEBUG).collect();

    let mut dropped: Vec<String> = Vec::new();
    for s in spans.iter().skip(TOP_SPAN_DEBUG) {
        if s.score < DROP_SCORE {
            dropped.push(s.text.chars().take(200).collect());
        }
    }

    // --- what_happened ---
    // Priority: display_summary (if not a template/filename) → metadata construction → snippet
    let what = {
        let candidate = record.display_summary.trim().to_string();
        if !candidate.is_empty() && !is_template_summary(&candidate) {
            dedupe_repeating_phrases(&candidate)
        } else {
            let from_meta = coherent_what_happened_from_metadata(record);
            if !from_meta.is_empty() {
                from_meta
            } else if !record.snippet.trim().is_empty() && !is_template_summary(&record.snippet) {
                record.snippet.trim().to_string()
            } else {
                String::new()
            }
        }
    };
    record.insight_what_happened = clip_chars(what, MAX_WHAT_CHARS);

    // --- why_mattered ---
    // A salient OCR line is evidence of what was on screen, not a reason, so
    // it is never promoted to this field. Empty is the honest answer.
    let why = coherent_why_mattered_from_metadata(record, &record.insight_what_happened);
    record.insight_why_mattered = clip_chars(why, MAX_WHY_CHARS);

    // --- what_changed ---
    // Outcomes only: results, decisions not already shown above, and the files
    // involved. Next steps are future work and stay on their own field.
    let mut changed: Vec<String> = record
        .results
        .iter()
        .chain(record.decisions.iter())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty() && !says_the_same(s, &record.insight_why_mattered))
        .map(str::to_string)
        .collect();
    let files: Vec<&str> = record
        .files_touched
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    if !files.is_empty() {
        changed.push(format!("Files: {}", files.join(", ")));
    }
    record.insight_what_changed = clip_chars(changed.join("; "), MAX_CHANGED_CHARS);

    // --- context_thread (only real links; a session id is not a thread) ---
    match record.related_memory_ids.len() {
        0 => {}
        1 => record.insight_context_thread = "1 linked memory".to_string(),
        n => record.insight_context_thread = format!("{n} linked memories"),
    }

    let pollution = pollution_for_insight(record);
    let salience = spans
        .first()
        .map(|s| s.score)
        .unwrap_or(0.0)
        .clamp(0.0, 1.0);
    record.insight_card_confidence =
        (salience * (1.0 - pollution) * record.ocr_confidence.clamp(0.0, 1.0)).clamp(0.0, 1.0);

    record.insight_spans_json = serde_json::json!({
        "top": top.iter().map(|s| serde_json::json!({"text": s.text, "score": s.score})).collect::<Vec<_>>(),
        "dropped": dropped
    })
    .to_string();

    apply_fluff_strip(record);
}

/// Run fluff stripping on the insight fields, using the record's own metadata
/// to identify redundant terms. Idempotent — safe to call multiple times.
fn apply_fluff_strip(record: &mut MemoryRecord) {
    let domain = record
        .url
        .as_deref()
        .and_then(|u| {
            u.split("://")
                .nth(1)
                .unwrap_or(u)
                .split('/')
                .next()
                .map(|h| h.trim_start_matches("www.").to_string())
        })
        .unwrap_or_default();

    let app = record.app_name.clone();
    let project = record.project.clone();

    if !record.insight_what_happened.is_empty() {
        record.insight_what_happened = crate::memory_insight::fluff::strip_fluff(
            &record.insight_what_happened,
            &app,
            &project,
            &domain,
        );
    }
    if !record.insight_why_mattered.is_empty() {
        record.insight_why_mattered = crate::memory_insight::fluff::strip_fluff(
            &record.insight_why_mattered,
            &app,
            &project,
            &domain,
        );
    }
    if !record.insight_what_changed.is_empty() {
        record.insight_what_changed = crate::memory_insight::fluff::strip_fluff(
            &record.insight_what_changed,
            &app,
            &project,
            &domain,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> MemoryRecord {
        MemoryRecord {
            ocr_confidence: 0.85,
            ..MemoryRecord::default()
        }
    }

    #[test]
    fn is_template_catches_filename_patterns() {
        assert!(is_template_summary(
            "Screen capture (visual): Claude_1778938598807.png. Claude"
        ));
        assert!(is_template_summary("Captured recent activity at 08:30 AM"));
        assert!(is_template_summary(
            "URL-only surface capture for x.com at 12:00 PM"
        ));
        assert!(is_template_summary(""));
        assert!(!is_template_summary(
            "You watched the IPL match on Willow TV"
        ));
        assert!(!is_template_summary(
            "Reviewed authentication PR for security correctness"
        ));
    }

    #[test]
    fn what_happened_strips_filename_pattern() {
        let mut r = base();
        r.app_name = "Claude".to_string();
        r.window_title = "Claude".to_string();
        r.display_summary = "Screen capture (visual): Claude_1778938598807.png. Claude".to_string();
        derive_insight_for_record(&mut r);
        assert!(
            !r.insight_what_happened.contains(".png"),
            "filename leaked: {}",
            r.insight_what_happened
        );
        assert!(
            !r.insight_what_happened
                .to_lowercase()
                .contains("screen capture"),
            "template leaked: {}",
            r.insight_what_happened
        );
    }

    #[test]
    fn what_happened_uses_window_title_when_more_specific_than_app() {
        let mut r = base();
        r.app_name = "Google Chrome".to_string();
        r.window_title = "React hooks documentation - MDN".to_string();
        r.activity_type = "research".to_string();
        derive_insight_for_record(&mut r);
        let w = &r.insight_what_happened;
        assert!(
            w.contains("React") || w.contains("MDN"),
            "specific window content missing: {}",
            w
        );
        // Should NOT contain the app name redundantly (chip already shows it)
        assert!(
            !w.contains("Google Chrome"),
            "app name leaked into what_happened: {}",
            w
        );
    }

    #[test]
    fn what_happened_falls_back_to_entities_when_no_window_title() {
        let mut r = base();
        r.app_name = "Terminal".to_string();
        r.window_title = "Terminal".to_string();
        r.entities = vec!["cargo build".to_string(), "release target".to_string()];
        r.activity_type = "building".to_string();
        derive_insight_for_record(&mut r);
        assert_eq!(
            r.insight_what_happened,
            "Building: cargo build, release target."
        );
    }

    #[test]
    fn what_happened_never_shows_an_activity_identifier_or_a_narrator() {
        let mut r = base();
        r.app_name = "Google Chrome".to_string();
        r.window_title = "Fix retrieval ranking by anurup · Pull Request #42".to_string();
        r.activity_type = "reviewing_agent_output".to_string();
        r.user_intent = "review the PR".to_string();
        derive_insight_for_record(&mut r);
        assert_eq!(
            r.insight_what_happened,
            "Reviewing agent output: Fix retrieval ranking by anurup · Pull Request #42."
        );
        assert_eq!(r.insight_why_mattered, "Goal: review the PR.");
    }

    #[test]
    fn why_mattered_does_not_repeat_what_happened() {
        let mut r = base();
        r.display_summary = "Compared three embedding models on the seeded vault.".to_string();
        r.memory_context =
            "Compared three embedding models on the seeded vault. MiniLM stayed active."
                .to_string();
        derive_insight_for_record(&mut r);
        assert!(r
            .insight_what_happened
            .starts_with("Compared three embedding"));
        assert_eq!(r.insight_why_mattered, "");
    }

    #[test]
    fn what_changed_lists_outcomes_and_files_not_next_steps() {
        let mut r = base();
        r.window_title = "retrieval notes".to_string();
        r.results = vec!["Recall@5 rose from 0.71 to 0.83".to_string()];
        r.next_steps = vec!["identify key themes in the video summary".to_string()];
        r.files_touched = vec!["hybrid.rs".to_string(), "reranker.rs".to_string()];
        derive_insight_for_record(&mut r);
        assert_eq!(
            r.insight_what_changed,
            "Recall@5 rose from 0.71 to 0.83; Files: hybrid.rs, reranker.rs"
        );
    }

    #[test]
    fn thread_is_empty_without_real_links() {
        let mut r = base();
        r.window_title = "retrieval notes".to_string();
        r.session_id = "9f3c2a1b7d".to_string();
        derive_insight_for_record(&mut r);
        assert_eq!(r.insight_context_thread, "");

        let mut linked = base();
        linked.window_title = "retrieval notes".to_string();
        linked.related_memory_ids = vec!["m-1".to_string()];
        derive_insight_for_record(&mut linked);
        assert_eq!(linked.insight_context_thread, "1 linked memory");
    }

    #[test]
    fn why_mattered_prefers_decisions_over_ocr_span() {
        let mut r = base();
        r.clean_text = "irrelevant noisy OCR text that shouldnt appear in why_mattered".to_string();
        r.decisions = vec!["Switched to JWT for stateless auth".to_string()];
        derive_insight_for_record(&mut r);
        assert!(
            r.insight_why_mattered.contains("JWT"),
            "got: {}",
            r.insight_why_mattered
        );
    }

    #[test]
    fn why_mattered_does_not_dump_short_ocr_span() {
        let mut r = base();
        r.app_name = "Chrome".to_string();
        // Short OCR fragment that's not a real explanation
        r.clean_text = "Page Title".to_string();
        derive_insight_for_record(&mut r);
        // Should be empty or a constructed sentence, not "Page Title"
        assert!(
            r.insight_why_mattered.is_empty()
                || r.insight_why_mattered.split_whitespace().count() >= 4,
            "got short fragment: {}",
            r.insight_why_mattered
        );
    }

    #[test]
    fn dedupe_repeating_phrases_collapses_runs() {
        assert_eq!(
            dedupe_repeating_phrases("Google Chrome — Google Chrome — News article"),
            "Google Chrome — News article"
        );
        // No separators, no change
        assert_eq!(
            dedupe_repeating_phrases("You watched the match"),
            "You watched the match"
        );
    }

    #[test]
    fn idempotent_when_what_happened_already_set() {
        let mut r = base();
        r.insight_what_happened = "Pre-filled by LLM synthesis.".to_string();
        let before = r.insight_what_happened.clone();
        derive_insight_for_record(&mut r);
        assert_eq!(r.insight_what_happened, before);
    }

    #[test]
    fn fluff_strip_runs_on_prefilled_insights() {
        let mut r = base();
        r.app_name = "Google Chrome".to_string();
        // LLM produced fluff-heavy output with repeated app name
        r.insight_what_happened = "Google Chrome — Google Chrome news article on AI".to_string();
        derive_insight_for_record(&mut r);
        // App name should appear at most once
        let count = r.insight_what_happened.matches("Google Chrome").count();
        assert_eq!(
            count, 1,
            "expected one app mention, got: {}",
            r.insight_what_happened
        );
    }

    #[test]
    fn fluff_strip_drops_the_user_prefix_from_llm_output() {
        let mut r = base();
        r.insight_what_happened = "the user is debugging a Rust borrow error".to_string();
        derive_insight_for_record(&mut r);
        assert!(
            !r.insight_what_happened
                .to_lowercase()
                .starts_with("the user"),
            "the user prefix leaked: {}",
            r.insight_what_happened
        );
    }
}
