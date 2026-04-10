//! Drop obvious browser chrome from OCR text before embeddings and storage.
//!
//! Vision still sees the full screenshot; we only trim lines that usually come from
//! tab strips and compact toolbar captions so memory records favor page content,
//! titles, and body text already kept by the OCR noise filter.

/// Match the default OCR `min_line_length` so we do not resurrect junk lines.
const MIN_LINE_LEN: usize = 7;
const MAX_FALLBACK_SNIPPET_CHARS: usize = 140;

/// Lines with several middots and short segments are almost always Safari/Chrome tab rows.
fn looks_like_tab_strip_line(line: &str) -> bool {
    let dots = line.matches('·').count();
    if dots < 2 {
        return false;
    }
    let segments: Vec<usize> = line.split('·').map(|s| s.trim().len()).collect();
    if segments.is_empty() {
        return false;
    }
    let max_seg = *segments.iter().max().unwrap_or(&0);
    max_seg <= 42 && line.len() <= 220
}

/// Same idea for toolbars that OCR as "A | B | C" with short labels.
fn looks_like_pipe_tab_row(line: &str) -> bool {
    let pipes = line.matches('|').count();
    if pipes < 2 {
        return false;
    }
    let segments: Vec<usize> = line.split('|').map(|s| s.trim().len()).collect();
    if segments.len() < 3 {
        return false;
    }
    let max_seg = *segments.iter().max().unwrap_or(&0);
    max_seg <= 36 && line.len() <= 200
}

/// Very short lines that are almost always window or browser chrome (conservative).
fn is_compact_chrome_caption(line: &str) -> bool {
    if line.len() > 56 {
        return false;
    }
    let lower = line.to_lowercase();
    // OCR often glues adjacent toolbar labels into one token.
    if matches!(lower.as_str(), "backforward" | "forwardback") {
        return true;
    }
    lower.contains("back")
        && lower.contains("forward")
        && lower.len() < 36
        && (lower.contains("reload") || lower.contains("refresh"))
}

fn is_separator_line(line: &str) -> bool {
    !line.is_empty()
        && line
            .chars()
            .all(|ch| ch == '-' || ch == '_' || ch == '=' || ch == '.' || ch == ' ')
}

fn symbol_ratio(line: &str) -> f32 {
    let chars: Vec<char> = line.chars().collect();
    if chars.is_empty() {
        return 1.0;
    }
    let symbol_count = chars
        .iter()
        .filter(|ch| !ch.is_alphanumeric() && !ch.is_whitespace())
        .count();
    symbol_count as f32 / chars.len() as f32
}

fn looks_like_file_inventory(line: &str) -> bool {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    if tokens.len() < 4 {
        return false;
    }

    let pathish = tokens
        .iter()
        .filter(|token| {
            let token = token.trim_matches(|ch: char| ",;:()[]{}".contains(ch));
            token.contains('/')
                || token.contains('\\')
                || (token.contains('.')
                    && (token.contains('_') || token.contains('-') || token.ends_with(".rs")))
        })
        .count();

    pathish >= 3
}

