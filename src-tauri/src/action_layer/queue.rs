//! The approval queue: what is waiting for the person, what it will do, and
//! what happened to it.
//!
//! A card is built from the structured call (registry row plus arguments),
//! never from model prose. Only the gate can put a call in the queue as
//! pending, only a tap resolves it, and every entry keeps its undo statement.

use super::gate::{decide, Call, Gate, Settings, KILL_SWITCH_MESSAGE};
use super::registry::{spec_for, Surface, Undo};
use parking_lot::Mutex;
use serde::Serialize;
use serde_json::Value;
use std::sync::OnceLock;
use std::time::Duration;
use tokio::sync::oneshot;

pub const APPROVAL_TTL: Duration = Duration::from_secs(120);
const KEPT: usize = 100;
const ARG_CHARS: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Pending,
    Approved,
    Denied,
    Expired,
    /// The gate refused it; it never reached the person.
    Refused,
    /// The gate let it run without a card.
    Ran,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Approved,
    Denied,
    Expired,
}

/// How the person answered. Approval by voice is not accepted (ADR 024, item 8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Via {
    Tap,
    Voice,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueueError {
    NotFound,
    NotPending(Status),
    NeedsATap,
    Expired,
    KillSwitch,
}

impl QueueError {
    pub fn message(&self) -> String {
        match self {
            QueueError::NotFound => "That action is no longer in the queue.".to_string(),
            QueueError::NotPending(status) => format!("That action was already {status:?}."),
            QueueError::NeedsATap => "An action is approved with a tap, not by voice.".to_string(),
            QueueError::Expired => "That request expired. The action was not run.".to_string(),
            QueueError::KillSwitch => KILL_SWITCH_MESSAGE.to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ActionArg {
    pub name: String,
    pub value: String,
}

/// What the card shows. Everything here comes from the registry row and the
/// call's arguments.
#[derive(Debug, Clone, Serialize)]
pub struct ActionCard {
    pub id: String,
    pub caller: &'static str,
    pub surface: Surface,
    pub tool: String,
    pub title: &'static str,
    pub target: Option<String>,
    pub args: Vec<ActionArg>,
    pub reason: String,
    pub undo: Undo,
    pub status: Status,
    pub detail: Option<String>,
    pub created_at_ms: i64,
    pub expires_at_ms: i64,
}

pub enum Submitted {
    /// The gate lets it start without a card.
    Run(String),
    /// It waits in the queue.
    Pending(String),
    Refused(String),
}

struct Entry {
    card: ActionCard,
    answer: Option<oneshot::Sender<Verdict>>,
}

#[derive(Default)]
pub struct ApprovalQueue {
    entries: Mutex<Vec<Entry>>,
}

fn caller_name(call: &Call) -> &'static str {
    match call.caller {
        super::gate::Caller::CommandBar => "command_bar",
        super::gate::Caller::Voice => "voice",
        super::gate::Caller::Mcp => "mcp",
        super::gate::Caller::Agent => "agent",
    }
}

fn shown(value: &Value) -> String {
    let text = match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    };
    if text.chars().count() > ARG_CHARS {
        let head: String = text.chars().take(ARG_CHARS).collect();
        format!("{head}... ({} characters)", text.chars().count())
    } else {
        text
    }
}

fn card_for(
    call: &Call,
    id: String,
    reason: String,
    status: Status,
    now: i64,
    ttl_ms: i64,
) -> ActionCard {
    let spec = spec_for(call.surface, call.tool);
    let mut args: Vec<ActionArg> = call
        .args
        .as_object()
        .map(|map| {
            map.iter()
                .map(|(name, value)| ActionArg {
                    name: name.clone(),
                    value: shown(value),
                })
                .collect()
        })
        .unwrap_or_default();
    args.sort_by(|a, b| a.name.cmp(&b.name));
    let target = spec
        .and_then(|spec| spec.target_key)
        .and_then(|key| call.args.get(key))
        .map(shown);
    ActionCard {
        id,
        caller: caller_name(call),
        surface: call.surface,
        tool: call.tool.to_string(),
        title: spec.map(|spec| spec.title).unwrap_or("Unknown action"),
        target,
        args,
        reason,
        undo: spec.map(|spec| spec.undo).unwrap_or(Undo::Unavailable {
            reason: "Not a tool FNDR offers.",
        }),
        status,
        detail: None,
        created_at_ms: now,
        expires_at_ms: now + ttl_ms,
    }
}

impl ApprovalQueue {
    fn push(&self, entry: Entry) {
        let mut entries = self.entries.lock();
        entries.push(entry);
        while entries.len() > KEPT {
            match entries
                .iter()
                .position(|e| e.card.status != Status::Pending)
            {
                Some(oldest) => {
                    entries.remove(oldest);
                }
                None => break,
            }
        }
    }

