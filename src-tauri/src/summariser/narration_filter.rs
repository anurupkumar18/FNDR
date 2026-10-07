use once_cell::sync::Lazy;
use regex::Regex;

use super::display_summary::{build_display_summary, clean_sentence, fallback_display_summary};

static BANNED_PATTERNS: Lazy<Vec<Regex>> = Lazy::new(|| {
    [
        r"(?i)^you reviewed",
        r"(?i)^user viewed",
        r"(?i)^the user is viewing",
        r"(?i)^the ocr text indicates",
        r"(?i)\bvisual-only frame\b",
        r"(?i)\bno visible content\b",
        r"(?i)^screen capture shows",
        r"(?i)noting\s+[A-Z]",
        r"(?i)then\s+you",
        r"(?i)\buser\.$",
        r"(?i)memory_compaction",
        r"(?i)src-tauri",
    ]
    .iter()
    .map(|pattern| Regex::new(pattern).expect("valid narration filter regex"))
    .collect()
});

static INSTRUCTION_PATTERNS: Lazy<Vec<Regex>> = Lazy::new(|| {
    [r"(?i)^(?:extract|analy[sz]e|identify|summari[sz]e|describe|provide|generate)\b.{0,200}\b(?:ocr text|ocr content|key themes|narrative elements|video summary|content from the screen)\b"]
        .iter()
        .map(|pattern| Regex::new(pattern).expect("valid summary instruction regex"))
        .collect()
});

static SCRUB_PATTERNS: Lazy<Vec<Regex>> = Lazy::new(|| {
    [
        r"(?i)^you reviewed\s+",
        r"(?i)^user viewed\s+",
        r"(?i)^the user is viewing\s+",
        r"(?i)^the ocr text indicates\s+",
        r"(?i)\bvisual-only frame\b",
        r"(?i)\bno visible content\b",
        r"(?i)^screen capture shows\s+",
        r"(?i)^then\s+you\s+",
        r"(?i)\bnoting\s+[A-Z][^,.!?]*",
        r"(?i)\bmemory_compaction\b",
        r"(?i)\bsrc-tauri\b",
    ]
    .iter()
    .map(|pattern| Regex::new(pattern).expect("valid narration scrub regex"))
    .collect()
});

static LEADING_PERSON: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)^\s*(?:the\s+user|you)\s+(?:(?:is|was|are|were|has been|have been|had been)\s+)?",
    )
    .expect("valid leading person regex")
});

// A bare "user" is a narrator only before a verb ("User opened VS Code",
// "User is debugging", "User checks the logs"), never before a noun ("User
// guide", "User testing", "User settings").
static LEADING_BARE_USER: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)^\s*user\s+(?:(?:is|was|has been|had been)\s+([a-z]+)|([a-z]+ed)\b|((?:checks|discusses|reviews|views|opens|reads|writes|runs|asks|uses|works|manages|managing|discussing|reviewing|viewing|working|checking|asking|using)\b))")
        .expect("valid bare user regex")
});

// Openers that describe the screen or the assistant instead of the work:
// "Claude is responding with updates on X", "The screen shows a chat
// interface with multiple lines of text related to X". Only X is kept.
static NARRATION_OPENERS: Lazy<Vec<Regex>> = Lazy::new(|| {
    [
        r"(?i)^\s*the screen (?:shows|displays)\s+",
        r"(?i)^\s*[a-z][\w.]*\s+is responding with\s+(?:(?:updates?|details|information|info)\s+(?:on|about)\s+)?",
        r"(?i)^\s*a chat interface with\s+(?:multiple lines of\s+)?text\s+(?:related to|about)\s+",
    ]
    .iter()
    .map(|pattern| Regex::new(pattern).expect("valid narration opener regex"))
    .collect()
});

// A template that lost its last value leaves ", ." or " in." at the end.
static DANGLING_END: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)(?:\s*,|\s+(?:in|on|at|-))\s*\.\s*$").expect("valid dangling end regex")
});

