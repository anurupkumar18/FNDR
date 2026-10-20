//! Every action FNDR can take, on every surface, with what it does to the
//! Mac and how it is undone. The tier is fixed here, in code. A model, a page
//! or a memory cannot choose it (actions policy, "Rules that hold for every
//! tool").
//!
//! There is no second registry to keep in step with the executors: the
//! drift tests in `tests/action_injection.rs` read the MCP listing, the
//! operator's tool list and `AgentActionKind`, and fail on a name this table
//! does not know.

use crate::agent::actions::AgentActionKind;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Surface {
    /// A step in a Notch Do plan (`operator::plan::StepAction`).
    NotchPlan,
    /// A computer-use call inside an operate step, decided by
    /// `operator::policy::classify`.
    ComputerUse,
    /// A tool an MCP client calls.
    Mcp,
    /// An action the in-app agent proposes (`AgentActionKind`).
    Agent,
}

/// What a call does, which decides how it may start.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Effect {
    /// Reads and reports; changes nothing.
    Read,
    /// Opens an app, a page or a memory. Starts by itself only when the
    /// person's own words account for the target.
    Opens,
    /// Changes something local that can be put back.
    Reversible,
    /// An assistant's note, written only while the person allows assistant
    /// notes.
    Note,
    /// Operates another app's UI. Each call is decided by the operator policy.
    Operates,
    /// Sends, pastes, runs, records or schedules. Always waits for a tap.
    Committing,
    /// Not offered to anyone.
    Never,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Undo {
    /// `how` says what the person does; `inverse` names the tool that
    /// reverses it, when there is one.
    Available {
        how: &'static str,
        inverse: Option<&'static str>,
    },
    Unavailable {
        reason: &'static str,
    },
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct ActionSpec {
    pub surface: Surface,
    pub tool: &'static str,
    pub effect: Effect,
    pub title: &'static str,
    pub undo: Undo,
    /// The argument that names what the action is about, for the card.
    pub target_key: Option<&'static str>,
    /// The result carries memory text, so it is framed as untrusted data.
    pub returns_memory_text: bool,
}

const NOTHING_CHANGES: Undo = Undo::Unavailable {
    reason: "Reads only; nothing changed, so there is nothing to undo.",
};
const CANNOT_CLOSE: Undo = Undo::Unavailable {
    reason: "FNDR does not close what it opens; close the window or tab yourself.",
};
const NOTE_REMOVABLE: Undo = Undo::Available {
    how: "Delete the note from the Memory Vault.",
    inverse: None,
};
const OPERATED: Undo = Undo::Unavailable {
    reason: "Clicks and typing inside another app are that app's to undo (Cmd+Z); FNDR cannot roll them back.",
};
const WINDOWS_RESTORABLE: Undo = Undo::Available {
    how: "Put the windows back where they were.",
    inverse: Some("arrange_windows"),
};

const fn spec(
    surface: Surface,
    tool: &'static str,
    effect: Effect,
    title: &'static str,
    undo: Undo,
    target_key: Option<&'static str>,
) -> ActionSpec {
    ActionSpec {
        surface,
        tool,
        effect,
        title,
        undo,
        target_key,
        returns_memory_text: false,
    }
}

const fn read(surface: Surface, tool: &'static str, title: &'static str) -> ActionSpec {
    spec(surface, tool, Effect::Read, title, NOTHING_CHANGES, None)
}

const fn memory_read(tool: &'static str, title: &'static str) -> ActionSpec {
    ActionSpec {
        returns_memory_text: true,
        ..read(Surface::Mcp, tool, title)
    }
}

use Effect::{Committing, Never, Note, Opens, Operates, Reversible};
use Surface::{Agent, ComputerUse, Mcp, NotchPlan};

pub const ALL: &[ActionSpec] = &[
    // Notch Do plan steps.
    spec(
        NotchPlan,
        "open_app",
        Opens,
        "Open an app",
        CANNOT_CLOSE,
        Some("app"),
    ),
    spec(
        NotchPlan,
        "open_url",
        Opens,
        "Open a link",
        CANNOT_CLOSE,
        Some("url"),
    ),
    spec(
        NotchPlan,
        "reopen_memory",
        Opens,
        "Reopen a memory",
        CANNOT_CLOSE,
        Some("memory_id"),
    ),
    spec(
        NotchPlan,
        "operate",
        Operates,
        "Operate an app",
        OPERATED,
        Some("app"),
    ),
    spec(
        NotchPlan,
        "paste",
        Committing,
        "Paste text",
        Undo::Unavailable {
            reason:
                "Text pasted into another app stays there; that app's Undo (Cmd+Z) may remove it.",
        },
        Some("app"),
    ),
    // Computer-use tools Notch Do serves to Codex.
    read(ComputerUse, "list_apps", "List running apps"),
    read(ComputerUse, "get_app_state", "Read an app's window"),
    read(ComputerUse, "list_windows", "List open windows"),
    read(ComputerUse, "scroll", "Scroll"),
    spec(
        ComputerUse,
        "click",
        Operates,
        "Click",
        OPERATED,
        Some("app"),
    ),
    spec(
        ComputerUse,
        "set_value",
        Operates,
        "Set a field",
        OPERATED,
        Some("app"),
    ),
    spec(
        ComputerUse,
        "type_text",
        Operates,
        "Type text",
        OPERATED,
        Some("app"),
    ),
    spec(
        ComputerUse,
        "press_key",
        Operates,
        "Press a key",
        OPERATED,
        Some("app"),
    ),
    spec(
        ComputerUse,
        "move_window",
        Reversible,
        "Move a window",
        WINDOWS_RESTORABLE,
        Some("app"),
    ),
    spec(
        ComputerUse,
        "resize_window",
        Reversible,
        "Resize a window",
        WINDOWS_RESTORABLE,
        Some("app"),
    ),
    spec(
        ComputerUse,
        "arrange_windows",
        Reversible,
        "Arrange windows",
        WINDOWS_RESTORABLE,
        None,
    ),
    // MCP tools that act.
    spec(
        Mcp,
        "agent.run",
        Committing,
        "Run an agent request",
        Undo::Unavailable {
            reason: "The run is recorded in the agent audit log; the log is not rewritten.",
        },
        None,
    ),
    spec(
        Mcp,
        "start_meeting",
        Committing,
        "Start recording a meeting",
        Undo::Available {
            how: "Stop the recording.",
            inverse: Some("stop_meeting"),
        },
        None,
    ),
    spec(
        Mcp,
        "stop_meeting",
        Committing,
        "Stop recording a meeting",
        Undo::Unavailable {
            reason: "A stopped recording cannot be resumed; a new one can be started.",
        },
        None,
    ),
    spec(
        Mcp,
        "fndr.open_target",
        Opens,
        "Reopen a memory's source",
        CANNOT_CLOSE,
        Some("memory_id"),
    ),
    spec(
        Mcp,
        "fndr.remember",
        Note,
        "Save a note",
        NOTE_REMOVABLE,
        None,
    ),
    spec(
        Mcp,
        "fndr_remember_decision",
        Note,
        "Save a decision",
        NOTE_REMOVABLE,
        None,
    ),
    spec(
        Mcp,
        "fndr.rate_result",
        Note,
        "Rate a retrieval",
        Undo::Unavailable {
            reason: "Ratings are appended to the feedback log, which is not rewritten.",
        },
        None,
    ),
    // MCP tools that only read. Those that return memory text say so.
    memory_read("memory.search_full_context", "Search memories"),
    memory_read("memory.get_context_pack", "Build a context pack"),
    memory_read("memory.resume_work", "Resume work"),
    memory_read("memory.agent_brief", "Brief an agent"),
    memory_read("agent.build_context_pack", "Build a context pack"),
    read(Mcp, "agent.privacy_status", "Read the privacy status"),
    memory_read("agent.explain_retrieval", "Explain a retrieval"),
    read(Mcp, "agent.list_prompts", "List prompts"),
    read(Mcp, "agent.get_prompt", "Read a prompt"),
    memory_read("memory.timeline", "Read the timeline"),
    memory_read("fndr.active_focus", "Read the active focus"),
    memory_read("memory.warm_start", "Warm start"),
    memory_read("memory.agent_onboarding", "Agent onboarding"),
    memory_read("memory.project_wiki", "Read the project wiki"),
    memory_read("memory.claims", "Read claims"),
    memory_read("memory.breakthroughs", "Read breakthroughs"),
    memory_read("memory.source_evidence", "Read source evidence"),
    memory_read("memory.projects", "List projects"),
    memory_read("memory.project_context", "Read project context"),
    memory_read("memory.decisions", "Read decisions"),
    memory_read("memory.errors", "Read errors"),
    memory_read("memory.blockers", "Read blockers"),
    memory_read("memory.todos", "Read to-dos"),
    memory_read("memory.graph_query", "Query the graph"),
    memory_read("memory.graph_context", "Read graph context"),
    memory_read("memory.recent_changes", "Read recent changes"),
    memory_read("ask_fndr", "Ask FNDR"),
    read(Mcp, "get_fndr_stats", "Read statistics"),
    memory_read("get_meeting_transcript", "Read a meeting transcript"),
    memory_read("search_meeting_transcripts", "Search meeting transcripts"),
    memory_read("get_ambient_context", "Read ambient context"),
    memory_read("fndr_context", "Read context"),
    memory_read("fndr_search_code_context", "Search code context"),
    memory_read("fndr_diff", "Read a diff"),
    memory_read("fndr_get_recent_working_state", "Read recent working state"),
    read(Mcp, "fndr_health_check", "Check health"),
    memory_read("fndr.search", "Search memories"),
    memory_read("fndr.answer", "Answer from memories"),
    memory_read("fndr.build_context_pack", "Build a context pack"),
    memory_read("fndr.get_related_memories", "Read related memories"),
    memory_read("fndr.get_memory_subgraph", "Read a memory subgraph"),
    memory_read("fndr.timeline", "Read the timeline"),
    read(Mcp, "fndr.quality_status", "Read quality status"),
    read(Mcp, "fndr.privacy_status", "Read the privacy status"),
    memory_read("fndr.recent_activity", "Read recent activity"),
    memory_read(
        "fndr.search_memories",
        "Search imported and captured memories",
    ),
    memory_read("fndr.standup", "Read a standup draft"),
    read(Mcp, "fndr.list_sources", "List memory sources"),
    // Actions the in-app agent can propose.
    spec(
        Agent,
        "open_url",
        Opens,
        "Open a link",
        CANNOT_CLOSE,
        Some("url"),
    ),
    spec(
        Agent,
        "open_file",
        Opens,
        "Open a file",
        CANNOT_CLOSE,
        Some("path"),
    ),
    spec(
        Agent,
        "reveal_in_finder",
        Opens,
        "Show a file in Finder",
        CANNOT_CLOSE,
        Some("path"),
    ),
    spec(
        Agent,
        "run_read_only_command",
        Committing,
        "Run a command",
        Undo::Unavailable {
            reason: "A command's effects are outside FNDR; its output is kept in the audit log.",
        },
        Some("command"),
    ),
    spec(
        Agent,
        "create_draft_note",
        Reversible,
        "Create a draft note",
        NOTE_REMOVABLE,
        Some("title"),
    ),
    spec(
        Agent,
        "create_checklist",
        Reversible,
        "Create a checklist",
        NOTE_REMOVABLE,
        Some("title"),
    ),
    spec(
        Agent,
        "run_project_test",
        Committing,
        "Run a project's tests",
        Undo::Unavailable {
            reason: "Tests can write files in the project; FNDR does not track them.",
        },
        Some("project"),
    ),
    spec(
        Agent,
        "delegate_to_hermes",
        Committing,
        "Hand a task to Hermes",
        Undo::Unavailable {
            reason: "Hermes acts outside FNDR once the task is sent.",
        },
        Some("task"),
    ),
    spec(
        Agent,
        "delegate_to_claude_code",
        Committing,
        "Hand a task to Claude Code",
        Undo::Unavailable {
            reason: "Claude Code acts outside FNDR once the task is sent.",
        },
        Some("task"),
    ),
    spec(
        Agent,
        "send_external_message",
        Committing,
        "Send a daily output to Slack, mail or Linear",
        Undo::Unavailable {
            reason: "The message leaves FNDR once it is sent.",
        },
        Some("destination"),
    ),
    spec(
        Agent,
        "create_skill_candidate",
        Reversible,
        "Draft a skill",
        Undo::Available {
            how: "Discard the draft; it is never active until reviewed.",
            inverse: None,
        },
        Some("title"),
    ),
    spec(
        Agent,
        "create_eval_candidate",
        Reversible,
        "Draft an evaluation case",
        Undo::Available {
            how: "Discard the draft; it is never active until reviewed.",
            inverse: None,
        },
        Some("title"),
    ),
    spec(
        Agent,
        "schedule_agent_job",
        Committing,
        "Schedule an agent job",
        Undo::Available {
            how: "Cancel the scheduled job.",
            inverse: None,
        },
        Some("title"),
    ),
    spec(
        Agent,
        "unsupported",
        Never,
        "Unsupported action",
        Undo::Unavailable {
            reason: "Never runs.",
        },
        None,
    ),
];

pub fn all() -> &'static [ActionSpec] {
    ALL
}

