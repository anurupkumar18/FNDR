//! Screen-aware dictation: hold a key, speak, and the cleaned text lands in the
//! field that had focus when the key went down.
//!
//! This module holds the decisions that need no window server: how a raw
//! transcript is cleaned, how words on the user's screen correct its spelling,
//! and whether the result is typed, copied or dropped. The shortcut, the voice
//! session and the field capture stay in `ipc/commands/dictation.rs`.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

/// A key held for less than this is a tap, not speech.
pub const QUICK_TAP: Duration = Duration::from_millis(250);

const MAX_VOCABULARY: usize = 500;
const MIN_TERM_CHARS: usize = 3;
const FILLERS: &[&str] = &["um", "umm", "uh", "uhh", "er", "erm", "hmm"];

/// Terms read off the screen. Keys are lowercase: the compact spelling
/// (`fieldcontext`) and the spoken one (`field context`).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ScreenVocabulary {
    terms: HashMap<String, String>,
}

impl ScreenVocabulary {
    /// Collects identifiers, acronyms and mid-sentence proper nouns from OCR text.
    pub fn from_screen_text(text: &str) -> Self {
        let mut terms: HashMap<String, String> = HashMap::new();
        let mut ambiguous: HashSet<String> = HashSet::new();
        for term in screen_terms(text) {
            for key in spoken_keys(&term) {
                if ambiguous.contains(&key) {
                    continue;
                }
                match terms.get(&key) {
                    Some(existing) if *existing != term => {
                        terms.remove(&key);
                        ambiguous.insert(key);
                    }
                    Some(_) => {}
                    None if terms.len() < MAX_VOCABULARY => {
                        terms.insert(key, term.clone());
                    }
                    None => {}
                }
            }
        }
        Self { terms }
    }

    /// Words the user taught FNDR. They win over anything read off the screen.
    pub fn with_learned_words(mut self, words: &[String]) -> Self {
        for word in words {
            let word = word.trim();
            if word.chars().count() < MIN_TERM_CHARS {
                continue;
            }
            for key in spoken_keys(word) {
                self.terms.insert(key, word.to_string());
            }
        }
        self
    }

    pub fn len(&self) -> usize {
        self.terms.len()
    }

    pub fn is_empty(&self) -> bool {
        self.terms.is_empty()
    }
}

fn is_term_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn screen_terms(text: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for line in text.lines() {
        let mut sentence_start = true;
        let mut token = String::new();
        let flush = |token: &mut String, sentence_start: &mut bool, found: &mut Vec<String>| {
            if token.is_empty() {
                return;
            }
            if qualifies(token, *sentence_start) {
                found.push(std::mem::take(token));
            } else {
                token.clear();
            }
            *sentence_start = false;
        };
        for c in line.chars() {
            if is_term_char(c) {
                token.push(c);
                continue;
            }
            flush(&mut token, &mut sentence_start, &mut found);
            if matches!(c, '.' | '!' | '?' | ':') {
                sentence_start = true;
            }
        }
        flush(&mut token, &mut sentence_start, &mut found);
    }
    found
}

fn qualifies(token: &str, sentence_start: bool) -> bool {
    if token.chars().count() < MIN_TERM_CHARS || !token.chars().any(char::is_alphabetic) {
        return false;
    }
    if token.contains('_') {
        return true;
    }
    let mut chars = token.chars();
    let first = chars.next().unwrap_or(' ');
    let rest: Vec<char> = chars.collect();
    if rest.iter().any(|c| c.is_uppercase()) {
        return true;
    }
    first.is_uppercase() && !sentence_start && token.chars().count() > MIN_TERM_CHARS
}

/// `FieldContext` is said "field context"; `capture_focused_context` is said the same way.
fn spoken_keys(term: &str) -> Vec<String> {
    let compact: String = term.chars().filter(|c| *c != '_').collect::<String>().to_lowercase();
    let mut parts: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut previous: Option<char> = None;
    for c in term.chars() {
        let boundary = c == '_' || (c.is_uppercase() && previous.is_some_and(char::is_lowercase));
        if boundary && !current.is_empty() {
            parts.push(std::mem::take(&mut current).to_lowercase());
        }
        if c != '_' {
            current.push(c);
        }
        previous = Some(c);
    }
    if !current.is_empty() {
        parts.push(current.to_lowercase());
    }
    let mut keys = vec![compact];
    if parts.len() >= 2 {
        keys.push(parts.join(" "));
    }
    keys
}