/// FNDR's display voice has no narrator and no reader. Remove a leading "You",
/// "The user", narrating "User" or screen-describing opener so stored text
/// written under older prompts reads the same as new text: "You reviewed the
/// PR" becomes "Reviewed the PR".
pub fn neutral_voice(text: &str) -> String {
    let mut opened = text.to_string();
    for _ in 0..3 {
        let Some(end) = NARRATION_OPENERS
            .iter()
            .find_map(|opener| opener.find(&opened).map(|found| found.end()))
        else {
            break;
        };
        opened = opened[end..].to_string();
    }
    let tidied = DANGLING_END.replace(&opened, ".").into_owned();
    if tidied == text && !LEADING_PERSON.is_match(text) && !LEADING_BARE_USER.is_match(text) {
        return text.to_string();
    }
    let text = tidied.as_str();
    let rest = if let Some(found) = LEADING_PERSON.find(text) {
        &text[found.end()..]
    } else if let Some(captures) = LEADING_BARE_USER.captures(text) {
        let verb = captures
            .get(1)
            .or_else(|| captures.get(2))
            .or_else(|| captures.get(3));
        &text[verb.map_or(0, |verb| verb.start())..]
    } else {
        text
    };
    let mut chars = rest.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => text.to_string(),
    }
}

/// True when `text` is a machine placeholder, not a description: a capture
/// file name ("Screen capture visual : ChatGPT_1789709739566."), a timed
/// stub ("Captured recent activity at 08:30 AM"), or an app name said twice
/// ("Claude: Claude"). Empty text counts. Such text is never shown as a
/// summary; the window title says more.
pub fn is_placeholder_summary(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return true;
    }
    let lower = trimmed.to_ascii_lowercase();
    // Punctuation is dropped before matching because display cleanup strips
    // brackets, which hid "Screen capture (visual)" from the older check.
    let words = lower
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>();
    let words = words.split_whitespace().collect::<Vec<_>>().join(" ");
    let digit_run = lower
        .split(|c: char| !c.is_ascii_digit())
        .any(|run| run.len() >= 10);
    let said_twice = lower.split_once(':').is_some_and(|(left, right)| {
        let (left, right) = (left.trim(), right.trim().trim_end_matches('.'));
        !left.is_empty() && left == right
    });
    words.starts_with("screen capture")
        || words.starts_with("captured recent")
        || words.starts_with("viewed content on")
        || words.starts_with("url only surface capture")
        || (words.starts_with("viewed ") && lower.contains(" at "))
        || (lower.contains(".png") && lower.chars().filter(char::is_ascii_digit).count() >= 6)
        || digit_run
        || said_twice
}

pub fn narration_filter_hits(summary: &str) -> bool {
    let value = summary.trim();
    if value.is_empty() {
        return false;
    }
    BANNED_PATTERNS
        .iter()
        .any(|pattern| pattern.is_match(value))
        || INSTRUCTION_PATTERNS
            .iter()
            .any(|pattern| pattern.is_match(value))
}

/// Returns true when a value reads like instructions to a summarizer rather
/// than a description of the captured memory.
pub fn is_summary_instruction(value: &str) -> bool {
    let value = value.trim();
    !value.is_empty()
        && INSTRUCTION_PATTERNS
            .iter()
            .any(|pattern| pattern.is_match(value))
}