pub fn spec_for(surface: Surface, tool: &str) -> Option<&'static ActionSpec> {
    ALL.iter()
        .find(|spec| spec.surface == surface && spec.tool == tool)
}

/// The registry name of an agent action kind. The match is exhaustive, so a
/// new kind does not compile until it is named here and given a row above.
pub fn agent_tool(kind: &AgentActionKind) -> &'static str {
    match kind {
        AgentActionKind::OpenUrl => "open_url",
        AgentActionKind::OpenFile => "open_file",
        AgentActionKind::RevealInFinder => "reveal_in_finder",
        AgentActionKind::RunReadOnlyCommand => "run_read_only_command",
        AgentActionKind::CreateDraftNote => "create_draft_note",
        AgentActionKind::CreateChecklist => "create_checklist",
        AgentActionKind::RunProjectTest => "run_project_test",
        AgentActionKind::DelegateToHermes => "delegate_to_hermes",
        AgentActionKind::DelegateToClaudeCode => "delegate_to_claude_code",
        AgentActionKind::CreateSkillCandidate => "create_skill_candidate",
        AgentActionKind::CreateEvalCandidate => "create_eval_candidate",
        AgentActionKind::ScheduleAgentJob => "schedule_agent_job",
        AgentActionKind::SendExternalMessage => "send_external_message",
        AgentActionKind::Unsupported => "unsupported",
    }
}

