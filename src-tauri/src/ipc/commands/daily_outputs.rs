//! Daily outputs shown in the app and sent only after a visible approval.

use crate::outputs::{self, send::SecretStore, OutputDoc, OutputFormat, OutputKind};
use crate::AppState;
use serde::Serialize;
use std::sync::Arc;
use tauri::State;

const MAX_RANGE_MS: i64 = 31 * 86_400_000;

fn validate_range(start_ms: i64, end_ms: i64) -> Result<(), String> {
    if start_ms < 0 || end_ms <= start_ms || end_ms - start_ms > MAX_RANGE_MS {
        return Err("Choose a range from one moment up to 31 days.".to_string());
    }
    Ok(())
}

fn document(id: &str) -> Result<OutputDoc, String> {
    outputs::state::doc_by_id(id)
        .ok_or_else(|| "That output is no longer open. Generate it again.".to_string())
}

fn proposal_for(
    doc: &OutputDoc,
    tool: &str,
    recipient: Option<&str>,
    secrets: &dyn SecretStore,
) -> Result<outputs::tools::Proposal, String> {
    match tool {
        outputs::tools::TOOL_SLACK => outputs::send::slack_proposal(
            secrets,
            &doc.title,
            &outputs::render::render(doc, OutputFormat::Markdown, true),
        ),
        outputs::tools::TOOL_LINEAR => outputs::send::linear_proposal(
            secrets,
            outputs::send::LINEAR_ENDPOINT,
            &doc.title,
            &outputs::render::render(doc, OutputFormat::Markdown, true),
        ),
        outputs::tools::TOOL_MAIL => outputs::send::mail_proposal(
            recipient,
            &doc.title,
            &outputs::render::render(doc, OutputFormat::PlainText, true),
        ),
        _ => Err("That send is not a known output action.".to_string()),
    }
}

async fn perform_send(
    send: &outputs::tools::ApprovedSend,
) -> Result<outputs::send::SendReceipt, String> {
    let secrets = outputs::send::KeychainSecrets;
    let mut receipt = match send.tool.as_str() {
        outputs::tools::TOOL_SLACK => outputs::send::post_slack(send, &secrets).await?,
        outputs::tools::TOOL_LINEAR => {
            outputs::send::create_linear_issue(send, &secrets, outputs::send::LINEAR_ENDPOINT)
                .await?
        }
        outputs::tools::TOOL_MAIL => outputs::send::open_mail_draft(send)?,
        _ => return Err("That send is not a known output action.".to_string()),
    };
    receipt.trust_id = outputs::state::remember_successful_send(
        send,
        &receipt.destination,
        chrono::Utc::now().timestamp_millis(),
    );
    Ok(receipt)
}

#[tauri::command]
pub async fn generate_daily_output(
    state: State<'_, Arc<AppState>>,
    kind: OutputKind,
    start_ms: i64,
    end_ms: i64,
    polish: bool,
) -> Result<OutputDoc, String> {
    validate_range(start_ms, end_ms)?;
    let evidence = outputs::gather::gather(&state, start_ms, end_ms).await?;
    let mut doc = outputs::build(
        kind,
        &evidence,
        start_ms,
        end_ms,
        chrono::Utc::now().timestamp_millis(),
    );
    if polish {
        if let Some(engine) = state.inference_engine() {
            outputs::polish::polish_doc(&mut doc, 4, |line| {
                let engine = engine.clone();
                async move { engine.rephrase_output_line(&line).await }
            })
            .await;
        }
    }
    outputs::state::remember_doc(doc.clone());
    Ok(doc)
}

#[tauri::command]
pub fn render_daily_output(id: String, format: OutputFormat) -> Result<String, String> {
    Ok(outputs::render::render(&document(&id)?, format, true))
}

#[tauri::command]
pub async fn save_daily_output(id: String, format: OutputFormat) -> Result<Option<String>, String> {
    let doc = document(&id)?;
    let text = outputs::render::render(&doc, format, true);
    let (ext, label) = match format {
        OutputFormat::Markdown => ("md", "Markdown"),
        OutputFormat::PlainText => ("txt", "Plain text"),
        OutputFormat::Html => ("html", "HTML"),
    };
    let filename = format!("FNDR-{}.{}", doc.kind.as_str(), ext);
    tokio::task::spawn_blocking(move || {
        let path = rfd::FileDialog::new()
            .set_file_name(filename)
            .add_filter(label, &[ext])
            .save_file();
        let Some(path) = path else { return Ok(None) };
        outputs::send::write_export(&path, &text)?;
        Ok(Some(path.to_string_lossy().to_string()))
    })
    .await
    .map_err(|e| format!("The save dialog closed unexpectedly: {e}"))?
}

#[derive(Serialize)]
pub struct OutputDestinationStatus {
    slack_webhook: bool,
    linear_api_key: bool,
    linear_team_id: bool,
}