    /// Runs the gate. A call that must wait is queued and its answer comes
    /// back on the receiver; a refusal or an unattended run is recorded.
    pub fn submit(
        &self,
        call: &Call,
        settings: &Settings,
        now_ms: i64,
        ttl: Duration,
    ) -> (Submitted, Option<oneshot::Receiver<Verdict>>) {
        let id = uuid::Uuid::new_v4().to_string();
        let ttl_ms = ttl.as_millis() as i64;
        match decide(call, settings) {
            Gate::Run => {
                let card = card_for(
                    call,
                    id.clone(),
                    "Starts by itself.".into(),
                    Status::Ran,
                    now_ms,
                    ttl_ms,
                );
                self.push(Entry { card, answer: None });
                (Submitted::Run(id), None)
            }
            Gate::Refuse(reason) => {
                let card = card_for(call, id, reason.clone(), Status::Refused, now_ms, ttl_ms);
                self.push(Entry { card, answer: None });
                (Submitted::Refused(reason), None)
            }
            Gate::Confirm(reason) => {
                let (sender, receiver) = oneshot::channel();
                let card = card_for(call, id.clone(), reason, Status::Pending, now_ms, ttl_ms);
                self.push(Entry {
                    card,
                    answer: Some(sender),
                });
                (Submitted::Pending(id), Some(receiver))
            }
        }
    }

    /// Submits and waits for the answer. `Approved` is the only result that
    /// lets a pending call run.
    pub async fn request(&self, call: &Call<'_>, settings: &Settings, ttl: Duration) -> Outcome {
        let now = chrono::Utc::now().timestamp_millis();
        match self.submit(call, settings, now, ttl) {
            (Submitted::Run(id), _) => Outcome::Run(id),
            (Submitted::Refused(reason), _) => Outcome::Refused(reason),
            (Submitted::Pending(id), Some(receiver)) => {
                match tokio::time::timeout(ttl, receiver).await {
                    Ok(Ok(Verdict::Approved)) => Outcome::Approved(id),
                    Ok(Ok(Verdict::Denied)) => Outcome::Denied,
                    Ok(Ok(Verdict::Expired)) | Ok(Err(_)) | Err(_) => {
                        self.expire(&id);
                        Outcome::Expired
                    }
                }
            }
            (Submitted::Pending(_), None) => Outcome::Expired,
        }
    }

    fn expire(&self, id: &str) {
        if let Some(entry) = self.entries.lock().iter_mut().find(|e| e.card.id == id) {
            if entry.card.status == Status::Pending {
                entry.card.status = Status::Expired;
                entry.answer = None;
            }
        }
    }

    /// Newest first. A card past its time is expired here, so a stale one
    /// cannot be approved from a screen that was left open.
    pub fn list(&self, now_ms: i64) -> Vec<ActionCard> {
        let mut entries = self.entries.lock();
        for entry in entries.iter_mut() {
            if entry.card.status == Status::Pending && entry.card.expires_at_ms <= now_ms {
                entry.card.status = Status::Expired;
                if let Some(answer) = entry.answer.take() {
                    let _ = answer.send(Verdict::Expired);
                }
            }
        }
        entries.iter().rev().map(|e| e.card.clone()).collect()
    }

