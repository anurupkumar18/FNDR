//! Moving an approved output out: a Slack incoming webhook, a Linear issue,
//! a mail draft, a file.
//!
//! Every network send takes an `ApprovedSend` (see `tools`), checks that its
//! tool and host are the ones the person approved, and writes one line to the
//! egress ledger (`privacy_proof`) before the request leaves. Secrets come
//! from a `SecretStore`: the macOS Keychain in the app, a map in tests.

use super::tools::{ApprovedSend, Proposal, TOOL_LINEAR, TOOL_MAIL, TOOL_SLACK};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

pub const SECRET_SLACK_WEBHOOK: &str = "slack_webhook";
pub const SECRET_LINEAR_KEY: &str = "linear_api_key";
pub const SECRET_LINEAR_TEAM: &str = "linear_team_id";
const SECRET_ALLOWED_DESTINATIONS: &str = "allowed_destinations";
pub const LINEAR_ENDPOINT: &str = "https://api.linear.app/graphql";

const KEYCHAIN_SERVICE: &str = "com.fndr.app.outputs";
/// Slack cuts messages near 40,000 characters.
const SLACK_MAX_CHARS: usize = 38_000;
const LINEAR_MAX_CHARS: usize = 50_000;
/// Mail apps drop a `mailto:` URL much longer than this.
const MAILTO_MAX_ENCODED: usize = 1_800;
const TRIM_NOTE: &str = "(trimmed to fit)";

pub trait SecretStore: Send + Sync {
    fn get(&self, name: &str) -> Option<String>;
    fn set(&self, name: &str, value: &str) -> Result<(), String>;
    fn delete(&self, name: &str) -> Result<(), String>;
}

#[derive(Default)]
pub struct MemorySecrets(Mutex<HashMap<String, String>>);

impl SecretStore for MemorySecrets {
    fn get(&self, name: &str) -> Option<String> {
        self.0.lock().get(name).cloned()
    }
    fn set(&self, name: &str, value: &str) -> Result<(), String> {
        self.0.lock().insert(name.to_string(), value.to_string());
        Ok(())
    }
    fn delete(&self, name: &str) -> Result<(), String> {
        self.0.lock().remove(name);
        Ok(())
    }
}

/// The login Keychain, one generic password per secret.
pub struct KeychainSecrets;

#[cfg(target_os = "macos")]
impl SecretStore for KeychainSecrets {
    fn get(&self, name: &str) -> Option<String> {
        security_framework::passwords::get_generic_password(KEYCHAIN_SERVICE, name)
            .ok()
            .and_then(|bytes| String::from_utf8(bytes).ok())
            .filter(|value| !value.is_empty())
    }
    fn set(&self, name: &str, value: &str) -> Result<(), String> {
        security_framework::passwords::set_generic_password(
            KEYCHAIN_SERVICE,
            name,
            value.as_bytes(),
        )
        .map_err(|e| format!("The Keychain refused the value: {e}"))
    }
    fn delete(&self, name: &str) -> Result<(), String> {
        match security_framework::passwords::delete_generic_password(KEYCHAIN_SERVICE, name) {
            Ok(()) => Ok(()),
            // errSecItemNotFound: already gone.
            Err(e) if e.code() == -25300 => Ok(()),
            Err(e) => Err(format!("The Keychain could not delete the value: {e}")),
        }
    }
}

