//! Task suggestions from a capture.
//!
//! The model proposes; nothing here is trusted until it is checked against
//! the screen text. A suggestion survives only when it quotes words that are
//! really in the capture and those words state a commitment or a request.
//! Audit of the earlier, unchecked extractor (2026-10-07): 471 open tasks
//! from 147 memories, none ever completed, over half read off AI chat
//! screens, and up to 33 from one memory because every merge re-extracted.

use super::{is_actionable_task_title, Task, TaskType};
use std::collections::HashMap;

/// Most screens hold no task. Two is already generous.
pub const MAX_SUGGESTIONS_PER_CAPTURE: usize = 2;
/// One extraction per app in this window, however many captures arrive.
pub const MIN_GAP_BETWEEN_EXTRACTIONS_MS: i64 = 10 * 60 * 1000;
const MIN_QUOTE_WORDS: usize = 3;

/// A task the model proposed and the capture supports.
#[derive(Debug, Clone, PartialEq)]
pub struct Suggestion {
    pub task_type: TaskType,
    pub title: String,
    /// The words on screen that state the task, as captured.
    pub quote: String,
}

/// Apps whose screens are an assistant talking. Its plans and status lines
/// read like tasks and are not the person's.
const AI_CHAT_APPS: &[&str] = &[
    "claude",
    "chatgpt",
    "codex",
    "gemini",
    "copilot",
    "perplexity",
];
const AI_CHAT_HOSTS: &[&str] = &[
    "chatgpt.com",
    "chat.openai.com",
    "claude.ai",
    "gemini.google.com",
    "copilot.microsoft.com",
    "perplexity.ai",
];
/// System surfaces that never hold a person's task.
const SYSTEM_SURFACES: &[&str] = &[
    "usernotificationcenter",
    "notification center",
    "control center",
    "coreautha",
    "loginwindow",
    "systemuiserver",
    "dock",
    "spotlight",
    "system settings",
    "system preferences",
];

/// Whether captures from this app or page may be asked for tasks at all.
pub fn is_task_source(app_name: &str, url: Option<&str>) -> bool {
    let app = app_name.trim().to_lowercase();
    if app.is_empty() || SYSTEM_SURFACES.contains(&app.as_str()) {
        return false;
    }
    if AI_CHAT_APPS
        .iter()
        .any(|name| app == *name || app.starts_with(&format!("{name} ")))
    {
        return false;
    }
    let host = url
        .map(|url| url.split("://").nth(1).unwrap_or(url))
        .and_then(|rest| rest.split('/').next())
        .map(|host| host.trim_start_matches("www.").to_lowercase())
        .unwrap_or_default();
    !AI_CHAT_HOSTS
        .iter()
        .any(|known| host == *known || host.ends_with(&format!(".{known}")))
}

