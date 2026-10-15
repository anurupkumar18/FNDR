//! The plan a spoken request becomes, and how FNDR checks that each step landed.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const MAX_STEPS: usize = 8;
/// The most a `paste` step puts into a field: a link, an address, a short
/// paragraph, small enough to read in full on the approval card.
pub const MAX_PASTE_CHARS: usize = 500;

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
    /// FNDR pastes `text` into the focused field of `app` (or of the app in
    /// front) once the person has tapped Allow on the exact text. Empty
    /// `text` pastes what is on the clipboard, which an earlier step copied.
    Paste,
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
    /// The focused field holds the pasted text.
    Pasted,
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
    /// What a `paste` step puts into the focused field, exactly as the
    /// person sees it before tapping Allow; empty for the clipboard.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub text: String,
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
                    "required": ["action", "label", "app", "url", "goal", "text", "check"],
                    "properties": {
                        "action": { "type": "string", "enum": ["open_app", "open_url", "operate", "paste"] },
                        "label": { "type": "string" },
                        "app": { "type": "string" },
                        "url": { "type": "string" },
                        "goal": { "type": "string" },
                        "text": { "type": "string" },
                        "check": { "type": "string", "enum": ["frontmost", "media_playing", "page_loaded", "pasted", "none"] }
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

/// Where a request goes, decided on this Mac before anything leaves it. The
/// run and the evaluation suite (`operator::eval`) both ask this one function.
#[derive(Debug, Clone, PartialEq)]
pub enum Route {
    /// "Save this as <name>": the set this session opened last is saved.
    SaveSet { name: String },
    /// A set the person saved, by its index in their saved names.
    SavedSet {
        index: usize,
        layout: Option<crate::operator::layout::Layout>,
    },
    /// A whole piece of work, resolved from memories with no model (ADR 027).
    WorkSet {
        layout: Option<crate::operator::layout::Layout>,
    },
    /// A saved skill, by its index in the saved skill names: its steps run
    /// again with no planning.
    Skill { index: usize },
    /// Only opening apps, opening links and web searches: planned here by
    /// `local_plan`, carried out with no Codex session.
    Local { plan: Plan },
    /// The cloud planner turn, with memory snippets only when the request
    /// points at the past.
    Planner { memory_snippets: bool },
}

/// What a request is routed against, all of it on this Mac.
pub struct Known<'a> {
    /// The names of the person's saved sets.
    pub saved_sets: &'a [String],
    /// The names of the person's saved skills.
    pub skills: &'a [String],
    /// Whether a name is an app installed on this Mac.
    pub is_app: &'a dyn Fn(&str) -> bool,
}

pub fn route(request: &str, known: &Known) -> Route {
    if let Some(name) = save_set_name(request) {
        return Route::SaveSet { name };
    }
    if let Some(index) = crate::operator::skills::skill_asked_for(request, known.skills) {
        return Route::Skill { index };
    }
    let layout = layout_asked_for(request);
    if let Some(index) = named_set_asked_for(request, known.saved_sets) {
        return Route::SavedSet { index, layout };
    }
    if asks_for_work_set(request) {
        return Route::WorkSet { layout };
    }
    if let Some(plan) = local_plan(request, known.is_app) {
        return Route::Local { plan };
    }
    Route::Planner {
        memory_snippets: refers_to_past(request),
    }
}

/// The plan without steps that change nothing: opening the app that is
/// already in front right before operating it. The planner adds one for
/// "summarize this page" (3 of 23 recorded answers, 2026-10-14), and each
/// costs a launch, a wait and a check.
pub fn settled(mut plan: Plan, in_front: Option<&str>) -> Plan {
    let Some(front) = in_front.map(str::trim).filter(|app| !app.is_empty()) else {
        return plan;
    };
    let mut index = 0;
    while index + 1 < plan.steps.len() {
        let (step, next) = (&plan.steps[index], &plan.steps[index + 1]);
        let opens_front =
            step.action == StepAction::OpenApp && step.app.trim().eq_ignore_ascii_case(front);
        let then_operates =
            next.action == StepAction::Operate && next.app.trim().eq_ignore_ascii_case(front);
        if opens_front && then_operates {
            plan.steps.remove(index);
        } else {
            index += 1;
        }
    }
    plan
}

/// Words that open an app or a link.
const OPEN_VERBS: &[&str] = &[
    "open",
    "launch",
    "start",
    "go to",
    "switch to",
    "bring up",
    "show",
];

