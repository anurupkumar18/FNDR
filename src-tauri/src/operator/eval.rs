//! The Notch Do evaluation suite: labeled spoken requests in
//! `scripts/operator/notch-do-cases.json`, scored against what FNDR decides
//! on the Mac (`plan::route`, the left-out note) and, for a recorded planner
//! answer, against what a correct plan holds.
//!
//! `cargo test --lib operator::eval` scores the routing of every case, every
//! policy case in `scripts/operator/notch-do-policy-cases.json`, and every
//! recorded plan under `scripts/operator/recordings/`. A planner answer
//! is recorded by the ignored live test in `ipc::commands::computer_use`.

use crate::operator::layout::Layout;
use crate::operator::plan::{self, Route, StepAction, StepCheck};
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
pub(crate) struct Suite {
    pub cases: Vec<Case>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct Case {
    pub id: String,
    pub request: String,
    #[serde(default)]
    pub in_front: Option<String>,
    #[serde(default)]
    pub saved_sets: Vec<String>,
    pub route: String,
    #[serde(default)]
    pub memory_snippets: Option<bool>,
    #[serde(default)]
    pub left_out: Option<bool>,
    #[serde(default)]
    pub layout: Option<String>,
    #[serde(default)]
    pub save_name: Option<String>,
    #[serde(default)]
    pub saved_set: Option<String>,
    #[serde(default)]
    pub plan: Option<PlanExpectation>,
}

/// What a correct planner answer holds. Steps are matched in order and need
/// not be adjacent; a field left out is not checked.
#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct PlanExpectation {
    #[serde(default)]
    pub steps: Vec<StepExpectation>,
    #[serde(default)]
    pub empty: bool,
    #[serde(default)]
    pub max_steps: Option<usize>,
    #[serde(default)]
    pub forbid_actions: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct StepExpectation {
    #[serde(default)]
    pub action: Option<String>,
    #[serde(default)]
    pub app: Option<String>,
    #[serde(default)]
    pub check: Option<String>,
    #[serde(default)]
    pub url_host: Option<String>,
    #[serde(default)]
    pub url_has: Option<String>,
}

pub(crate) fn repo_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(relative)
}

pub(crate) fn load_suite() -> Suite {
    let path = repo_path("scripts/operator/notch-do-cases.json");
    serde_json::from_slice(&std::fs::read(&path).expect("suite file")).expect("suite parses")
}

fn layout_name(layout: Option<Layout>) -> Option<String> {
    layout.map(|layout| {
        serde_json::to_value(layout)
            .ok()
            .and_then(|value| value.as_str().map(str::to_string))
            .unwrap_or_else(|| format!("{layout:?}"))
    })
}

/// What FNDR decides about a case on the Mac, against its labels. Each
/// mismatch is one sentence; none means the case passes.
pub(crate) fn routing_mismatches(case: &Case) -> Vec<String> {
    let mut wrong = Vec::new();
    let decided = plan::route(&case.request, &case.saved_sets);
    let (route, snippets, layout) = match &decided {
        Route::SaveSet { name } => {
            if case.save_name.as_deref() != Some(name.as_str()) {
                wrong.push(format!(
                    "saved under {name:?}, labeled {:?}",
                    case.save_name
                ));
            }
            ("save_set", None, None)
        }
        Route::SavedSet { index, layout } => {
            let name = case.saved_sets.get(*index).cloned();
            if case.saved_set != name {
                wrong.push(format!(
                    "opened saved set {name:?}, labeled {:?}",
                    case.saved_set
                ));
            }
            ("saved_set", None, *layout)
        }
        Route::WorkSet { layout } => ("work_set", None, *layout),
        Route::Planner { memory_snippets } => ("planner", Some(*memory_snippets), None),
    };
    if route != case.route {
        wrong.push(format!("went to {route}, labeled {}", case.route));
    }
    if let (Some(want), Some(got)) = (case.memory_snippets, snippets) {
        if want != got {
            wrong.push(format!("memory snippets {got}, labeled {want}"));
        }
    }
    if route == "work_set" && case.route == "work_set" && layout_name(layout) != case.layout {
        wrong.push(format!(
            "layout {:?}, labeled {:?}",
            layout_name(layout),
            case.layout
        ));
    }
    if let Some(want) = case.left_out {
        let got = plan::leaves_something_out(&case.request);
        if want != got {
            wrong.push(format!("left-out note {got}, labeled {want}"));
        }
    }
    wrong
}

