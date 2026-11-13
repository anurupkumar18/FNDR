//! The done rule: whether a later memory records a next step as done.
//!
//! A step counts only when one clause of the later text holds every word of
//! what the step acts on ("go/no-go meeting", "LL-1482") and a done word:
//! the step's own verb in the past, or one of a short list. The clause is
//! returned so the row can show it. FNDR does not claim the step is done; it
//! shows what was recorded (ADR 039).
//!
//! The first version asked only for the words before a preposition. On the
//! seeded office-PM store that marked "Retest spam placement on staging" from
//! a clause about the production fix, so every word is required now.

/// Done words that count whatever the step's verb is.
const DONE_WORDS: &[&str] = &[
    "closed",
    "done",
    "fixed",
    "live",
    "resolved",
    "shipped",
    "merged",
    "completed",
];

/// A clause holding one of these says the thing is not done.
const NOT_DONE: &[&str] = &[
    "not", "no", "never", "still", "yet", "open", "pending", "tbd", "blocked", "waiting", "unless",
    "until", "if", "should", "must", "will", "need", "needs", "todo",
];

/// Words that end what a step acts on: "Fill in the slide after LL-1482
/// closes" acts on the slide.
const CLAUSE_ENDS: &[&str] = &[
    "after", "before", "once", "until", "when", "so", "then", "if", "while", "unless",
];

/// Words that start a phrase about where, with what or for whom.
const PREPOSITIONS: &[&str] = &[
    "with", "to", "on", "at", "for", "in", "from", "by", "into", "about", "of", "as",
];

const FILLER: &[&str] = &[
    "the", "a", "an", "and", "or", "its", "their", "his", "her", "my", "our", "this", "that",
    "these", "those", "all", "any", "some", "up", "out", "off", "down", "over", "final", "new",
];

const IRREGULAR_PAST: &[(&str, &[&str])] = &[
    ("send", &["sent"]),
    ("write", &["wrote", "written"]),
    ("make", &["made"]),
    ("build", &["built"]),
    ("get", &["got", "gotten"]),
    ("do", &["did", "done"]),
    ("run", &["ran", "run"]),
    ("set", &["set"]),
    ("put", &["put"]),
    ("give", &["gave", "given"]),
    ("take", &["took", "taken"]),
    ("hold", &["held"]),
    ("find", &["found"]),
    ("choose", &["chose", "chosen"]),
    ("draw", &["drew", "drawn"]),
    ("pay", &["paid"]),
    ("buy", &["bought"]),
    ("meet", &["met"]),
    ("read", &["read"]),
    ("tell", &["told"]),
    ("bring", &["brought"]),
    ("teach", &["taught"]),
    ("begin", &["began", "begun"]),
    ("leave", &["left"]),
    ("keep", &["kept"]),
    ("ship", &["shipped"]),
    ("submit", &["submitted"]),
    ("drop", &["dropped"]),
    ("plan", &["planned"]),
    ("swap", &["swapped"]),
    ("chat", &["chatted"]),
    ("flag", &["flagged"]),
    ("tag", &["tagged"]),
    ("log", &["logged"]),
    ("plug", &["plugged"]),
    ("trim", &["trimmed"]),
    ("cut", &["cut"]),
    ("split", &["split"]),
    ("rewrite", &["rewrote", "rewritten"]),
    ("redo", &["redid", "redone"]),
    ("rebuild", &["rebuilt"]),
    ("resend", &["resent"]),
];

fn tokens(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(|word| {
            word.trim_matches(|ch: char| !ch.is_alphanumeric())
                .to_lowercase()
        })
        .filter(|word| !word.is_empty())
        .collect()
}

/// One spelling for a word and its plural.
fn stem(word: &str) -> String {
    if word.chars().count() > 4 && word.ends_with('s') && !word.ends_with("ss") {
        word[..word.len() - 1].to_string()
    } else {
        word.to_string()
    }
}

