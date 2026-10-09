//! Proactive signals: the stuck detector, what changed in a Resume Work
//! thread since it was last viewed, and meeting prep. Each runs on device
//! from stored memories, honors Private Mode and the blocklist, and reaches
//! the person through `notify` (a toast, and a banner with fixed text).

#[cfg(test)]
mod eval;
pub mod meeting_prep;
pub mod stuck;
pub mod thread_digest;

use crate::notify::{send_with_target, NotificationTarget};
use crate::storage::StateStore;
use crate::AppState;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};
use tauri::AppHandle;

const SENT_STATE_KEY: &str = "proactive_signals_sent_v1";
static SENT_MUTATION: Mutex<()> = Mutex::new(());

/// The stuck detector reads the last half hour of captures this often.
pub const STUCK_CHECK_EVERY: Duration = Duration::from_secs(120);
/// Seen threads are compared with new captures this often.
pub const THREAD_CHECK_EVERY: Duration = Duration::from_secs(1800);

/// A signal runs only when switched on and outside Private Mode.
pub fn signal_allowed(enabled: bool, incognito: bool) -> bool {
    enabled && !incognito
}

/// Issues already notified, each with the local day it was sent on.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SentToday(HashMap<String, String>);

impl SentToday {
    pub fn allows(&self, issue: &str, day: &str) -> bool {
        self.0.get(issue).is_none_or(|sent_on| sent_on != day)
    }

    /// Records `issue` for `day` and forgets every other day.
    pub fn record(&mut self, issue: &str, day: &str) {
        self.0.retain(|_, sent_on| sent_on == day);
        self.0.insert(issue.to_string(), day.to_string());
    }
}

/// True the first time `issue` is claimed on `day`, false after, across
/// restarts: one notification per issue per day.
pub fn claim_once_today(store: &StateStore, issue: &str, day: &str) -> Result<bool, String> {
    let _lock = SENT_MUTATION.lock();
    let mut sent = store
        .load_json::<SentToday>(SENT_STATE_KEY)?
        .unwrap_or_default();
    if !sent.allows(issue, day) {
        return Ok(false);
    }
    sent.record(issue, day);
    store.save_json(SENT_STATE_KEY, &sent)?;
    Ok(true)
}

/// When each signal last ran in the background loop.
#[derive(Debug, Default)]
pub struct SignalClock {
    stuck: Option<Instant>,
    threads: Option<Instant>,
}

fn due(last: &mut Option<Instant>, every: Duration) -> bool {
    if last.is_some_and(|at| at.elapsed() < every) {
        return false;
    }
    *last = Some(Instant::now());
    true
}

/// One pass of the background loop: each signal that is switched on and due
/// runs, and sends at most one notification per issue per day.
pub async fn run_due(app: &AppHandle, state: &AppState, clock: &mut SignalClock) {
    let switches = state.config.read().proactive_signals.clone();
    let incognito = state.is_incognito.load(Ordering::SeqCst);
    let now = chrono::Local::now();
    let now_ms = now.timestamp_millis();
    let day = now.format("%Y-%m-%d").to_string();
    let claim = |issue: &str| claim_once_today(&state.state_store, issue, &day).unwrap_or(false);

    if signal_allowed(switches.stuck, incognito) && due(&mut clock.stuck, STUCK_CHECK_EVERY) {
        if let Some((episode, past)) = stuck::find_stuck(state, now_ms).await {
            if claim(&episode.issue_key()) {
                let (title, body) =
                    stuck::stuck_toast(&episode, &stuck::date_label(past.timestamp));
                let target = NotificationTarget {
                    memory_id: Some(past.memory_id),
                    thread_key: None,
                };
                send_with_target(app, state, "stuck", &title, &body, &target);
            }
        }
    }

    if signal_allowed(switches.meeting_prep, incognito) {
        let recording = crate::meeting::recorder_status()
            .ok()
            .and_then(|status| meeting_prep::from_recording(&status));
        if let Some((meeting_id, meeting)) = recording {
            if meeting_prep::prep_due(&meeting, now_ms) && claim(&format!("meeting:{meeting_id}")) {
                if let Some(thread) = meeting_prep::related_thread(state, &meeting.title).await {
                    let (title, body) = meeting_prep::prep_toast(&thread.title);
                    let target = NotificationTarget {
                        memory_id: None,
                        thread_key: Some(thread.key),
                    };
                    send_with_target(app, state, "meeting_prep", &title, &body, &target);
                }
            }
        }
    }

    if signal_allowed(switches.thread_updates, incognito)
        && due(&mut clock.threads, THREAD_CHECK_EVERY)
    {
        for digest in thread_digest::nudge_candidates(state, now_ms).await {
            if claim(&format!("thread:{}", digest.thread_key)) {
                let (title, body) = thread_digest::thread_update_toast(&digest);
                let target = NotificationTarget {
                    memory_id: None,
                    thread_key: Some(digest.thread_key.clone()),
                };
                send_with_target(app, state, "thread_update", &title, &body, &target);
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_signal_needs_its_switch_and_no_private_mode() {
        assert!(signal_allowed(true, false));
        assert!(!signal_allowed(false, false));
        assert!(!signal_allowed(true, true));
        assert!(!signal_allowed(false, true));
    }

    #[test]
    fn an_issue_is_sent_once_a_day() {
        let mut sent = SentToday::default();
        assert!(sent.allows("stuck:a", "2026-10-09"));
        sent.record("stuck:a", "2026-10-09");
        assert!(!sent.allows("stuck:a", "2026-10-09"));
        assert!(sent.allows("stuck:b", "2026-10-09"));
        assert!(sent.allows("stuck:a", "2026-10-10"));
        sent.record("stuck:b", "2026-10-10");
        assert!(
            sent.allows("stuck:a", "2026-10-10"),
            "other days are forgotten"
        );
    }

    #[test]
    fn the_daily_claim_survives_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        {
            let store = StateStore::new(dir.path()).unwrap();
            assert!(claim_once_today(&store, "stuck:a", "2026-10-09").unwrap());
            assert!(!claim_once_today(&store, "stuck:a", "2026-10-09").unwrap());
        }
        let store = StateStore::new(dir.path()).unwrap();
        assert!(!claim_once_today(&store, "stuck:a", "2026-10-09").unwrap());
        assert!(claim_once_today(&store, "stuck:a", "2026-10-10").unwrap());
    }
}
