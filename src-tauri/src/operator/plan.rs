//! The plan a spoken request becomes, and how FNDR checks that each step landed.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const MAX_STEPS: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepAction {
    /// FNDR launches or focuses the app natively.
    OpenApp,
    /// FNDR opens an http(s) link in the default browser.
    OpenUrl,
    /// Codex operates the app's UI through the computer-use tools.
    Operate,
    /// FNDR reopens a memory it resolved itself (ADR 027). Never planned by
    /// a model: the schema does not offer it and `parse_plan` refuses it.
    ReopenMemory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepCheck {
    /// The app is frontmost.
    Frontmost,
    /// A media app is playing.
    MediaPlaying,
    /// The browser is frontmost on the requested page.
    PageLoaded,
    /// Only the operator's own report.
    None,
    /// The reopen core's typed outcome says the target opened.
    Reopened,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanStep {
    pub action: StepAction,
    /// Short label for the notch, e.g. "Open Spotify".
    pub label: String,
    #[serde(default)]
    pub app: String,
    #[serde(default)]
    pub url: String,
    /// What `operate` should achieve, in the person's words.
    #[serde(default)]
    pub goal: String,
    pub check: StepCheck,
    /// The memory a `reopen_memory` step opens. Only FNDR's code sets it:
    /// it is never read from a model's plan.
    #[serde(default, skip_deserializing, skip_serializing_if = "Option::is_none")]
    pub item: Option<crate::workset::WorkItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Plan {
    pub steps: Vec<PlanStep>,
}

/// JSON schema the planner turn must answer with (strict structured output).
pub fn plan_output_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["steps"],
        "properties": {
            "steps": {
                "type": "array",
                "maxItems": MAX_STEPS,
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["action", "label", "app", "url", "goal", "check"],
                    "properties": {
                        "action": { "type": "string", "enum": ["open_app", "open_url", "operate"] },
                        "label": { "type": "string" },
                        "app": { "type": "string" },
                        "url": { "type": "string" },
                        "goal": { "type": "string" },
                        "check": { "type": "string", "enum": ["frontmost", "media_playing", "page_loaded", "none"] }
                    }
                }
            }
        }
    })
}

/// JSON schema for the report at the end of an `operate` step.
pub fn step_report_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["done", "detail"],
        "properties": {
            "done": { "type": "boolean" },
            "detail": { "type": "string" }
        }
    })
}

/// Parses and validates the planner's answer. A plan that cannot run as
/// written is refused here, before anything touches the Mac.
const NOTHING_TO_DO: &str = "Nothing to do in that request.";

/// Whether the request's words ask for something Notch Do never does.
fn asks_for_something_never_done(request: &str) -> bool {
    static NEVER: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let never = NEVER.get_or_init(|| {
        regex::Regex::new(
            r"(?i)\b(send|sends|sending|email|message|text|reply|delete|deletes|remove|erase|trash|buy|purchase|pay|order|checkout|password|passcode|sign\s+in|log\s+in|terminal|system\s+settings)\b",
        )
        .expect("never-tier pattern compiles")
    });
    never.is_match(request)
}

/// What a finished run adds when the request also asked for something Notch
/// Do never does. The planner leaves that part out, so without this a run
/// for "buy it" would end in a plain "Done".
pub const LEFT_OUT: &str = "Left out: Notch Do does not send, delete, buy, enter passwords or change settings.";

pub fn with_left_out_note(summary: String, request: &str) -> String {
    if asks_for_something_never_done(request) {
        format!("{summary} {LEFT_OUT}")
    } else {
        summary
    }
}

/// Says why a request came back with no steps. The planner leaves out
/// what Notch Do never does, so a request that was only that comes back
/// empty; the person should hear the reason, not "nothing to do".
pub fn explain_empty_plan(error: String, request: &str) -> String {
    if error != NOTHING_TO_DO {
        return error;
    }
    if asks_for_something_never_done(request) {
        "Notch Do does not send, delete, buy, enter passwords or change settings, so there was nothing it could do for that.".to_string()
    } else {
        error
    }
}