/// Lowercase words only, single spaces, padded so phrases match whole words.
fn plain(text: &str) -> String {
    let words = text
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '\'' {
                c.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect::<String>();
    format!(
        " {} ",
        words.split_whitespace().collect::<Vec<_>>().join(" ")
    )
}

/// Words that make a line a commitment or a request rather than a description.
const COMMITMENT_CUES: &[&str] = &[
    "i need to",
    "i have to",
    "i should",
    "i must",
    "i'll",
    "i will",
    "i'm going to",
    "we need to",
    "we have to",
    "we should",
    "need to",
    "have to",
    "remember to",
    "don't forget",
    "do not forget",
    "remind me",
    "todo",
    "to do",
    "action item",
    "follow up",
    "due",
    "deadline",
    "please",
    "can you",
    "could you",
    "would you",
    "make sure",
    "assigned to",
    "submit by",
    "send by",
    "reply by",
];

const DATE_CUES: &[&str] = &[
    "today",
    "tonight",
    "tomorrow",
    "monday",
    "tuesday",
    "wednesday",
    "thursday",
    "friday",
    "saturday",
    "sunday",
    "next week",
    "next month",
    "this week",
    "deadline",
    "due",
    "am",
    "pm",
    "january",
    "february",
    "march",
    "april",
    "june",
    "july",
    "august",
    "september",
    "october",
    "november",
    "december",
    "noon",
    "midnight",
    "eod",
];

fn has_cue(plain_text: &str, cues: &[&str]) -> bool {
    cues.iter()
        .any(|cue| plain_text.contains(&format!(" {cue} ")))
}

fn has_date(quote: &str) -> bool {
    has_cue(&plain(quote), DATE_CUES)
        || quote
            .split_whitespace()
            .any(|word| word.chars().any(|c| c.is_ascii_digit()) && word.contains(['/', ':', '-']))
}

/// A capitalized word that is not the first word, a day or a month: the
/// nearest cheap sign that the quote names someone.
fn names_someone(quote: &str) -> bool {
    quote.split_whitespace().skip(1).any(|word| {
        let word = word.trim_matches(|c: char| !c.is_alphanumeric());
        let mut chars = word.chars();
        let capitalized = chars.next().is_some_and(char::is_uppercase)
            && chars.clone().count() >= 2
            && chars.all(|c| c.is_lowercase());
        capitalized && word != "I" && !has_cue(&plain(word), DATE_CUES)
    })
}

/// Wording the prompt uses to describe the format. Seeing it in a title
/// means the model echoed the instructions.
fn echoes_the_format(title: &str) -> bool {
    let lower = title.to_lowercase();
    title.contains(['[', ']'])
        || lower.contains(" + ")
        || lower.contains("person/team")
        || lower.contains("task in a few words")
        || lower.contains("copied words")
        || lower.contains("words copied")
}

/// Keep the model's lines that the capture supports. `evidence` is the
/// screen text the model was shown, never a model-written summary.
pub fn parse_suggestions(raw: &str, evidence: &str) -> Vec<Suggestion> {
    let plain_evidence = plain(evidence);
    let mut kept: Vec<Suggestion> = Vec::new();
    for line in raw.lines() {
        let parts: Vec<&str> = line.split('|').map(str::trim).collect();
        let [kind, title, quote] = parts[..] else {
            continue;
        };
        let kind = kind
            .trim_start_matches(['-', '*', ' '])
            .to_ascii_uppercase();
        let mut task_type = match kind.as_str() {
            "TODO" => TaskType::Todo,
            "REMINDER" => TaskType::Reminder,
            "FOLLOWUP" | "FOLLOW-UP" => TaskType::Followup,
            _ => continue,
        };
        let title = title.trim_matches(['"', '\'', '`', ' ']);
        let quote = quote.trim_matches(['"', '\'', '`', ' ']);
        if !is_actionable_task_title(title) || echoes_the_format(title) {
            continue;
        }
        let plain_quote = plain(quote);
        let on_screen = quote.split_whitespace().count() >= MIN_QUOTE_WORDS
            && plain_evidence.contains(plain_quote.trim_end());
        if !on_screen || !has_cue(&plain_quote, COMMITMENT_CUES) {
            continue;
        }
        // A reminder with no date and a follow-up with no one to follow up
        // with are still things to do.
        if (task_type == TaskType::Reminder && !has_date(quote))
            || (task_type == TaskType::Followup && !names_someone(quote))
        {
            task_type = TaskType::Todo;
        }
        if kept.iter().any(|other| plain(&other.quote) == plain_quote) {
            continue;
        }
        kept.push(Suggestion {
            task_type,
            title: title.to_string(),
            quote: quote.to_string(),
        });
        if kept.len() == MAX_SUGGESTIONS_PER_CAPTURE {
            break;
        }
    }
    kept
}

/// Limits extraction to one run per app per window, so a long stretch in one
/// app is asked once and not once per capture.
#[derive(Debug, Default)]
pub struct ExtractionGate {
    last_run_ms: HashMap<String, i64>,
}

impl ExtractionGate {
    /// Whether to extract now. Records the run when it answers yes.
    pub fn admit(&mut self, app_name: &str, now_ms: i64) -> bool {
        let app = app_name.trim().to_lowercase();
        let due = self
            .last_run_ms
            .get(&app)
            .is_none_or(|last| now_ms - last >= MIN_GAP_BETWEEN_EXTRACTIONS_MS);
        if due {
            self.last_run_ms.insert(app, now_ms);
        }
        due
    }
}

/// An unaccepted suggestion is offered for this long, then quietly dropped.
/// A person who has not taken it in three days is not going to.
pub const SUGGESTION_LIFESPAN_MS: i64 = 3 * 24 * 60 * 60 * 1000;

/// A task FNDR proposed from a capture that the person has not accepted.
pub fn is_suggestion(task: &Task) -> bool {
    task.source_app.starts_with("Memory:") || task.source_app.eq_ignore_ascii_case("auto")
}

/// A suggestion the person accepted: theirs now, still linked to its memory.
pub fn is_accepted(task: &Task) -> bool {
    task.source_app.starts_with("Accepted:")
}

/// Make a suggestion the person's own task.
pub fn accept(task: &mut Task) {
    if let Some(app) = task.source_app.strip_prefix("Memory:") {
        task.source_app = format!("Accepted:{app}");
    } else if task.source_app.eq_ignore_ascii_case("auto") {
        task.source_app = "Accepted:".to_string();
    }
}

/// Whether an open suggestion should still be put in front of the person:
/// it carries the words from the screen that state it, and it is recent.
/// Suggestions made before the quote check existed carry none.
pub fn is_offered(task: &Task, now_ms: i64) -> bool {
    is_suggestion(task)
        && !task.description.trim().is_empty()
        && now_ms - task.created_at <= SUGGESTION_LIFESPAN_MS
}

/// Dismiss open suggestions that are no longer offered. The person's own,
/// accepted, meeting, completed and already dismissed tasks are untouched.
/// Returns how many were dismissed.
pub fn retire_unoffered(tasks: &mut [Task], now_ms: i64) -> usize {
    let mut retired = 0;
    for task in tasks
        .iter_mut()
        .filter(|task| !task.is_completed && !task.is_dismissed)
        .filter(|task| is_suggestion(task) && !is_offered(task, now_ms))
    {
        task.is_dismissed = true;
        retired += 1;
    }
    retired
}
