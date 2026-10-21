// Content-free first-run events. The file lives only in the active FNDR data directory.

use crate::ipc::onboarding::OnboardingStep;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager};

const FILE_NAME: &str = "onboarding-funnel.jsonl";
const ACTIVATION_WINDOW_MS: u64 = 10 * 60 * 1000;
static LOG_LOCK: Mutex<()> = Mutex::new(());

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    StepStarted,
    StepFinished,
    StepSkipped,
    Error,
    Completed,
    SearchSubmitted,
    ResultSelected,
    ReopenSucceeded,
    ReopenFailed,
}

#[derive(Debug, Deserialize, Serialize)]
struct Event {
    event: EventKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    step: Option<OnboardingStep>,
    at_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    monotonic_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    duration_ms: Option<u64>,
}

#[derive(Clone, Copy)]
struct Clock {
    at_ms: u64,
    monotonic_ms: Option<u64>,
}

impl Clock {
    fn now() -> Self {
        Self {
            at_ms: now_ms(),
            monotonic_ms: monotonic_ms(),
        }
    }

    #[cfg(test)]
    fn at(at_ms: u64) -> Self {
        Self {
            at_ms,
            monotonic_ms: Some(at_ms),
        }
    }
}

fn monotonic_ms() -> Option<u64> {
    #[cfg(unix)]
    {
        let mut time = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        if unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut time) } == 0 {
            return Some(time.tv_sec as u64 * 1_000 + time.tv_nsec as u64 / 1_000_000);
        }
    }
    None
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn append(dir: &Path, event: &Event) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let path = dir.join(FILE_NAME);
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|e| e.to_string())?;
    let mut line = serde_json::to_vec(event).map_err(|e| e.to_string())?;
    line.push(b'\n');
    file.write_all(&line).map_err(|e| e.to_string())
}

fn read_events(dir: &Path) -> Result<Vec<Event>, String> {
    let path = dir.join(FILE_NAME);
    let raw = fs::read_to_string(path).map_err(|e| e.to_string())?;
    raw.lines()
        .map(|line| serde_json::from_str(line).map_err(|e| e.to_string()))
        .collect()
}

pub fn start_if_new(dir: &Path, state_exists: bool, step: &OnboardingStep) -> Result<(), String> {
    start_if_new_with_clock(dir, state_exists, step, Clock::now())
}

#[cfg(test)]
fn start_if_new_at(
    dir: &Path,
    state_exists: bool,
    step: &OnboardingStep,
    at_ms: u64,
) -> Result<(), String> {
    start_if_new_with_clock(dir, state_exists, step, Clock::at(at_ms))
}

fn start_if_new_with_clock(
    dir: &Path,
    state_exists: bool,
    step: &OnboardingStep,
    clock: Clock,
) -> Result<(), String> {
    let _guard = LOG_LOCK.lock();
    if state_exists || dir.join(FILE_NAME).exists() {
        return Ok(());
    }
    append(
        dir,
        &Event {
            event: EventKind::StepStarted,
            step: Some(step.clone()),
            at_ms: clock.at_ms,
            monotonic_ms: clock.monotonic_ms,
            duration_ms: None,
        },
    )
}

pub fn transition(
    dir: &Path,
    from: &OnboardingStep,
    to: &OnboardingStep,
    skipped: bool,
) -> Result<(), String> {
    transition_with_clock(dir, from, to, skipped, Clock::now())
}

#[cfg(test)]
fn transition_at(
    dir: &Path,
    from: &OnboardingStep,
    to: &OnboardingStep,
    skipped: bool,
    at_ms: u64,
) -> Result<(), String> {
    transition_with_clock(dir, from, to, skipped, Clock::at(at_ms))
}

fn transition_with_clock(
    dir: &Path,
    from: &OnboardingStep,
    to: &OnboardingStep,
    skipped: bool,
    clock: Clock,
) -> Result<(), String> {
    let _guard = LOG_LOCK.lock();
    if from == to || !dir.join(FILE_NAME).exists() {
        return Ok(());
    }
    let events = read_events(dir)?;
    let started = events
        .iter()
        .rev()
        .find(|row| row.event == EventKind::StepStarted && row.step.as_ref() == Some(from));
    append(
        dir,
        &Event {
            event: if skipped {
                EventKind::StepSkipped
            } else {
                EventKind::StepFinished
            },
            step: Some(from.clone()),
            at_ms: clock.at_ms,
            monotonic_ms: clock.monotonic_ms,
            duration_ms: started
                .and_then(|row| row.monotonic_ms.zip(clock.monotonic_ms))
                .and_then(|(start, end)| end.checked_sub(start)),
        },
    )?;
    append(
        dir,
        &Event {
            event: if *to == OnboardingStep::Complete {
                EventKind::Completed
            } else {
                EventKind::StepStarted
            },
            step: Some(to.clone()),
            at_ms: clock.at_ms,
            monotonic_ms: clock.monotonic_ms,
            duration_ms: None,
        },
    )
}