#[cfg(not(target_os = "macos"))]
impl SecretStore for KeychainSecrets {
    fn get(&self, _name: &str) -> Option<String> {
        None
    }
    fn set(&self, _name: &str, _value: &str) -> Result<(), String> {
        Err("Secrets are stored in the macOS Keychain only.".to_string())
    }
    fn delete(&self, _name: &str) -> Result<(), String> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SendReceipt {
    pub tool: String,
    pub destination: String,
    pub detail: String,
    pub trust_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AllowedDestination {
    pub key: String,
    pub tool: String,
    pub destination: String,
}

fn destination_key(tool: &str, target: &str) -> String {
    format!(
        "{:x}",
        Sha256::digest(format!("{tool}\0{target}").as_bytes())
    )
}

pub fn list_allowed_destinations(secrets: &dyn SecretStore) -> Vec<AllowedDestination> {
    secrets
        .get(SECRET_ALLOWED_DESTINATIONS)
        .and_then(|value| serde_json::from_str(&value).ok())
        .unwrap_or_default()
}

pub fn is_destination_allowed(secrets: &dyn SecretStore, tool: &str, target: &str) -> bool {
    let key = destination_key(tool, target);
    list_allowed_destinations(secrets)
        .iter()
        .any(|entry| entry.key == key)
}

pub fn allow_destination(
    secrets: &dyn SecretStore,
    tool: &str,
    destination: &str,
    target: &str,
) -> Result<String, String> {
    if !super::tools::tools().contains(&tool)
        || destination.trim().is_empty()
        || target.trim().is_empty()
    {
        return Err("That destination cannot be allowed.".to_string());
    }
    let key = destination_key(tool, target);
    let mut entries = list_allowed_destinations(secrets);
    entries.retain(|entry| entry.key != key);
    entries.push(AllowedDestination {
        key: key.clone(),
        tool: tool.to_string(),
        destination: destination.to_string(),
    });
    if entries.len() > 32 {
        entries.drain(..entries.len() - 32);
    }
    let value = serde_json::to_string(&entries).map_err(|e| e.to_string())?;
    secrets.set(SECRET_ALLOWED_DESTINATIONS, &value)?;
    Ok(key)
}

pub fn forget_destination(secrets: &dyn SecretStore, key: &str) -> Result<(), String> {
    let mut entries = list_allowed_destinations(secrets);
    entries.retain(|entry| entry.key != key);
    let value = serde_json::to_string(&entries).map_err(|e| e.to_string())?;
    secrets.set(SECRET_ALLOWED_DESTINATIONS, &value)
}

/// Cuts `text` to `max_chars` at a line end and says so. Returns whether it cut.
pub fn fit(text: &str, max_chars: usize) -> (String, bool) {
    if text.chars().count() <= max_chars {
        return (text.to_string(), false);
    }
    let room = max_chars.saturating_sub(TRIM_NOTE.len() + 1);
    let mut kept = String::new();
    for line in text.lines() {
        if kept.chars().count() + line.chars().count() + 1 > room {
            break;
        }
        kept.push_str(line);
        kept.push('\n');
    }
    if kept.is_empty() {
        kept = text.chars().take(room).collect();
        kept.push('\n');
    }
    kept.push_str(TRIM_NOTE);
    (kept, true)
}

fn percent_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 3);
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.~".contains(&byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn valid_address(to: &str) -> bool {
    let to = to.trim();
    !to.is_empty()
        && to.contains('@')
        && !to
            .chars()
            .any(|c| c.is_control() || c.is_whitespace() || "?&#%<>\"".contains(c))
}

/// `mailto:` with the subject and body encoded. The body is cut by lines
/// until the whole URL fits; the second value says whether it was.
pub fn mailto_url(
    to: Option<&str>,
    subject: &str,
    body: &str,
) -> Result<(String, String, bool), String> {
    let recipient = match to.map(str::trim).filter(|value| !value.is_empty()) {
        Some(address) if valid_address(address) => address.to_string(),
        Some(_) => return Err("That is not a plain email address.".to_string()),
        None => String::new(),
    };
    let subject = percent_encode(subject);
    let mut text = body.to_string();
    let mut trimmed = false;
    while percent_encode(&text).len() + subject.len() > MAILTO_MAX_ENCODED {
        let keep = text.trim_end().rfind('\n').unwrap_or(0);
        if keep == 0 {
            text.truncate(text.chars().take(200).map(char::len_utf8).sum());
            trimmed = true;
            break;
        }
        text.truncate(keep);
        trimmed = true;
    }
    if trimmed {
        text.push_str("\n");
        text.push_str(TRIM_NOTE);
    }
    let url = format!(
        "mailto:{recipient}?subject={subject}&body={}",
        percent_encode(&text)
    );
    Ok((url, text, trimmed))
}

pub fn parse_webhook(url: &str) -> Result<reqwest::Url, String> {
    let parsed: reqwest::Url = url
        .trim()
        .parse()
        .map_err(|_| "That is not a valid webhook URL.".to_string())?;
    let host = parsed.host_str().ok_or("The webhook URL has no host.")?;
    let loopback = matches!(host, "127.0.0.1" | "localhost" | "[::1]" | "::1");
    match parsed.scheme() {
        "https" => Ok(parsed),
        "http" if loopback => Ok(parsed),
        _ => Err("A webhook must be an https:// URL.".to_string()),
    }
}

