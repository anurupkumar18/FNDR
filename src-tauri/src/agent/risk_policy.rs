//! One run, confirm, or refuse decision for every command-surface action (GS-11).
//!
//! The command bar, voice, and MCP all ask `decide`; nothing else maps a tool's
//! risk to behavior. See `docs/product/command-surface.md`.

use crate::agent::tools::{ToolError, ToolRegistry, ToolRisk};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Caller {
    CommandBar,
    /// A final transcript the person has reviewed (ADR 020).
    Voice,
    /// Another agent calling through the MCP server.
    Mcp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefuseReason {
    KillSwitch,
    /// "Let assistants add notes" is off (VS-68).
    AgentNotesOff,
}

impl RefuseReason {
    pub fn message(self) -> &'static str {
        match self {
            RefuseReason::KillSwitch => "Actions are turned off in FNDR settings.",
            RefuseReason::AgentNotesOff => {
                "Assistant notes are off. The person can turn on \"Let assistants add notes\" in FNDR settings."
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Run,
    Confirm,
    Refuse(RefuseReason),
}

/// The kill switch refuses first. An MCP caller is another agent, so every one
/// of its actions needs the person's confirmation on the Mac, whatever the
/// tool's own risk level.
pub fn decide(risk: ToolRisk, caller: Caller, kill_switch: bool) -> Decision {
    if kill_switch {
        return Decision::Refuse(RefuseReason::KillSwitch);
    }
    match (caller, risk) {
        (Caller::Mcp, _) => Decision::Confirm,
        (_, ToolRisk::OneTap) => Decision::Confirm,
        (_, ToolRisk::Runs) => Decision::Run,
    }
}

/// Looks the tool up in the registry first, so an unregistered tool is an
/// error rather than a default decision.
pub fn decide_for_tool(
    registry: &ToolRegistry,
    tool: &str,
    caller: Caller,
    kill_switch: bool,
) -> Result<Decision, ToolError> {
    Ok(decide(registry.risk_of(tool)?, caller, kill_switch))
}

/// MCP tools with side effects that are not command-bar registry tools. Each
/// goes through `decide` before its handler runs. Assistant note and decision
/// writes are listed separately in `MCP_WRITE_TOOLS`.
const MCP_SIDE_EFFECT_TOOLS: [&str; 4] = [
    "agent.run",
    "start_meeting",
    "stop_meeting",
    "fndr.open_target",
];

pub fn mcp_side_effect_tools() -> &'static [&'static str] {
    &MCP_SIDE_EFFECT_TOOLS
}

/// Assistant note and decision writes (VS-68, VS-35). `decide` would confirm
/// every MCP call, so these use the person's explicit notes setting through
/// `decide_mcp_write` instead.
const MCP_WRITE_TOOLS: [&str; 2] = ["fndr.remember", "fndr_remember_decision"];

pub fn mcp_write_tools() -> &'static [&'static str] {
    &MCP_WRITE_TOOLS
}

/// The kill switch refuses first, then the notes setting; otherwise the
/// write runs.
pub fn decide_mcp_write(kill_switch: bool, agent_notes_enabled: bool) -> Decision {
    if kill_switch {
        return Decision::Refuse(RefuseReason::KillSwitch);
    }
    if !agent_notes_enabled {
        return Decision::Refuse(RefuseReason::AgentNotesOff);
    }
    Decision::Run
}

pub fn mcp_tool_risk(name: &str) -> Option<ToolRisk> {
    MCP_SIDE_EFFECT_TOOLS
        .contains(&name)
        .then_some(ToolRisk::OneTap)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::tools::october_registry;

    const CALLERS: [Caller; 3] = [Caller::CommandBar, Caller::Voice, Caller::Mcp];

    #[test]
    fn command_bar_and_voice_follow_the_tool_risk() {
        for caller in [Caller::CommandBar, Caller::Voice] {
            assert_eq!(decide(ToolRisk::Runs, caller, false), Decision::Run);
            assert_eq!(decide(ToolRisk::OneTap, caller, false), Decision::Confirm);
        }
    }

    #[test]
    fn mcp_always_confirms() {
        assert_eq!(
            decide(ToolRisk::Runs, Caller::Mcp, false),
            Decision::Confirm
        );
        assert_eq!(
            decide(ToolRisk::OneTap, Caller::Mcp, false),
            Decision::Confirm
        );
    }

    #[test]
    fn kill_switch_refuses_every_tool_and_caller() {
        let registry = october_registry();
        for spec in registry.specs() {
            for caller in CALLERS {
                assert_eq!(
                    decide_for_tool(&registry, spec.name, caller, true),
                    Ok(Decision::Refuse(RefuseReason::KillSwitch)),
                    "{} via {caller:?}",
                    spec.name
                );
            }
        }
        for name in mcp_side_effect_tools() {
            let risk = mcp_tool_risk(name).expect("listed tool has a risk");
            for caller in CALLERS {
                assert_eq!(
                    decide(risk, caller, true),
                    Decision::Refuse(RefuseReason::KillSwitch)
                );
            }
        }
    }

    #[test]
    fn every_registry_tool_is_decided_for_every_caller() {
        let registry = october_registry();
        for spec in registry.specs() {
            for caller in CALLERS {
                let decision = decide_for_tool(&registry, spec.name, caller, false).unwrap();
                let expected = match (caller, spec.risk) {
                    (Caller::Mcp, _) | (_, ToolRisk::OneTap) => Decision::Confirm,
                    (_, ToolRisk::Runs) => Decision::Run,
                };
                assert_eq!(decision, expected, "{} via {caller:?}", spec.name);
            }
        }
    }

    #[test]
    fn unregistered_tool_is_an_error_not_a_decision() {
        let registry = october_registry();
        assert_eq!(
            decide_for_tool(&registry, "send_email", Caller::CommandBar, false),
            Err(ToolError::UnknownTool("send_email".into()))
        );
    }

    #[test]
    fn the_four_audited_mcp_side_effects_are_gated_and_never_run_unattended() {
        for name in [
            "agent.run",
            "start_meeting",
            "stop_meeting",
            "fndr.open_target",
        ] {
            let risk = mcp_tool_risk(name).unwrap_or_else(|| panic!("{name} is not gated"));
            assert_eq!(
                decide(risk, Caller::Mcp, false),
                Decision::Confirm,
                "{name}"
            );
        }
        assert_eq!(mcp_tool_risk("fndr.search"), None);
    }

    #[test]
    fn mcp_call_tool_checks_the_gate_before_dispatching_any_tool() {
        let source = include_str!("../mcp/mod.rs");
        let gate = source
            .find("mcp_tool_risk(params.name.as_str())")
            .expect("call_tool consults the risk policy");
        let dispatch = source
            .find("match params.name.as_str()")
            .expect("call_tool dispatches by name");
        assert!(gate < dispatch, "the gate must run before the dispatch");
    }

    #[test]
    fn mcp_writes_refuse_on_the_kill_switch_then_the_notes_setting() {
        assert_eq!(
            decide_mcp_write(true, true),
            Decision::Refuse(RefuseReason::KillSwitch)
        );
        assert_eq!(
            decide_mcp_write(true, false),
            Decision::Refuse(RefuseReason::KillSwitch)
        );
        assert_eq!(
            decide_mcp_write(false, false),
            Decision::Refuse(RefuseReason::AgentNotesOff)
        );
        assert_eq!(decide_mcp_write(false, true), Decision::Run);
        assert_eq!(
            mcp_write_tools(),
            ["fndr.remember", "fndr_remember_decision"]
        );
        for name in mcp_write_tools() {
            assert_eq!(mcp_tool_risk(name), None, "{name} is gated once, not twice");
        }
    }

    #[test]
    fn remember_gate_runs_before_dispatch() {
        let source = include_str!("../mcp/mod.rs");
        let gate = source
            .find("decide_mcp_write(")
            .expect("call_tool gates MCP writes");
        let dispatch = source
            .find("match params.name.as_str()")
            .expect("call_tool dispatches by name");
        assert!(
            gate < dispatch,
            "the write gate must run before the dispatch"
        );
    }
}
