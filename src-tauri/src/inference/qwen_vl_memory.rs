use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize)]
pub enum MemorySourceType {
    Screen,
    GlassesImport,
    FileImport,
}

impl MemorySourceType {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Screen => "screen",
            Self::GlassesImport => "glasses_import",
            Self::FileImport => "file_import",
        }
    }
}

#[derive(Debug, Clone)]
pub struct MemorySynthesisInput {
    pub image_path: Option<PathBuf>,
    pub ocr_text: String,
    pub app_name: Option<String>,
    pub window_title: Option<String>,
    pub url: Option<String>,
    pub timestamp: DateTime<Utc>,
    pub source_type: MemorySourceType,
    pub ocr_confidence: Option<f32>,
}

#[derive(Debug, Clone, Default)]
pub struct MemorySynthesisOutput {
    pub memory_context: String,
    pub summary_short: String,
    pub topic: Option<String>,
    pub activity_type: Option<String>,
    pub user_intent: Option<String>,
    pub entities: Vec<String>,
    pub files: Vec<String>,
    pub urls: Vec<String>,
    pub decisions: Vec<String>,
    pub errors: Vec<String>,
    pub next_steps: Vec<String>,
    pub search_aliases: Vec<String>,
    pub confidence_score: f32,
    pub importance_score: f32,
}

pub const MEMORY_SYNTHESIS_PROMPT: &str = r#"You are FNDR's local memory extraction model.

Create a structured memory from the user's screen.

Use all available evidence:
- screenshot image (if provided)
- OCR text
- app name
- window title
- URL
- timestamp
- source type

Rules:
- Do not invent details not supported by the evidence.
- Prefer concrete nouns, app names, file names, URLs, project names, commands, errors, decisions, todos, and next steps.
- memory_context should help a future AI agent understand what the user was doing, why it mattered, and what context should be remembered.
- Use OCR as primary text evidence.
- Use the image for layout, visual context, screenshots, logos, diagrams, UI state, and image-heavy screens.
- If evidence is weak, lower confidence_score.
- Avoid storing unnecessary sensitive raw text.
- Return JSON only. No markdown. No explanation.

Required JSON schema:
{
  "memory_context": "...",
  "summary_short": "...",
  "topic": "...",
  "activity_type": "...",
  "user_intent": "...",
  "entities": [],
  "files": [],
  "urls": [],
  "decisions": [],
  "errors": [],
  "next_steps": [],
  "search_aliases": [],
  "confidence_score": 0.0,
  "importance_score": 0.0
}"#;

#[derive(Debug, Deserialize, Default)]
struct SynthesisJsonRow {
    #[serde(default)]
    memory_context: String,
    #[serde(default)]
    summary_short: String,
    #[serde(default)]
    topic: Option<String>,
    #[serde(default)]
    activity_type: Option<String>,
    #[serde(default)]
    user_intent: Option<String>,
    #[serde(default)]
    entities: Vec<String>,
    #[serde(default)]
    files: Vec<String>,
    #[serde(default)]
    urls: Vec<String>,
    #[serde(default)]
    decisions: Vec<String>,
    #[serde(default)]
    errors: Vec<String>,
    #[serde(default)]
    next_steps: Vec<String>,
    #[serde(default)]
    search_aliases: Vec<String>,
    #[serde(default)]
    confidence_score: f32,
    #[serde(default)]
    importance_score: f32,
}

pub fn build_user_prompt(input: &MemorySynthesisInput) -> String {
    let mut parts = Vec::new();
    parts.push(format!("Source: {}", input.source_type.label()));
    parts.push(format!("Timestamp: {}", input.timestamp.to_rfc3339()));
    if let Some(ref app) = input.app_name {
        parts.push(format!("App: {app}"));
    }
    if let Some(ref title) = input.window_title {
        parts.push(format!("Window: {title}"));
    }
    if let Some(ref url) = input.url {
        parts.push(format!("URL: {url}"));
    }
    if let Some(conf) = input.ocr_confidence {
        parts.push(format!("OCR confidence: {conf:.2}"));
    }
    if !input.ocr_text.trim().is_empty() {
        let excerpt: String = input.ocr_text.chars().take(2400).collect();
        parts.push(format!("OCR text:\n{excerpt}"));
    }
    if input.image_path.is_some() {
        parts.push("[screenshot attached]".to_string());
    }
    parts.join("\n")
}

pub fn parse_synthesis_json(raw: &str) -> Result<MemorySynthesisOutput, String> {
    let trimmed = strip_markdown_fence(raw);
    let slice = extract_json_object(&trimmed)
        .ok_or_else(|| {
            let preview: String = trimmed.chars().take(200).collect();
            format!("no JSON object in output: {preview}")
        })?;
    let row: SynthesisJsonRow = serde_json::from_str(slice)
        .map_err(|e| format!("JSON parse: {e}"))?;
    if row.memory_context.trim().is_empty() {
        return Err("memory_context is empty".to_string());
    }
    Ok(MemorySynthesisOutput {
        memory_context: clamp(row.memory_context, 2000),
        summary_short: clamp(row.summary_short, 280),
        topic: row.topic.map(|s| clamp(s, 120)),
        activity_type: row.activity_type.map(|s| clamp(s, 80)),
        user_intent: row.user_intent.map(|s| clamp(s, 200)),
        entities: sanitize_list(row.entities, 20, 64),
        files: sanitize_list(row.files, 16, 120),
        urls: sanitize_list(row.urls, 12, 200),
        decisions: sanitize_list(row.decisions, 12, 200),
        errors: sanitize_list(row.errors, 12, 200),
        next_steps: sanitize_list(row.next_steps, 12, 200),
        search_aliases: sanitize_list(row.search_aliases, 24, 48),
        confidence_score: row.confidence_score.clamp(0.0, 1.0),
        importance_score: row.importance_score.clamp(0.0, 1.0),
    })
}

