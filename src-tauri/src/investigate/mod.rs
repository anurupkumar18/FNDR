//! Investigate (ADR 039): a bounded agent loop over the person's own
//! memories. A request about a piece of work ("catch me up on the Smart
//! Reminders launch") becomes a case file: what happened, what was decided,
//! what is still open, what a later memory records as done, what went wrong,
//! and the places to reopen, each line citing its memory.
//!
//! - `plan` reads the request: topic words, a time filter, lines of inquiry.
//! - `agent` is the loop. `next_step` picks one move from the case so far.
//! - `tools` are the moves, over paths every surface already uses.
//! - `done` decides whether a later clause records a next step as done.
//! - `case` composes the file from stored fields. No model is read.
//!
//! A run only reads. Opening places and adding to-dos happen on a tap, in
//! the panel, through `open_work_set` and `add_todo`.

pub mod agent;
pub mod case;
pub mod done;
pub mod plan;
pub mod tools;

pub use agent::{StepEvent, StepKind, StepStatus};
pub use case::{CaseFile, Outcome};

use crate::AppState;
use std::sync::atomic::{AtomicBool, Ordering};

/// Runs one investigation on this Mac. `on_step` hears each step as it
/// starts and ends, with counts only; `stop` ends the run before its next
/// step. Never an error: a run that cannot start says why in its outcome.
pub async fn run(
    state: &AppState,
    goal: &str,
    stop: &AtomicBool,
    on_step: impl FnMut(&StepEvent),
) -> CaseFile {
    if state.is_incognito.load(Ordering::SeqCst) {
        return CaseFile::empty(goal, Outcome::Private);
    }
    let Ok(plan) = plan::plan(goal) else {
        return CaseFile::empty(goal, Outcome::NeedsTopic);
    };
    let tools = tools::StoreTools::new(state);
    agent::run_loop(&tools, plan, stop, on_step).await
}