fn action_name(action: StepAction) -> &'static str {
    match action {
        StepAction::OpenApp => "open_app",
        StepAction::OpenUrl => "open_url",
        StepAction::Operate => "operate",
        StepAction::ReopenMemory => "reopen_memory",
    }
}

fn check_name(check: StepCheck) -> &'static str {
    match check {
        StepCheck::Frontmost => "frontmost",
        StepCheck::MediaPlaying => "media_playing",
        StepCheck::PageLoaded => "page_loaded",
        StepCheck::None => "none",
        StepCheck::Reopened => "reopened",
    }
}

fn step_fits(want: &StepExpectation, step: &plan::PlanStep) -> bool {
    let same = |label: &Option<String>, actual: &str| {
        label
            .as_deref()
            .is_none_or(|label| label.eq_ignore_ascii_case(actual.trim()))
    };
    let url = step.url.to_lowercase();
    let host = url
        .split("://")
        .nth(1)
        .and_then(|rest| rest.split(['/', '?', '#']).next())
        .unwrap_or_default()
        .to_string();
    same(&want.action, action_name(step.action))
        && same(&want.app, &step.app)
        && same(&want.check, check_name(step.check))
        && want
            .url_host
            .as_deref()
            .is_none_or(|wanted| host == wanted.to_lowercase())
        && want
            .url_has
            .as_deref()
            .is_none_or(|wanted| url.contains(&wanted.to_lowercase()))
}

