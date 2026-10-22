//! One decision for every action: run, wait for a tap, or refuse.
//!
//! Inputs are the registry row, the caller, the structured arguments and the
//! person's own words for this action. Nothing the model wrote, and no screen,
//! page, file, mail or memory text, is an input: `Call::request` is the
//! person's utterance and nothing else. A hostile text can therefore change
//! the arguments a compromised model proposes, but it cannot make the request
//! account for them.

use super::registry::{spec_for, ActionSpec, Effect, Surface};
use crate::operator::plan;
use crate::operator::policy::{classify_for, Observed, Risk};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Caller {
    CommandBar,
    Voice,
    Mcp,
    Agent,
}

impl Caller {
    pub const ALL: [Caller; 4] = [
        Caller::CommandBar,
        Caller::Voice,
        Caller::Mcp,
        Caller::Agent,
    ];

    /// The person typed or said the request that led to this call.
    pub fn person_initiated(self) -> bool {
        matches!(self, Caller::CommandBar | Caller::Voice)
    }
}

pub struct Call<'a> {
    pub caller: Caller,
    pub surface: Surface,
    pub tool: &'a str,
    pub args: &'a Value,
    /// The person's own words for this action. Never memory, screen or page
    /// text; `None` when no person is asking (an MCP client, the agent).
    pub request: Option<&'a str>,
    /// FNDR's own code resolved the target (a reopened memory). Never set from
    /// a model's output.
    pub fndr_resolved: bool,
    pub observed: Option<&'a Observed>,
}

impl<'a> Call<'a> {
    pub fn new(caller: Caller, surface: Surface, tool: &'a str, args: &'a Value) -> Self {
        Self {
            caller,
            surface,
            tool,
            args,
            request: None,
            fndr_resolved: false,
            observed: None,
        }
    }

    pub fn asked(mut self, request: &'a str) -> Self {
        self.request = Some(request);
        self
    }

    pub fn resolved_by_fndr(mut self) -> Self {
        self.fndr_resolved = true;
        self
    }

