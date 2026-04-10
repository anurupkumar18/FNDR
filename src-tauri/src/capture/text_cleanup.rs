//! Drop obvious browser chrome from OCR text before embeddings and storage.
//!
//! Vision still sees the full screenshot; we only trim lines that usually come from
//! tab strips and compact toolbar captions so memory records favor page content,
//! titles, and body text already kept by the OCR noise filter.

/// Match the default OCR `min_line_length` so we do not resurrect junk lines.
const MIN_LINE_LEN: usize = 7;

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
}
