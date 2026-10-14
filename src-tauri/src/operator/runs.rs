//! What Notch Do keeps about each run so it can learn from it: the request,
//! how it was planned, the steps, how it ended and the person's rating. A
//! run rated as not working is a draft case for the evaluation suite
//! (`scripts/operator/notch-do-cases.json`); a run that worked can be saved
//! as a skill (`operator::skills`). Screen text is never kept here; the
//! journal (`operator/journal.jsonl`) holds the actions, with typed text
//! hashed.

use crate::operator::plan::{PlanStep, StepAction};
use crate::storage::StateStore;
use serde::{Deserialize, Serialize};

const RUNS_KEY: &str = "notch_do_runs";
/// The newest runs kept; older ones fall off.
const MAX_RUNS: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Rating {
    Worked,
    DidNotWork,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunRecord {
    pub run_id: String,
    pub at: String,
    pub request: String,
    /// How it was planned: `local`, `planner` or `skill`.
    pub planned_by: String,
    pub steps: Vec<PlanStep>,
    /// None while the run has not ended.
    pub ok: Option<bool>,
    pub summary: String,
    pub rating: Option<Rating>,
}

fn load(store: &StateStore) -> Result<Vec<RunRecord>, String> {
    Ok(store
        .load_json::<Vec<RunRecord>>(RUNS_KEY)?
        .unwrap_or_default())
}

fn save(store: &StateStore, runs: &[RunRecord]) -> Result<(), String> {
    store.save_json(RUNS_KEY, &runs)
}

/// Keeps a planned run, newest first.
pub fn record_plan(
    store: &StateStore,
    run_id: &str,
    request: &str,
    planned_by: &str,
    steps: &[PlanStep],
) -> Result<(), String> {
    let mut runs = load(store)?;
    runs.retain(|run| run.run_id != run_id);
    runs.insert(
        0,
        RunRecord {
            run_id: run_id.to_string(),
            at: chrono::Utc::now().to_rfc3339(),
            request: request.to_string(),
            planned_by: planned_by.to_string(),
            steps: steps.to_vec(),
            ok: None,
            summary: String::new(),
            rating: None,
        },
    );
    runs.truncate(MAX_RUNS);
    save(store, &runs)
}

/// Keeps how a run ended. A run that is no longer kept is left alone.
pub fn record_outcome(
    store: &StateStore,
    run_id: &str,
    ok: bool,
    summary: &str,
) -> Result<(), String> {
    let mut runs = load(store)?;
    let Some(run) = runs.iter_mut().find(|run| run.run_id == run_id) else {
        return Ok(());
    };
    run.ok = Some(ok);
    run.summary = summary.to_string();
    save(store, &runs)
}

/// The person's rating of a run that ended.
pub fn rate(store: &StateStore, run_id: &str, rating: Rating) -> Result<RunRecord, String> {
    let mut runs = load(store)?;
    let run = runs
        .iter_mut()
        .find(|run| run.run_id == run_id)
        .ok_or_else(|| "That run is no longer kept.".to_string())?;
    if run.ok.is_none() {
        return Err("A run can be rated once it has ended.".to_string());
    }
    run.rating = Some(rating);
    let rated = run.clone();
    save(store, &runs)?;
    Ok(rated)
}

pub fn get(store: &StateStore, run_id: &str) -> Result<Option<RunRecord>, String> {
    Ok(load(store)?.into_iter().find(|run| run.run_id == run_id))
}

fn action_name(action: StepAction) -> &'static str {
    match action {
        StepAction::OpenApp => "open_app",
        StepAction::OpenUrl => "open_url",
        StepAction::Operate => "operate",
        StepAction::ReopenMemory => "reopen_memory",
    }
}

/// Every run rated as not working, as a case for the suite to be labeled:
/// the request and what FNDR planned, with `route` and `plan` left for the
/// person to fill in with what should have happened. Newest first.
pub fn case_drafts(store: &StateStore) -> Result<Vec<serde_json::Value>, String> {
    Ok(load(store)?
        .into_iter()
        .filter(|run| run.rating == Some(Rating::DidNotWork))
        .map(|run| {
            serde_json::json!({
                "id": format!("from-run-{}", run.run_id.chars().take(8).collect::<String>()),
                "request": run.request,
                "route": "TODO: what it should have been",
                "observed": {
                    "planned_by": run.planned_by,
                    "steps": run.steps.iter().map(|step| serde_json::json!({
                        "action": action_name(step.action),
                        "app": step.app,
                        "url": step.url,
                    })).collect::<Vec<_>>(),
                    "summary": run.summary,
                },
            })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operator::plan::StepCheck;

    fn step(app: &str) -> PlanStep {
        PlanStep {
            action: StepAction::OpenApp,
            label: format!("Open {app}"),
            app: app.into(),
            url: String::new(),
            goal: String::new(),
            check: StepCheck::Frontmost,
            item: None,
        }
    }

    #[test]
    fn a_run_is_kept_rated_and_a_wrong_one_becomes_a_case_draft() {
        let dir = tempfile::tempdir().unwrap();
        let store = StateStore::new(dir.path()).unwrap();
        record_plan(&store, "r1", "open Notes", "local", &[step("Notes")]).unwrap();
        assert!(rate(&store, "r1", Rating::Worked).is_err(), "not ended yet");
        record_outcome(&store, "r1", true, "Done: Open Notes.").unwrap();
        assert_eq!(
            rate(&store, "r1", Rating::Worked).unwrap().rating,
            Some(Rating::Worked)
        );
        assert!(case_drafts(&store).unwrap().is_empty());

        record_plan(
            &store,
            "r2",
            "open the Messages app",
            "local",
            &[step("Messages")],
        )
        .unwrap();
        record_outcome(&store, "r2", false, "Stopped").unwrap();
        rate(&store, "r2", Rating::DidNotWork).unwrap();
        let drafts = case_drafts(&store).unwrap();
        assert_eq!(drafts.len(), 1);
        assert_eq!(drafts[0]["request"], "open the Messages app");
        assert_eq!(drafts[0]["observed"]["steps"][0]["app"], "Messages");
        assert_eq!(get(&store, "r1").unwrap().unwrap().ok, Some(true));
    }

    #[test]
    fn only_the_newest_runs_are_kept() {
        let dir = tempfile::tempdir().unwrap();
        let store = StateStore::new(dir.path()).unwrap();
        for index in 0..MAX_RUNS + 5 {
            record_plan(&store, &format!("r{index}"), "open Notes", "local", &[]).unwrap();
        }
        assert!(get(&store, "r0").unwrap().is_none());
        assert!(get(&store, &format!("r{}", MAX_RUNS + 4))
            .unwrap()
            .is_some());
    }
}