/// The card for posting `text` to the saved Slack webhook.
pub fn slack_proposal(
    secrets: &dyn SecretStore,
    title: &str,
    text: &str,
) -> Result<Proposal, String> {
    let url = secrets
        .get(SECRET_SLACK_WEBHOOK)
        .ok_or("No Slack webhook is saved. Add one first.")?;
    let host = parse_webhook(&url)?
        .host_str()
        .unwrap_or_default()
        .to_string();
    let (body, trimmed) = fit(text, SLACK_MAX_CHARS);
    Ok(Proposal {
        tool: TOOL_SLACK,
        action: "Post to Slack".to_string(),
        destination: host.clone(),
        target: Some(url.to_string()),
        title: title.to_string(),
        body,
        trimmed,
    })
}

pub fn linear_proposal(
    secrets: &dyn SecretStore,
    endpoint: &str,
    title: &str,
    markdown: &str,
) -> Result<Proposal, String> {
    if secrets.get(SECRET_LINEAR_KEY).is_none() {
        return Err("No Linear API key is saved. Add one first.".to_string());
    }
    if secrets.get(SECRET_LINEAR_TEAM).is_none() {
        return Err("No Linear team id is saved. Add one first.".to_string());
    }
    let host = endpoint
        .parse::<reqwest::Url>()
        .ok()
        .and_then(|url| url.host_str().map(str::to_string))
        .ok_or("The Linear endpoint is not a URL.")?;
    let (body, trimmed) = fit(markdown, LINEAR_MAX_CHARS);
    Ok(Proposal {
        tool: TOOL_LINEAR,
        action: "Create a Linear issue".to_string(),
        destination: host.clone(),
        target: Some(endpoint.to_string()),
        title: title.to_string(),
        body,
        trimmed,
    })
}

pub fn mail_proposal(to: Option<&str>, title: &str, text: &str) -> Result<Proposal, String> {
    let (_, body, trimmed) = mailto_url(to, title, text)?;
    Ok(Proposal {
        tool: TOOL_MAIL,
        action: match to.map(str::trim).filter(|value| !value.is_empty()) {
            Some(address) => format!("Open a mail draft to {address}"),
            None => "Open a mail draft".to_string(),
        },
        destination: "your mail app (nothing is sent until you send it there)".to_string(),
        target: to
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        title: title.to_string(),
        body,
        trimmed,
    })
}

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent("FNDR/1.0")
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| format!("Could not build the HTTP client: {e}"))
}

fn log_egress(host: &str, bytes: usize) {
    crate::privacy_proof::record_model_request_including(
        crate::privacy_proof::Feature::DailyOutputSend,
        host,
        bytes,
        &["memory_summaries"],
    );
}

fn require(approved: &ApprovedSend, tool: &str, target: &str) -> Result<(), String> {
    if approved.tool != tool {
        return Err("This approval was for a different action. Nothing was sent.".to_string());
    }
    if approved.target.as_deref() != Some(target) {
        return Err("The destination changed after the approval. Nothing was sent.".to_string());
    }
    Ok(())
}

pub async fn post_slack(
    approved: &ApprovedSend,
    secrets: &dyn SecretStore,
) -> Result<SendReceipt, String> {
    let url = parse_webhook(
        &secrets
            .get(SECRET_SLACK_WEBHOOK)
            .ok_or("No Slack webhook is saved.")?,
    )?;
    let host = url.host_str().unwrap_or_default().to_string();
    require(approved, TOOL_SLACK, url.as_str())?;
    let payload = json!({ "text": approved.body });
    log_egress(&host, approved.body.len());
    let response = client()?
        .post(url)
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("Slack could not be reached: {e}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!(
            "Slack answered {}. Nothing was posted.",
            status.as_u16()
        ));
    }
    Ok(SendReceipt {
        tool: TOOL_SLACK.to_string(),
        destination: host,
        detail: "Posted to Slack.".to_string(),
        trust_id: None,
    })
}

const ISSUE_CREATE: &str = "mutation IssueCreate($input: IssueCreateInput!) { issueCreate(input: $input) { success issue { id identifier url } } }";