/// The kinds, in registry order, so a test can walk them.
pub const AGENT_KINDS: [AgentActionKind; 14] = [
    AgentActionKind::OpenUrl,
    AgentActionKind::OpenFile,
    AgentActionKind::RevealInFinder,
    AgentActionKind::RunReadOnlyCommand,
    AgentActionKind::CreateDraftNote,
    AgentActionKind::CreateChecklist,
    AgentActionKind::RunProjectTest,
    AgentActionKind::DelegateToHermes,
    AgentActionKind::DelegateToClaudeCode,
    AgentActionKind::CreateSkillCandidate,
    AgentActionKind::CreateEvalCandidate,
    AgentActionKind::ScheduleAgentJob,
    AgentActionKind::SendExternalMessage,
    AgentActionKind::Unsupported,
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn a_surface_never_lists_the_same_tool_twice() {
        let mut seen = HashSet::new();
        for spec in ALL {
            assert!(
                seen.insert((spec.surface, spec.tool)),
                "{:?} {} is listed twice",
                spec.surface,
                spec.tool
            );
        }
    }

    #[test]
    fn every_action_that_changes_something_says_how_it_is_undone_or_why_it_cannot_be() {
        for spec in ALL.iter().filter(|spec| spec.effect != Effect::Read) {
            match spec.undo {
                Undo::Available { how, .. } => {
                    assert!(!how.trim().is_empty(), "{} has an empty undo", spec.tool)
                }
                Undo::Unavailable { reason } => {
                    assert!(!reason.trim().is_empty(), "{} has no reason", spec.tool)
                }
            }
        }
    }

    #[test]
    fn an_undo_names_a_tool_the_registry_knows() {
        for spec in ALL {
            if let Undo::Available {
                inverse: Some(tool),
                ..
            } = spec.undo
            {
                assert!(
                    ALL.iter().any(|other| other.tool == tool),
                    "{} undoes with unknown tool {tool}",
                    spec.tool
                );
            }
        }
    }

    #[test]
    fn every_agent_kind_has_a_row_named_by_its_serialized_form() {
        for kind in AGENT_KINDS.iter() {
            let serialized = serde_json::to_value(kind).unwrap();
            assert_eq!(serialized, agent_tool(kind), "{kind:?}");
            assert!(
                spec_for(Surface::Agent, agent_tool(kind)).is_some(),
                "{kind:?}"
            );
        }
        assert_eq!(
            ALL.iter()
                .filter(|spec| spec.surface == Surface::Agent)
                .count(),
            AGENT_KINDS.len()
        );
    }

    #[test]
    fn only_mcp_reads_can_return_memory_text() {
        for spec in ALL.iter().filter(|spec| spec.returns_memory_text) {
            assert_eq!(spec.surface, Surface::Mcp, "{}", spec.tool);
            assert_eq!(spec.effect, Effect::Read, "{}", spec.tool);
        }
    }
}
