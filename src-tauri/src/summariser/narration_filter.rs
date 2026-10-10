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
        // "In a Google Chrome window titled 'X' and has been prompted ..."
        // describes the window, not what happened in it.
        r"(?i)^in an? [^.]{0,60}\bwindow titled\b",
        // A sentence about the window, the capture or the OCR is about the
        // screen, not about the work: "The Finder window displays ...",
        // "A screen capture of ...", "The OCR text extracted from ...".
        r"(?i)^(?:the|a|an) [\w. ]{0,40}\bwindow (?:contains|displays|shows|is showing|is open)\b",
        r"(?i)^an? screen ?(?:capture|shot) (?:of|showing)\b",
        r"(?i)^the ocr text\b",
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
        r"(?i)^\s*(?:the\s+user|you)\s+(?:(?:is|was|are|were|has been|have been|had been|has|have|had)\s+)?",
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
        r"(?i)^\s*the (?:session|conversation|chat|page|window|document|video|thread)\s+(?:involves|is about|covers|contains|includes|focuses on|is focused on|shows|displays)\s+",
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

/// "Reviewing the PR" left over from "The user is reviewing the PR" becomes
/// "Reviewed the PR". Only verbs listed here are changed; guessing a past
/// tense gets irregular verbs wrong.
const PAST_TENSE: &[(&str, &str)] = &[
    ("reviewing", "Reviewed"),
    ("viewing", "Viewed"),
    ("working", "Worked"),
    ("reading", "Read"),
    ("watching", "Watched"),
    ("editing", "Edited"),
    ("browsing", "Browsed"),
    ("discussing", "Discussed"),
    ("checking", "Checked"),
    ("writing", "Wrote"),
    ("using", "Used"),
    ("looking", "Looked"),
    ("interacting", "Interacted"),
    ("debugging", "Debugged"),
    ("studying", "Studied"),
    ("searching", "Searched"),
    ("testing", "Tested"),
    ("running", "Ran"),
    ("comparing", "Compared"),
    ("exploring", "Explored"),
    ("configuring", "Configured"),
    ("managing", "Managed"),
    ("analyzing", "Analyzed"),
    ("preparing", "Prepared"),
    ("drafting", "Drafted"),
    ("planning", "Planned"),
    ("opening", "Opened"),
    ("navigating", "Navigated"),
    ("scrolling", "Scrolled"),
    ("typing", "Typed"),
    ("composing", "Composed"),
    ("completing", "Completed"),
    ("learning", "Learned"),
    ("examining", "Examined"),
    ("monitoring", "Monitored"),
    ("updating", "Updated"),
    ("creating", "Created"),
    ("fixing", "Fixed"),
    ("asking", "Asked"),
    ("listening", "Listened"),
    ("playing", "Played"),
    ("organizing", "Organized"),
    ("copying", "Copied"),
    ("messaging", "Messaged"),
    ("attempting", "Attempted"),
    // "Discusses cache management": the present tense with the subject left
    // out. Only verbs that are not also common plural nouns ("checks",
    // "reviews", "covers") are listed.
    ("discusses", "Discussed"),
    ("explains", "Explained"),
    ("describes", "Described"),
    ("summarizes", "Summarized"),
    ("explores", "Explored"),
    ("compares", "Compared"),
];

static NARRATOR_POSSESSIVE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\b(?:their|your|his or her|the user's)\b").expect("valid possessive regex")
});

fn past_tense_lead(text: &str) -> String {
    // "Debugging: the capture test" opens with a category label, not a verb.
    let opens_with_a_label = text
        .split_whitespace()
        .take(4)
        .any(|word| word.ends_with(':'));
    if opens_with_a_label {
        return text.to_string();
    }
    // "Currently using ChatGPT" says the same as "Using ChatGPT".
    let text = match text.split_once(' ') {
        Some((first, rest)) if first.eq_ignore_ascii_case("currently") => rest.trim_start(),
        _ => text,
    };
    let first = text.split_whitespace().next().unwrap_or("");
    match PAST_TENSE
        .iter()
        .find(|(ing, _)| first.eq_ignore_ascii_case(ing))
    {
        Some((_, past)) => format!("{past}{}", &text[first.len()..]),
        None => text.to_string(),
    }
}