fn is_identifier(word: &str) -> bool {
    word.chars().any(|ch| ch.is_ascii_digit())
}

fn past_forms(verb: &str) -> Vec<String> {
    if let Some((_, forms)) = IRREGULAR_PAST.iter().find(|(base, _)| *base == verb) {
        return forms.iter().map(|form| form.to_string()).collect();
    }
    let regular = if verb.ends_with('e') {
        format!("{verb}d")
    } else if verb.ends_with('y')
        && !verb.ends_with("ay")
        && !verb.ends_with("ey")
        && !verb.ends_with("oy")
    {
        format!("{}ied", &verb[..verb.len() - 1])
    } else {
        format!("{verb}ed")
    };
    vec![regular]
}

/// A step read into its verb and the words of what it acts on, up to a
/// clause end: "recommendation slide" in "Fill in the recommendation slide
/// after LL-1482 closes".
struct Step {
    verb: String,
    object: Vec<String>,
}

fn read_step(step: &str) -> Option<Step> {
    let words = tokens(step);
    // "Rafael to fix DKIM alignment": a name, "to", then the verb. Names are
    // capitalized and a verb is not, which keeps "Send screenshots to
    // Hannah" reading "send" as its verb.
    let written: Vec<&str> = step
        .split_whitespace()
        .map(|word| word.trim_matches(|ch: char| !ch.is_alphanumeric()))
        .filter(|word| !word.is_empty())
        .collect();
    let starts_upper = |word: &str| word.chars().next().is_some_and(char::is_uppercase);
    let verb_at = (1..=2)
        .find(|&at| {
            written.get(at).is_some_and(|word| *word == "to")
                && written[..at].iter().all(|word| starts_upper(word))
                && written.get(at + 1).is_some_and(|word| !starts_upper(word))
        })
        .map(|at| at + 1)
        .unwrap_or(0);
    let verb = words.get(verb_at)?.clone();
    if !verb.chars().all(|ch| ch.is_alphabetic()) {
        return None;
    }
    let mut object = Vec::new();
    for word in &words[verb_at + 1..] {
        if CLAUSE_ENDS.contains(&word.as_str()) {
            break;
        }
        if PREPOSITIONS.contains(&word.as_str()) || FILLER.contains(&word.as_str()) {
            continue;
        }
        let word = stem(word);
        if !object.contains(&word) {
            object.push(word);
        }
    }
    // One plain word ("Publish the FAQ") names too little to tell apart.
    let names_enough = object.len() >= 2 || object.iter().any(|word| is_identifier(word));
    names_enough.then_some(Step { verb, object })
}

fn clauses(text: &str) -> Vec<String> {
    let mut out = vec![String::new()];
    let words: Vec<&str> = text.split_whitespace().collect();
    for word in words {
        let lower = word.to_lowercase();
        if matches!(lower.as_str(), "while" | "but" | "although" | "though") {
            out.push(String::new());
            continue;
        }
        let current = out.last_mut().expect("one clause");
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
        if word.ends_with([',', ';', ':', '.', '!', '?']) {
            out.push(String::new());
        }
    }
    out.into_iter()
        .map(|clause| clause.trim().trim_end_matches([',', ';', ':']).to_string())
        .filter(|clause| !clause.is_empty())
        .collect()
}