    pub fn resolve(
        &self,
        id: &str,
        approve: bool,
        via: Via,
        settings: &Settings,
        now_ms: i64,
    ) -> Result<ActionCard, QueueError> {
        if approve && via == Via::Voice {
            return Err(QueueError::NeedsATap);
        }
        self.list(now_ms);
        let mut entries = self.entries.lock();
        let entry = entries
            .iter_mut()
            .find(|e| e.card.id == id)
            .ok_or(QueueError::NotFound)?;
        match entry.card.status {
            Status::Pending => {}
            Status::Expired => return Err(QueueError::Expired),
            other => return Err(QueueError::NotPending(other)),
        }
        let answer = entry.answer.take();
        if approve && settings.kill_switch {
            entry.card.status = Status::Denied;
            if let Some(answer) = answer {
                let _ = answer.send(Verdict::Denied);
            }
            return Err(QueueError::KillSwitch);
        }
        entry.card.status = if approve {
            Status::Approved
        } else {
            Status::Denied
        };
        if let Some(answer) = answer {
            let _ = answer.send(if approve {
                Verdict::Approved
            } else {
                Verdict::Denied
            });
        }
        Ok(entry.card.clone())
    }

    /// Records how an approved or unattended action ended.
    pub fn finish(&self, id: &str, ok: bool, detail: &str) {
        if let Some(entry) = self.entries.lock().iter_mut().find(|e| e.card.id == id) {
            if matches!(entry.card.status, Status::Approved | Status::Ran) {
                entry.card.status = if ok { Status::Ran } else { Status::Failed };
                entry.card.detail = Some(detail.to_string());
            }
        }
    }

    /// The tool that reverses an entry, if the registry names one. Undoing
    /// is itself an action: the caller submits it through the gate.
    pub fn inverse_of(&self, id: &str) -> Option<&'static str> {
        let entries = self.entries.lock();
        match entries.iter().find(|e| e.card.id == id)?.card.undo {
            Undo::Available { inverse, .. } => inverse,
            Undo::Unavailable { .. } => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Run(String),
    Approved(String),
    Denied,
    Expired,
    Refused(String),
}

impl Outcome {
    /// Whether the call may start.
    pub fn proceeds(&self) -> bool {
        matches!(self, Outcome::Run(_) | Outcome::Approved(_))
    }
}

pub fn global() -> &'static ApprovalQueue {
    static QUEUE: OnceLock<ApprovalQueue> = OnceLock::new();
    QUEUE.get_or_init(ApprovalQueue::default)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action_layer::gate::Caller;
    use serde_json::json;

    fn settings() -> Settings {
        Settings {
            kill_switch: false,
            agent_notes_enabled: true,
            confirm_unnamed_apps: false,
            autonomy: crate::operator::policy::Autonomy::AskEach,
        }
    }

    fn paste_args() -> Value {
        json!({"text": "hello there", "app": "Notes"})
    }

    fn pending_paste(queue: &ApprovalQueue) -> (String, oneshot::Receiver<Verdict>) {
        let args = paste_args();
        let call = Call::new(Caller::CommandBar, Surface::NotchPlan, "paste", &args)
            .asked("paste hello there");
        match queue.submit(&call, &settings(), 1_000, Duration::from_secs(60)) {
            (Submitted::Pending(id), Some(rx)) => (id, rx),
            _ => panic!("a paste must wait for a tap"),
        }
    }

    #[test]
    fn the_card_shows_the_tool_target_and_every_argument_from_the_call() {
        let queue = ApprovalQueue::default();
        let (id, _rx) = pending_paste(&queue);
        let card = queue.list(2_000).into_iter().find(|c| c.id == id).unwrap();
        assert_eq!(card.tool, "paste");
        assert_eq!(card.title, "Paste text");
        assert_eq!(card.target.as_deref(), Some("Notes"));
        let names: Vec<_> = card
            .args
            .iter()
            .map(|a| (a.name.as_str(), a.value.as_str()))
            .collect();
        assert_eq!(names, [("app", "Notes"), ("text", "hello there")]);
        assert!(
            matches!(card.undo, Undo::Unavailable { .. }),
            "a stated reason, not silence"
        );
    }

    #[test]
    fn changing_an_argument_changes_the_card() {
        let queue = ApprovalQueue::default();
        let one = json!({"text": "one"});
        let two = json!({"text": "two"});
        for args in [&one, &two] {
            let call = Call::new(Caller::Mcp, Surface::NotchPlan, "paste", args);
            queue.submit(&call, &settings(), 1, Duration::from_secs(60));
        }
        let shown: Vec<_> = queue
            .list(2)
            .iter()
            .map(|c| c.args[0].value.clone())
            .collect();
        assert_eq!(shown, ["two", "one"]);
    }