/// FNDR's display voice has no narrator and no reader. Remove a leading "You",
/// "The user", narrating "User" or screen-describing opener so stored text
/// written under older prompts reads the same as new text: "You reviewed the
/// PR" becomes "Reviewed the PR", and "The user has completed their review"
/// becomes "Completed the review".
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
    let had_person = LEADING_PERSON.is_match(&tidied) || LEADING_BARE_USER.is_match(&tidied);
    if tidied == text && !had_person {
        // No narrator, but a summary that opens "Reviewing ..." still names
        // an activity in progress. 35 of 132 visible summaries on the owner
        // vault opened that way (2026-10-07).
        return past_tense_lead(text);
    }
    let rest = if let Some(found) = LEADING_PERSON.find(&tidied) {
        &tidied[found.end()..]
    } else if let Some(captures) = LEADING_BARE_USER.captures(&tidied) {
        let verb = captures
            .get(1)
            .or_else(|| captures.get(2))
            .or_else(|| captures.get(3));
        &tidied[verb.map_or(0, |verb| verb.start())..]
    } else {
        tidied.as_str()
    };
    // With the narrator gone, "their notes" has no one to belong to.
    let rest = if had_person {
        NARRATOR_POSSESSIVE.replace_all(rest, "the").into_owned()
    } else {
        rest.to_string()
    };
    let rest = past_tense_lead(&rest);
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

const MAX_LABEL_WORDS: usize = 8;

fn label_words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| word.len() >= 2)
        .map(str::to_string)
        .collect()
}

/// A summary that only repeats words of the window title, such as
/// "Netflix." or "System Settings - Storage.", is a label, not a statement
/// of what happened. `restate_label` turns it into "Viewed {label}.".
fn restate_label(summary: &str, page_title: &str) -> Option<String> {
    let words = label_words(summary);
    let title = label_words(page_title);
    let leads_with_a_verb = words.first().is_some_and(|first| {
        first.ends_with("ed") || PAST_TENSE.iter().any(|(_, past)| past == first)
    });
    if words.is_empty()
        || words.len() > MAX_LABEL_WORDS
        || leads_with_a_verb
        || !words.iter().all(|word| title.contains(word))
    {
        return None;
    }
    let label = summary
        .trim()
        .trim_end_matches(|c: char| !c.is_alphanumeric() && c != ')')
        .trim();
    Some(format!("Viewed {label}."))
}

