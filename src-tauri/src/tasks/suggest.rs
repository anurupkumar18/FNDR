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

/// A system process, not an app a person chose to use.
pub fn is_system_surface(app_name: &str) -> bool {
    SYSTEM_SURFACES.contains(&app_name.trim().to_lowercase().as_str())
}

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
    let host = host_of(url);
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

/// Where the words were seen decides what they can mean.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Surface {
    /// Mail, chat and notes: someone is talking to this person, or they are
    /// writing to themselves. A request or an instruction there is theirs.
    Personal,
    /// Any other page, document or window. "You need to" and "make sure"
    /// there are addressed to any reader.
    Public,
}

const PERSONAL_APPS: &[&str] = &[
    "mail",
    "messages",
    "slack",
    "microsoft teams",
    "teams",
    "discord",
    "microsoft outlook",
    "outlook",
    "whatsapp",
    "telegram",
    "signal",
    "notes",
    "notion",
    "obsidian",
    "reminders",
    "things",
    "todoist",
    "bear",
    "textedit",
    "calendar",
];
const PERSONAL_HOSTS: &[&str] = &[
    "mail.google.com",
    "outlook.live.com",
    "outlook.office.com",
    "app.slack.com",
    "teams.microsoft.com",
    "discord.com",
    "web.whatsapp.com",
    "web.telegram.org",
    "notion.so",
];

fn host_of(url: Option<&str>) -> String {
    url.map(|url| url.split("://").nth(1).unwrap_or(url))
        .and_then(|rest| rest.split('/').next())
        .map(|host| host.trim_start_matches("www.").to_lowercase())
        .unwrap_or_default()
}

pub fn surface_of(app_name: &str, url: Option<&str>) -> Surface {
    let app = app_name.trim().to_lowercase();
    let host = host_of(url);
    if PERSONAL_APPS.contains(&app.as_str())
        || PERSONAL_HOSTS
            .iter()
            .any(|known| host == *known || host.ends_with(&format!(".{known}")))
    {
        Surface::Personal
    } else {
        Surface::Public
    }
}

