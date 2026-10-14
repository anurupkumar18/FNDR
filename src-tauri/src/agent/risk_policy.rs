//! The gate for MCP writes: assistant notes, decisions and retrieval
//! feedback. Actions with side effects are gated by `mcp_action_policy` in
//! `mcp/mod.rs`, and Notch Do's actions by `operator::policy`.

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
    Refuse(RefuseReason),
}

/// Assistant note, decision, and retrieval-feedback writes. Each runs only
/// while actions are on and the person allows assistant notes.
const MCP_WRITE_TOOLS: [&str; 3] = [
    "fndr.remember",
    "fndr_remember_decision",
    "agent.rate_result",
];

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

#[cfg(test)]
mod tests {
    use super::*;

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
            ["fndr.remember", "fndr_remember_decision", "agent.rate_result"]
        );
    }

    #[test]
    fn mcp_call_tool_checks_both_gates_before_dispatching_any_tool() {
        let source = include_str!("../mcp/mod.rs");
        let dispatch = source
            .find("match params.name.as_str()")
            .expect("call_tool dispatches by name");
        for gate in ["mcp_action_policy(params.name.as_str())", "decide_mcp_write("] {
            let at = source.find(gate).unwrap_or_else(|| panic!("call_tool checks {gate}"));
            assert!(at < dispatch, "{gate} must run before the dispatch");
        }
    }
}