/// Build a fallback MemorySynthesisOutput from metadata when Qwen is unavailable.
pub fn synthesis_ocr_only_fallback(input: &MemorySynthesisInput) -> MemorySynthesisOutput {
    let app = input.app_name.as_deref().unwrap_or("").trim();
    let title = input.window_title.as_deref().unwrap_or("").trim();
    let url_str = input.url.as_deref().unwrap_or("");

    let memory_context = if !title.is_empty() && !app.is_empty() {
        format!("{app} — {title}. {}", input.ocr_text.chars().take(600).collect::<String>())
    } else if !input.ocr_text.trim().is_empty() {
        input.ocr_text.chars().take(800).collect()
    } else {
        format!("Screen capture: {app} {title}")
    };

    let summary_short = if !title.is_empty() {
        if !app.is_empty() { format!("{app}: {title}") } else { title.to_string() }
    } else {
        app.to_string()
    };

    let mut urls = Vec::new();
    if !url_str.is_empty() { urls.push(url_str.to_string()); }

    MemorySynthesisOutput {
        memory_context,
        summary_short,
        topic: if !title.is_empty() { Some(title.to_string()) } else { None },
        activity_type: Some("observing".to_string()),
        user_intent: None,
        entities: Vec::new(),
        files: Vec::new(),
        urls,
        decisions: Vec::new(),
        errors: Vec::new(),
        next_steps: Vec::new(),
        search_aliases: Vec::new(),
        confidence_score: 0.30,
        importance_score: 0.30,
    }
}

fn strip_markdown_fence(s: &str) -> String {
    let t = s.trim();
    if let Some(rest) = t.strip_prefix("```json") {
        return rest.trim_end_matches('`').trim().to_string();
    }
    if let Some(rest) = t.strip_prefix("```") {
        return rest.trim_end_matches('`').trim().to_string();
    }
    t.to_string()
}

fn extract_json_object(s: &str) -> Option<&str> {
    let start = s.find('{')?;
    let end = s.rfind('}')?;
    if end > start { Some(&s[start..=end]) } else { None }
}

fn clamp(mut s: String, max_chars: usize) -> String {
    s.retain(|c| c != '\0');
    if s.chars().count() <= max_chars { s }
    else { s.chars().take(max_chars.saturating_sub(1)).collect::<String>() + "…" }
}

fn sanitize_list(mut v: Vec<String>, max_items: usize, max_each: usize) -> Vec<String> {
    v.retain(|s| !s.trim().is_empty());
    v.truncate(max_items);
    v.into_iter().map(|s| clamp(s, max_each)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_synthesis_json() {
        let raw = r#"{"memory_context":"User reviewed PRs on GitHub","summary_short":"GitHub PR review","topic":"code review","activity_type":"reviewing","user_intent":"review pull requests","entities":["GitHub","PR #42"],"files":[],"urls":["https://github.com"],"decisions":[],"errors":[],"next_steps":["merge PR #42"],"search_aliases":["PR review","GitHub"],"confidence_score":0.85,"importance_score":0.7}"#;
        let out = parse_synthesis_json(raw).unwrap();
        assert_eq!(out.summary_short, "GitHub PR review");
        assert!(!out.memory_context.is_empty());
        assert!((out.confidence_score - 0.85).abs() < 0.01);
        assert_eq!(out.next_steps, vec!["merge PR #42"]);
    }

    #[test]
    fn tolerates_markdown_fence() {
        let raw = "```json\n{\"memory_context\":\"test\",\"summary_short\":\"t\",\"confidence_score\":0.5,\"importance_score\":0.4}\n```";
        let out = parse_synthesis_json(raw).unwrap();
        assert_eq!(out.memory_context, "test");
    }

    #[test]
    fn rejects_empty_memory_context() {
        let raw = r#"{"memory_context":"","summary_short":"x","confidence_score":0.5,"importance_score":0.4}"#;
        assert!(parse_synthesis_json(raw).is_err());
    }

    #[test]
    fn ocr_only_fallback_produces_valid_output() {
        let input = MemorySynthesisInput {
            image_path: None,
            ocr_text: "def main(): pass".to_string(),
            app_name: Some("VS Code".to_string()),
            window_title: Some("main.py".to_string()),
            url: None,
            timestamp: chrono::Utc::now(),
            source_type: MemorySourceType::Screen,
            ocr_confidence: Some(0.75),
        };
        let out = synthesis_ocr_only_fallback(&input);
        assert!(out.summary_short.contains("VS Code"));
        assert!(out.confidence_score < 0.55);
        assert!(!out.memory_context.is_empty());
    }

    #[test]
    fn required_fields_present_in_output() {
        let out = MemorySynthesisOutput {
            memory_context: "ctx".to_string(),
            summary_short: "sum".to_string(),
            confidence_score: 0.8,
            importance_score: 0.6,
            ..Default::default()
        };
        assert!(!out.memory_context.is_empty());
        assert!(!out.summary_short.is_empty());
        assert!(out.confidence_score >= 0.0 && out.confidence_score <= 1.0);
    }
}
