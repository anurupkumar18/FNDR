//! Content-free local record of reviewed peer egress.

use crate::storage::StateStore;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

const STATE_KEY: &str = "peer_runs_v1";
const MAX_RUNS: usize = 100;
static MUTATION: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerRun {
    pub local_id: String,
    pub message_id: String,
    pub peer_id: String,
    pub host: String,
    pub attachment_ids: Vec<String>,
    pub payload_bytes: usize,
    pub created_at_ms: i64,
    /// A process crash or lost reply after this record is written may mean the
    /// peer accepted the task. FNDR never retries such a run automatically.
    pub status: String,
    pub remote_task_id: Option<String>,
    pub remote_state: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PeerRunView {
    pub local_id: String,
    pub peer_id: String,
    pub host: String,
    pub created_at_ms: i64,
    pub payload_bytes: usize,
    pub status: String,
    pub remote_task_id: Option<String>,
    pub remote_state: Option<String>,
}

impl From<PeerRun> for PeerRunView {
    fn from(run: PeerRun) -> Self {
        Self {
            local_id: run.local_id,
            peer_id: run.peer_id,
            host: run.host,
            created_at_ms: run.created_at_ms,
            payload_bytes: run.payload_bytes,
            status: run.status,
            remote_task_id: run.remote_task_id,
            remote_state: run.remote_state,
        }
    }
}

pub fn list_runs(store: &StateStore) -> Result<Vec<PeerRun>, String> {
    store
        .load_json::<Vec<PeerRun>>(STATE_KEY)
        .map(Option::unwrap_or_default)
}

pub fn begin_send(
    store: &StateStore,
    message_id: &str,
    peer_id: &str,
    host: &str,
    attachment_ids: &[String],
    payload_bytes: usize,
    now_ms: i64,
) -> Result<PeerRun, String> {
    let _lock = MUTATION.lock();
    let mut runs = list_runs(store)?;
    let run = PeerRun {
        local_id: uuid::Uuid::new_v4().to_string(),
        message_id: message_id.to_string(),
        peer_id: peer_id.to_string(),
        host: host.to_string(),
        attachment_ids: attachment_ids.to_vec(),
        payload_bytes,
        created_at_ms: now_ms,
        status: "uncertain".into(),
        remote_task_id: None,
        remote_state: None,
    };
    if runs.len() >= MAX_RUNS {
        let finished = runs.iter().position(|prior| {
            prior.status == "direct_reply"
                || matches!(
                    prior.remote_state.as_deref(),
                    Some(
                        "TASK_STATE_COMPLETED"
                            | "TASK_STATE_FAILED"
                            | "TASK_STATE_CANCELED"
                            | "TASK_STATE_REJECTED"
                    )
                )
        });
        match finished {
            Some(index) => {
                runs.remove(index);
            }
            None => return Err("Peer task history is full of unfinished sends".into()),
        }
    }
    runs.push(run.clone());
    store.save_json(STATE_KEY, &runs)?;
    Ok(run)
}

pub fn finish_send(
    store: &StateStore,
    local_id: &str,
    remote_task_id: &str,
    remote_state: &str,
) -> Result<PeerRun, String> {
    let _lock = MUTATION.lock();
    let mut runs = list_runs(store)?;
    let run = runs
        .iter_mut()
        .find(|run| run.local_id == local_id)
        .ok_or("Peer run was not found")?;
    run.status = if remote_task_id.is_empty() {
        "direct_reply"
    } else {
        "acknowledged"
    }
    .into();
    run.remote_task_id = (!remote_task_id.is_empty()).then(|| remote_task_id.to_string());
    run.remote_state = Some(remote_state.to_string());
    let result = run.clone();
    store.save_json(STATE_KEY, &runs)?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_send_survives_restart_without_the_task_text() {
        let dir = tempfile::tempdir().unwrap();
        let store = crate::storage::StateStore::new(dir.path()).unwrap();
        let run = begin_send(
            &store,
            "msg-123",
            "peer-1",
            "peer.example",
            &["memory-1".into()],
            315,
            123,
        )
        .unwrap();
        assert_eq!(run.status, "uncertain");
        let serialized = serde_json::to_value(&run).unwrap();
        assert!(serialized.get("message_text").is_none());
        assert!(serialized.get("output_text").is_none());
        drop(store);
        let reopened = crate::storage::StateStore::new(dir.path()).unwrap();
        let rows = list_runs(&reopened).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].local_id, run.local_id);
        assert_eq!(rows[0].attachment_ids, vec!["memory-1"]);
        let view: PeerRunView = rows[0].clone().into();
        assert_eq!(view.payload_bytes, 315);
        assert!(serde_json::to_value(&view)
            .unwrap()
            .get("attachment_ids")
            .is_none());
        let serialized = serde_json::to_value(&rows[0]).unwrap();
        assert!(serialized.get("message_text").is_none());
        assert!(serialized.get("output_text").is_none());
        finish_send(
            &reopened,
            &run.local_id,
            "remote-task",
            "TASK_STATE_WORKING",
        )
        .unwrap();
        let rows = list_runs(&reopened).unwrap();
        assert_eq!(rows[0].remote_task_id.as_deref(), Some("remote-task"));
        assert_eq!(rows[0].status, "acknowledged");
    }

    #[test]
    fn full_ledger_never_discards_an_unfinished_task() {
        let dir = tempfile::tempdir().unwrap();
        let store = crate::storage::StateStore::new(dir.path()).unwrap();
        let mut ids = Vec::new();
        for index in 0..MAX_RUNS {
            let run = begin_send(
                &store,
                &format!("message-{index}"),
                "peer-1",
                "peer.example",
                &[],
                100,
                index as i64,
            )
            .unwrap();
            ids.push(run.local_id);
        }
        finish_send(&store, &ids[0], "remote-1", "TASK_STATE_WORKING").unwrap();

        let result = begin_send(
            &store,
            "message-overflow",
            "peer-1",
            "peer.example",
            &[],
            100,
            101,
        );
        assert!(
            result.is_err(),
            "a full ledger of unfinished tasks must block a new send"
        );
        let rows = list_runs(&store).unwrap();
        assert_eq!(rows.len(), MAX_RUNS);
        assert!(rows.iter().any(|run| run.local_id == ids[0]));
        assert!(rows.iter().any(|run| run.local_id == ids[1]));
        finish_send(&store, &ids[1], "remote-2", "TASK_STATE_COMPLETED").unwrap();

        let new = begin_send(
            &store,
            "message-next",
            "peer-1",
            "peer.example",
            &[],
            100,
            101,
        )
        .unwrap();
        let rows = list_runs(&store).unwrap();
        assert_eq!(rows.len(), MAX_RUNS);
        assert!(rows.iter().any(|run| run.local_id == ids[0]));
        assert!(!rows.iter().any(|run| run.local_id == ids[1]));
        assert!(rows.iter().any(|run| run.local_id == new.local_id));
    }
}