pub async fn create_linear_issue(
    approved: &ApprovedSend,
    secrets: &dyn SecretStore,
    endpoint: &str,
) -> Result<SendReceipt, String> {
    let host = endpoint
        .parse::<reqwest::Url>()
        .ok()
        .and_then(|url| url.host_str().map(str::to_string))
        .ok_or("The Linear endpoint is not a URL.")?;
    require(approved, TOOL_LINEAR, endpoint)?;
    let key = secrets
        .get(SECRET_LINEAR_KEY)
        .ok_or("No Linear API key is saved.")?;
    let team = secrets
        .get(SECRET_LINEAR_TEAM)
        .ok_or("No Linear team id is saved.")?;
    let payload = json!({
        "query": ISSUE_CREATE,
        "variables": { "input": {
            "teamId": team,
            "title": approved.title,
            "description": approved.body,
        }},
    });
    log_egress(&host, approved.body.len() + approved.title.len());
    let response = client()?
        .post(endpoint)
        .header("Authorization", key)
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("Linear could not be reached: {e}"))?;
    let status = response.status();
    let reply: serde_json::Value = response.json().await.unwrap_or_default();
    if let Some(message) = reply["errors"][0]["message"].as_str() {
        return Err(format!("Linear refused the issue: {message}"));
    }
    if !status.is_success() || reply["data"]["issueCreate"]["success"] != true {
        return Err(format!(
            "Linear answered {}. No issue was created.",
            status.as_u16()
        ));
    }
    let issue = &reply["data"]["issueCreate"]["issue"];
    let label = issue["identifier"].as_str().unwrap_or("the issue");
    let detail = match issue["url"].as_str() {
        Some(url) => format!("Created {label}: {url}"),
        None => format!("Created {label}."),
    };
    Ok(SendReceipt {
        tool: TOOL_LINEAR.to_string(),
        destination: host,
        detail,
        trust_id: None,
    })
}

/// Opens the draft in the default mail app. FNDR sends nothing; the person
/// sends it from their mail app.
pub fn open_mail_draft(approved: &ApprovedSend) -> Result<SendReceipt, String> {
    if approved.tool != TOOL_MAIL {
        return Err("This approval was for a different action. Nothing was opened.".to_string());
    }
    let (url, _, _) = mailto_url(approved.target.as_deref(), &approved.title, &approved.body)?;
    std::process::Command::new("open")
        .arg(&url)
        .status()
        .map_err(|e| format!("Could not open the mail app: {e}"))
        .and_then(|status| {
            status
                .success()
                .then_some(())
                .ok_or_else(|| "The mail app did not open.".to_string())
        })?;
    Ok(SendReceipt {
        tool: TOOL_MAIL.to_string(),
        destination: approved
            .target
            .clone()
            .unwrap_or_else(|| "mail app".to_string()),
        detail: "Opened a draft in the mail app.".to_string(),
        trust_id: None,
    })
}