pub fn parse_plan(text: &str) -> Result<Plan, String> {
    let mut plan: Plan =
        serde_json::from_str(text.trim()).map_err(|e| format!("The plan was not readable: {e}"))?;
    if plan.steps.is_empty() {
        return Err(NOTHING_TO_DO.to_string());
    }
    if plan.steps.len() > MAX_STEPS {
        return Err(format!("That plan has more than {MAX_STEPS} steps."));
    }
    for step in &mut plan.steps {
        step.app = step.app.trim().to_string();
        step.url = step.url.trim().to_string();
        step.label = step.label.trim().to_string();
        match step.action {
            StepAction::OpenApp | StepAction::Operate if step.app.is_empty() => {
                return Err(format!("Step \"{}\" names no app.", step.label));
            }
            StepAction::OpenUrl => {
                let lower = step.url.to_lowercase();
                if !(lower.starts_with("https://") || lower.starts_with("http://")) {
                    return Err(format!("Step \"{}\" is not a web link.", step.label));
                }
            }
            StepAction::ReopenMemory => {
                return Err(format!(
                    "Step \"{}\" reopens a memory, which only FNDR may plan.",
                    step.label
                ));
            }
            _ => {}
        }
        if step.label.is_empty() {
            step.label = match step.action {
                StepAction::OpenApp => format!("Open {}", step.app),
                StepAction::OpenUrl => "Open the page".to_string(),
                StepAction::Operate => step.goal.clone(),
                StepAction::ReopenMemory => String::new(),
            };
        }
    }
    Ok(plan)
}

/// Whether a request points at something in the past ("the song from
/// yesterday", "that paper I was reading"). Decided on the Mac, before any
/// request leaves it; only then are memory snippets sent.
pub fn refers_to_past(request: &str) -> bool {
    static PAST: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let pattern = PAST.get_or_init(|| {
        regex::Regex::new(
            r"(?ix)
            \b(yesterday|earlier|again|ago|previous(ly)?|last\s+(night|week|month|time)|this\s+(morning|afternoon))\b
            | \b(i|we)\s+(was|were|had\s+been)\s+\w+ing\b
            | \b(i|we)\s+(read|watched|saw|heard|played|opened|visited|listened)\b
            | \bthe\s+one\s+(i|we)\b",
        )
        .expect("past-reference pattern compiles")
    });
    pattern.is_match(request)
}

/// Whether a request asks for a whole piece of work back ("pull up
/// everything related to the assignment", "the essay I was working on").
/// Decided on the Mac; such a request is resolved from memories here and
/// never sent to a planner (ADR 027).
pub fn asks_for_work_set(request: &str) -> bool {
    static WORK_SET: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let pattern = WORK_SET.get_or_init(|| {
        regex::Regex::new(
            r"(?ix)
            \b(pull|bring|get|set|load|gather|open|reopen|show)\s+(me\s+)?(it\s+)?(up|out|back(\s+up)?|again)?\s*
              (everything|all\s+(of\s+)?(my|the)\s+(stuff|things|tabs|files|windows|docs|pages|work)|all\s+(my|the)\s+\w+\s+(stuff|things|tabs|files)|(my|the)\s+(stuff|things|whole\s+\w+|workspace|setup))\b
            | \b(pull|bring|set|get)\s+(it\s+|everything\s+|my\s+\w+\s+|the\s+\w+\s+)?(up|back\s+up)\s+(for|from|related\s+to|on)\b
            | \b(the|my|that)\s+(\w+\s+){0,3}(i|we)\s+(was|were|had\s+been|am|are)\s+working\s+on\b
            | \bwhat\s+(i|we)\s+(was|were|had\s+been)\s+working\s+on\b",
        )
        .expect("work-set pattern compiles")
    });
    pattern.is_match(request) && !asks_for_something_never_done(request)
}