/// The clause of `later_text` that records `step` as done, if one does.
pub fn done_clause(step: &str, later_text: &str) -> Option<String> {
    let step = read_step(step)?;
    let mut done_words: Vec<String> = DONE_WORDS.iter().map(|word| word.to_string()).collect();
    done_words.extend(past_forms(&step.verb));

    clauses(later_text).into_iter().find(|clause| {
        let words: Vec<String> = tokens(clause).iter().map(|word| stem(word)).collect();
        step.object.iter().all(|word| words.contains(word))
            && words.iter().any(|word| done_words.contains(word))
            && !words.iter().any(|word| NOT_DONE.contains(&word.as_str()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIVE: &str = "Rafael O. confirmed the LL-1482 fix is live in production with 1.8% spam placement and closed the ticket, taking the blocker off the list before the go/no-go.";
    const CHECKLIST_V4: &str = "Updated the Smart Reminders launch checklist v4: the go/no-go meeting is now booked, while LL-1482 and the announcement email are still open.";
    const CHECKLIST_V3: &str = "Updated the Smart Reminders launch checklist v3: LL-1482, the announcement email, and booking the go/no-go meeting are still open.";
    const BOARD: &str = "Checked the Sprint 14 board: LL-1482 is still in progress as the only launch blocker, LL-1490 is in review, and 11 of 15 issues are done.";
    const DECK: &str = "Filled in the launch review recommendation slide: GO for a 10% rollout now that LL-1482 is closed at 1.8% spam placement.";

    #[test]
    fn an_identifier_and_a_done_word_in_one_clause_mark_the_step() {
        let clause = done_clause("Close LL-1482", LIVE).expect("recorded as done");
        assert!(clause.contains("LL-1482 fix is live"), "{clause}");
    }

    #[test]
    fn the_steps_own_verb_in_the_past_counts() {
        assert_eq!(
            done_clause("Book the go/no-go meeting", CHECKLIST_V4).as_deref(),
            Some("the go/no-go meeting is now booked")
        );
        assert!(done_clause(
            "Fill in the recommendation slide after LL-1482 closes",
            DECK
        )
        .is_some());
        assert!(done_clause(
            "Send the Calendar invite",
            "Sent the Calendar invite for the Smart Reminders go/no-go at 2:00 PM."
        )
        .is_some());
    }

    #[test]
    fn a_clause_that_says_still_open_never_marks_a_step() {
        assert_eq!(done_clause("Close LL-1482", CHECKLIST_V4), None);
        assert_eq!(done_clause("Close LL-1482", CHECKLIST_V3), None);
        assert_eq!(done_clause("Book the go/no-go meeting", CHECKLIST_V3), None);
        assert_eq!(done_clause("Close LL-1482", BOARD), None);
    }

    #[test]
    fn a_done_word_about_something_else_does_not_count() {
        // "done" is about the other issues, in another clause.
        assert_eq!(done_clause("Chase the LL-1490 review", BOARD), None);
        // The same verb, another object.
        assert_eq!(
            done_clause(
                "Send final screenshots to Hannah K.",
                "Sent the Calendar invite to eng."
            ),
            None
        );
        // The object, no done word.
        assert_eq!(
            done_clause(
                "Publish the FAQ to the help center",
                "Reviewed the help center FAQ with Noor A."
            ),
            None
        );
    }

    #[test]
    fn a_person_named_before_the_verb_is_not_the_verb() {
        assert!(done_clause(
            "Rafael to fix DKIM alignment on the reminders subdomain",
            "Rafael O. fixed DKIM alignment on the reminders subdomain."
        )
        .is_some());
    }

    #[test]
    fn a_step_that_names_too_little_is_never_marked() {
        assert_eq!(
            done_clause("Follow up", "Followed up and it is done."),
            None
        );
        assert_eq!(done_clause("Publish the FAQ", "The FAQ is done."), None);
    }

    #[test]
    fn every_word_of_what_the_step_acts_on_is_required() {
        // Seen on the seeded store: "spam placement" and "live" matched a
        // clause about the production fix, not the staging retest.
        assert_eq!(
            done_clause(
                "Retest spam placement on staging after DNS propagates",
                LIVE
            ),
            None
        );
        // The checklist was updated; nothing says the rollout plan went in.
        assert_eq!(
            done_clause(
                "Update the launch checklist with the rollout plan",
                CHECKLIST_V4
            ),
            None
        );
    }

    #[test]
    fn a_second_identifier_after_a_preposition_is_still_required() {
        assert_eq!(
            done_clause("Link LL-1490 to LL-1482", "LL-1490 is merged."),
            None
        );
    }
}
