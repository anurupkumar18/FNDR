//! The tools that move an output out of FNDR, and the approval each needs.
//!
//! These are registered here, not in `agent::risk_policy` or the MCP
//! dispatcher, so the daily outputs add no line to files other lanes are
//! editing. The decision is still the agent module's: `policy_for_action`
//! says a send needs approval, and `actions_kill_switch` refuses it first.
//!
//! The only way to get an `ApprovedSend`, which every send function takes, is
//! `SendApprovals::resolve(id, true, ..)` on a card the person was shown and
//! has not let expire. Nothing in the background holds one.

use crate::agent::actions::{policy_for_action, AgentActionKind};
use crate::agent::policy::{AgentMode, RiskLevel};
use parking_lot::Mutex;
use serde::Serialize;
use std::collections::HashMap;

pub const TOOL_SLACK: &str = "outputs.post_slack";
pub const TOOL_LINEAR: &str = "outputs.create_linear_issue";
pub const TOOL_MAIL: &str = "outputs.open_mail_draft";

/// How long a card waits for the person, the same as an MCP approval.
pub const APPROVAL_TTL_MS: i64 = 120_000;

pub fn tools() -> [&'static str; 3] {
    [TOOL_SLACK, TOOL_LINEAR, TOOL_MAIL]
}

/// The agent policy kind for each tool: a mail draft opens a URL in the
/// person's mail app; Slack and Linear send text from this Mac.
fn kind_of(tool: &str) -> Option<(AgentActionKind, RiskLevel)> {
    match tool {
        TOOL_SLACK | TOOL_LINEAR => Some((AgentActionKind::SendExternalMessage, RiskLevel::Medium)),
        TOOL_MAIL => Some((AgentActionKind::OpenUrl, RiskLevel::Low)),
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    UnknownTool,
    KillSwitch,
    NotAllowed(String),
    /// No card with this id: never shown, already answered, or swept.
    NoSuchApproval,
    Expired,
    Declined,
}

impl Refusal {
    pub fn message(&self) -> String {
        match self {
            Refusal::UnknownTool => "That send is not a known output action.".to_string(),
            Refusal::KillSwitch => crate::agent::risk_policy::RefuseReason::KillSwitch
                .message()
                .to_string(),
            Refusal::NotAllowed(reason) => reason.clone(),
            Refusal::NoSuchApproval => {
                "There is no pending approval for this send. Nothing was sent.".to_string()
            }
            Refusal::Expired => {
                "The approval expired before it was answered. Nothing was sent.".to_string()
            }
            Refusal::Declined => "Declined. Nothing was sent.".to_string(),
        }
    }
}

/// Whether `tool` may ask for approval at all. A tool in this registry always
/// needs approval; the test pins that.
pub fn gate(tool: &str, kill_switch: bool) -> Result<(), Refusal> {
    let (kind, risk) = kind_of(tool).ok_or(Refusal::UnknownTool)?;
    if kill_switch {
        return Err(Refusal::KillSwitch);
    }
    let decision = policy_for_action(&kind, &risk, &AgentMode::Act);
    if !decision.allowed {
        return Err(Refusal::NotAllowed(
            decision.blocked_because.unwrap_or(decision.reason),
        ));
    }
    Ok(())
}

/// What the person sees before anything leaves: the exact body, where it goes
/// and what the send will log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ApprovalCard {
    pub approval_id: String,
    pub tool: String,
    /// "Slack webhook", "Linear issue", "Mail draft".
    pub action: String,
    /// The host the request goes to, or "your mail app" for a draft.
    pub destination: String,
    pub title: String,
    pub body: String,
    pub bytes: usize,
    pub expires_at_ms: i64,
    /// True when the body was cut to fit the destination's limit.
    pub trimmed: bool,
}

#[derive(Debug, Clone)]
struct Pending {
    card: ApprovalCard,
    /// The exact URL or mail recipient the request must use.
    target: Option<String>,
}

/// A send the person approved. Constructed only by `SendApprovals::resolve`.
#[derive(Debug)]
pub struct ApprovedSend {
    pub tool: String,
    pub title: String,
    pub body: String,
    pub target: Option<String>,
}

#[derive(Default)]
pub struct SendApprovals {
    pending: Mutex<HashMap<String, Pending>>,
}

#[derive(Debug)]
pub struct Proposal {
    pub tool: &'static str,
    pub action: String,
    pub destination: String,
    pub target: Option<String>,
    pub title: String,
    pub body: String,
    pub trimmed: bool,
}

impl SendApprovals {
    /// Shows a card for a send. Nothing is sent.
    pub fn propose(
        &self,
        proposal: Proposal,
        kill_switch: bool,
        now_ms: i64,
    ) -> Result<ApprovalCard, Refusal> {
        gate(proposal.tool, kill_switch)?;
        let card = ApprovalCard {
            approval_id: uuid::Uuid::new_v4().to_string(),
            tool: proposal.tool.to_string(),
            action: proposal.action,
            destination: proposal.destination,
            bytes: proposal.body.len() + proposal.title.len(),
            title: proposal.title,
            body: proposal.body,
            expires_at_ms: now_ms + APPROVAL_TTL_MS,
            trimmed: proposal.trimmed,
        };
        let mut pending = self.pending.lock();
        pending.retain(|_, item| item.card.expires_at_ms > now_ms);
        pending.insert(
            card.approval_id.clone(),
            Pending {
                card: card.clone(),
                target: proposal.target,
            },
        );
        Ok(card)
    }