/// The plan for a work set: one `reopen_memory` step per item, in the
/// set's order. Built by code from what FNDR resolved, never by a model.
pub fn from_work_set(set: &crate::workset::WorkSet) -> Plan {
    Plan {
        steps: set
            .items
            .iter()
            .take(MAX_STEPS)
            .map(|item| PlanStep {
                action: StepAction::ReopenMemory,
                label: format!("Open {}", item.label),
                app: item.app_name.clone(),
                url: String::new(),
                goal: String::new(),
                check: StepCheck::Reopened,
                item: Some(item.clone()),
            })
            .collect(),
    }
}

/// Whether a reopen landed, from the reopen core's typed outcome. FNDR
/// checked the target exists and handed it to macOS; no model is asked.
pub fn verify_reopen(outcome: &crate::memory::reopen::ReopenOutcome) -> Verdict {
    use crate::memory::reopen::ReopenOutcome as O;
    let file = |path: &str| {
        std::path::Path::new(path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(path)
            .to_string()
    };
    match outcome {
        O::Opened => verdict(true, "Opened"),
        O::OpenedMoved { new_path } => verdict(
            true,
            format!("Opened from its new place: {}", file(new_path)),
        ),
        O::AppOnly { app_name } => verdict(
            true,
            format!(
                "Opened {} (the app only)",
                app_name.as_deref().unwrap_or("the app")
            ),
        ),
        O::Missing { path } => verdict(false, format!("{} is no longer there", file(path))),
        O::DriveNotConnected { volume, .. } => {
            verdict(false, format!("The drive {volume} is not connected"))
        }
        O::AppMissing {
            app_name,
            bundle_id,
        } => verdict(
            false,
            format!(
                "{} is not installed",
                app_name.as_deref().unwrap_or(bundle_id.as_str())
            ),
        ),
        O::Blocked { .. } => verdict(false, "FNDR does not open that kind of link"),
        O::NoTarget => verdict(false, "FNDR has no place to reopen for this memory"),
    }
}

/// What FNDR saw on the Mac after a step.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Observation {
    pub frontmost_app: String,
    pub frontmost_bundle: String,
    pub window_title: String,
    pub browser_url: Option<String>,
    pub media_playing: Option<bool>,
    pub media_track: Option<String>,
    pub media_track_before: Option<String>,
    /// The operator's own `done` report for an `operate` step.
    pub reported_done: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Verdict {
    pub ok: bool,
    pub detail: String,
}

fn same_app(wanted: &str, seen: &Observation) -> bool {
    let wanted = wanted.trim().to_lowercase();
    if wanted.is_empty() {
        return false;
    }
    let name = seen.frontmost_app.to_lowercase();
    let bundle = seen.frontmost_bundle.to_lowercase();
    name == wanted
        || name.contains(&wanted)
        || wanted.contains(&name) && !name.is_empty()
        || bundle == wanted
}

fn is_browser(seen: &Observation) -> bool {
    let name = seen.frontmost_app.to_lowercase();
    let bundle = seen.frontmost_bundle.to_lowercase();
    [
        "safari", "chrome", "arc", "dia", "firefox", "brave", "edge", "opera", "vivaldi", "orion",
        "zen",
    ]
    .iter()
    .any(|browser| name.contains(browser))
        || [
            "com.apple.safari",
            "com.google.chrome",
            "company.thebrowser",
            "org.mozilla",
            "com.brave",
            "com.microsoft.edgemac",
        ]
        .iter()
        .any(|prefix| bundle.starts_with(prefix))
}

/// Search words in a link's query (`q=looped+transformers`).
fn query_words(url: &str) -> Vec<String> {
    let query = url.split_once('?').map(|(_, q)| q).unwrap_or_default();
    query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .filter(|(key, _)| matches!(*key, "q" | "query" | "search_query" | "p" | "text"))
        .flat_map(|(_, value)| {
            value
                .split(['+', ' '])
                .map(|word| word.replace("%20", " ").to_lowercase())
                .filter(|word| word.len() > 2)
                .collect::<Vec<_>>()
        })
        .collect()
}

const SEARCH_HOSTS: &[&str] = &["google.com", "bing.com", "duckduckgo.com"];

/// What an operate step reports when the person said no to an action in it.
pub const ACTION_DECLINED: &str = "You said no, so this was not done";

/// What a link step reports when the person said no to opening it.
pub const LINK_DECLINED: &str = "The link was not opened";

/// Whether a planned link is one the person's own words account for: a web
/// search for words they said, or a site they named with nothing attached.
/// Any other link (an unnamed host, a query they did not say, a fragment)
/// waits for a yes, because a link can carry text off the Mac or trigger an
/// action just by being opened.
pub fn link_was_asked_for(url: &str, request: &str) -> bool {
    let request = request.to_lowercase();
    let said: Vec<&str> = request
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect();
    // A space is the one encoded character a search for spoken words needs.
    let lower = url.trim().to_lowercase().replace("%20", "+");
    if lower.contains('#') || lower.contains('@') {
        return false;
    }
    let host = host(&lower);
    let site = host.strip_prefix("www.").unwrap_or(&host);
    let after_host = lower
        .split("://")
        .nth(1)
        .unwrap_or_default()
        .get(host.len()..)
        .unwrap_or_default();
    let (path, query) = after_host.split_once('?').unwrap_or((after_host, ""));
    if query.is_empty() {
        let name = site.split('.').next().unwrap_or_default();
        return name.len() > 2 && said.contains(&name);
    }
    let is_search = SEARCH_HOSTS.contains(&site) && matches!(path, "/search" | "/" | "");
    let single_search_term = query.matches('=').count() == 1 && !query.contains('%');
    let words = query_words(&lower);
    is_search
        && single_search_term
        && !words.is_empty()
        && words.iter().all(|word| said.contains(&word.as_str()))
}

fn host(url: &str) -> String {
    url.split("://")
        .nth(1)
        .unwrap_or(url)
        .split(['/', '?'])
        .next()
        .unwrap_or_default()
        .to_lowercase()
}

/// Whether a player's window title names what the goal asked for. Spotify and
/// Music title the window "Artist - Track" while playing.
fn title_names_goal(goal: &str, title: &str) -> bool {
    const FILLER: &[&str] = &[
        "play", "the", "song", "track", "music", "by", "listen", "to", "some", "start",
    ];
    let title = title.to_lowercase();
    let words: Vec<String> = goal
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| word.len() > 1 && !FILLER.contains(word))
        .map(str::to_string)
        .collect();
    !words.is_empty() && words.iter().all(|word| title.contains(word.as_str()))
}