/// Words that ask for a web search; the rest of the clause is the query.
const SEARCH_VERBS: &[&str] = &[
    "look up",
    "search the web for",
    "search google for",
    "search online for",
    "search for",
    "google",
];

/// Words before a request that change nothing.
const REQUEST_LEAD: &[&str] = &["please", "can you", "could you", "hey fndr", "fndr"];

/// A search query for a URL: unreserved characters kept, spaces as `+`,
/// every other byte as `%XX`.
fn query_encoded(query: &str) -> String {
    query
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (byte as char).to_string()
            }
            b' ' => "+".to_string(),
            other => format!("%{other:02X}"),
        })
        .collect()
}

/// Browsers as people name them in a request ("in Safari", "in Chrome").
const BROWSER_NAMES: &[&str] = &[
    "safari",
    "chrome",
    "google chrome",
    "arc",
    "dia",
    "firefox",
    "brave",
    "brave browser",
    "edge",
    "microsoft edge",
    "opera",
    "vivaldi",
    "orion",
    "zen",
];

/// Whether a name, as said, is a web browser.
pub fn is_browser_name(name: &str) -> bool {
    BROWSER_NAMES.contains(&name.trim().to_lowercase().as_str())
}

/// "github.com in Safari": the text before a browser named at the end, and
/// that browser, when it is one and is installed.
fn in_browser(text: &str, is_app: &dyn Fn(&str) -> bool) -> Option<(String, String)> {
    let at = text.to_lowercase().rfind(" in ")?;
    let (head, place) = (text[..at].trim(), text[at + 4..].trim());
    (!head.is_empty() && is_browser_name(place) && is_app(place))
        .then(|| (head.to_string(), place.to_string()))
}

/// A clause that opens an app, opens a link or searches the web, as a step.
/// None for anything else, so the request goes to the planner.
fn local_step(clause: &str, is_app: &dyn Fn(&str) -> bool) -> Option<PlanStep> {
    let mut clause = clause
        .trim()
        .trim_end_matches(['.', '!', '?'])
        .trim()
        .to_string();
    loop {
        let lower = clause.to_lowercase();
        let Some(lead) = REQUEST_LEAD
            .iter()
            .find(|lead| lower.starts_with(&format!("{lead} ")))
        else {
            break;
        };
        clause = clause[lead.len()..].trim().to_string();
    }
    let lower = clause.to_lowercase();
    let after = |verb: &str| -> Option<String> {
        lower
            .starts_with(&format!("{verb} "))
            .then(|| clause[verb.len()..].trim().to_string())
            .filter(|rest| !rest.is_empty())
    };
    if let Some(query) = SEARCH_VERBS.iter().find_map(|verb| after(verb)) {
        let (query, browser) = match in_browser(&query, is_app) {
            Some((query, browser)) => (query, browser),
            None => (query, String::new()),
        };
        let encoded = query_encoded(&query);
        let label = format!(
            "Look up {}",
            query
                .split_whitespace()
                .take(3)
                .collect::<Vec<_>>()
                .join(" ")
        );
        return Some(PlanStep {
            action: StepAction::OpenUrl,
            label,
            app: browser,
            url: format!("https://www.google.com/search?q={encoded}"),
            goal: String::new(),
            text: String::new(),
            check: StepCheck::PageLoaded,
            item: None,
        });
    }
    let target = OPEN_VERBS.iter().find_map(|verb| after(verb))?;
    let (target, browser) = match in_browser(&target, is_app) {
        Some((target, browser)) => (target, browser),
        None => (target, String::new()),
    };
    // "in Notes", "on my phone": a place the planner has to reason about.
    if target
        .split_whitespace()
        .any(|word| matches!(word.to_lowercase().as_str(), "in" | "on" | "with" | "from"))
    {
        return None;
    }
    let target = target
        .strip_prefix("the ")
        .or_else(|| target.strip_prefix("The "))
        .unwrap_or(&target)
        .trim();
    let target = target
        .strip_suffix(" app")
        .or_else(|| target.strip_suffix(" App"))
        .unwrap_or(target)
        .trim();
    let looks_like_link = !target.contains(' ')
        && (target.starts_with("http://")
            || target.starts_with("https://")
            || target.rsplit_once('.').is_some_and(|(host, tld)| {
                !host.is_empty() && tld.len() >= 2 && tld.chars().all(|c| c.is_ascii_alphabetic())
            }));
    if looks_like_link {
        let url = if target.starts_with("http") {
            target.to_string()
        } else {
            format!("https://{target}")
        };
        return Some(PlanStep {
            action: StepAction::OpenUrl,
            label: format!("Open {target}"),
            app: browser,
            url,
            goal: String::new(),
            text: String::new(),
            check: StepCheck::PageLoaded,
            item: None,
        });
    }
    // A browser is named only for a link: "open Notes in Safari" goes on.
    (browser.is_empty() && is_app(target)).then(|| PlanStep {
        action: StepAction::OpenApp,
        label: format!("Open {target}"),
        app: target.to_string(),
        url: String::new(),
        goal: String::new(),
        text: String::new(),
        check: StepCheck::Frontmost,
        item: None,
    })
}

