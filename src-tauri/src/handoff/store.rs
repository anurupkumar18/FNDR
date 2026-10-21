//! What FNDR keeps about each hand-off (PRD FR14): the request, what was
//! attached by id, where the job ran, how it ended and the person's rating.
//! No memory text and no file content; those live in the job folder the
//! person owns. Kept in the state store like peer runs (`agent::peer_runs`).

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use super::draft::OutputGoal;
use crate::operator::runs::Rating;
use crate::storage::StateStore;

const STATE_KEY: &str = "hand_off_jobs_v1";
/// Jobs kept; the oldest finished one goes first.
pub const MAX_JOBS: usize = 50;
static MUTATION: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Starting,
    Working,
    Stopping,
    Checking,
    Ready,
    ReadyWithGaps,
    Stopped,
    Failed,
}

impl JobStatus {
    /// A worker process may still exist for a job in this state.
    pub fn is_running(self) -> bool {
        matches!(
            self,
            Self::Starting | Self::Working | Self::Stopping | Self::Checking
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobRecord {
    pub id: String,
    pub created_at_ms: i64,
    /// The person's words or the suggestion's quote, as the draft showed it.
    pub request: String,
    pub goal: OutputGoal,
    pub memory_ids: Vec<String>,
    pub file_memory_ids: Vec<String>,
    /// The job folder, absolute.
    pub folder: String,
    pub status: JobStatus,
    /// Why a job stopped or failed, in one sentence for the result card.
    pub reason: Option<String>,
    pub finished_at_ms: Option<i64>,
    /// What reached the model: the first message and every command output.
    pub bytes_sent: u64,
    pub checked: usize,
    pub unsupported: usize,
    pub not_checked: usize,
    /// Verified deliverables, relative to the job folder.
    pub deliverables: Vec<String>,
    pub rating: Option<Rating>,
}

impl JobRecord {
    /// A record for a job about to start.
    pub fn starting(
        request: &str,
        goal: OutputGoal,
        memory_ids: Vec<String>,
        file_memory_ids: Vec<String>,
        folder: &str,
        now_ms: i64,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            created_at_ms: now_ms,
            request: request.to_string(),
            goal,
            memory_ids,
            file_memory_ids,
            folder: folder.to_string(),
            status: JobStatus::Starting,
            reason: None,
            finished_at_ms: None,
            bytes_sent: 0,
            checked: 0,
            unsupported: 0,
            not_checked: 0,
            deliverables: Vec::new(),
            rating: None,
        }
    }
}

pub fn list(store: &StateStore) -> Result<Vec<JobRecord>, String> {
    store
        .load_json::<Vec<JobRecord>>(STATE_KEY)
        .map(Option::unwrap_or_default)
}

pub fn get(store: &StateStore, id: &str) -> Result<JobRecord, String> {
    list(store)?
        .into_iter()
        .find(|job| job.id == id)
        .ok_or_else(|| "That hand-off is no longer kept.".to_string())
}

/// Adds a job before its worker starts. At most one job runs at a time
/// (PRD, Memory), so a second start is refused.
pub fn begin(store: &StateStore, job: JobRecord) -> Result<JobRecord, String> {
    let _lock = MUTATION.lock();
    let mut jobs = list(store)?;
    if jobs.iter().any(|prior| prior.status.is_running()) {
        return Err("A hand-off is already working. Stop it or wait for it to finish.".into());
    }
    while jobs.len() >= MAX_JOBS {
        // Nothing is running, so the oldest is finished.
        let oldest = jobs
            .iter()
            .enumerate()
            .min_by_key(|(_, prior)| prior.created_at_ms)
            .map(|(index, _)| index)
            .unwrap_or(0);
        jobs.remove(oldest);
    }
    jobs.push(job.clone());
    store.save_json(STATE_KEY, &jobs)?;
    Ok(job)
}

/// Changes one job and saves it.
pub fn update(
    store: &StateStore,
    id: &str,
    change: impl FnOnce(&mut JobRecord),
) -> Result<JobRecord, String> {
    let _lock = MUTATION.lock();
    let mut jobs = list(store)?;
    let job = jobs
        .iter_mut()
        .find(|job| job.id == id)
        .ok_or("That hand-off is no longer kept.")?;
    change(job);
    let changed = job.clone();
    store.save_json(STATE_KEY, &jobs)?;
    Ok(changed)
}

/// At launch: a job that was running when FNDR quit has no worker any more
/// (PRD FR9). It is marked stopped; its folder keeps what was written.
pub fn sweep_after_restart(store: &StateStore, now_ms: i64) -> Result<usize, String> {
    let _lock = MUTATION.lock();
    let mut jobs = list(store)?;
    let mut swept = 0;
    for job in jobs.iter_mut().filter(|job| job.status.is_running()) {
        job.status = JobStatus::Stopped;
        job.reason = Some("FNDR quit while it was working. What it wrote is kept.".into());
        job.finished_at_ms = Some(now_ms);
        swept += 1;
    }
    if swept > 0 {
        store.save_json(STATE_KEY, &jobs)?;
    }
    Ok(swept)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(at: i64) -> JobRecord {
        JobRecord::starting(
            "Own a one-page Q4 onboarding survey readout",
            OutputGoal::Doc,
            vec!["m_mail".into()],
            vec!["m_csv".into()],
            "/Users/sam/Documents/FNDR Hand-offs/2026-10-16 Q4 readout",
            at,
        )
    }

    fn finished(store: &StateStore, id: &str) {
        update(store, id, |j| j.status = JobStatus::Ready).unwrap();
    }

    #[test]
    fn a_job_survives_a_restart_with_ids_not_content() {
        let dir = tempfile::tempdir().unwrap();
        let store = StateStore::new(dir.path()).unwrap();
        let started = begin(&store, job(1)).unwrap();
        drop(store);
        let store = StateStore::new(dir.path()).unwrap();
        let kept = get(&store, &started.id).unwrap();
        assert_eq!(kept, started);
        let saved = serde_json::to_value(&kept).unwrap();
        assert_eq!(saved["memoryIds"], serde_json::json!(["m_mail"]));
        assert!(saved.get("rendered").is_none() && saved.get("summary").is_none());
    }

    #[test]
    fn only_one_job_runs_at_a_time() {
        let dir = tempfile::tempdir().unwrap();
        let store = StateStore::new(dir.path()).unwrap();
        let first = begin(&store, job(1)).unwrap();
        assert!(begin(&store, job(2)).is_err());
        finished(&store, &first.id);
        assert!(begin(&store, job(3)).is_ok());
    }

    #[test]
    fn the_oldest_job_goes_when_the_list_is_full() {
        let dir = tempfile::tempdir().unwrap();
        let store = StateStore::new(dir.path()).unwrap();
        let mut ids = Vec::new();
        for at in 0..MAX_JOBS as i64 {
            let started = begin(&store, job(at)).unwrap();
            finished(&store, &started.id);
            ids.push(started.id);
        }
        begin(&store, job(1_000)).unwrap();
        let kept = list(&store).unwrap();
        assert_eq!(kept.len(), MAX_JOBS);
        assert!(!kept.iter().any(|j| j.id == ids[0]));
        assert!(kept.iter().any(|j| j.id == ids[1]));
    }

    #[test]
    fn a_running_job_is_stopped_after_a_restart_and_a_finished_one_is_not() {
        let dir = tempfile::tempdir().unwrap();
        let store = StateStore::new(dir.path()).unwrap();
        let done = begin(&store, job(1)).unwrap();
        finished(&store, &done.id);
        let running = begin(&store, job(2)).unwrap();
        update(&store, &running.id, |j| j.status = JobStatus::Working).unwrap();
        assert_eq!(sweep_after_restart(&store, 99).unwrap(), 1);
        let stopped = get(&store, &running.id).unwrap();
        assert_eq!(stopped.status, JobStatus::Stopped);
        assert_eq!(stopped.finished_at_ms, Some(99));
        assert!(stopped.reason.unwrap().contains("What it wrote is kept"));
        assert_eq!(get(&store, &done.id).unwrap().status, JobStatus::Ready);
        assert_eq!(sweep_after_restart(&store, 100).unwrap(), 0);
    }

    #[test]
    fn a_rating_is_kept_with_the_job() {
        let dir = tempfile::tempdir().unwrap();
        let store = StateStore::new(dir.path()).unwrap();
        let started = begin(&store, job(1)).unwrap();
        update(&store, &started.id, |j| j.rating = Some(Rating::Worked)).unwrap();
        assert_eq!(
            get(&store, &started.id).unwrap().rating,
            Some(Rating::Worked)
        );
        assert!(update(&store, "gone", |_| {}).is_err());
    }
}