pub fn clean_or_fallback_display_summary(
    candidate: &str,
    page_title: &str,
    url: Option<&str>,
    timestamp_ms: i64,
) -> (String, bool) {
    let voiced = neutral_voice(candidate);
    let voiced = restate_label(&voiced, page_title).unwrap_or(voiced);
    let generated = build_display_summary(page_title, url, &voiced, timestamp_ms);
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
    fn a_present_tense_lead_without_a_subject_goes_to_the_past() {
        assert_eq!(
            neutral_voice("Discusses cache management for the build."),
            "Discussed cache management for the build."
        );
        assert_eq!(
            neutral_voice("Currently using ChatGPT, attempting a fix."),
            "Used ChatGPT, attempting a fix."
        );
        assert_eq!(
            neutral_voice("Copying a file named report.pdf."),
            "Copied a file named report.pdf."
        );
        // A plural noun that is also a verb is left alone.
        assert_eq!(
            neutral_voice("Reviews of the laptop stand."),
            "Reviews of the laptop stand."
        );
        assert_eq!(
            neutral_voice("Checks passed on CI."),
            "Checks passed on CI."
        );
    }

    #[test]
    fn a_summary_that_is_only_the_title_becomes_a_statement() {
        let shown = |summary: &str, title: &str| {
            clean_or_fallback_display_summary(summary, title, None, 1_700_000_000_000).0
        };
        assert_eq!(
            shown("Netflix.", "Netflix - Google Chrome"),
            "Viewed Netflix."
        );
        assert_eq!(
            shown("System Settings - Storage.", "Storage - System Settings"),
            "Viewed System Settings - Storage."
        );
        // A sentence with words of its own is left as written.
        let sentence = "Compared two laptop stands and picked the cheaper one.";
        assert_eq!(shown(sentence, "Laptop stands - Safari"), sentence);
        // So is one that already says what happened.
        assert_eq!(
            shown("Opened Storage.", "Opened Storage"),
            "Opened Storage."
        );
        // No title, nothing to compare with.
        assert_eq!(shown("Netflix.", ""), "Netflix.");
    }

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
            "Listened to James Blake"
        );
        assert_eq!(
            neutral_voice("User is debugging a borrow error"),
            "Debugged a borrow error"
        );
        assert_eq!(
            neutral_voice("User checks FNDR logs and trust settings."),
            "Checks FNDR logs and trust settings."
        );
        assert_eq!(
            neutral_voice("User managing demo prep"),
            "Managed demo prep"
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
            "Reviewed the failed fixture counts on ChatGPT."
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
    #[test]
    fn a_stripped_narrator_leaves_a_finished_past_tense_sentence() {
        for (stored, shown) in [
            (
                "The user has completed a repair of their FNDR database, backed it up, and verified it.",
                "Completed a repair of the FNDR database, backed it up, and verified it.",
            ),
            ("You have reviewed your notes on meiosis.", "Reviewed the notes on meiosis."),
            ("The user had opened the lab report.", "Opened the lab report."),
            ("The user is reviewing the rule refinement process.", "Reviewed the rule refinement process."),
            ("The user was working on the capstone demo script.", "Worked on the capstone demo script."),
            ("User is debugging the capture test.", "Debugged the capture test."),
            ("The user is interacting with a chromosome count exercise.", "Interacted with a chromosome count exercise."),
        ] {
            assert_eq!(neutral_voice(stored), shown, "{stored}");
        }
    }

    #[test]
    fn a_sentence_about_the_session_keeps_only_the_work() {
        assert_eq!(
            neutral_voice("The session involves reviewing the FNDR app's development status, including commits and UI rendering."),
            "Reviewed the FNDR app's development status, including commits and UI rendering."
        );
        assert_eq!(
            neutral_voice("The conversation is about choosing an embedding model."),
            "Choosing an embedding model."
        );
    }

    #[test]
    fn verbs_the_cleanup_does_not_know_and_neutral_text_are_left_alone() {
        // No narrator, but a listed activity verb still goes to the past tense.
        assert_eq!(
            neutral_voice("Reviewing guidelines for lab safety."),
            "Reviewed guidelines for lab safety."
        );
        assert_eq!(
            neutral_voice("Meeting notes for the capstone."),
            "Meeting notes for the capstone."
        );
        // A category label is not a verb.
        assert_eq!(
            neutral_voice("Debugging: the capture test fails on merge."),
            "Debugging: the capture test fails on merge."
        );
        assert_eq!(
            neutral_voice("Reviewing agent output: Fix retrieval ranking."),
            "Reviewing agent output: Fix retrieval ranking."
        );
        assert_eq!(
            neutral_voice("Their team shipped the fix."),
            "Their team shipped the fix."
        );
        // Narrator stripped, verb unknown: keep the word rather than guess.
        assert_eq!(
            neutral_voice("The user is triaging the inbox."),
            "Triaging the inbox."
        );
    }

    #[test]
    fn sentences_about_the_window_the_capture_or_the_ocr_are_narration() {
        for line in [
            "The Finder window displays various app folders and recent files.",
            "A Google Chrome window shows the course page for CS 4500.",
            "The Spotify app window is open on a playlist.",
            "A screen capture of the settings page.",
            "The OCR text extracted from the page lists three grades.",
            "The OCR text shows a login form.",
        ] {
            assert!(narration_filter_hits(line), "{line}");
        }
        for line in [
            "The window function in the SQL query was rewritten.",
            "Fixed the window resize bug in the notch.",
            "The capstone demo is scheduled for Thursday.",
        ] {
            assert!(!narration_filter_hits(line), "{line}");
        }
    }

    #[test]
    fn a_description_of_the_window_is_narration() {
        assert!(narration_filter_hits(
            "In a Google Chrome window titled 'Gamete Chromosome Count' and has been prompted about accessing screen and audio recordings."
        ));
        assert!(!narration_filter_hits(
            "In a meeting with Priya, agreed on the Friday deadline."
        ));
    }
}