fn verdict(ok: bool, detail: impl Into<String>) -> Verdict {
    Verdict {
        ok,
        detail: detail.into(),
    }
}

/// Whether a step landed, judged from what FNDR saw rather than what the
/// operator says, except where FNDR has no reading of its own.
const REPORTED_ONLY: &str = "Playback could not be read; went by the operator's report";

/// Whether FNDR itself saw the step's result, as opposed to taking the
/// model's word that it was done. Only a frontmost app, playing media and an
/// open page can be seen; every other step is a report.
pub fn checked_by_fndr(step: &PlanStep, verdict: &Verdict) -> bool {
    verdict.ok && step.check != StepCheck::None && verdict.detail != REPORTED_ONLY
}

pub fn verify(step: &PlanStep, seen: &Observation) -> Verdict {
    match step.check {
        StepCheck::Frontmost => {
            if same_app(&step.app, seen) {
                verdict(true, format!("{} is open", step.app))
            } else {
                verdict(
                    false,
                    format!("{} is not in front ({} is)", step.app, seen.frontmost_app),
                )
            }
        }
        StepCheck::MediaPlaying => match seen.media_playing {
            Some(true) => {
                let track = seen.media_track.clone().unwrap_or_default();
                let unchanged = seen.media_track_before.as_deref() == Some(track.as_str());
                if unchanged && seen.reported_done != Some(true) {
                    verdict(false, "The same track is still playing")
                } else if track.is_empty() {
                    verdict(true, "Playing")
                } else {
                    verdict(true, format!("Playing {track}"))
                }
            }
            Some(false) => verdict(false, "Nothing is playing"),
            None if title_names_goal(&step.goal, &seen.window_title) => verdict(
                true,
                format!("{} shows {}", step.app, seen.window_title.trim()),
            ),
            None => verdict(seen.reported_done == Some(true), REPORTED_ONLY),
        },
        StepCheck::PageLoaded => {
            if !is_browser(seen) {
                return verdict(
                    false,
                    format!("The browser is not in front ({} is)", seen.frontmost_app),
                );
            }
            let words = query_words(&step.url);
            let url_matches = seen.browser_url.as_deref().is_some_and(|url| {
                let url = url.to_lowercase();
                host(&url) == host(&step.url)
                    || words.iter().all(|word| url.contains(word.as_str()))
            });
            let title = seen.window_title.to_lowercase();
            let title_matches =
                !words.is_empty() && words.iter().all(|word| title.contains(word.as_str()));
            if url_matches || title_matches {
                verdict(true, "The page is open")
            } else if seen.browser_url.is_none() && seen.window_title.trim().is_empty() {
                verdict(true, "The browser is in front; the page could not be read")
            } else {
                verdict(false, "The browser is showing a different page")
            }
        }
        StepCheck::None => match seen.reported_done {
            Some(true) => verdict(true, "Done"),
            _ => verdict(false, "The operator could not finish this step"),
        },
        StepCheck::Reopened => verdict(false, "A reopen is judged by its outcome"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_plan_for_something_notch_do_never_does_says_so() {
        let empty = parse_plan(r#"{"steps":[]}"#).unwrap_err();
        for request in [
            "send this email to Sam",
            "delete this file",
            "buy it",
            "type my password",
        ] {
            let said = explain_empty_plan(empty.clone(), request);
            assert!(
                said.starts_with("Notch Do does not send, delete, buy"),
                "{request}: {said}"
            );
        }
        assert_eq!(explain_empty_plan(empty.clone(), "hmm"), NOTHING_TO_DO);
        assert_eq!(
            explain_empty_plan(
                "The plan was not readable: x".to_string(),
                "send this email"
            ),
            "The plan was not readable: x"
        );
    }

    #[test]
    fn a_step_counts_as_checked_only_when_fndr_saw_its_result() {
        let step = |check: &str| -> PlanStep {
            serde_json::from_str(&format!(
                r#"{{"action":"operate","label":"x","app":"Spotify","url":"","goal":"g","check":"{check}"}}"#
            ))
            .unwrap()
        };
        let ok = |detail: &str| Verdict {
            ok: true,
            detail: detail.to_string(),
        };
        assert!(checked_by_fndr(
            &step("media_playing"),
            &ok("Playing Blinding Lights")
        ));
        assert!(checked_by_fndr(&step("frontmost"), &ok("Spotify is open")));
        assert!(!checked_by_fndr(&step("none"), &ok("Done")));
        assert!(!checked_by_fndr(&step("media_playing"), &ok(REPORTED_ONLY)));
        let failed = Verdict {
            ok: false,
            detail: "Nothing is playing".to_string(),
        };
        assert!(!checked_by_fndr(&step("media_playing"), &failed));
    }

    #[test]
    fn a_finished_run_says_what_it_left_out() {
        // Live run, 2026-10-08: "buy it" planned only "Open Google Chrome".
        let done = "Done: Open Google Chrome.".to_string();
        assert_eq!(
            with_left_out_note(done.clone(), "in Chrome, buy it on the page that is open"),
            format!("{done} {LEFT_OUT}")
        );
        assert_eq!(with_left_out_note(done.clone(), "open Chrome"), done);
    }

    #[test]
    fn only_links_the_persons_words_account_for_open_without_asking() {
        let request = "open YouTube, then look up looped transformers";
        for url in [
            "https://www.youtube.com",
            "https://www.youtube.com/",
            "https://www.google.com/search?q=looped+transformers",
            // Models often write a space as %20 (live run, 2026-10-08).
            "https://www.google.com/search?q=looped%20transformers",
        ] {
            assert!(link_was_asked_for(url, request), "{url}");
        }
        for url in [
            "https://evil.example/c?d=looped+transformers",
            "https://www.google.com/search?q=my+bank+balance+is+low",
            "https://www.google.com/search?q=looped+transformers&next=https://evil.example",
            "https://www.google.com/search?q=looped%2Btransformers",
            "https://mail.example/unsubscribe?id=1",
            "http://192.168.1.1/reboot?confirm=1",
            "https://www.youtube.com/watch?v=abc",
            "https://www.youtube.com/#looped",
            "https://youtube@evil.example/",
            "https://vimeo.com",
        ] {
            assert!(!link_was_asked_for(url, request), "{url}");
        }
    }

    const EXAMPLE: &str = r#"{"steps":[
        {"action":"open_app","label":"Open Spotify","app":"Spotify","url":"","goal":"","check":"frontmost"},
        {"action":"operate","label":"Play Blinding Lights","app":"Spotify","url":"","goal":"play Blinding Lights","check":"media_playing"},
        {"action":"open_url","label":"Search looped transformers","app":"","url":"https://www.google.com/search?q=looped+transformers","goal":"","check":"page_loaded"}
    ]}"#;

    fn step(action: StepAction, app: &str, url: &str, check: StepCheck) -> PlanStep {
        PlanStep {
            action,
            label: "x".into(),
            app: app.into(),
            url: url.into(),
            goal: "g".into(),
            check,
            item: None,
        }
    }

    #[test]
    fn parses_the_example_plan_in_order() {
        let plan = parse_plan(EXAMPLE).unwrap();
        let actions: Vec<_> = plan.steps.iter().map(|s| s.action).collect();
        assert_eq!(
            actions,
            vec![
                StepAction::OpenApp,
                StepAction::Operate,
                StepAction::OpenUrl
            ]
        );
    }

    #[test]
    fn rejects_plans_that_cannot_run() {
        assert!(parse_plan(r#"{"steps":[]}"#).is_err(), "empty plan");
        assert!(parse_plan("not json").is_err());
        let bad_url = r#"{"steps":[{"action":"open_url","label":"x","app":"","url":"file:///etc","goal":"","check":"none"}]}"#;
        assert!(parse_plan(bad_url).is_err());
        let no_app = r#"{"steps":[{"action":"operate","label":"x","app":"","url":"","goal":"play","check":"none"}]}"#;
        assert!(parse_plan(no_app).is_err());
        let many = format!(
            r#"{{"steps":[{}]}}"#,
            [r#"{"action":"open_app","label":"x","app":"Notes","url":"","goal":"","check":"frontmost"}"#; MAX_STEPS + 1].join(",")
        );
        assert!(parse_plan(&many).is_err());
    }

    #[test]
    fn past_references_are_detected_and_plain_commands_are_not() {
        for text in [
            "play the song I was listening to yesterday",
            "open that paper I was reading",
            "reopen the doc from last week",
            "play it again",
            "open the article I read this morning",
        ] {
            assert!(refers_to_past(text), "{text}");
        }
        for text in [
            "open Spotify, play Blinding Lights, then open the browser and look up looped transformers",
            "open Safari",
            "search for the best pasta recipe",
        ] {
            assert!(!refers_to_past(text), "{text}");
        }
    }

    #[test]
    fn open_app_lands_when_the_app_is_frontmost() {
        let s = step(StepAction::OpenApp, "Spotify", "", StepCheck::Frontmost);
        let seen = Observation {
            frontmost_app: "Spotify".into(),
            ..Default::default()
        };
        assert!(verify(&s, &seen).ok);
        let other = Observation {
            frontmost_app: "Finder".into(),
            ..Default::default()
        };
        assert!(!verify(&s, &other).ok);
    }

    #[test]
    fn media_step_needs_playback_not_just_a_report() {
        let s = step(StepAction::Operate, "Spotify", "", StepCheck::MediaPlaying);
        let playing = Observation {
            frontmost_app: "Spotify".into(),
            media_playing: Some(true),
            media_track: Some("Blinding Lights".into()),
            reported_done: Some(true),
            ..Default::default()
        };
        assert!(verify(&s, &playing).ok);
        let paused = Observation {
            media_playing: Some(false),
            reported_done: Some(true),
            ..playing.clone()
        };
        assert!(!verify(&s, &paused).ok);
        let unchanged = Observation {
            media_track_before: Some("Blinding Lights".into()),
            reported_done: Some(false),
            ..playing.clone()
        };
        assert!(
            !verify(&s, &unchanged).ok,
            "same track and the operator says it failed"
        );
        let unknown = Observation {
            media_playing: None,
            reported_done: Some(true),
            ..playing
        };
        assert!(
            verify(&s, &unknown).ok,
            "no media reading falls back to the report"
        );
    }

    #[test]
    fn unreadable_playback_falls_back_to_the_players_window_title() {
        let mut s = step(StepAction::Operate, "Spotify", "", StepCheck::MediaPlaying);
        s.goal = "play Blinding Lights".into();
        let titled = Observation {
            frontmost_app: "Spotify".into(),
            window_title: "The Weeknd - Blinding Lights".into(),
            media_playing: None,
            reported_done: Some(false),
            ..Default::default()
        };
        let verdict = verify(&s, &titled);
        assert!(verdict.ok, "{}", verdict.detail);
        assert!(verdict.detail.contains("Blinding Lights"));
        let untitled = Observation {
            window_title: "Spotify Premium".into(),
            ..titled
        };
        assert!(!verify(&s, &untitled).ok);
    }

    #[test]
    fn page_loaded_checks_the_browser_url_or_title() {
        let s = step(
            StepAction::OpenUrl,
            "",
            "https://www.google.com/search?q=looped+transformers",
            StepCheck::PageLoaded,
        );
        let by_url = Observation {
            frontmost_app: "Dia".into(),
            frontmost_bundle: "company.thebrowser.dia".into(),
            browser_url: Some("https://www.google.com/search?q=looped+transformers&sca=1".into()),
            ..Default::default()
        };
        assert!(verify(&s, &by_url).ok);
        let by_title = Observation {
            browser_url: None,
            window_title: "looped transformers - Google Search".into(),
            ..by_url.clone()
        };
        assert!(verify(&s, &by_title).ok);
        let not_browser = Observation {
            frontmost_app: "Spotify".into(),
            frontmost_bundle: "com.spotify.client".into(),
            ..by_url
        };
        assert!(!verify(&s, &not_browser).ok);
    }

    #[test]
    fn operate_without_a_check_trusts_the_report() {
        let s = step(StepAction::Operate, "Notes", "", StepCheck::None);
        assert!(
            verify(
                &s,
                &Observation {
                    reported_done: Some(true),
                    ..Default::default()
                }
            )
            .ok
        );
        assert!(
            !verify(
                &s,
                &Observation {
                    reported_done: Some(false),
                    ..Default::default()
                }
            )
            .ok
        );
    }

    #[test]
    fn work_set_requests_are_told_apart_from_other_requests() {
        for text in [
            "pull up everything related to the assignment I was working on",
            "Open everything for my essay",
            "set up my workspace for the biology lab",
            "bring up all the stuff from the chem lab",
            "get out everything related to the essay",
            "the assignment I was working on",
            "pull up what I was working on yesterday",
            "set everything back up for the lab report",
            "reopen everything from the biology lab",
            "pull up the doc I was working on",
        ] {
            assert!(asks_for_work_set(text), "{text}");
        }
        for text in [
            "open Spotify, play Blinding Lights, then open the browser and look up looped transformers",
            "open Safari",
            "play the song I was listening to yesterday",
            "open that paper I was reading",
            "search for the best pasta recipe",
            "delete everything related to the essay",
            "pull up everything and email it to Sam",
            "set a timer for 10 minutes",
            "open all tabs in Safari",
            "show me the way to the station",
        ] {
            assert!(!asks_for_work_set(text), "{text}");
        }
    }

    #[test]
    fn a_model_can_never_plan_a_reopen() {
        let planned = r#"{"steps":[{"action":"reopen_memory","label":"x","app":"Safari","url":"","goal":"","check":"reopened","item":{"memoryId":"m","label":"x","kind":"url","reopenRank":4,"appName":"Safari","capturedAt":1}}]}"#;
        assert!(parse_plan(planned)
            .unwrap_err()
            .contains("only FNDR may plan"));
        let smuggled = r#"{"steps":[{"action":"open_app","label":"x","app":"Safari","url":"","goal":"","check":"frontmost","item":{"memoryId":"m","label":"x","kind":"url","reopenRank":4,"appName":"Safari","capturedAt":1}}]}"#;
        assert_eq!(
            parse_plan(smuggled).unwrap().steps[0].item,
            None,
            "an item is never read from a plan"
        );
        let schema = plan_output_schema().to_string();
        assert!(!schema.contains("reopen_memory") && !schema.contains("reopened"));
    }

    fn work_item(id: &str, label: &str) -> crate::workset::WorkItem {
        crate::workset::WorkItem {
            memory_id: id.into(),
            label: label.into(),
            kind: crate::workset::ItemKind::Url,
            reopen_rank: 4,
            app_name: "Google Chrome".into(),
            host: Some("canvas.example".into()),
            page: None,
            captured_at: 1,
        }
    }

    #[test]
    fn a_work_set_becomes_one_reopen_step_per_item() {
        let set = crate::workset::WorkSet {
            id: "ws".into(),
            title: "Assignment 3".into(),
            reason: "r".into(),
            score: 1.0,
            items: vec![
                work_item("a", "Assignment 3"),
                work_item("b", "notes.pdf, page 4"),
            ],
        };
        let plan = from_work_set(&set);
        let labels: Vec<&str> = plan.steps.iter().map(|s| s.label.as_str()).collect();
        assert_eq!(labels, ["Open Assignment 3", "Open notes.pdf, page 4"]);
        assert!(plan
            .steps
            .iter()
            .all(|s| s.action == StepAction::ReopenMemory
                && s.check == StepCheck::Reopened
                && s.app == "Google Chrome"));
        assert_eq!(
            plan.steps[1].item.as_ref().map(|i| i.memory_id.as_str()),
            Some("b")
        );
    }

    #[test]
    fn a_reopen_is_judged_by_its_typed_outcome() {
        use crate::memory::reopen::ReopenOutcome as O;
        let step = from_work_set(&crate::workset::WorkSet {
            id: "ws".into(),
            title: "t".into(),
            reason: "r".into(),
            score: 1.0,
            items: vec![work_item("a", "A")],
        })
        .steps
        .remove(0);
        let opened = verify_reopen(&O::Opened);
        assert!(opened.ok && checked_by_fndr(&step, &opened));
        let moved = verify_reopen(&O::OpenedMoved {
            new_path: "/Users/k/Documents/notes.pdf".into(),
        });
        assert_eq!(moved.detail, "Opened from its new place: notes.pdf");
        for failed in [
            O::Missing {
                path: "/Users/k/a.pdf".into(),
            },
            O::DriveNotConnected {
                volume: "USB".into(),
                path: "/Volumes/USB/a".into(),
            },
            O::AppMissing {
                bundle_id: "com.x".into(),
                app_name: Some("X".into()),
            },
            O::Blocked {
                target: "javascript:x".into(),
            },
            O::NoTarget,
        ] {
            let verdict = verify_reopen(&failed);
            assert!(!verdict.ok, "{failed:?}");
            assert!(!checked_by_fndr(&step, &verdict));
        }
        assert_eq!(
            verify_reopen(&O::Missing {
                path: "/Users/k/a.pdf".into()
            })
            .detail,
            "a.pdf is no longer there"
        );
    }
}