    /// The person's answer. A card is answered once: approving or declining
    /// removes it, so a replay finds nothing.
    pub fn resolve(
        &self,
        approval_id: &str,
        approved: bool,
        kill_switch: bool,
        now_ms: i64,
    ) -> Result<ApprovedSend, Refusal> {
        let item = self
            .pending
            .lock()
            .remove(approval_id)
            .ok_or(Refusal::NoSuchApproval)?;
        if item.card.expires_at_ms <= now_ms {
            return Err(Refusal::Expired);
        }
        if !approved {
            return Err(Refusal::Declined);
        }
        gate(&item.card.tool, kill_switch)?;
        Ok(ApprovedSend {
            tool: item.card.tool,
            title: item.card.title,
            body: item.card.body,
            target: item.target,
        })
    }

    pub fn pending_count(&self) -> usize {
        self.pending.lock().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proposal(tool: &'static str) -> Proposal {
        Proposal {
            tool,
            action: "Slack webhook".to_string(),
            destination: "hooks.slack.com".to_string(),
            target: Some("https://hooks.slack.com/services/T/B/X".to_string()),
            title: "Standup".to_string(),
            body: "- Fixed the export job".to_string(),
            trimmed: false,
        }
    }

    #[test]
    fn every_registered_tool_is_allowed_and_needs_approval_under_the_agent_policy() {
        for tool in tools() {
            let (kind, risk) = kind_of(tool).expect("registered");
            let decision = policy_for_action(&kind, &risk, &AgentMode::Act);
            assert!(decision.allowed, "{tool}");
            assert!(decision.requires_approval, "{tool} must ask first");
            assert_eq!(gate(tool, false), Ok(()));
        }
        assert_eq!(gate("outputs.rm_rf", false), Err(Refusal::UnknownTool));
    }

    #[test]
    fn the_kill_switch_refuses_before_a_card_is_shown() {
        let approvals = SendApprovals::default();
        let refused = approvals.propose(proposal(TOOL_SLACK), true, 0);
        assert_eq!(refused, Err(Refusal::KillSwitch));
        assert_eq!(approvals.pending_count(), 0);
    }

    #[test]
    fn a_send_without_a_card_is_refused() {
        let approvals = SendApprovals::default();
        assert_eq!(
            approvals.resolve("never-shown", true, false, 0).err(),
            Some(Refusal::NoSuchApproval)
        );
    }

    #[test]
    fn approving_yields_the_exact_card_content_once() {
        let approvals = SendApprovals::default();
        let card = approvals
            .propose(proposal(TOOL_SLACK), false, 1_000)
            .unwrap();
        assert_eq!(card.expires_at_ms, 1_000 + APPROVAL_TTL_MS);
        let approved = approvals
            .resolve(&card.approval_id, true, false, 2_000)
            .unwrap();
        assert_eq!(approved.tool, TOOL_SLACK);
        assert_eq!(approved.body, "- Fixed the export job");
        assert_eq!(
            approved.target.as_deref(),
            Some("https://hooks.slack.com/services/T/B/X")
        );
        assert_eq!(
            approvals
                .resolve(&card.approval_id, true, false, 2_001)
                .err(),
            Some(Refusal::NoSuchApproval),
            "a replayed approval finds nothing"
        );
    }

    #[test]
    fn declining_and_expiring_send_nothing() {
        let approvals = SendApprovals::default();
        let card = approvals.propose(proposal(TOOL_LINEAR), false, 0).unwrap();
        assert_eq!(
            approvals.resolve(&card.approval_id, false, false, 1).err(),
            Some(Refusal::Declined)
        );
        let card = approvals.propose(proposal(TOOL_LINEAR), false, 0).unwrap();
        assert_eq!(
            approvals
                .resolve(&card.approval_id, true, false, APPROVAL_TTL_MS)
                .err(),
            Some(Refusal::Expired)
        );
        assert_eq!(approvals.pending_count(), 0);
    }

    #[test]
    fn turning_actions_off_while_a_card_waits_refuses_the_approval() {
        let approvals = SendApprovals::default();
        let card = approvals.propose(proposal(TOOL_MAIL), false, 0).unwrap();
        assert_eq!(
            approvals.resolve(&card.approval_id, true, true, 5).err(),
            Some(Refusal::KillSwitch)
        );
    }

    #[test]
    fn stale_cards_are_swept_when_a_new_one_is_shown() {
        let approvals = SendApprovals::default();
        approvals.propose(proposal(TOOL_SLACK), false, 0).unwrap();
        approvals
            .propose(proposal(TOOL_SLACK), false, APPROVAL_TTL_MS + 1)
            .unwrap();
        assert_eq!(approvals.pending_count(), 1);
    }
}