struct Chunk<'a> {
    lead: &'a str,
    core: &'a str,
    trail: &'a str,
}

fn split_chunk(raw: &str) -> Chunk<'_> {
    let start = raw.find(is_term_char).unwrap_or(raw.len());
    let end = raw.rfind(is_term_char).map_or(start, |i| {
        i + raw[i..].chars().next().map_or(0, char::len_utf8)
    });
    Chunk {
        lead: &raw[..start],
        core: &raw[start..end],
        trail: &raw[end..],
    }
}

const LONGEST_SPOKEN_TERM: usize = 4;

/// Drops filler words, restores the spelling of terms on screen, and capitalises.
pub fn clean_transcript(raw: &str, vocabulary: &ScreenVocabulary) -> String {
    let chunks: Vec<Chunk> = raw
        .split_whitespace()
        .map(split_chunk)
        .filter(|chunk| !FILLERS.contains(&chunk.core.to_lowercase().as_str()))
        .collect();

    let mut words: Vec<String> = Vec::with_capacity(chunks.len());
    let mut first_is_term = false;
    let mut i = 0;
    while i < chunks.len() {
        let mut matched: Option<(usize, String)> = None;
        if !vocabulary.is_empty() {
            let longest = LONGEST_SPOKEN_TERM.min(chunks.len() - i);
            for n in (1..=longest).rev() {
                let joined_cleanly = chunks[i..i + n].iter().enumerate().all(|(k, c)| {
                    (k == 0 || c.lead.is_empty()) && (k == n - 1 || c.trail.is_empty())
                });
                if !joined_cleanly || chunks[i..i + n].iter().any(|c| c.core.is_empty()) {
                    continue;
                }
                let key = chunks[i..i + n]
                    .iter()
                    .map(|c| c.core.to_lowercase())
                    .collect::<Vec<_>>()
                    .join(" ");
                if let Some(term) = vocabulary.terms.get(&key) {
                    matched = Some((n, term.clone()));
                    break;
                }
            }
        }
        match matched {
            Some((n, term)) => {
                if words.is_empty() {
                    first_is_term = true;
                }
                words.push(format!("{}{}{}", chunks[i].lead, term, chunks[i + n - 1].trail));
                i += n;
            }
            None => {
                let c = &chunks[i];
                words.push(format!("{}{}{}", c.lead, c.core, c.trail));
                i += 1;
            }
        }
    }

    let mut out = words.join(" ");
    if !first_is_term {
        if let Some(first) = out.chars().next() {
            if first.is_lowercase() {
                let upper: String = first.to_uppercase().collect();
                out.replace_range(..first.len_utf8(), &upper);
            }
        }
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InsertFacts {
    pub secure_input_active: bool,
    pub target_captured: bool,
    pub target_still_focused: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardReason {
    FocusMoved,
    NoTarget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    Empty,
    SecureField,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InsertPlan {
    Type(String),
    Clipboard(String, ClipboardReason),
    Skip(SkipReason),
}

/// What a `voice://state` event means for a dictation session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VoiceOutcome {
    Final { session_id: String, text: String },
    Ended { session_id: String },
}

/// Reads a `voice://state` payload. Events for other surfaces and the
/// listening and partial states that precede the end are `None`.
pub fn parse_voice_event(payload: &str) -> Option<VoiceOutcome> {
    let event: serde_json::Value = serde_json::from_str(payload).ok()?;
    if event.get("surface")?.as_str()? != "dictation" {
        return None;
    }
    let session_id = event.get("sessionId")?.as_str()?.to_string();
    let state = event.get("state")?;
    match state.get("kind")?.as_str()? {
        "final" => Some(VoiceOutcome::Final {
            session_id,
            text: state.get("text")?.as_str()?.to_string(),
        }),
        "error" | "unavailable" | "stop_word" | "speech_ignored" | "idle" => {
            Some(VoiceOutcome::Ended { session_id })
        }
        _ => None,
    }
}

/// A dictated password never reaches the clipboard: a secure field skips both paths.
pub fn plan_insertion(text: &str, facts: InsertFacts) -> InsertPlan {
    let text = text.trim();
    if text.is_empty() {
        return InsertPlan::Skip(SkipReason::Empty);
    }
    if facts.secure_input_active {
        return InsertPlan::Skip(SkipReason::SecureField);
    }
    if !facts.target_captured {
        return InsertPlan::Clipboard(text.to_string(), ClipboardReason::NoTarget);
    }
    if !facts.target_still_focused {
        return InsertPlan::Clipboard(text.to_string(), ClipboardReason::FocusMoved);
    }
    InsertPlan::Type(text.to_string())
}

pub fn is_quick_tap(held: Duration) -> bool {
    held < QUICK_TAP
}

/// How long the key was down, measured between the two key events themselves.
/// Capturing the field and starting the microphone take time after key-down; a
/// hold timed from the end of that work would read as a tap.
pub fn held_for(pressed: Instant, released: Instant) -> Duration {
    released.saturating_duration_since(pressed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vocab(screen: &str) -> ScreenVocabulary {
        ScreenVocabulary::from_screen_text(screen)
    }

    #[test]
    fn drops_filler_words_and_collapses_spaces() {
        let out = clean_transcript("um  send the, uh, report  to Sam", &ScreenVocabulary::default());
        assert_eq!(out, "Send the, report to Sam");
    }

    #[test]
    fn keeps_words_that_only_contain_a_filler() {
        let out = clean_transcript("the umbrella is here", &ScreenVocabulary::default());
        assert_eq!(out, "The umbrella is here");
    }

    #[test]
    fn capitalises_the_first_letter_only() {
        assert_eq!(
            clean_transcript("open the pull request", &ScreenVocabulary::default()),
            "Open the pull request"
        );
    }

    #[test]
    fn empty_and_filler_only_input_clean_to_nothing() {
        assert_eq!(clean_transcript("   ", &ScreenVocabulary::default()), "");
        assert_eq!(clean_transcript("um uh", &ScreenVocabulary::default()), "");
    }

    #[test]
    fn camel_case_identifiers_on_screen_fix_the_spelling() {
        let v = vocab("fn inject_text_into_field(); struct FieldContext { label }");
        assert_eq!(
            clean_transcript("read the field context struct", &v),
            "Read the FieldContext struct"
        );
    }

    #[test]
    fn snake_case_identifiers_on_screen_fix_the_spelling() {
        let v = vocab("call capture_focused_context first");
        assert_eq!(
            clean_transcript("call capture_focused_context now", &v),
            "Call capture_focused_context now"
        );
    }

    #[test]
    fn acronyms_and_product_names_on_screen_fix_the_spelling() {
        let v = vocab("Ask the LanceDB index about the iPhone build, then check OAuth");
        assert_eq!(
            clean_transcript("lancedb and iphone and oauth", &v),
            "LanceDB and iPhone and OAuth"
        );
    }

    #[test]
    fn mid_sentence_proper_nouns_count_but_sentence_starts_do_not() {
        let v = vocab("Meeting notes. Invite Priyanka to the sync. Thanks for coming");
        assert_eq!(clean_transcript("priyanka is in", &v), "Priyanka is in");
        assert_eq!(clean_transcript("thanks again", &v), "Thanks again");
        assert!(v.terms.get("thanks").is_none());
        assert!(v.terms.get("meeting").is_none());
    }

    #[test]
    fn punctuation_next_to_a_term_is_kept() {
        let v = vocab("see FieldContext");
        assert_eq!(
            clean_transcript("is it field context?", &v),
            "Is it FieldContext?"
        );
        assert_eq!(clean_transcript("fieldcontext, then", &v), "FieldContext, then");
    }

    #[test]
    fn a_term_spelled_two_ways_on_screen_is_not_used() {
        let v = vocab("both iPhone and IPhone appear");
        assert!(v.terms.get("iphone").is_none());
    }

    #[test]
    fn learned_words_override_the_screen() {
        let v = vocab("the iPhone case").with_learned_words(&["IPHONE".to_string()]);
        assert_eq!(clean_transcript("iphone", &v), "IPHONE");
    }

    #[test]
    fn learned_words_work_with_a_blank_screen() {
        let v = ScreenVocabulary::default().with_learned_words(&["Kunj".to_string()]);
        assert_eq!(clean_transcript("hi kunj", &v), "Hi Kunj");
    }

    #[test]
    fn vocabulary_is_capped() {
        let screen: String = (0..2000).map(|i| format!("itemName{i} ")).collect();
        assert!(vocab(&screen).len() <= MAX_VOCABULARY);
    }

    #[test]
    fn short_tokens_are_ignored() {
        let v = vocab("go to aB and xY");
        assert!(v.is_empty());
    }

    fn facts(secure: bool, captured: bool, focused: bool) -> InsertFacts {
        InsertFacts {
            secure_input_active: secure,
            target_captured: captured,
            target_still_focused: focused,
        }
    }

    #[test]
    fn types_into_the_field_that_kept_focus() {
        assert_eq!(
            plan_insertion("Hello", facts(false, true, true)),
            InsertPlan::Type("Hello".into())
        );
    }

    #[test]
    fn copies_when_focus_moved() {
        assert_eq!(
            plan_insertion("Hello", facts(false, true, false)),
            InsertPlan::Clipboard("Hello".into(), ClipboardReason::FocusMoved)
        );
    }

    #[test]
    fn copies_when_no_field_was_captured() {
        assert_eq!(
            plan_insertion("Hello", facts(false, false, false)),
            InsertPlan::Clipboard("Hello".into(), ClipboardReason::NoTarget)
        );
    }

    #[test]
    fn a_secure_field_gets_nothing_and_the_clipboard_stays_clean() {
        assert_eq!(
            plan_insertion("hunter2", facts(true, true, true)),
            InsertPlan::Skip(SkipReason::SecureField)
        );
        assert_eq!(
            plan_insertion("hunter2", facts(true, false, false)),
            InsertPlan::Skip(SkipReason::SecureField)
        );
    }

    #[test]
    fn empty_text_is_skipped_before_anything_else() {
        assert_eq!(
            plan_insertion("  ", facts(false, true, true)),
            InsertPlan::Skip(SkipReason::Empty)
        );
    }

    #[test]
    fn reads_the_final_text_of_a_dictation_session() {
        let payload = r#"{"version":1,"sessionId":"s-1","surface":"dictation","state":{"kind":"final","text":"hello there"}}"#;
        assert_eq!(
            parse_voice_event(payload),
            Some(VoiceOutcome::Final {
                session_id: "s-1".into(),
                text: "hello there".into()
            })
        );
    }

    #[test]
    fn errors_and_missing_speech_end_the_session() {
        for kind in ["error", "unavailable", "stop_word", "speech_ignored", "idle"] {
            let payload = format!(
                r#"{{"version":1,"sessionId":"s-2","surface":"dictation","state":{{"kind":"{kind}"}}}}"#
            );
            assert_eq!(
                parse_voice_event(&payload),
                Some(VoiceOutcome::Ended {
                    session_id: "s-2".into()
                }),
                "{kind}"
            );
        }
    }

    #[test]
    fn listening_and_partial_states_keep_the_session_open() {
        for state in [r#"{"kind":"listening","level":0.2}"#, r#"{"kind":"partial","text":"hel"}"#] {
            let payload = format!(
                r#"{{"version":1,"sessionId":"s-3","surface":"dictation","state":{state}}}"#
            );
            assert_eq!(parse_voice_event(&payload), None);
        }
    }

    #[test]
    fn other_surfaces_and_junk_are_not_dictation() {
        let other = r#"{"version":1,"sessionId":"s-4","surface":"screen_guide","state":{"kind":"final","text":"x"}}"#;
        assert_eq!(parse_voice_event(other), None);
        assert_eq!(parse_voice_event("not json"), None);
        assert_eq!(parse_voice_event(r#"{"surface":"dictation"}"#), None);
    }

    #[test]
    fn a_slow_start_does_not_turn_a_long_hold_into_a_tap() {
        let pressed = Instant::now();
        let released = pressed + Duration::from_millis(1500);
        assert!(!is_quick_tap(held_for(pressed, released)));
        assert!(is_quick_tap(held_for(pressed, pressed + Duration::from_millis(90))));
        assert_eq!(held_for(released, pressed), Duration::ZERO);
    }

    #[test]
    fn a_short_hold_is_a_tap() {
        assert!(is_quick_tap(Duration::from_millis(120)));
        assert!(!is_quick_tap(Duration::from_millis(250)));
        assert!(!is_quick_tap(Duration::from_secs(2)));
    }
}
