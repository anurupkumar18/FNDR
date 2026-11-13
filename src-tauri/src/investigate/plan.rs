//! Reads a request into what an investigation looks for: the words that
//! name the work, a time filter, and the lines of inquiry to probe.

use crate::workset::topic_words;
use serde::Serialize;

/// A line of inquiry: one kind of finding a request can ask about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Inquiry {
    Decisions,
    Open,
    Problems,
}

impl Inquiry {
    pub const ALL: [Inquiry; 3] = [Inquiry::Decisions, Inquiry::Open, Inquiry::Problems];

    /// Words in a request that ask for this line.
    fn cues(self) -> &'static [&'static str] {
        match self {
            Inquiry::Decisions => &[
                "decide",
                "decided",
                "decision",
                "decisions",
                "agreed",
                "agree",
                "chose",
                "choice",
            ],
            Inquiry::Open => &[
                "left",
                "remaining",
                "remains",
                "open",
                "still",
                "todo",
                "todos",
                "next",
                "pending",
                "outstanding",
                "unfinished",
            ],
            Inquiry::Problems => &[
                "wrong", "problem", "problems", "issue", "issues", "error", "errors", "bug",
                "bugs", "broke", "broken", "fail", "failed", "failing", "blocker", "blockers",
                "blocked", "stuck",
            ],
        }
    }

    /// Words added to the topic when probing this line.
    pub fn probe_words(self) -> &'static str {
        match self {
            Inquiry::Decisions => "decided decision agreed",
            Inquiry::Open => "next steps remaining todo",
            Inquiry::Problems => "error failed blocker issue",
        }
    }
}

/// Words of a question that never name the work. `topic_words` already
/// drops the phrasing of a reopen request ("pull up everything for").
const QUESTION_WORDS: &[&str] = &[
    "a",
    "about",
    "all",
    "any",
    "are",
    "as",
    "at",
    "be",
    "been",
    "but",
    "by",
    "catch",
    "caught",
    "deeper",
    "dig",
    "do",
    "does",
    "done",
    "far",
    "find",
    "further",
    "go",
    "going",
    "gone",
    "happen",
    "happened",
    "happening",
    "has",
    "how",
    "into",
    "investigate",
    "know",
    "latest",
    "look",
    "need",
    "needs",
    "now",
    "or",
    "progress",
    "so",
    "stand",
    "stands",
    "status",
    "step",
    "steps",
    "summarize",
    "summary",
    "tell",
    "there",
    "there's",
    "update",
    "updates",
    "went",
    "what's",
    "when",
    "where",
    "where's",
    "which",
    "who",
    "why",
];

/// Cue words that are also things a request can be about ("the spam bug",
/// "the launch blocker"). They choose a line of inquiry and stay in the topic.
const CUES_THAT_NAME_WORK: &[&str] = &["bug", "bugs", "blocker", "blockers", "error", "errors"];

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Plan {
    /// The request as typed.
    pub goal: String,
    /// The request's own tokens that name the work, in order, as written:
    /// "LL-1482" stays whole so search can match it as an identifier.
    pub topic: Vec<String>,
    /// The topic as lowercase words, for checking a hit holds all of them.
    pub words: Vec<String>,
    /// A store time filter read from the request ("yesterday", "7d").
    pub time: Option<String>,
    /// Lines the request asks about; all three when it names none.
    pub focus: Vec<Inquiry>,
}

impl Plan {
    pub fn query(&self) -> String {
        self.topic.join(" ")
    }
}

/// A request that names no work. FNDR Review answers "what did I do
/// yesterday"; an investigation needs a project, a document or a ticket.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NeedsTopic;

fn names_work(word: &str) -> bool {
    if CUES_THAT_NAME_WORK.contains(&word) {
        return true;
    }
    !QUESTION_WORDS.contains(&word) && !Inquiry::ALL.iter().any(|line| line.cues().contains(&word))
}

pub fn plan(goal: &str) -> Result<Plan, NeedsTopic> {
    let goal = goal.trim();
    let lower = goal.to_lowercase();
    let asked: Vec<String> = lower
        .split(|ch: char| !ch.is_alphanumeric() && ch != '\'')
        .map(|word| word.trim_matches('\'').to_string())
        .filter(|word| !word.is_empty())
        .collect();

    let mut topic = Vec::new();
    let mut words = Vec::new();
    for token in goal.split_whitespace() {
        let parts: Vec<String> = topic_words(token)
            .into_iter()
            .filter(|word| names_work(word))
            .collect();
        if parts.is_empty() {
            continue;
        }
        let cleaned = token.trim_matches(|ch: char| !ch.is_alphanumeric());
        if cleaned.is_empty()
            || topic
                .iter()
                .any(|kept: &String| kept.eq_ignore_ascii_case(cleaned))
        {
            continue;
        }
        topic.push(cleaned.to_string());
        for part in parts {
            if !words.contains(&part) {
                words.push(part);
            }
        }
    }
    if topic.is_empty() {
        return Err(NeedsTopic);
    }

    let mut focus: Vec<Inquiry> = Inquiry::ALL
        .into_iter()
        .filter(|line| {
            asked
                .iter()
                .any(|word| line.cues().contains(&word.as_str()))
        })
        .collect();
    if focus.is_empty() {
        focus = Inquiry::ALL.to_vec();
    }

    Ok(Plan {
        goal: goal.to_string(),
        topic,
        words,
        time: crate::workset::time_filter(goal),
        focus,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_catch_up_request_keeps_the_words_that_name_the_work() {
        let plan = plan("Catch me up on the Smart Reminders launch").unwrap();
        assert_eq!(plan.topic, vec!["Smart", "Reminders", "launch"]);
        assert_eq!(plan.words, vec!["smart", "reminders", "launch"]);
        assert_eq!(plan.query(), "Smart Reminders launch");
        assert_eq!(plan.focus, Inquiry::ALL.to_vec());
        assert_eq!(plan.time, None);
    }

    #[test]
    fn an_identifier_stays_whole_in_the_query() {
        let plan = plan("what went wrong with LL-1482?").unwrap();
        assert_eq!(plan.topic, vec!["LL-1482"]);
        assert_eq!(plan.words, vec!["ll", "1482"]);
        assert_eq!(plan.focus, vec![Inquiry::Problems]);
    }

    #[test]
    fn cue_words_choose_the_lines_and_leave_the_topic() {
        let plan = plan("what did we decide about pricing and what is still open").unwrap();
        assert_eq!(plan.topic, vec!["pricing"]);
        assert_eq!(plan.focus, vec![Inquiry::Decisions, Inquiry::Open]);
    }

    #[test]
    fn a_cue_that_names_the_work_stays_in_the_topic() {
        let plan = plan("the spam bug").unwrap();
        assert_eq!(plan.topic, vec!["spam", "bug"]);
        assert_eq!(plan.focus, vec![Inquiry::Problems]);
    }

    #[test]
    fn a_time_phrase_becomes_the_filter_and_not_a_topic_word() {
        let plan = plan("what happened with the vendor contract yesterday").unwrap();
        assert_eq!(plan.topic, vec!["vendor", "contract"]);
        assert_eq!(plan.time.as_deref(), Some("yesterday"));
    }

    #[test]
    fn a_request_that_names_no_work_needs_a_topic() {
        assert_eq!(plan("what did I do yesterday"), Err(NeedsTopic));
        assert_eq!(plan("catch me up"), Err(NeedsTopic));
        assert_eq!(plan("   "), Err(NeedsTopic));
    }
}