#[tauri::command]
pub fn output_destination_status() -> OutputDestinationStatus {
    let secrets = outputs::send::KeychainSecrets;
    OutputDestinationStatus {
        slack_webhook: secrets.get(outputs::send::SECRET_SLACK_WEBHOOK).is_some(),
        linear_api_key: secrets.get(outputs::send::SECRET_LINEAR_KEY).is_some(),
        linear_team_id: secrets.get(outputs::send::SECRET_LINEAR_TEAM).is_some(),
    }
}

#[tauri::command]
pub fn set_output_destination(name: String, value: String) -> Result<(), String> {
    let key = match name.as_str() {
        outputs::send::SECRET_SLACK_WEBHOOK => outputs::send::SECRET_SLACK_WEBHOOK,
        outputs::send::SECRET_LINEAR_KEY => outputs::send::SECRET_LINEAR_KEY,
        outputs::send::SECRET_LINEAR_TEAM => outputs::send::SECRET_LINEAR_TEAM,
        _ => return Err("That destination setting is not supported.".to_string()),
    };
    let value = value.trim();
    let secrets = outputs::send::KeychainSecrets;
    if value.is_empty() {
        return secrets.delete(key);
    }
    if key == outputs::send::SECRET_SLACK_WEBHOOK {
        outputs::send::parse_webhook(value)?;
    }
    if value.len() > 2_000 {
        return Err("That destination setting is too long.".to_string());
    }
    secrets.set(key, value)
}

#[tauri::command]
pub fn propose_daily_output_send(
    state: State<'_, Arc<AppState>>,
    id: String,
    tool: String,
    recipient: Option<String>,
) -> Result<outputs::tools::ApprovalCard, String> {
    let doc = document(&id)?;
    let secrets = outputs::send::KeychainSecrets;
    let proposal = proposal_for(&doc, &tool, recipient.as_deref(), &secrets)?;
    let kill_switch = state.config.read().actions_kill_switch;
    outputs::state::approvals()
        .propose(proposal, kill_switch, chrono::Utc::now().timestamp_millis())
        .map_err(|e| e.message())
}

#[tauri::command]
pub async fn resolve_daily_output_send(
    state: State<'_, Arc<AppState>>,
    approval_id: String,
    approved: bool,
) -> Result<Option<outputs::send::SendReceipt>, String> {
    let kill_switch = state.config.read().actions_kill_switch;
    let resolved = outputs::state::approvals().resolve(
        &approval_id,
        approved,
        kill_switch,
        chrono::Utc::now().timestamp_millis(),
    );
    let send = match resolved {
        Ok(send) => send,
        Err(outputs::tools::Refusal::Declined) => return Ok(None),
        Err(reason) => return Err(reason.message()),
    };
    Ok(Some(perform_send(&send).await?))
}

/// A direct click may reuse a destination the person allowed after an earlier
/// successful approved send. Without a matching exact target this returns
/// None, so the caller shows the ordinary approval card.
#[tauri::command]
pub async fn send_daily_output_allowed(
    state: State<'_, Arc<AppState>>,
    id: String,
    tool: String,
    recipient: Option<String>,
) -> Result<Option<outputs::send::SendReceipt>, String> {
    let doc = document(&id)?;
    let secrets = outputs::send::KeychainSecrets;
    let proposal = proposal_for(&doc, &tool, recipient.as_deref(), &secrets)?;
    let kill_switch = state.config.read().actions_kill_switch;
    outputs::tools::gate(proposal.tool, kill_switch).map_err(|e| e.message())?;
    let Some(target) = proposal.target.as_deref() else {
        return Ok(None);
    };
    if !outputs::send::is_destination_allowed(&secrets, proposal.tool, target) {
        return Ok(None);
    }
    let send = outputs::tools::ApprovedSend {
        tool: proposal.tool.to_string(),
        title: proposal.title,
        body: proposal.body,
        target: proposal.target,
    };
    Ok(Some(perform_send(&send).await?))
}

#[tauri::command]
pub fn trust_output_destination(trust_id: String) -> Result<String, String> {
    let success =
        outputs::state::take_successful_send(&trust_id, chrono::Utc::now().timestamp_millis())
            .ok_or_else(|| "That send can no longer be used to allow a destination.".to_string())?;
    outputs::send::allow_destination(
        &outputs::send::KeychainSecrets,
        &success.tool,
        &success.destination,
        &success.target,
    )
}

#[tauri::command]
pub fn list_output_allowed_destinations() -> Vec<outputs::send::AllowedDestination> {
    outputs::send::list_allowed_destinations(&outputs::send::KeychainSecrets)
}

#[tauri::command]
pub fn forget_output_allowed_destination(key: String) -> Result<(), String> {
    outputs::send::forget_destination(&outputs::send::KeychainSecrets, &key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_range_is_bounded_and_ordered() {
        const DAY: i64 = 86_400_000;
        assert!(validate_range(0, DAY).is_ok());
        assert!(validate_range(0, 31 * DAY).is_ok());
        assert!(validate_range(-1, DAY).is_err());
        assert!(validate_range(DAY, DAY).is_err());
        assert!(validate_range(DAY, 0).is_err());
        assert!(validate_range(0, 32 * DAY).is_err());
    }
}
