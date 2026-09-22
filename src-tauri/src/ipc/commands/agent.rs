use crate::agent::actions::{policy_for_action, AgentAction, AgentActionKind, AgentActionStatus};
use crate::agent::approvals::is_action_approved;
use crate::agent::audit::{append_agent_action, get_agent_action_by_id, update_action_status};
use crate::agent::audit::{
    append_feedback, explanation_from_audit, get_agent_audit_run as load_agent_audit_run,
    list_agent_audit_runs as load_agent_audit_runs, AgentAuditRecord, AgentRunStatus,
    ExplainRetrievalRequest, RateResultRequest, RetrievalExplanation,
};
use crate::agent::evals::{append_eval_draft, list_eval_drafts, propose_eval_from_audit};
use crate::agent::execution::execute_action;
use crate::agent::policy::RiskLevel;
use crate::agent::skills::{append_skill_draft, list_skill_drafts, propose_skill_from_audit};
use crate::agent::{
    self, AgentContextPack, AgentContextRequest, AgentEvalCase, AgentMode, AgentPrompt,
    AgentRunResponse, AgentSkillCandidate,
};
use crate::AppState;
use std::path::Path;
use std::sync::Arc;
use tauri::State;

#[tauri::command]
pub async fn build_agent_context_pack(
    state: State<'_, Arc<AppState>>,
    request: AgentContextRequest,
) -> Result<AgentContextPack, String> {
    agent::build_agent_context_pack(state.inner(), request).await
}

#[tauri::command]
pub async fn run_agent_request(
    state: State<'_, Arc<AppState>>,
    request: AgentContextRequest,
) -> Result<AgentRunResponse, String> {
    agent::context::run_agent_request(state.inner(), request).await
}

#[tauri::command]
pub async fn list_agent_audit_runs(
    state: State<'_, Arc<AppState>>,
    limit: Option<usize>,
    mode: Option<AgentMode>,
    status: Option<AgentRunStatus>,
) -> Result<Vec<AgentAuditRecord>, String> {
    load_agent_audit_runs(
        state.inner().app_data_dir.as_path(),
        limit.unwrap_or(20),
        mode,
        status,
    )
}

#[tauri::command]
pub async fn get_agent_audit_run(
    state: State<'_, Arc<AppState>>,
    run_id: String,
) -> Result<Option<AgentAuditRecord>, String> {
    load_agent_audit_run(state.inner().app_data_dir.as_path(), &run_id)
}

#[tauri::command]
pub async fn explain_agent_retrieval(
    state: State<'_, Arc<AppState>>,
    request: ExplainRetrievalRequest,
) -> Result<RetrievalExplanation, String> {
    if let Some(run_id) = request.run_id.as_deref() {
        let record = load_agent_audit_run(state.inner().app_data_dir.as_path(), run_id)?
            .ok_or_else(|| format!("No agent audit run found for {run_id}"))?;
        return Ok(explanation_from_audit(&record));
    }

    let query = request
        .query
        .clone()
        .or_else(|| request.context_pack_id.clone())
        .unwrap_or_else(|| "recent agent context".to_string());
    let response = agent::context::run_agent_request(
        state.inner(),
        AgentContextRequest {
            user_goal: query,
            mode: AgentMode::Ask,
            project: request.project,
            budget_tokens: 900,
            ..Default::default()
        },
    )
    .await?;
    let record = load_agent_audit_run(state.inner().app_data_dir.as_path(), &response.run_id)?
        .ok_or_else(|| "Agent run was created but audit detail was unavailable".to_string())?;
    Ok(explanation_from_audit(&record))
}

#[tauri::command]
pub async fn rate_agent_result(
    state: State<'_, Arc<AppState>>,
    request: RateResultRequest,
) -> Result<crate::agent::audit::AgentRetrievalFeedback, String> {
    append_feedback(state.inner().app_data_dir.as_path(), request)
}

#[tauri::command]
pub async fn propose_skill_from_run(
    state: State<'_, Arc<AppState>>,
    run_id: String,
) -> Result<AgentSkillCandidate, String> {
    let record = load_agent_audit_run(state.inner().app_data_dir.as_path(), &run_id)?
        .ok_or_else(|| format!("No agent audit run found for {run_id}"))?;
    let draft = propose_skill_from_audit(&record)?;
    append_skill_draft(state.inner().app_data_dir.as_path(), &draft)?;
    Ok(draft)
}

#[tauri::command]
pub async fn list_agent_skill_drafts(
    state: State<'_, Arc<AppState>>,
    limit: Option<usize>,
) -> Result<Vec<AgentSkillCandidate>, String> {
    list_skill_drafts(state.inner().app_data_dir.as_path(), limit.unwrap_or(20))
}

#[tauri::command]
pub async fn propose_eval_from_run(
    state: State<'_, Arc<AppState>>,
    run_id: String,
) -> Result<AgentEvalCase, String> {
    let record = load_agent_audit_run(state.inner().app_data_dir.as_path(), &run_id)?
        .ok_or_else(|| format!("No agent audit run found for {run_id}"))?;
    let draft = propose_eval_from_audit(&record)?;
    append_eval_draft(state.inner().app_data_dir.as_path(), &draft)?;
    Ok(draft)
}