fn normalize_inline(line: &str) -> String {
    line.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn truncate_snippet(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let keep = max_chars.saturating_sub(3);
    let mut out: String = text.chars().take(keep).collect();
    out.push_str("...");
    out
}

fn title_is_generic_for_app(app_name: &str, title: &str) -> bool {
    let title_lower = title.to_lowercase();
    let app_lower = app_name.to_lowercase();

    if !app_lower.is_empty() && title_lower == app_lower {
        return true;
    }

    matches!(
        title_lower.as_str(),
        "new tab" | "untitled" | "home" | "settings" | "preferences" | "dashboard" | "start page"
    )
}

fn is_useful_snippet_line(app_name: &str, line: &str) -> bool {
    let normalized = normalize_inline(line);
    if normalized.len() < MIN_LINE_LEN {
        return false;
    }
    if normalized.len() > 220 {
        return false;
    }
    if is_separator_line(&normalized) {
        return false;
    }
    if looks_like_tab_strip_line(&normalized)
        || looks_like_pipe_tab_row(&normalized)
        || is_compact_chrome_caption(&normalized)
    {
        return false;
    }
    if looks_like_file_inventory(&normalized) {
        return false;
    }
    if symbol_ratio(&normalized) > 0.34 {
        return false;
    }
    if title_is_generic_for_app(app_name, &normalized) {
        return false;
    }
    true
}

/// Build a compact fallback snippet when model summarization is unavailable.
pub fn concise_fallback_snippet(app_name: &str, window_title: &str, text: &str) -> String {
    let normalized_title = normalize_inline(window_title.trim());
    if !normalized_title.is_empty() && is_useful_snippet_line(app_name, &normalized_title) {
        return truncate_snippet(&normalized_title, MAX_FALLBACK_SNIPPET_CHARS);
    }

    for line in text.lines() {
        if is_useful_snippet_line(app_name, line) {
            return truncate_snippet(&normalize_inline(line), MAX_FALLBACK_SNIPPET_CHARS);
        }
    }

    if !normalized_title.is_empty() {
        return truncate_snippet(&normalized_title, MAX_FALLBACK_SNIPPET_CHARS);
    }

    if !app_name.trim().is_empty() {
        return format!("Using {}", app_name.trim());
    }

    String::new()
}

/// Remove noisy lines; keep structure and duplicates handled upstream in OCR when possible.
pub fn reduce_chrome_noise(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut prev = String::new();

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.len() < MIN_LINE_LEN {
            continue;
        }
        if looks_like_tab_strip_line(trimmed)
            || looks_like_pipe_tab_row(trimmed)
            || is_compact_chrome_caption(trimmed)
        {
            tracing::trace!("Dropped likely browser chrome line from capture text");
            continue;
        }
        if trimmed == prev.as_str() {
            continue;
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(trimmed);
        prev = trimmed.to_string();
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_tab_strip_middots() {
        let raw = "Project roadmap for Q2\nGmail · Calendar · Drive · GitHub\nActual paragraph content here";
        let cleaned = reduce_chrome_noise(raw);
        assert!(cleaned.contains("Project roadmap"));
        assert!(cleaned.contains("Actual paragraph"));
        assert!(!cleaned.contains("Gmail"));
        assert!(!cleaned.contains("Calendar"));
    }

    #[test]
    fn keeps_content_with_single_middot() {
        let raw = "Notes · Implementation details for the API";
        let cleaned = reduce_chrome_noise(raw);
        assert!(cleaned.contains("Implementation details"));
    }

    #[test]
    fn drops_pipe_tab_style_row() {
        let raw = "Intro line\nA | B | C | D\nReal content starts here";
        let cleaned = reduce_chrome_noise(raw);
        assert!(cleaned.contains("Intro line"));
        assert!(cleaned.contains("Real content"));
        assert!(!cleaned.contains("| B |"));
    }

    #[test]
    fn drops_glued_back_forward() {
        let raw = "BackForward\nMain article text continues";
        let cleaned = reduce_chrome_noise(raw);
        assert!(!cleaned.contains("BackForward"));
        assert!(cleaned.contains("Main article"));
    }

    #[test]
    fn keeps_normal_sentence_with_back_and_forward() {
        let raw = "You can go back forward through the tutorial steps easily";
        let cleaned = reduce_chrome_noise(raw);
        assert!(cleaned.contains("tutorial"));
    }

    #[test]
    fn fallback_prefers_window_title() {
        let snippet = concise_fallback_snippet(
            "VSCode",
            "fndr - download_model.sh",
            "src app.rs src/lib.rs src/main.rs src-tauri/src/graph/mod.rs",
        );
        assert_eq!(snippet, "fndr - download_model.sh");
    }

    #[test]
    fn fallback_skips_file_inventory_lines() {
        let snippet = concise_fallback_snippet(
            "Terminal",
            "Terminal",
            "src/app.tsx src/lib.rs src/main.rs src-tauri/src/store/schema.rs\nFix memory summarization for OCR snippets",
        );
        assert_eq!(snippet, "Fix memory summarization for OCR snippets");
    }

    #[test]
    fn fallback_uses_app_name_as_last_resort() {
        let snippet = concise_fallback_snippet("Chrome", "", "---- --- ---");
        assert_eq!(snippet, "Using Chrome");
    }
}
