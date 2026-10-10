//! Proactive signals: the stuck detector, what changed in a Resume Work
//! thread since it was last viewed, and meeting prep. Each runs on device
//! from stored memories, honors Private Mode and the blocklist, and reaches
//! the person through `notify` (a toast, and a banner with fixed text).
//! Meeting prep also reads the calendar on this Mac when switched on
//! (`calendar`, ADR 029).

pub mod calendar;
#[cfg(test)]
mod eval;
pub mod eventkit;
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
/// The calendar is read this often while calendar meeting prep is on.
pub const CALENDAR_CHECK_EVERY: Duration = Duration::from_secs(60);

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
    calendar: Option<Instant>,
}

fn due(last: &mut Option<Instant>, every: Duration) -> bool {
    if last.is_some_and(|at| at.elapsed() < every) {
        return false;
    }
    *last = Some(Instant::now());
    true
}

/// The meetings, keyed by an issue such as `calendar:<event>@<start>`, that
/// prep is due for now and that `claim` has not seen: each issue fires once.
pub fn claim_due_meetings(
    meetings: Vec<(String, meeting_prep::UpcomingMeeting)>,
    now_ms: i64,
    mut claim: impl FnMut(&str) -> bool,
) -> Vec<meeting_prep::UpcomingMeeting> {
    meetings
        .into_iter()
        .filter(|(_, meeting)| meeting_prep::prep_due(meeting, now_ms))
        .filter(|(issue, _)| claim(issue))
        .map(|(_, meeting)| meeting)
        .collect()
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
        let mut meetings: Vec<(String, meeting_prep::UpcomingMeeting)> =
            crate::meeting::recorder_status()
                .ok()
                .and_then(|status| meeting_prep::from_recording(&status))
                .map(|(meeting_id, meeting)| (format!("meeting:{meeting_id}"), meeting))
                .into_iter()
                .collect();
        if switches.calendar_meeting_prep && due(&mut clock.calendar, CALENDAR_CHECK_EVERY) {
            let selected = switches.calendar_ids.clone();
            let from_calendar = tokio::task::spawn_blocking(move || {
                calendar::due_meetings(
                    &eventkit::EventKitCalendar,
                    true,
                    incognito,
                    &selected,
                    now_ms,
                )
            })
            .await
            .unwrap_or_default();
            meetings.extend(
                from_calendar
                    .into_iter()
                    .map(|(event, meeting)| (format!("calendar:{event}"), meeting)),
            );
        }
        for meeting in claim_due_meetings(meetings, now_ms, &claim) {
            let Some(thread) = meeting_prep::related_thread(state, &meeting.title).await else {
                continue;
            };
            // A recording of a calendar meeting is the same meeting: one
            // toast per related thread a day.
            if !claim(&format!("meeting_thread:{}", thread.key)) {
                continue;
            }
            let (title, body) = meeting_prep::prep_toast(&thread.title);
            let target = NotificationTarget {
                memory_id: None,
                thread_key: Some(thread.key),
            };
            send_with_target(app, state, "meeting_prep", &title, &body, &target);
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

    #[test]
    fn a_calendar_meeting_fires_once_per_event_and_not_twice() {
        use calendar::tests::{event, FakeCalendar, MIN, NOW};
        let dir = tempfile::tempdir().unwrap();
        let store = StateStore::new(dir.path()).unwrap();
        let fake = FakeCalendar::granted(vec![
            event("hiring", "work", "Hiring sync", 4),
            event("design", "work", "Design review", 1),
        ]);
        let pass = |now_ms: i64| {
            let meetings = calendar::due_meetings(&fake, true, false, &[], now_ms)
                .into_iter()
                .map(|(event, meeting)| (format!("calendar:{event}"), meeting))
                .collect();
            claim_due_meetings(meetings, now_ms, |issue| {
                claim_once_today(&store, issue, "2026-10-09").unwrap()
            })
        };
        let first: Vec<String> = pass(NOW).into_iter().map(|m| m.title).collect();
        assert_eq!(first, vec!["Hiring sync", "Design review"]);
        assert!(pass(NOW).is_empty(), "the same events never fire twice");
        assert!(pass(NOW + MIN).is_empty(), "nor on the next pass");
    }

    #[test]
    fn private_mode_suppresses_calendar_meeting_prep() {
        use calendar::tests::{event, FakeCalendar, NOW};
        let fake = FakeCalendar::granted(vec![event("hiring", "work", "Hiring sync", 4)]);
        let mut claimed = Vec::new();
        let meetings = calendar::due_meetings(&fake, true, true, &[], NOW)
            .into_iter()
            .map(|(event, meeting)| (format!("calendar:{event}"), meeting))
            .collect();
        let due = claim_due_meetings(meetings, NOW, |issue| {
            claimed.push(issue.to_string());
            true
        });
        assert!(due.is_empty());
        assert!(claimed.is_empty(), "nothing is claimed in Private Mode");
        assert_eq!(fake.reads.get(), 0, "and the calendar is not read");
    }

    #[test]
    fn a_meeting_not_yet_due_is_not_claimed() {
        let mut claimed = Vec::new();
        let later = meeting_prep::UpcomingMeeting {
            title: "Design review".into(),
            starts_at: 1_800_000_000_000 + 20 * 60_000,
        };
        let due = claim_due_meetings(
            vec![("calendar:later".into(), later)],
            1_800_000_000_000,
            |issue| {
                claimed.push(issue.to_string());
                true
            },
        );
        assert!(due.is_empty());
        assert!(claimed.is_empty(), "a later pass can still fire it");
    }
}