#[cfg(test)]
fn record_at(
    dir: &Path,
    kind: EventKind,
    step: Option<OnboardingStep>,
    at_ms: u64,
) -> Result<(), String> {
    record_with_clock(dir, kind, step, Clock::at(at_ms))
}

fn record_with_clock(
    dir: &Path,
    kind: EventKind,
    step: Option<OnboardingStep>,
    clock: Clock,
) -> Result<(), String> {
    if !matches!(
        kind,
        EventKind::Error
            | EventKind::SearchSubmitted
            | EventKind::ResultSelected
            | EventKind::ReopenSucceeded
            | EventKind::ReopenFailed
    ) {
        return Err("Unsupported onboarding event".to_string());
    }
    let _guard = LOG_LOCK.lock();
    if !dir.join(FILE_NAME).exists() {
        return Ok(());
    }
    let events = read_events(dir)?;
    let completed = events.iter().find(|row| row.event == EventKind::Completed);
    if kind == EventKind::Error {
        if completed.is_some() || step.is_none() {
            return Ok(());
        }
    } else if !completed
        .and_then(|row| row.monotonic_ms.zip(clock.monotonic_ms))
        .is_some_and(|(start, end)| end >= start && end - start <= ACTIVATION_WINDOW_MS)
    {
        return Ok(());
    }
    append(
        dir,
        &Event {
            event: kind,
            step,
            at_ms: clock.at_ms,
            monotonic_ms: clock.monotonic_ms,
            duration_ms: None,
        },
    )
}

#[tauri::command]
pub fn record_onboarding_funnel_event(
    app: AppHandle,
    kind: EventKind,
    step: Option<OnboardingStep>,
) -> Result<(), String> {
    let dir = crate::config::fndr_app_data_dir(app.path()).map_err(|e| e.to_string())?;
    record_with_clock(&dir, kind, step, Clock::now())
}

#[cfg(test)]
mod tests {
    use super::{
        read_events, record_at, start_if_new_at, start_if_new_with_clock, transition_at,
        transition_with_clock, Clock, EventKind,
    };
    use crate::ipc::onboarding::OnboardingStep;
    use tempfile::tempdir;

    #[test]
    fn saves_step_durations_skips_and_errors_without_content() {
        let dir = tempdir().unwrap();
        start_if_new_at(dir.path(), false, &OnboardingStep::Welcome, 1_000).unwrap();
        start_if_new_at(dir.path(), false, &OnboardingStep::Welcome, 1_100).unwrap();
        transition_at(
            dir.path(),
            &OnboardingStep::Welcome,
            &OnboardingStep::PrivacyPromise,
            false,
            2_000,
        )
        .unwrap();
        transition_at(
            dir.path(),
            &OnboardingStep::PrivacyPromise,
            &OnboardingStep::Biometrics,
            true,
            4_000,
        )
        .unwrap();
        record_at(
            dir.path(),
            EventKind::Error,
            Some(OnboardingStep::Biometrics),
            4_500,
        )
        .unwrap();

        let rows = read_events(dir.path()).unwrap();
        assert_eq!(rows.len(), 6);
        assert_eq!(rows[1].event, EventKind::StepFinished);
        assert_eq!(rows[1].duration_ms, Some(1_000));
        assert_eq!(rows[3].event, EventKind::StepSkipped);
        assert_eq!(rows[3].duration_ms, Some(2_000));
        assert_eq!(rows[5].event, EventKind::Error);
        let raw = std::fs::read_to_string(dir.path().join("onboarding-funnel.jsonl")).unwrap();
        assert!(!raw.contains("display_name"));
        assert!(!raw.contains("query"));
    }

    #[test]
    fn step_duration_uses_monotonic_time_when_wall_clock_moves_backward() {
        let dir = tempdir().unwrap();
        start_if_new_with_clock(
            dir.path(),
            false,
            &OnboardingStep::Welcome,
            Clock {
                at_ms: 5_000,
                monotonic_ms: Some(1_000),
            },
        )
        .unwrap();
        transition_with_clock(
            dir.path(),
            &OnboardingStep::Welcome,
            &OnboardingStep::PrivacyPromise,
            false,
            Clock {
                at_ms: 4_000,
                monotonic_ms: Some(2_000),
            },
        )
        .unwrap();
        let rows = read_events(dir.path()).unwrap();
        assert_eq!(rows[1].duration_ms, Some(1_000));
    }