/// A request made only of opening apps, opening links and web searches,
/// planned on this Mac with no model: "open Notes", "look up looped
/// transformers", "open Notes, then look up the weather in Paris". None for
/// anything that has to operate an app, refers to the past, or asks for
/// something Notch Do never does; those go to the planner.
pub fn local_plan(request: &str, is_app: &dyn Fn(&str) -> bool) -> Option<Plan> {
    if asks_for_something_never_done(request) || refers_to_past(request) {
        return None;
    }
    let mut clauses = Vec::new();
    let mut rest = request.trim().to_string();
    // A clause ends at a comma, "and", "then" or "and then" only when an
    // opening or search verb follows, so "look up salt and pepper" stays one.
    'split: loop {
        let lower = rest.to_lowercase();
        for (at, _) in lower.match_indices([',', ' ']) {
            let tail = lower[at..].trim_start_matches([',', ' ']);
            let tail = tail
                .strip_prefix("and then ")
                .or_else(|| tail.strip_prefix("then "))
                .or_else(|| tail.strip_prefix("and "))
                .unwrap_or(tail);
            let joined =
                lower[at..].trim_start_matches([',', ' ']) != tail || lower[at..].starts_with(',');
            let opens = OPEN_VERBS
                .iter()
                .chain(SEARCH_VERBS)
                .any(|verb| tail.starts_with(&format!("{verb} ")));
            if joined && opens && at > 0 {
                clauses.push(rest[..at].to_string());
                let skip = lower.len() - tail.len();
                rest = rest[skip..].to_string();
                continue 'split;
            }
        }
        clauses.push(rest);
        break;
    }
    let steps = clauses
        .iter()
        .map(|clause| local_step(clause, is_app))
        .collect::<Option<Vec<_>>>()?;
    (!steps.is_empty() && steps.len() <= MAX_STEPS).then_some(Plan { steps })
}

/// Whether a finished run must say it left something out.
pub fn leaves_something_out(request: &str) -> bool {
    asks_for_something_never_done(request)
}

/// Words that ask for something Notch Do never does wherever they stand.
const NEVER_ANYWHERE: &str = r"(?i)\b(send|sends|sending|forward|delete|deletes|deleting|erase|trash|buy|buys|purchase|checkout|password|passcode|sign\s+in|log\s+in|terminal|system\s+settings)\b";

/// Words that ask for it only as the verb a request or a clause starts with:
/// "text Sam", "email the team", "reply to Ana", "order a pizza", "pay the
/// bill". As nouns ("this email", "the text of", "paste a reply", "in order",
/// "the pay range doc") they are what the person wants to read, open or paste.
const NEVER_AS_VERB: &[&str] = &["text", "message", "email", "reply", "order", "pay"];

/// Words before a clause's verb that change nothing.
const CLAUSE_LEAD: &[&str] = &[
    "please", "can", "could", "would", "you", "hey", "fndr", "and", "then", "also", "just", "now",
];

/// Whether the request's words ask for something Notch Do never does.
fn asks_for_something_never_done(request: &str) -> bool {
    static NEVER: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let never = NEVER
        .get_or_init(|| regex::Regex::new(NEVER_ANYWHERE).expect("never-tier pattern compiles"));
    if never.is_match(request) {
        return true;
    }
    let lower = request.to_lowercase();
    lower
        .split([',', ';', '.', '!', '?'])
        .flat_map(|clause| clause.split(" and ").flat_map(|part| part.split(" then ")))
        .any(|clause| {
            let words: Vec<&str> = clause
                .split(|c: char| !c.is_alphanumeric() && c != '\'')
                .filter(|word| !word.is_empty())
                .skip_while(|word| CLAUSE_LEAD.contains(word))
                .collect();
            words.len() >= 2 && NEVER_AS_VERB.contains(&words[0])
        })
}