    #[tokio::test]
    async fn approving_by_tap_releases_the_waiting_call_and_voice_cannot() {
        let queue = ApprovalQueue::default();
        let (id, rx) = pending_paste(&queue);
        assert_eq!(
            queue
                .resolve(&id, true, Via::Voice, &settings(), 2_000)
                .unwrap_err(),
            QueueError::NeedsATap
        );
        assert_eq!(queue.list(2_000)[0].status, Status::Pending);
        queue
            .resolve(&id, true, Via::Tap, &settings(), 2_000)
            .unwrap();
        assert_eq!(rx.await.unwrap(), Verdict::Approved);
        assert_eq!(
            queue
                .resolve(&id, true, Via::Tap, &settings(), 2_001)
                .unwrap_err(),
            QueueError::NotPending(Status::Approved)
        );
    }

    #[test]
    fn voice_may_decline() {
        let queue = ApprovalQueue::default();
        let (id, _rx) = pending_paste(&queue);
        let card = queue
            .resolve(&id, false, Via::Voice, &settings(), 2_000)
            .unwrap();
        assert_eq!(card.status, Status::Denied);
    }

    #[test]
    fn a_stale_card_cannot_be_approved() {
        let queue = ApprovalQueue::default();
        let (id, _rx) = pending_paste(&queue);
        assert_eq!(
            queue
                .resolve(&id, true, Via::Tap, &settings(), 1_000 + 61_000)
                .unwrap_err(),
            QueueError::Expired
        );
    }

    #[tokio::test]
    async fn approval_after_the_kill_switch_turns_on_is_refused() {
        let queue = ApprovalQueue::default();
        let (id, rx) = pending_paste(&queue);
        let off = Settings {
            kill_switch: true,
            ..settings()
        };
        assert_eq!(
            queue.resolve(&id, true, Via::Tap, &off, 2_000).unwrap_err(),
            QueueError::KillSwitch
        );
        assert_eq!(rx.await.unwrap(), Verdict::Denied);
    }

    #[test]
    fn a_refused_call_is_recorded_and_never_pending() {
        let queue = ApprovalQueue::default();
        let args = json!({});
        let call = Call::new(Caller::Mcp, Surface::Mcp, "run_shortcut", &args);
        let (submitted, rx) = queue.submit(&call, &settings(), 1, Duration::from_secs(60));
        assert!(matches!(submitted, Submitted::Refused(_)));
        assert!(rx.is_none());
        let card = &queue.list(2)[0];
        assert_eq!(card.status, Status::Refused);
    }

    #[tokio::test]
    async fn a_request_that_is_not_answered_in_time_does_not_proceed() {
        let queue = ApprovalQueue::default();
        let args = paste_args();
        let call = Call::new(Caller::Mcp, Surface::NotchPlan, "paste", &args);
        let outcome = queue
            .request(&call, &settings(), Duration::from_millis(5))
            .await;
        assert_eq!(outcome, Outcome::Expired);
        assert!(!outcome.proceeds());
        assert_eq!(
            queue.list(chrono::Utc::now().timestamp_millis())[0].status,
            Status::Expired
        );
    }

    #[test]
    fn an_undoable_action_names_the_inverse_to_submit_through_the_gate() {
        let queue = ApprovalQueue::default();
        let args = json!({});
        let call = Call::new(Caller::Mcp, Surface::Mcp, "start_meeting", &args);
        let (Submitted::Pending(id), _) =
            queue.submit(&call, &settings(), 1, Duration::from_secs(60))
        else {
            panic!("start_meeting waits for a tap");
        };
        assert_eq!(queue.inverse_of(&id), Some("stop_meeting"));
    }

    #[test]
    fn a_long_argument_is_cut_with_its_length_stated() {
        let queue = ApprovalQueue::default();
        let args = json!({"text": "x".repeat(2_000)});
        let call = Call::new(Caller::Mcp, Surface::NotchPlan, "paste", &args);
        queue.submit(&call, &settings(), 1, Duration::from_secs(60));
        let value = queue.list(2)[0].args[0].value.clone();
        assert!(value.ends_with("(2000 characters)"), "{value}");
    }
}