/// Writes an export the person chose a path for in the save dialog.
pub fn write_export(path: &Path, text: &str) -> Result<(), String> {
    let allowed = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_lowercase)
        .is_some_and(|ext| ["md", "txt", "html", "htm"].contains(&ext.as_str()));
    if !allowed {
        return Err("Save as .md, .txt or .html.".to_string());
    }
    if !path.parent().is_some_and(Path::is_dir) {
        return Err("That folder does not exist.".to_string());
    }
    std::fs::write(path, text).map_err(|e| format!("Could not write the file: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outputs::tools::SendApprovals;
    use axum::{body::Body, extract::State, http::Request, response::IntoResponse, Router};
    use std::sync::Arc;

    #[derive(Clone, Debug)]
    struct Captured {
        method: String,
        path: String,
        authorization: Option<String>,
        content_type: Option<String>,
        body: String,
    }

    #[derive(Clone)]
    struct Fake {
        seen: Arc<Mutex<Vec<Captured>>>,
        status: u16,
        reply: String,
    }

    async fn record(State(fake): State<Fake>, request: Request<Body>) -> impl IntoResponse {
        let (parts, body) = request.into_parts();
        let bytes = axum::body::to_bytes(body, 1 << 20).await.unwrap();
        fake.seen.lock().push(Captured {
            method: parts.method.to_string(),
            path: parts.uri.path().to_string(),
            authorization: parts
                .headers
                .get("authorization")
                .map(|v| v.to_str().unwrap().to_string()),
            content_type: parts
                .headers
                .get("content-type")
                .map(|v| v.to_str().unwrap().to_string()),
            body: String::from_utf8(bytes.to_vec()).unwrap(),
        });
        (
            axum::http::StatusCode::from_u16(fake.status).unwrap(),
            fake.reply.clone(),
        )
    }

    async fn serve(status: u16, reply: &str) -> (String, Arc<Mutex<Vec<Captured>>>) {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let fake = Fake {
            seen: seen.clone(),
            status,
            reply: reply.to_string(),
        };
        let app = Router::new().fallback(record).with_state(fake);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        (format!("http://{addr}"), seen)
    }

    fn approve(proposal: Proposal) -> ApprovedSend {
        let approvals = SendApprovals::default();
        let card = approvals.propose(proposal, false, 0).unwrap();
        approvals
            .resolve(&card.approval_id, true, false, 1)
            .unwrap()
    }

    #[tokio::test]
    async fn slack_gets_one_json_post_of_exactly_the_approved_text() {
        let (base, seen) = serve(200, "ok").await;
        let secrets = MemorySecrets::default();
        secrets
            .set(
                SECRET_SLACK_WEBHOOK,
                &format!("{base}/services/T000/B000/XXXX"),
            )
            .unwrap();
        let proposal =
            slack_proposal(&secrets, "Standup", "Yesterday\n- Fixed the export job [1]").unwrap();
        assert_eq!(
            proposal.target.as_deref(),
            Some(format!("{base}/services/T000/B000/XXXX").as_str())
        );
        let receipt = post_slack(&approve(proposal), &secrets).await.unwrap();
        assert_eq!(receipt.detail, "Posted to Slack.");
        let seen = seen.lock();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].method, "POST");
        assert_eq!(seen[0].path, "/services/T000/B000/XXXX");
        assert!(seen[0]
            .content_type
            .as_deref()
            .unwrap()
            .starts_with("application/json"));
        let body: serde_json::Value = serde_json::from_str(&seen[0].body).unwrap();
        assert_eq!(
            body,
            json!({ "text": "Yesterday\n- Fixed the export job [1]" })
        );
    }

    #[tokio::test]
    async fn a_slack_error_status_is_reported_not_swallowed() {
        let (base, _seen) = serve(404, "no_service").await;
        let secrets = MemorySecrets::default();
        secrets
            .set(SECRET_SLACK_WEBHOOK, &format!("{base}/x"))
            .unwrap();
        let proposal = slack_proposal(&secrets, "Standup", "text").unwrap();
        let error = post_slack(&approve(proposal), &secrets).await.unwrap_err();
        assert!(error.contains("404"), "{error}");
    }

    #[tokio::test]
    async fn linear_gets_a_graphql_issue_create_with_the_key_in_the_header() {
        let reply = r#"{"data":{"issueCreate":{"success":true,"issue":{"id":"i1","identifier":"ENG-42","url":"https://linear.app/x/issue/ENG-42"}}}}"#;
        let (base, seen) = serve(200, reply).await;
        let secrets = MemorySecrets::default();
        secrets.set(SECRET_LINEAR_KEY, "lin_api_TESTKEY").unwrap();
        secrets.set(SECRET_LINEAR_TEAM, "team-uuid").unwrap();
        let endpoint = format!("{base}/graphql");
        let proposal = linear_proposal(
            &secrets,
            &endpoint,
            "Weekly status",
            "## Completed\n- Shipped it [1]",
        )
        .unwrap();
        let receipt = create_linear_issue(&approve(proposal), &secrets, &endpoint)
            .await
            .unwrap();
        assert!(receipt.detail.contains("ENG-42"));
        assert!(receipt.detail.contains("https://linear.app/x/issue/ENG-42"));
        let seen = seen.lock();
        assert_eq!(seen[0].path, "/graphql");
        assert_eq!(seen[0].authorization.as_deref(), Some("lin_api_TESTKEY"));
        let body: serde_json::Value = serde_json::from_str(&seen[0].body).unwrap();
        assert!(body["query"].as_str().unwrap().contains("issueCreate"));
        assert_eq!(body["variables"]["input"]["teamId"], "team-uuid");
        assert_eq!(body["variables"]["input"]["title"], "Weekly status");
        assert_eq!(
            body["variables"]["input"]["description"],
            "## Completed\n- Shipped it [1]"
        );
        assert!(
            !seen[0].body.contains("lin_api_TESTKEY"),
            "the key goes in the header only"
        );
    }

    #[tokio::test]
    async fn a_linear_error_message_is_surfaced() {
        let (base, _seen) = serve(200, r#"{"errors":[{"message":"teamId is invalid"}]}"#).await;
        let secrets = MemorySecrets::default();
        secrets.set(SECRET_LINEAR_KEY, "k").unwrap();
        secrets.set(SECRET_LINEAR_TEAM, "t").unwrap();
        let endpoint = format!("{base}/graphql");
        let proposal = linear_proposal(&secrets, &endpoint, "T", "B").unwrap();
        let error = create_linear_issue(&approve(proposal), &secrets, &endpoint)
            .await
            .unwrap_err();
        assert!(error.contains("teamId is invalid"), "{error}");
    }

    #[tokio::test]
    async fn an_approval_for_one_tool_cannot_send_through_another() {
        let (base, seen) = serve(200, "ok").await;
        let secrets = MemorySecrets::default();
        secrets
            .set(SECRET_SLACK_WEBHOOK, &format!("{base}/x"))
            .unwrap();
        secrets.set(SECRET_LINEAR_KEY, "k").unwrap();
        secrets.set(SECRET_LINEAR_TEAM, "t").unwrap();
        let slack_card = approve(slack_proposal(&secrets, "T", "B").unwrap());
        let error = create_linear_issue(&slack_card, &secrets, &format!("{base}/graphql"))
            .await
            .unwrap_err();
        assert!(error.contains("different action"));
        assert!(seen.lock().is_empty(), "nothing left the machine");
    }

    #[tokio::test]
    async fn a_webhook_changed_after_the_card_was_shown_is_refused() {
        let (base, seen) = serve(200, "ok").await;
        let secrets = MemorySecrets::default();
        secrets
            .set(SECRET_SLACK_WEBHOOK, &format!("{base}/x"))
            .unwrap();
        let approved = approve(slack_proposal(&secrets, "T", "B").unwrap());
        secrets
            .set(
                SECRET_SLACK_WEBHOOK,
                &format!("{}/x", base.replace("127.0.0.1", "localhost")),
            )
            .unwrap();
        let error = post_slack(&approved, &secrets).await.unwrap_err();
        assert!(error.contains("destination changed"), "{error}");
        assert!(seen.lock().is_empty());
    }

    #[tokio::test]
    async fn a_webhook_path_changed_on_the_same_host_is_refused() {
        let (base, seen) = serve(200, "ok").await;
        let secrets = MemorySecrets::default();
        secrets
            .set(SECRET_SLACK_WEBHOOK, &format!("{base}/approved"))
            .unwrap();
        let approved = approve(slack_proposal(&secrets, "T", "B").unwrap());
        secrets
            .set(SECRET_SLACK_WEBHOOK, &format!("{base}/different"))
            .unwrap();
        let error = post_slack(&approved, &secrets).await.unwrap_err();
        assert!(error.contains("destination changed"), "{error}");
        assert!(seen.lock().is_empty());
    }

    #[tokio::test]
    async fn a_network_send_writes_the_egress_ledger() {
        let (base, _seen) = serve(200, "ok").await;
        let secrets = MemorySecrets::default();
        secrets
            .set(SECRET_SLACK_WEBHOOK, &format!("{base}/x"))
            .unwrap();
        let approved = approve(slack_proposal(&secrets, "T", "B").unwrap());
        let before =
            crate::privacy_proof::build_privacy_proof(&crate::CapturePipelineStats::default());
        post_slack(&approved, &secrets).await.unwrap();
        let after =
            crate::privacy_proof::build_privacy_proof(&crate::CapturePipelineStats::default());
        assert!(after.egress_requests > before.egress_requests);
        assert!(after
            .model_requests
            .iter()
            .any(|row| row.feature == "daily_output_send" && row.host == "127.0.0.1"));
    }

    #[test]
    fn webhooks_must_be_https_except_on_this_machine() {
        assert!(parse_webhook("https://hooks.slack.com/services/T/B/X").is_ok());
        assert!(parse_webhook("http://127.0.0.1:9/x").is_ok());
        assert!(parse_webhook("http://hooks.slack.com/services/T/B/X").is_err());
        assert!(parse_webhook("ftp://hooks.slack.com/x").is_err());
        assert!(parse_webhook("not a url").is_err());
    }

    #[test]
    fn proposals_without_saved_secrets_say_what_is_missing() {
        let secrets = MemorySecrets::default();
        assert!(slack_proposal(&secrets, "T", "B")
            .unwrap_err()
            .contains("webhook"));
        assert!(linear_proposal(&secrets, LINEAR_ENDPOINT, "T", "B")
            .unwrap_err()
            .contains("API key"));
        secrets.set(SECRET_LINEAR_KEY, "k").unwrap();
        assert!(linear_proposal(&secrets, LINEAR_ENDPOINT, "T", "B")
            .unwrap_err()
            .contains("team"));
    }

    #[test]
    fn long_text_is_cut_at_a_line_and_says_so() {
        let text = (0..100)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let (kept, trimmed) = fit(&text, 120);
        assert!(trimmed);
        assert!(kept.ends_with(TRIM_NOTE));
        assert!(kept.chars().count() <= 120);
        let (same, trimmed) = fit("short", 120);
        assert_eq!((same.as_str(), trimmed), ("short", false));
    }

    #[test]
    fn a_mail_draft_encodes_subject_and_body_and_stays_short() {
        let (url, body, trimmed) = mailto_url(
            Some("lead@example.com"),
            "Standup, Oct 14",
            "- Fixed it & shipped\n- Next",
        )
        .unwrap();
        assert_eq!(
            url,
            "mailto:lead@example.com?subject=Standup%2C%20Oct%2014&body=-%20Fixed%20it%20%26%20shipped%0A-%20Next"
        );
        assert_eq!(body, "- Fixed it & shipped\n- Next");
        assert!(!trimmed);
        let long = (0..400)
            .map(|i| format!("- line number {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let (url, body, trimmed) = mailto_url(None, "S", &long).unwrap();
        assert!(trimmed && body.ends_with(TRIM_NOTE));
        assert!(url.len() < MAILTO_MAX_ENCODED + 100);
        assert!(url.starts_with("mailto:?subject=S&body="));
    }

    #[test]
    fn a_recipient_cannot_smuggle_headers() {
        for bad in ["a@b.com?bcc=x@y.com", "a@b.com&cc=x", "a b@c.com", "nobody"] {
            assert!(mailto_url(Some(bad), "S", "B").is_err(), "{bad}");
        }
    }

    #[test]
    fn a_mail_approval_keeps_the_recipient_it_showed() {
        let proposal =
            mail_proposal(Some("team@example.com"), "Standup", "Reviewed Atlas [1]").unwrap();
        assert!(proposal.action.contains("team@example.com"));
        let approved = approve(proposal);
        assert_eq!(approved.target.as_deref(), Some("team@example.com"));
        let (url, _, _) =
            mailto_url(approved.target.as_deref(), &approved.title, &approved.body).unwrap();
        assert!(url.starts_with("mailto:team@example.com?"));
    }

    #[test]
    fn a_file_is_written_only_as_a_text_export_in_an_existing_folder() {
        let dir = tempfile::tempdir().unwrap();
        let ok = dir.path().join("standup.md");
        write_export(&ok, "# Standup").unwrap();
        assert_eq!(std::fs::read_to_string(&ok).unwrap(), "# Standup");
        assert!(write_export(&dir.path().join("run.sh"), "x").is_err());
        assert!(write_export(&dir.path().join("none").join("a.md"), "x").is_err());
    }

    #[test]
    fn allowed_destinations_match_the_exact_target_and_can_be_removed() {
        let secrets = MemorySecrets::default();
        let key = allow_destination(
            &secrets,
            TOOL_SLACK,
            "hooks.slack.com",
            "https://hooks.slack.com/services/approved",
        )
        .unwrap();
        assert!(is_destination_allowed(
            &secrets,
            TOOL_SLACK,
            "https://hooks.slack.com/services/approved"
        ));
        assert!(!is_destination_allowed(
            &secrets,
            TOOL_SLACK,
            "https://hooks.slack.com/services/changed"
        ));
        assert_eq!(list_allowed_destinations(&secrets)[0].key, key);
        forget_destination(&secrets, &key).unwrap();
        assert!(list_allowed_destinations(&secrets).is_empty());
    }
}