/// A planner answer, as text, against a case's plan labels. A refused answer
/// counts as an empty plan, which is what a refusal looks like to the person.
pub(crate) fn plan_mismatches(case: &Case, answer: &str) -> Vec<String> {
    let Some(want) = case.plan.as_ref() else {
        return Vec::new();
    };
    let steps = match plan::parse_plan(answer) {
        Ok(plan) => plan.steps,
        Err(_) => Vec::new(),
    };
    let mut wrong = Vec::new();
    if want.empty && !steps.is_empty() {
        wrong.push(format!("{} steps, labeled none", steps.len()));
    }
    if let Some(max) = want.max_steps {
        if steps.len() > max {
            wrong.push(format!("{} steps, at most {max} labeled", steps.len()));
        }
    }
    for step in &steps {
        if want
            .forbid_actions
            .iter()
            .any(|forbidden| forbidden == action_name(step.action))
        {
            wrong.push(format!(
                "plans {} ({}), which is forbidden",
                action_name(step.action),
                step.label
            ));
        }
    }
    let mut from = 0;
    for expected in &want.steps {
        match steps[from..]
            .iter()
            .position(|step| step_fits(expected, step))
        {
            Some(at) => from += at + 1,
            None => {
                wrong.push(format!("no step like {expected:?} in order"));
                break;
            }
        }
    }
    wrong
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_labeled_request_is_routed_as_labeled() {
        let suite = load_suite();
        assert!(
            suite.cases.len() >= 30,
            "the suite holds {}",
            suite.cases.len()
        );
        let mut ids = std::collections::HashSet::new();
        let mut failures = Vec::new();
        for case in &suite.cases {
            assert!(ids.insert(case.id.clone()), "duplicate id {}", case.id);
            let wrong = routing_mismatches(case);
            if !wrong.is_empty() {
                failures.push(format!("{}: {}", case.id, wrong.join("; ")));
            }
        }
        println!(
            "routing: {} of {} cases as labeled",
            suite.cases.len() - failures.len(),
            suite.cases.len()
        );
        assert!(failures.is_empty(), "\n{}", failures.join("\n"));
    }

    #[test]
    fn a_plan_is_scored_against_its_labels() {
        let suite = load_suite();
        let case = suite
            .cases
            .iter()
            .find(|case| case.id == "play-song")
            .expect("play-song case");
        let good = r#"{"steps":[
            {"action":"open_app","label":"Open Spotify","app":"Spotify","url":"","goal":"","check":"frontmost"},
            {"action":"operate","label":"Play Blinding Lights","app":"Spotify","url":"","goal":"play Blinding Lights","check":"media_playing"}]}"#;
        assert!(plan_mismatches(case, good).is_empty());
        let web_instead = r#"{"steps":[
            {"action":"open_url","label":"Search the song","app":"","url":"https://www.google.com/search?q=blinding+lights","goal":"","check":"page_loaded"}]}"#;
        assert!(!plan_mismatches(case, web_instead).is_empty());
        let refusal = suite
            .cases
            .iter()
            .find(|case| case.id == "send-text")
            .expect("send-text case");
        assert!(plan_mismatches(refusal, r#"{"steps":[]}"#).is_empty());
        let sent = r#"{"steps":[{"action":"operate","label":"Text Sam","app":"Messages","url":"","goal":"text Sam","check":"none"}]}"#;
        assert!(!plan_mismatches(refusal, sent).is_empty());
    }

    /// Every policy case in `scripts/operator/notch-do-policy-cases.json`:
    /// what FNDR decides for one computer-use call, given what it read of
    /// the app and the element last clicked.
    #[test]
    fn every_policy_case_is_decided_as_labeled() {
        use crate::operator::policy::{classify, Observed, Risk};
        let path = repo_path("scripts/operator/notch-do-policy-cases.json");
        let suite: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).expect("policy cases")).expect("parses");
        let cases = suite["cases"].as_array().expect("cases");
        assert!(cases.len() >= 20, "{} policy cases", cases.len());
        let mut failures = Vec::new();
        for case in cases {
            let mut observed = Observed::default();
            for (app, tree) in suite["trees"].as_object().expect("trees") {
                observed.observe_tree(app, tree.as_str().unwrap_or_default());
            }
            if let Some(focus) = case["focus"].as_array() {
                let at = |i: usize| focus[i].as_str().unwrap_or_default();
                observed.note_target(at(0), at(1));
            }
            let decided = classify(
                case["tool"].as_str().unwrap_or_default(),
                &case["args"],
                &observed,
            );
            let got = match decided.risk {
                Risk::Runs => "runs",
                Risk::Confirm => "confirm",
                Risk::Never => "never",
            };
            if Some(got) != case["risk"].as_str() {
                failures.push(format!(
                    "{}: {got} ({}), labeled {}",
                    case["id"], decided.reason, case["risk"]
                ));
            }
        }
        println!(
            "policy: {} of {} calls as labeled",
            cases.len() - failures.len(),
            cases.len()
        );
        assert!(failures.is_empty(), "\n{}", failures.join("\n"));
    }

    /// Every recorded planner run under `scripts/operator/recordings/`,
    /// rescored with today's labels and parser. A recording holds the
    /// planner's raw answers, so changing the labels or `parse_plan` never
    /// needs the cloud.
    #[test]
    fn recorded_planner_answers_meet_their_labels() {
        let dir = repo_path("scripts/operator/recordings");
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return;
        };
        let suite = load_suite();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }
            let recording: serde_json::Value =
                serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            let floor = recording["pass_floor"].as_f64().unwrap_or(1.0);
            let answers = recording["answers"]
                .as_object()
                .cloned()
                .unwrap_or_default();
            let mut passed = 0usize;
            let mut failures = Vec::new();
            for case in suite.cases.iter().filter(|case| case.plan.is_some()) {
                let Some(answer) = answers.get(&case.id).and_then(|value| value.as_str()) else {
                    continue;
                };
                let wrong = plan_mismatches(case, answer);
                if wrong.is_empty() {
                    passed += 1;
                } else {
                    failures.push(format!("{}: {}", case.id, wrong.join("; ")));
                }
            }
            let scored = passed + failures.len();
            let share = passed as f64 / scored.max(1) as f64;
            println!(
                "{}: {passed} of {scored} plans as labeled\n{}",
                path.display(),
                failures.join("\n")
            );
            assert!(
                share >= floor,
                "{}: {share:.2} below its floor {floor:.2}\n{}",
                path.display(),
                failures.join("\n")
            );
        }
    }
}