    #[test]
    fn only_logs_activation_proxy_events_after_real_completion() {
        let dir = tempdir().unwrap();
        start_if_new_at(dir.path(), false, &OnboardingStep::Welcome, 1_000).unwrap();
        record_at(dir.path(), EventKind::SearchSubmitted, None, 1_500).unwrap();
        transition_at(
            dir.path(),
            &OnboardingStep::Welcome,
            &OnboardingStep::Complete,
            false,
            2_000,
        )
        .unwrap();
        record_at(dir.path(), EventKind::SearchSubmitted, None, 3_000).unwrap();
        record_at(dir.path(), EventKind::ResultSelected, None, 4_000).unwrap();
        record_at(dir.path(), EventKind::ReopenSucceeded, None, 5_000).unwrap();
        record_at(dir.path(), EventKind::SearchSubmitted, None, 603_000).unwrap();
        let rows = read_events(dir.path()).unwrap();
        assert_eq!(
            rows.iter()
                .filter(|row| row.event == EventKind::SearchSubmitted)
                .count(),
            1
        );
        assert!(rows.iter().any(|row| row.event == EventKind::Completed));
        assert_eq!(rows.last().unwrap().event, EventKind::ReopenSucceeded);
    }

    #[test]
    fn three_isolated_runs_cover_complete_skip_and_error() {
        let root = tempdir().unwrap();
        let first = root.path().join("run-1");
        let second = root.path().join("run-2");
        let third = root.path().join("run-3");

        start_if_new_at(&first, false, &OnboardingStep::Welcome, 1_000).unwrap();
        transition_at(
            &first,
            &OnboardingStep::Welcome,
            &OnboardingStep::PrivacyPromise,
            false,
            2_000,
        )
        .unwrap();
        transition_at(
            &first,
            &OnboardingStep::PrivacyPromise,
            &OnboardingStep::Biometrics,
            false,
            3_000,
        )
        .unwrap();
        transition_at(
            &first,
            &OnboardingStep::Biometrics,
            &OnboardingStep::ModelDownload,
            false,
            4_000,
        )
        .unwrap();
        transition_at(
            &first,
            &OnboardingStep::ModelDownload,
            &OnboardingStep::Permissions,
            false,
            5_000,
        )
        .unwrap();
        transition_at(
            &first,
            &OnboardingStep::Permissions,
            &OnboardingStep::BringPast,
            false,
            6_000,
        )
        .unwrap();
        transition_at(
            &first,
            &OnboardingStep::BringPast,
            &OnboardingStep::Complete,
            false,
            7_000,
        )
        .unwrap();
        record_at(&first, EventKind::SearchSubmitted, None, 8_000).unwrap();
        record_at(&first, EventKind::ResultSelected, None, 9_000).unwrap();
        record_at(&first, EventKind::ReopenSucceeded, None, 10_000).unwrap();

        start_if_new_at(&second, false, &OnboardingStep::Welcome, 1_000).unwrap();
        transition_at(
            &second,
            &OnboardingStep::Welcome,
            &OnboardingStep::ModelDownload,
            false,
            2_000,
        )
        .unwrap();
        transition_at(
            &second,
            &OnboardingStep::ModelDownload,
            &OnboardingStep::Permissions,
            true,
            3_000,
        )
        .unwrap();
        transition_at(
            &second,
            &OnboardingStep::Permissions,
            &OnboardingStep::BringPast,
            false,
            4_000,
        )
        .unwrap();
        transition_at(
            &second,
            &OnboardingStep::BringPast,
            &OnboardingStep::ModelDownload,
            true,
            5_000,
        )
        .unwrap();

        start_if_new_at(&third, false, &OnboardingStep::Welcome, 1_000).unwrap();
        record_at(
            &third,
            EventKind::Error,
            Some(OnboardingStep::Welcome),
            2_000,
        )
        .unwrap();

        assert!(read_events(&first)
            .unwrap()
            .iter()
            .any(|row| row.event == EventKind::Completed));
        assert!(!read_events(&second)
            .unwrap()
            .iter()
            .any(|row| row.event == EventKind::Completed));
        assert!(read_events(&third)
            .unwrap()
            .iter()
            .any(|row| row.event == EventKind::Error));

        if let Some(destination) = std::env::var_os("OB07_EVIDENCE_DIR") {
            let destination = std::path::PathBuf::from(destination);
            std::fs::create_dir_all(&destination).unwrap();
            for (index, dir) in [first, second, third].iter().enumerate() {
                std::fs::copy(
                    dir.join("onboarding-funnel.jsonl"),
                    destination.join(format!("run-{}.jsonl", index + 1)),
                )
                .unwrap();
            }
        }
    }
}