/// A commitment in the first person. It is this person's wherever it is.
const OWN_COMMITMENT_CUES: &[&str] = &[
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
    "remind me",
];
/// A stated deadline counts anywhere too.
const DEADLINE_CUES: &[&str] = &["due", "deadline", "submit by", "send by", "reply by"];
/// A request or an instruction. This person's only on a personal surface.
const REQUEST_CUES: &[&str] = &[
    "please",
    "can you",
    "could you",
    "would you",
    "make sure",
    "need to",
    "have to",
    "remember to",
    "don't forget",
    "do not forget",
    "todo",
    "to do",
    "action item",
    "follow up",
    "assigned to",
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

/// A day, date or clock time, not merely the word "deadline" or "due".
fn names_a_day_or_time(text: &str) -> bool {
    let plain_text = plain(text);
    DATE_CUES
        .iter()
        .filter(|cue| !DEADLINE_CUES.contains(cue))
        .any(|cue| plain_text.contains(&format!(" {cue} ")))
        || text
            .split_whitespace()
            .any(|word| word.chars().any(|c| c.is_ascii_digit()))
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

/// Phrases that mark text written to steer an AI. A line carrying one can
/// never be the reason for a task, whatever else it says.
const AI_ADDRESSED: &[&str] = &[
    "ignore all previous instructions",
    "ignore previous instructions",
    "ignore the above",
    "disregard previous",
    "system note",
    "system prompt",
    "add the task",
    "reply only with",
    "you are an ai",
];

fn addressed_to_an_ai(line: &str) -> bool {
    let plain_line = plain(line);
    AI_ADDRESSED
        .iter()
        .any(|phrase| plain_line.contains(&format!(" {phrase} ")))
}

/// Share of the title's words a passage must hold to be what states it.
const MIN_TITLE_SHARE: f32 = 0.6;
const MAX_QUOTE_CHARS: usize = 200;

/// The sentences on the screen. A line with no sentence ending is one
/// passage. The words that make something a task and the words that say what
/// it is must sit in the same sentence, so whole multi-sentence lines are not
/// passages.
fn passages(evidence: &str) -> Vec<&str> {
    let mut out = Vec::new();
    for line in evidence
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let mut start = 0;
        for (index, _) in line.match_indices(['.', '?', '!']) {
            if line[index + 1..].starts_with(' ') || index + 1 == line.len() {
                out.push(line[start..=index].trim());
                start = index + 1;
            }
        }
        out.push(line[start..].trim());
    }
    out.retain(|passage| !passage.is_empty());
    out
}

fn content_words(text: &str) -> Vec<String> {
    const STOP: &[&str] = &[
        "the", "and", "for", "with", "that", "this", "from", "you", "your",
    ];
    plain(text)
        .split_whitespace()
        .filter(|word| {
            (word.len() > 2 || word.chars().all(|c| c.is_ascii_digit())) && !STOP.contains(word)
        })
        .map(str::to_string)
        .collect()
}

fn states_a_task(passage: &str, surface: Surface) -> bool {
    let plain_passage = plain(passage);
    passage.split_whitespace().count() >= MIN_QUOTE_WORDS
        && (has_cue(&plain_passage, OWN_COMMITMENT_CUES)
            // "The deadline effect" is a topic. A deadline has a date.
            || (has_cue(&plain_passage, DEADLINE_CUES) && names_a_day_or_time(passage))
            || (surface == Surface::Personal && has_cue(&plain_passage, REQUEST_CUES)))
}

/// The words on the screen that state `title`. The model's own copy is used
/// when it holds up; it usually does not (a 2B model returns one word of the
/// sentence, or another line), so otherwise the shortest passage that states
/// a commitment or request and holds most of the title's words is used.
fn supporting_quote(
    title: &str,
    model_quote: &str,
    evidence: &str,
    plain_evidence: &str,
    surface: Surface,
) -> Option<String> {
    let chosen = if states_a_task(model_quote, surface)
        && plain_evidence.contains(plain(model_quote).trim_end())
    {
        model_quote.to_string()
    } else {
        let wanted = content_words(title);
        if wanted.len() < 2 {
            return None;
        }
        passages(evidence)
            .into_iter()
            .filter(|passage| states_a_task(passage, surface))
            .filter(|passage| {
                let held = content_words(passage);
                let found = wanted.iter().filter(|word| held.contains(word)).count();
                found as f32 / wanted.len() as f32 >= MIN_TITLE_SHARE
            })
            .min_by_key(|passage| passage.len())?
            .chars()
            .take(MAX_QUOTE_CHARS)
            .collect()
    };
    // Judge the whole line the words sit on, not only the words chosen.
    let plain_chosen = plain(&chosen);
    let steered = evidence
        .lines()
        .filter(|line| plain(line).contains(plain_chosen.trim_end()))
        .any(addressed_to_an_ai);
    (!steered).then_some(chosen)
}

/// Keep the model's lines that the capture supports. `evidence` is the
/// screen text the model was shown, never a model-written summary.
pub fn parse_suggestions(raw: &str, evidence: &str, surface: Surface) -> Vec<Suggestion> {
    // The first line is the window title. It names the screen; it does not
    // state anything a person committed to.
    let evidence = evidence.split_once('\n').map_or("", |(_, body)| body);
    let plain_evidence = plain(evidence);
    let mut kept: Vec<Suggestion> = Vec::new();
    for line in raw.lines() {
        let parts: Vec<&str> = line.split('|').map(str::trim).collect();
        let (kind, title, model_quote) = match parts[..] {
            [kind, title] => (kind, title, ""),
            [kind, title, quote] => (kind, title, quote),
            _ => continue,
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
        let model_quote = model_quote.trim_matches(['"', '\'', '`', ' ']);
        if !is_actionable_task_title(title) || echoes_the_format(title) {
            continue;
        }
        let Some(quote) = supporting_quote(title, model_quote, evidence, &plain_evidence, surface)
        else {
            continue;
        };
        // A reminder with no date and a follow-up with no one to follow up
        // with are still things to do.
        if (task_type == TaskType::Reminder && !has_date(&quote))
            || (task_type == TaskType::Followup && !names_someone(&quote))
        {
            task_type = TaskType::Todo;
        }
        let plain_quote = plain(&quote);
        if kept.iter().any(|other| plain(&other.quote) == plain_quote) {
            continue;
        }
        kept.push(Suggestion {
            task_type,
            title: title.to_string(),
            quote,
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

/// The open tasks that are the person's own: added by them, from a meeting,
/// or accepted. Newest first. Suggestions are never counted as work a person
/// is carrying.
pub fn open_commitments(tasks: Vec<Task>) -> Vec<Task> {
    let mut own: Vec<Task> = tasks
        .into_iter()
        .filter(|task| !task.is_completed && !task.is_dismissed && !is_suggestion(task))
        .collect();
    own.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    own
}