#[tauri::command]
pub async fn list_agent_eval_drafts(
    state: State<'_, Arc<AppState>>,
    limit: Option<usize>,
) -> Result<Vec<AgentEvalCase>, String> {
    list_eval_drafts(state.inner().app_data_dir.as_path(), limit.unwrap_or(20))
}

#[tauri::command]
pub async fn list_agent_prompts() -> Result<Vec<AgentPrompt>, String> {
    Ok(agent::list_agent_prompts())
}

#[tauri::command]
pub async fn get_agent_prompt(name: String) -> Result<Option<AgentPrompt>, String> {
    Ok(agent::get_agent_prompt(&name))
}

fn propose_action_logic(
    app_data_dir: &Path,
    run_id: &str,
    kind: AgentActionKind,
    risk_level: RiskLevel,
    description: &str,
    input: serde_json::Value,
) -> Result<AgentAction, String> {
    let decision = policy_for_action(&kind, &risk_level, &AgentMode::Act);
    if !decision.allowed {
        return Err(decision.blocked_because.unwrap_or(decision.reason));
    }
    let input_map: std::collections::BTreeMap<String, serde_json::Value> = input
        .as_object()
        .ok_or("input must be a JSON object")?
        .clone()
        .into_iter()
        .collect();
    let status = if decision.requires_approval {
        AgentActionStatus::NeedsApproval
    } else {
        AgentActionStatus::Approved
    };
    let action = AgentAction {
        run_id: run_id.to_string(),
        title: description.to_string(),
        description: description.to_string(),
        kind,
        input: input_map,
        risk_level,
        status,
        ..Default::default()
    };
    append_agent_action(app_data_dir, &action)?;
    Ok(action)
}

fn approve_action_logic(app_data_dir: &Path, action_id: &str) -> Result<AgentAction, String> {
    update_action_status(
        app_data_dir,
        action_id,
        AgentActionStatus::Approved,
        Some(chrono::Utc::now().timestamp()),
        None,
        None,
    )
}

async fn execute_action_logic(app_data_dir: &Path, action_id: &str) -> Result<AgentAction, String> {
    if !is_action_approved(app_data_dir, action_id)? {
        return Err(format!("Action {} has not been approved", action_id));
    }
    let action = get_agent_action_by_id(app_data_dir, action_id)?
        .ok_or_else(|| format!("Action not found: {}", action_id))?;
    let result = execute_action(&action).await;
    let (status, result) = match result {
        Ok(r) => (AgentActionStatus::Succeeded, Some(r)),
        Err(e) => (
            AgentActionStatus::Failed,
            Some(crate::agent::actions::ActionResult {
                success: false,
                output: String::new(),
                error: Some(e),
                duration_ms: 0,
            }),
        ),
    };
    update_action_status(
        app_data_dir,
        action_id,
        status,
        None,
        Some(chrono::Utc::now().timestamp()),
        result,
    )
}

#[tauri::command]
pub async fn propose_agent_action(
    state: State<'_, Arc<AppState>>,
    run_id: String,
    kind: AgentActionKind,
    risk_level: RiskLevel,
    description: String,
    input: serde_json::Value,
) -> Result<AgentAction, String> {
    propose_action_logic(
        state.inner().app_data_dir.as_path(),
        &run_id,
        kind,
        risk_level,
        &description,
        input,
    )
}

#[tauri::command]
pub async fn approve_agent_action(
    state: State<'_, Arc<AppState>>,
    action_id: String,
) -> Result<AgentAction, String> {
    approve_action_logic(state.inner().app_data_dir.as_path(), &action_id)
}

#[tauri::command]
pub async fn execute_agent_action(
    state: State<'_, Arc<AppState>>,
    action_id: String,
) -> Result<AgentAction, String> {
    execute_action_logic(state.inner().app_data_dir.as_path(), &action_id).await
}

#[cfg(test)]
mod action_lifecycle_tests {
    use super::*;
    use crate::agent::actions::{AgentActionKind, AgentActionStatus};
    use crate::agent::policy::RiskLevel;
    use tempfile::tempdir;

    #[tokio::test]
    async fn propose_then_approve_then_execute_runs_the_command_and_records_the_result() {
        let dir = tempdir().unwrap();
        let action = propose_action_logic(
            dir.path(),
            "run-1",
            AgentActionKind::RunReadOnlyCommand,
            RiskLevel::Medium,
            "Check repo status",
            serde_json::json!({"command": "git", "args": ["status"]}),
        )
        .unwrap();
        assert_eq!(action.status, AgentActionStatus::NeedsApproval);

        let approved = approve_action_logic(dir.path(), &action.id).unwrap();
        assert_eq!(approved.status, AgentActionStatus::Approved);

        let executed = execute_action_logic(dir.path(), &action.id).await.unwrap();
        assert_eq!(executed.status, AgentActionStatus::Succeeded);
        assert!(executed.result.unwrap().success);
    }

    #[tokio::test]
    async fn executing_before_approval_is_rejected() {
        let dir = tempdir().unwrap();
        let action = propose_action_logic(
            dir.path(),
            "run-2",
            AgentActionKind::RunReadOnlyCommand,
            RiskLevel::Medium,
            "Check repo status",
            serde_json::json!({"command": "git", "args": ["status"]}),
        )
        .unwrap();
        let result = execute_action_logic(dir.path(), &action.id).await;
        assert!(result.is_err());
    }
}