pub fn clean_or_fallback_display_summary(
    candidate: &str,
    page_title: &str,
    url: Option<&str>,
    timestamp_ms: i64,
) -> (String, bool) {
    let generated = build_display_summary(page_title, url, &neutral_voice(candidate), timestamp_ms);
    if !narration_filter_hits(&generated) {
        return (generated, false);
    }

    // Regeneration pass: scrub known narration markers and re-normalize once.
    let mut scrubbed = generated.clone();
    for pattern in SCRUB_PATTERNS.iter() {
        scrubbed = pattern.replace_all(&scrubbed, " ").to_string();
    }
    let scrubbed = clean_sentence(&scrubbed);
    if !scrubbed.is_empty() {
        let regenerated = build_display_summary(page_title, url, &scrubbed, timestamp_ms);
        if !narration_filter_hits(&regenerated) {
            return (regenerated, true);
        }
    }

    (
        fallback_display_summary(page_title, url, timestamp_ms),
        true,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_narration_leaks() {
        assert!(narration_filter_hits(
            "You reviewed memory_compaction.rs and tests"
        ));
        assert!(narration_filter_hits("User viewed dashboard"));
        assert!(narration_filter_hits(
            "The OCR text indicates a visual-only frame with no visible content"
        ));
        assert!(narration_filter_hits("Screen capture shows a browser page"));
        assert!(!narration_filter_hits("Watched IPL highlights on YouTube."));
    }

    #[test]
    fn detects_summary_instructions_mistaken_for_memory_insights() {
        assert!(narration_filter_hits(
            "extract and analyze the content from the OCR text; identify key themes and narrative elements in the video summary"
        ));
        assert!(is_summary_instruction(
            "extract and analyze the content from the OCR text; identify key themes and narrative elements in the video summary"
        ));
        assert!(!narration_filter_hits(
            "Watched a James Blake live performance on YouTube."
        ));
        assert!(!is_summary_instruction(
            "Watched a James Blake live performance on YouTube."
        ));
    }

    #[test]
    fn neutral_voice_removes_narrator_and_reader() {
        assert_eq!(neutral_voice("The user reviewed the PR"), "Reviewed the PR");
        assert_eq!(neutral_voice("user opened VS Code"), "Opened VS Code");
        assert_eq!(
            neutral_voice("You were listening to James Blake"),
            "Listening to James Blake"
        );
        assert_eq!(
            neutral_voice("User is debugging a borrow error"),
            "Debugging a borrow error"
        );
        assert_eq!(
            neutral_voice("User checks FNDR logs and trust settings."),
            "Checks FNDR logs and trust settings."
        );
        assert_eq!(
            neutral_voice("User managing demo prep"),
            "Managing demo prep"
        );
    }

    #[test]
    fn placeholders_are_recognised_with_or_without_their_punctuation() {
        for placeholder in [
            "Screen capture (visual): Claude_1778938598807.png. Claude",
            "Screen capture visual : ChatGPT_1789709739566.",
            "Captured recent activity at 08:30 AM",
            "URL-only surface capture for x.com at 12:00 PM",
            "Claude: Claude",
            "",
        ] {
            assert!(is_placeholder_summary(placeholder), "{placeholder}");
        }
        for real in [
            "Watched the IPL match on Willow TV",
            "Got paged for INC-2291: p99 latency above 2 seconds",
            "Usage limits explained in Google Chrome.",
        ] {
            assert!(!is_placeholder_summary(real), "{real}");
        }
    }

    #[test]
    fn neutral_voice_drops_screen_and_assistant_narration() {
        assert_eq!(
            neutral_voice("Claude is responding with updates on prompt development and testing."),
            "Prompt development and testing."
        );
        assert_eq!(
            neutral_voice("The screen shows a chat interface with multiple lines of text related to OpenMP scheduling."),
            "OpenMP scheduling."
        );
        assert_eq!(
            neutral_voice("Reviewing the failed fixture counts on ChatGPT in."),
            "Reviewing the failed fixture counts on ChatGPT."
        );
        assert_eq!(
            neutral_voice("BGE prefixes and vector scores,."),
            "BGE prefixes and vector scores."
        );
    }

    #[test]
    fn neutral_voice_keeps_user_as_a_noun_and_inner_words() {
        for kept in [
            "User guide for React hooks",
            "User testing session notes",
            "User settings and permissions",
            "username field was edited",
            "Your invoice is ready",
            "Reviewed what you sent on Friday",
        ] {
            assert_eq!(neutral_voice(kept), kept);
        }
    }

    #[test]
    fn scrub_or_fallback_removes_internal_voice() {
        let (summary, filtered) = clean_or_fallback_display_summary(
            "You reviewed FNDR src-tauri memory_compaction while noting Refactor ideas",
            "FNDR Refactor",
            Some("https://github.com/org/repo"),
            1_700_000_000_000,
        );

        assert!(filtered);
        assert!(!narration_filter_hits(&summary));
        assert!(summary.ends_with('.'));
    }
}