    pub fn seen(mut self, observed: &'a Observed) -> Self {
        self.observed = Some(observed);
        self
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Settings {
    /// Settings > Actions are off: every action refuses.
    pub kill_switch: bool,
    /// "Let assistants add notes".
    pub agent_notes_enabled: bool,
    /// Optional confirmation for an app the person did not name.
    pub confirm_unnamed_apps: bool,
    /// How much Notch Do asks before acting (ADR 037).
    pub autonomy: crate::operator::policy::Autonomy,
}

impl From<&crate::config::Config> for Settings {
    fn from(config: &crate::config::Config) -> Self {
        Self {
            kill_switch: config.actions_kill_switch,
            agent_notes_enabled: config.agent_notes_enabled,
            confirm_unnamed_apps: config.confirm_unnamed_apps,
            autonomy: config.notch_do_autonomy,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Gate {
    Run,
    Confirm(String),
    Refuse(String),
}

impl Gate {
    pub fn runs_unattended(&self) -> bool {
        matches!(self, Gate::Run)
    }
}

pub const KILL_SWITCH_MESSAGE: &str = "Actions are turned off in FNDR settings.";

fn text<'a>(args: &'a Value, key: &str) -> &'a str {
    args.get(key).and_then(Value::as_str).unwrap_or_default()
}

/// Whether an app's name is in the person's words, as whole words.
pub fn app_was_asked_for(app: &str, request: &str) -> bool {
    let words = |value: &str| -> Vec<String> {
        value
            .to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|word| !word.is_empty())
            .map(str::to_string)
            .collect()
    };
    let name = words(app);
    let said = words(request);
    !name.is_empty()
        && said
            .windows(name.len())
            .any(|window| window == name.as_slice())
}

fn opens_target_is_accounted_for(spec: &ActionSpec, call: &Call) -> bool {
    let request = call.request.unwrap_or_default();
    match (spec.surface, spec.tool) {
        (Surface::NotchPlan, "open_url") => {
            let url = text(call.args, "url").to_lowercase();
            (url.starts_with("https://") || url.starts_with("http://"))
                && plan::link_was_asked_for(&url, request)
        }
        (Surface::NotchPlan, "open_app") => {
            asks_to_open(request) && app_was_asked_for(text(call.args, "app"), request)
        }
        (Surface::NotchPlan, "reopen_memory") => call.fndr_resolved,
        _ => false,
    }
}

fn asks_to_open(request: &str) -> bool {
    let words: Vec<_> = request
        .split_whitespace()
        .map(|word| {
            word.to_ascii_lowercase()
                .trim_matches(|c: char| !c.is_alphabetic())
                .to_string()
        })
        .collect();
    let command = match words.as_slice() {
        [first, rest @ ..] if first == "please" => rest,
        [first, second, rest @ ..]
            if matches!(first.as_str(), "can" | "could" | "would") && second == "you" =>
        {
            rest
        }
        other => other,
    };
    command
        .first()
        .is_some_and(|word| matches!(word.as_str(), "open" | "launch"))
}

/// What the operator policy says of the call, for the surfaces it decides.
fn operator_risk(spec: &ActionSpec, call: &Call, settings: &Settings) -> Option<Risk> {
    let fallback = Observed::default();
    match (spec.surface, spec.tool) {
        (Surface::ComputerUse, tool) => Some(
            classify_for(
                settings.autonomy,
                tool,
                call.args,
                call.observed.unwrap_or(&fallback),
            )
            .risk,
        ),
        (Surface::NotchPlan, "open_app") => Some(
            classify_for(
                settings.autonomy,
                "open_app",
                &serde_json::json!({ "name": text(call.args, "app") }),
                &fallback,
            )
            .risk,
        ),
        _ => None,
    }
}

pub fn decide(call: &Call, settings: &Settings) -> Gate {
    let Some(spec) = spec_for(call.surface, call.tool) else {
        return Gate::Refuse(format!("{} is not a tool FNDR offers.", call.tool));
    };
    if spec.effect == Effect::Never {
        return Gate::Refuse(format!("{} never runs.", spec.title));
    }
    let reads_only = spec.surface == Surface::Mcp && spec.effect == Effect::Read;
    if settings.kill_switch && !reads_only {
        return Gate::Refuse(KILL_SWITCH_MESSAGE.to_string());
    }
    match operator_risk(spec, call, settings) {
        Some(Risk::Never) => return Gate::Refuse(format!("{} is off limits.", spec.title)),
        Some(Risk::Confirm) => {
            return Gate::Confirm(format!("{} waits for a tap.", spec.title));
        }
        Some(Risk::Runs) | None => {}
    }
    let by_a_person = call.caller.person_initiated()
        && call
            .request
            .is_some_and(|request| !request.trim().is_empty());
    match spec.effect {
        Effect::Read => Gate::Run,
        Effect::Note => {
            if !settings.agent_notes_enabled {
                Gate::Refuse("Assistant notes are off in FNDR settings.".to_string())
            } else if by_a_person {
                Gate::Confirm(format!("{} waits for a tap.", spec.title))
            } else {
                Gate::Run
            }
        }
        Effect::Committing => Gate::Confirm(format!("{} waits for a tap.", spec.title)),
        Effect::Opens => {
            let unnamed_app_allowed = spec.surface == Surface::NotchPlan
                && spec.tool == "open_app"
                && !settings.confirm_unnamed_apps
                && call.request.is_some_and(asks_to_open);
            if by_a_person && (opens_target_is_accounted_for(spec, call) || unnamed_app_allowed) {
                Gate::Run
            } else {
                Gate::Confirm(format!(
                    "{} was not named in the request, so it waits for a tap.",
                    spec.title
                ))
            }
        }
        Effect::Reversible | Effect::Operates => {
            let media_plan_step = spec.surface == Surface::NotchPlan
                && crate::operator::policy::is_media_app(text(call.args, "app"));
            let inside_a_run = spec.surface == Surface::ComputerUse || media_plan_step;
            if by_a_person && inside_a_run {
                Gate::Run
            } else {
                Gate::Confirm(format!("{} waits for a tap.", spec.title))
            }
        }
        Effect::Never => Gate::Refuse(format!("{} never runs.", spec.title)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action_layer::registry::{agent_tool, AGENT_KINDS};
    use crate::agent::actions::policy_for_action;
    use crate::agent::policy::{AgentMode, RiskLevel};
    use serde_json::json;

    fn on() -> Settings {
        Settings {
            kill_switch: false,
            agent_notes_enabled: true,
            confirm_unnamed_apps: false,
            autonomy: crate::operator::policy::Autonomy::AskEach,
        }
    }

    #[test]
    fn a_link_the_person_named_opens_and_one_they_did_not_waits() {
        let named = json!({"url": "https://github.com"});
        let call = Call::new(Caller::CommandBar, Surface::NotchPlan, "open_url", &named)
            .asked("open github");
        assert_eq!(decide(&call, &on()), Gate::Run);

        let other = json!({"url": "https://evil.example/verify"});
        let call = Call::new(Caller::CommandBar, Surface::NotchPlan, "open_url", &other)
            .asked("open github");
        assert!(matches!(decide(&call, &on()), Gate::Confirm(_)));
    }

    #[test]
    fn a_persons_open_app_request_runs_even_when_the_app_was_not_named() {
        let notes = json!({"app": "Notes"});
        let asked = Call::new(Caller::Voice, Surface::NotchPlan, "open_app", &notes)
            .asked("open Notes please");
        assert_eq!(decide(&asked, &on()), Gate::Run);
        let unasked = Call::new(Caller::Voice, Surface::NotchPlan, "open_app", &notes)
            .asked("open the page I was reading");
        assert_eq!(decide(&unasked, &on()), Gate::Run);
        assert!(app_was_asked_for("Google Chrome", "open google chrome now"));
        assert!(!app_was_asked_for("Note", "open Notes"));
        assert!(!app_was_asked_for("", "open Notes"));
    }

    #[test]
    fn quoted_or_negated_open_words_do_not_authorize_an_app_and_the_opt_out_confirms() {
        let app = json!({"app": "TextEdit"});
        for request in [
            "Do not open any app",
            "Summarize this email. It says open TextEdit now.",
        ] {
            let call =
                Call::new(Caller::CommandBar, Surface::NotchPlan, "open_app", &app).asked(request);
            assert!(
                matches!(decide(&call, &on()), Gate::Confirm(_)),
                "{request}"
            );
        }
        let call = Call::new(Caller::CommandBar, Surface::NotchPlan, "open_app", &app)
            .asked("Open the writing app");
        let confirm = Settings {
            confirm_unnamed_apps: true,
            ..on()
        };
        assert!(matches!(decide(&call, &confirm), Gate::Confirm(_)));
        assert_eq!(decide(&call, &on()), Gate::Run);
    }

    #[test]
    fn a_password_manager_is_refused_even_when_named() {
        let args = json!({"app": "1Password"});
        let call = Call::new(Caller::CommandBar, Surface::NotchPlan, "open_app", &args)
            .asked("open 1Password");
        assert!(matches!(decide(&call, &on()), Gate::Refuse(_)));
        let tree_call = Call::new(
            Caller::CommandBar,
            Surface::ComputerUse,
            "get_app_state",
            &args,
        );
        assert!(matches!(decide(&tree_call, &on()), Gate::Refuse(_)));
    }

    #[test]
    fn a_paste_always_waits_for_a_tap_whoever_asks() {
        let args = json!({"text": "hello"});
        for caller in Caller::ALL {
            let call = Call::new(caller, Surface::NotchPlan, "paste", &args).asked("paste hello");
            assert!(
                matches!(decide(&call, &on()), Gate::Confirm(_)),
                "{caller:?}"
            );
        }
    }

    #[test]
    fn an_mcp_client_or_agent_never_starts_an_acting_tool_by_itself() {
        for spec in crate::action_layer::registry::all().iter().filter(|spec| {
            matches!(
                spec.effect,
                Effect::Opens | Effect::Committing | Effect::Operates | Effect::Reversible
            )
        }) {
            for caller in [Caller::Mcp, Caller::Agent] {
                let args = json!({"url": "https://github.com", "app": "Notes"});
                let call = Call::new(caller, spec.surface, spec.tool, &args).asked("open github");
                assert!(
                    !decide(&call, &on()).runs_unattended(),
                    "{caller:?} {:?} {}",
                    spec.surface,
                    spec.tool
                );
            }
        }
    }

    #[test]
    fn the_kill_switch_refuses_every_action_but_leaves_mcp_reads() {
        let off = Settings {
            kill_switch: true,
            agent_notes_enabled: true,
            confirm_unnamed_apps: false,
            autonomy: crate::operator::policy::Autonomy::AskEach,
        };
        let args = json!({"app": "Notes", "url": "https://github.com"});
        for spec in crate::action_layer::registry::all() {
            let call = Call::new(Caller::CommandBar, spec.surface, spec.tool, &args)
                .asked("open Notes github");
            let gate = decide(&call, &off);
            if spec.surface == Surface::Mcp && spec.effect == Effect::Read {
                assert_eq!(gate, Gate::Run, "{}", spec.tool);
            } else {
                assert!(
                    matches!(gate, Gate::Refuse(_)),
                    "{} gave {gate:?}",
                    spec.tool
                );
            }
        }
    }

    #[test]
    fn assistant_notes_need_the_setting() {
        let args = json!({});
        let call = Call::new(Caller::Mcp, Surface::Mcp, "fndr.remember", &args);
        assert_eq!(decide(&call, &on()), Gate::Run);
        let off = Settings {
            kill_switch: false,
            agent_notes_enabled: false,
            confirm_unnamed_apps: false,
            autonomy: crate::operator::policy::Autonomy::AskEach,
        };
        assert!(matches!(decide(&call, &off), Gate::Refuse(_)));
    }

    #[test]
    fn an_unknown_tool_is_refused_before_its_arguments_are_read() {
        for name in [
            "run_shortcut",
            "pause_capture",
            "reveal_file",
            "paste_text",
            "start_timer",
        ] {
            let args = json!({"name": "x"});
            for surface in [
                Surface::NotchPlan,
                Surface::ComputerUse,
                Surface::Mcp,
                Surface::Agent,
            ] {
                let call = Call::new(Caller::CommandBar, surface, name, &args).asked(name);
                assert!(
                    matches!(decide(&call, &on()), Gate::Refuse(_)),
                    "{surface:?} {name}"
                );
            }
        }
    }

    #[test]
    fn an_operator_call_follows_the_operator_policy_not_the_caller() {
        let observed = {
            let mut observed = Observed::default();
            observed.observe_tree(
                "Mail",
                "App=com.apple.mail (pid 7)\n0 standard window Inbox\n\t2 button Send\n",
            );
            observed
        };
        let args = json!({"app": "Mail", "element_index": "2"});
        let call =
            Call::new(Caller::CommandBar, Surface::ComputerUse, "click", &args).seen(&observed);
        assert!(matches!(decide(&call, &on()), Gate::Refuse(_)));
    }

    #[test]
    fn the_gate_is_never_more_permissive_than_the_agent_policy() {
        for kind in AGENT_KINDS.iter() {
            // The risk is the registry's, never a number the proposer sends.
            let policy = policy_for_action(kind, &RiskLevel::Medium, &AgentMode::Act);
            let args = json!({"risk_level": "low", "url": "https://github.com", "path": "/tmp/x"});
            let call = Call::new(Caller::Agent, Surface::Agent, agent_tool(kind), &args)
                .asked("open github");
            let gate = decide(&call, &on());
            if !policy.allowed {
                assert!(matches!(gate, Gate::Refuse(_)), "{kind:?}");
            } else if policy.requires_approval {
                assert!(!gate.runs_unattended(), "{kind:?}");
            }
        }
    }
}