/// What a finished run adds when the request also asked for something Notch
/// Do never does. The planner leaves that part out, so without this a run
/// for "buy it" would end in a plain "Done".
pub const LEFT_OUT: &str =
    "Left out: Notch Do does not send, delete, buy, enter passwords or change settings.";

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
                // A link opens in the browser it names, else in the default one.
                if !is_browser_name(&step.app) {
                    step.app.clear();
                }
            }
            StepAction::ReopenMemory => {
                return Err(format!(
                    "Step \"{}\" reopens a memory, which only FNDR may plan.",
                    step.label
                ));
            }
            StepAction::Paste => {
                step.text = step.text.trim().to_string();
                if step.text.chars().count() > MAX_PASTE_CHARS {
                    return Err(format!(
                        "Step \"{}\" pastes more than {MAX_PASTE_CHARS} characters.",
                        step.label
                    ));
                }
                // Whatever the model said, a paste is checked by reading the field.
                step.check = StepCheck::Pasted;
            }
            _ => {}
        }
        if step.action != StepAction::Paste {
            step.text.clear();
        }
        if step.label.is_empty() {
            step.label = match step.action {
                StepAction::OpenApp => format!("Open {}", step.app),
                StepAction::OpenUrl => "Open the page".to_string(),
                StepAction::Operate => step.goal.clone(),
                StepAction::ReopenMemory => String::new(),
                StepAction::Paste if step.app.is_empty() => "Paste the text".to_string(),
                StepAction::Paste => format!("Paste into {}", step.app),
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

/// Lowercase words of a request, without punctuation.
fn request_words(request: &str) -> Vec<String> {
    request
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '\'')
        .filter(|word| !word.is_empty())
        .map(str::to_string)
        .collect()
}

/// Words that say how the windows should sit, at the end of a request.
const LAYOUT_PHRASES: &[(&str, crate::operator::layout::Layout)] = &[
    (
        "side by side",
        crate::operator::layout::Layout::LeftRightSplit,
    ),
    (
        "next to each other",
        crate::operator::layout::Layout::LeftRightSplit,
    ),
    (
        "in a split",
        crate::operator::layout::Layout::LeftRightSplit,
    ),
    ("split", crate::operator::layout::Layout::LeftRightSplit),
];

/// The layout a request asks for: "side by side", "split" or "next to each
/// other" put the windows in columns. None when it names no layout.
pub fn layout_asked_for(request: &str) -> Option<crate::operator::layout::Layout> {
    let words = format!(" {} ", request_words(request).join(" "));
    LAYOUT_PHRASES
        .iter()
        .find(|(phrase, _)| words.contains(&format!(" {phrase} ")))
        .map(|(_, layout)| *layout)
}

/// The name in "save this as capstone demo prep", said right after a work
/// set opened. None for anything else.
pub fn save_set_name(request: &str) -> Option<String> {
    static SAVE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let pattern = SAVE.get_or_init(|| {
        regex::Regex::new(
            r#"(?i)^\s*(please\s+)?(save|keep|name|call)\s+(this\s+set|that\s+set|the\s+set|these|those|them|this|that|it)\s+(as|to|under)?\s*(?P<name>.+?)[\s.!"'\u{201d}]*$"#,
        )
        .expect("save pattern compiles")
    });
    let name = pattern.captures(request)?.name("name")?.as_str();
    let name = name.trim_matches(|c: char| {
        c == '"' || c == '\u{201c}' || c == '\u{201d}' || c.is_whitespace()
    });
    let name = ["my ", "the "]
        .iter()
        .find_map(|lead| {
            name.get(..lead.len())
                .filter(|head| head.eq_ignore_ascii_case(lead))
                .map(|_| &name[lead.len()..])
        })
        .unwrap_or(name)
        .trim();
    (!name.is_empty()).then(|| name.to_string())
}

/// Openers that start a request for a saved set, longest first.
const SET_OPENERS: &[&str] = &[
    "set back up",
    "bring back",
    "set up",
    "pull up",
    "bring up",
    "open up",
    "get out",
    "switch to",
    "reopen",
    "open",
    "load",
    "start",
    "resume",
];

/// Words around a set's name that are not part of it.
const SET_FILLER: &[&str] = &[
    "my",
    "the",
    "our",
    "set",
    "work",
    "workspace",
    "again",
    "please",
    "now",
    "for",
    "me",
];

/// Which of the person's saved sets a request names, decided on this Mac:
/// the name itself ("set up capstone demo prep"), then a close name ("open
/// my capstone set"). A close name counts only when it fits one set alone;
/// two sets that fit equally are left to the usual resolution. `names` are
/// the saved names; the result is an index into them.
pub fn named_set_asked_for(request: &str, names: &[String]) -> Option<usize> {
    if names.is_empty() || asks_for_something_never_done(request) {
        return None;
    }
    let mut words = request_words(request);
    for lead in ["please", "can", "could", "you", "fndr", "hey"] {
        if words.first().is_some_and(|word| word == lead) {
            words.remove(0);
        }
    }
    let said = words.join(" ");
    let opener = SET_OPENERS
        .iter()
        .find(|opener| said == **opener || said.starts_with(&format!("{opener} ")))?;
    let mut rest: Vec<String> = words[opener.split(' ').count()..].to_vec();
    for (phrase, _) in LAYOUT_PHRASES {
        let phrase: Vec<&str> = phrase.split(' ').collect();
        if rest.len() >= phrase.len() && rest[rest.len() - phrase.len()..] == phrase[..] {
            rest.truncate(rest.len() - phrase.len());
        }
    }
    let called_a_set = rest.iter().any(|word| word == "set");
    let exact = rest.join(" ");
    let normalized: Vec<Vec<String>> = names.iter().map(|name| request_words(name)).collect();
    if let Some(at) = normalized.iter().position(|name| name.join(" ") == exact) {
        return Some(at);
    }
    let rest: Vec<String> = rest
        .into_iter()
        .filter(|word| !SET_FILLER.contains(&word.as_str()))
        .collect();
    if rest.is_empty() {
        return None;
    }
    let only = |fits: Vec<usize>| (fits.len() == 1).then(|| fits[0]);
    if let Some(at) = normalized.iter().position(|name| {
        name.iter()
            .filter(|w| !SET_FILLER.contains(&w.as_str()))
            .eq(rest.iter())
    }) {
        return Some(at);
    }
    // The request says part of one name: "open my capstone set".
    if called_a_set || rest.len() >= 2 {
        let part: Vec<usize> = normalized
            .iter()
            .enumerate()
            .filter(|(_, name)| rest.iter().all(|word| name.contains(word)))
            .map(|(at, _)| at)
            .collect();
        if let Some(at) = only(part) {
            return Some(at);
        }
    }
    // The request says a whole name and a word or two more.
    let whole: Vec<usize> = normalized
        .iter()
        .enumerate()
        .filter(|(_, name)| {
            !name.is_empty()
                && rest.len() <= name.len() + 2
                && name.iter().all(|word| rest.contains(word))
        })
        .map(|(at, _)| at)
        .collect();
    only(whole)
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
                text: String::new(),
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
        O::Unavailable => verdict(false, "This source is unavailable to reopen right now"),
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
    verdict.ok
        && step.check != StepCheck::None
        && verdict.detail != REPORTED_ONLY
        && verdict.detail != PASTE_UNREAD
}

const PASTE_UNREAD: &str = "Pasted; the field does not show its text to FNDR";

/// Whether a paste landed, from the focused field's text read right after
/// it (`None` when the field exposes none, as some apps' editors do).
pub fn verify_paste(app: &str, text: &str, field: Option<&str>) -> Verdict {
    let flat = |value: &str| value.split_whitespace().collect::<Vec<_>>().join(" ");
    match field {
        Some(field) if flat(field).contains(&flat(text)) => {
            verdict(true, format!("Pasted into {app}"))
        }
        Some(_) => verdict(false, format!("The field in {app} does not show the text")),
        None => verdict(true, PASTE_UNREAD),
    }
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
            if !step.app.is_empty() && !same_app(&step.app, seen) {
                return verdict(
                    false,
                    format!("{} is not in front ({} is)", step.app, seen.frontmost_app),
                );
            }
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
        StepCheck::Pasted => verdict(false, "A paste is judged by reading the field"),
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
            text: String::new(),
            check,
            item: None,
        }
    }

    #[test]
    fn a_paste_is_checked_by_reading_the_field_names_its_app_and_may_paste_what_was_copied() {
        let paste = |text: &str, app: &str| {
            format!(
                r#"{{"steps":[{{"action":"paste","label":"","app":"{app}","url":"","goal":"","text":"{text}","check":"none"}}]}}"#
            )
        };
        let plan = parse_plan(&paste("https://zoom.us/j/123", "Notes")).unwrap();
        assert_eq!(
            plan.steps[0].check,
            StepCheck::Pasted,
            "the model's check is not taken"
        );
        assert_eq!(plan.steps[0].label, "Paste into Notes");
        assert_eq!(
            parse_plan(&paste("hi", "")).unwrap().steps[0].label,
            "Paste the text"
        );
        let copied = parse_plan(&paste("  ", "Notes")).unwrap();
        assert!(
            copied.steps[0].text.is_empty(),
            "empty text pastes what was copied"
        );
        assert!(parse_plan(&paste(&"x".repeat(MAX_PASTE_CHARS + 1), "Notes")).is_err());
        let open = parse_plan(
            r#"{"steps":[{"action":"open_app","label":"","app":"Notes","url":"","goal":"","text":"stray","check":"frontmost"}]}"#,
        )
        .unwrap();
        assert!(open.steps[0].text.is_empty(), "only a paste keeps text");

        let link = "https://zoom.us/j/123";
        assert!(verify_paste("Notes", link, Some(&format!("Call:\n{link}  "))).ok);
        let missing = verify_paste("Notes", link, Some("Call:"));
        assert!(!missing.ok);
        let unread = verify_paste("Slack", link, None);
        assert!(unread.ok);
        let pasted = step(StepAction::Paste, "Notes", "", StepCheck::Pasted);
        assert!(
            !checked_by_fndr(&pasted, &unread),
            "a field FNDR cannot read is not a check"
        );
        assert!(checked_by_fndr(
            &pasted,
            &verify_paste("Notes", link, Some(link))
        ));
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
    fn a_saved_set_is_found_by_its_name_then_by_a_close_name() {
        let names: Vec<String> = ["Capstone demo prep", "Bio lab", "Capstone report"]
            .iter()
            .map(|name| name.to_string())
            .collect();
        for (text, want) in [
            ("set up capstone demo prep", 0),
            ("Set up Capstone Demo Prep.", 0),
            ("open bio lab", 1),
            ("please open my bio lab set", 1),
            ("open my capstone demo set", 0),
            ("pull up capstone report side by side", 2),
            ("bring up the capstone demo prep stuff", 0),
            ("reopen capstone report again", 2),
        ] {
            assert_eq!(named_set_asked_for(text, &names), Some(want), "{text}");
        }
        for text in [
            "open my capstone set",
            "open notes",
            "open Safari",
            "delete capstone demo prep",
            "set up capstone demo prep and email it to Sam",
            "what is capstone demo prep",
            "open",
            "set a timer for 10 minutes",
        ] {
            assert_eq!(named_set_asked_for(text, &names), None, "{text}");
        }
        let one = vec!["Capstone demo prep".to_string()];
        assert_eq!(
            named_set_asked_for("open my capstone set", &one),
            Some(0),
            "a part of a name is enough when one set alone has it"
        );
        assert_eq!(named_set_asked_for("open capstone", &one), None);
        assert_eq!(named_set_asked_for("set up capstone demo prep", &[]), None);
    }

    #[test]
    fn save_this_as_takes_the_name_the_person_said() {
        for (text, want) in [
            ("save this as capstone demo prep", "capstone demo prep"),
            ("Save this as \u{201c}Bio lab\u{201d}.", "Bio lab"),
            ("please save that set as my capstone set", "capstone set"),
            ("call this Thursday standup", "Thursday standup"),
        ] {
            assert_eq!(save_set_name(text).as_deref(), Some(want), "{text}");
        }
        for text in [
            "save this",
            "open capstone demo prep",
            "pull up everything for the essay",
            "save the file",
        ] {
            assert_eq!(save_set_name(text), None, "{text}");
        }
    }

    #[test]
    fn side_by_side_asks_for_columns() {
        use crate::operator::layout::Layout;
        for text in [
            "pull up everything for the essay side by side",
            "open my bio lab set next to each other",
            "set up the lab report in a split",
        ] {
            assert_eq!(
                layout_asked_for(text),
                Some(Layout::LeftRightSplit),
                "{text}"
            );
        }
        for text in ["pull up everything for the essay", "open the splitter app"] {
            assert_eq!(layout_asked_for(text), None, "{text}");
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
            O::Unavailable,
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
