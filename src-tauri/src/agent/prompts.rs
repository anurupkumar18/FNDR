use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPrompt {
    pub name: String,
    pub title: String,
    pub description: String,
    pub template: String,
}

pub fn list_agent_prompts() -> Vec<AgentPrompt> {
    prompt_specs()
        .iter()
        .map(|(name, title, description, template)| AgentPrompt {
            name: (*name).to_string(),
            title: (*title).to_string(),
            description: (*description).to_string(),
            template: (*template).to_string(),
        })
        .collect()
}

pub fn get_agent_prompt(name: &str) -> Option<AgentPrompt> {
    list_agent_prompts()
        .into_iter()
        .find(|prompt| prompt.name == name)
}

fn prompt_specs() -> &'static [(&'static str, &'static str, &'static str, &'static str)] {
    &[
        (
            "resume_work",
            "Resume Work",
            "Reconstruct the user's recent work and propose the next safe step.",
            "Call memory.resume_work for the recent threads, then fndr.build_context_pack for the user's goal. Cite FNDR memories by id, explain uncertainty, and do not take actions unless the user approves them.",
        ),
        (
            "debug_with_history",
            "Debug With History",
            "Use prior FNDR debugging memories before suggesting a fix.",
            "Call fndr.search for prior errors, files, commands, and decisions, and fndr.build_context_pack for the goal. Use evidence from memories; if exact ranking scores are unavailable, say so. Suggest commands; do not run them.",
        ),
        (
            "write_status_update",
            "Write Status Update",
            "Draft a progress update grounded in captured work.",
            "Call fndr.privacy_status and fndr.build_context_pack. Group evidence by theme, cite memory ids, exclude sensitive/private context, and mark uncertain claims.",
        ),
        (
            "prepare_for_meeting",
            "Prepare For Meeting",
            "Build a meeting prep brief from recent project memory.",
            "Call FNDR context tools first. Summarize recent decisions, blockers, todos, and relevant files. Cite evidence and do not expose raw private history by default.",
        ),
        (
            "handoff_to_coding_agent",
            "Handoff To Coding Agent",
            "Create a coding-agent handoff from FNDR memory.",
            "Call fndr.build_context_pack with agent_type coding_agent and the open files as active_files. Include goals, files, constraints, blockers, commands, and next steps from evidence. Tell the receiving agent to inspect the repo before editing.",
        ),
        (
            "explain_my_thinking",
            "Explain My Thinking",
            "Explain the likely reasoning behind recent work.",
            "Use selected memories and agent.explain_retrieval. Separate evidence-backed facts from inferred intent. Mention dropped/redacted context.",
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_registry_contains_safe_handoff_prompt() {
        let prompt = get_agent_prompt("handoff_to_coding_agent").expect("prompt");

        assert!(prompt.template.contains("fndr.build_context_pack"));
        assert!(prompt.template.contains("before editing"));
    }
}
