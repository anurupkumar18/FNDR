//! Short lived output documents and approval cards kept inside FNDR.

use super::{
    tools::{ApprovedSend, SendApprovals},
    OutputDoc,
};
use parking_lot::Mutex;
use std::collections::{HashMap, VecDeque};
use std::sync::OnceLock;

const MAX_DOCS: usize = 8;

static APPROVALS: OnceLock<SendApprovals> = OnceLock::new();
static DOCS: OnceLock<Mutex<VecDeque<OutputDoc>>> = OnceLock::new();
static SUCCESSFUL_SENDS: OnceLock<Mutex<HashMap<String, SuccessfulSend>>> = OnceLock::new();
const TRUST_WINDOW_MS: i64 = 600_000;

pub struct SuccessfulSend {
    pub tool: String,
    pub destination: String,
    pub target: String,
    expires_at_ms: i64,
}

fn successful_sends() -> &'static Mutex<HashMap<String, SuccessfulSend>> {
    SUCCESSFUL_SENDS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// A destination can be added to the allow list only after this process
/// finished an approved send to its exact target.
pub fn remember_successful_send(
    send: &ApprovedSend,
    destination: &str,
    now_ms: i64,
) -> Option<String> {
    let target = send.target.as_ref()?.clone();
    let id = uuid::Uuid::new_v4().to_string();
    let mut sends = successful_sends().lock();
    sends.retain(|_, item| item.expires_at_ms > now_ms);
    sends.insert(
        id.clone(),
        SuccessfulSend {
            tool: send.tool.clone(),
            destination: destination.to_string(),
            target,
            expires_at_ms: now_ms + TRUST_WINDOW_MS,
        },
    );
    Some(id)
}

pub fn take_successful_send(id: &str, now_ms: i64) -> Option<SuccessfulSend> {
    let item = successful_sends().lock().remove(id)?;
    (item.expires_at_ms > now_ms).then_some(item)
}

pub fn approvals() -> &'static SendApprovals {
    APPROVALS.get_or_init(SendApprovals::default)
}

fn docs() -> &'static Mutex<VecDeque<OutputDoc>> {
    DOCS.get_or_init(|| Mutex::new(VecDeque::new()))
}

pub fn remember_doc(doc: OutputDoc) {
    let mut docs = docs().lock();
    docs.retain(|old| old.id != doc.id);
    docs.push_back(doc);
    while docs.len() > MAX_DOCS {
        docs.pop_front();
    }
}

pub fn doc_by_id(id: &str) -> Option<OutputDoc> {
    docs().lock().iter().find(|doc| doc.id == id).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outputs::OutputKind;

    fn doc(id: &str) -> OutputDoc {
        OutputDoc {
            id: id.to_string(),
            kind: OutputKind::Standup,
            title: id.to_string(),
            start_ms: 0,
            end_ms: 1,
            generated_at_ms: 0,
            model_polished: false,
            sections: vec![],
        }
    }

    #[test]
    fn keeps_only_the_last_eight_documents_and_replaces_an_id() {
        let prefix = uuid::Uuid::new_v4().to_string();
        for index in 0..9 {
            remember_doc(doc(&format!("{prefix}-{index}")));
        }
        assert!(doc_by_id(&format!("{prefix}-0")).is_none());
        assert_eq!(
            doc_by_id(&format!("{prefix}-8")).unwrap().title,
            format!("{prefix}-8")
        );
        let mut updated = doc(&format!("{prefix}-8"));
        updated.title = "Updated".to_string();
        remember_doc(updated);
        assert_eq!(doc_by_id(&format!("{prefix}-8")).unwrap().title, "Updated");
    }

    #[test]
    fn a_success_token_is_one_use_and_expires() {
        let send = ApprovedSend {
            tool: super::super::tools::TOOL_SLACK.to_string(),
            title: "Standup".to_string(),
            body: "Reviewed Atlas".to_string(),
            target: Some("https://hooks.slack.com/services/T/B/X".to_string()),
        };
        let id = remember_successful_send(&send, "hooks.slack.com", 100).unwrap();
        let item = take_successful_send(&id, 101).unwrap();
        assert_eq!(item.destination, "hooks.slack.com");
        assert!(take_successful_send(&id, 102).is_none());
        let id = remember_successful_send(&send, "hooks.slack.com", 100).unwrap();
        assert!(take_successful_send(&id, 100 + TRUST_WINDOW_MS).is_none());
    }
}
