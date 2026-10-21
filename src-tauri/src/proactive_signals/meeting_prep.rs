//! Meeting prep: the Resume Work thread related to a meeting, shown as the
//! meeting begins.
//!
//! Two sources feed it an [`UpcomingMeeting`]: a recording the person starts
//! in FNDR, and, when switched on, the calendar on this Mac
//! (`super::calendar`, ADR 029), which is what makes prep arrive before a
//! meeting.

use crate::meeting::MeetingRecorderStatus;
use crate::resume::ResumeThread;
use crate::AppState;
use std::collections::HashSet;

/// Prep is due from this long before a meeting starts...
pub const LEAD_MS: i64 = 5 * 60_000;
/// ...until this long after, which covers a recording started on time.
pub const GRACE_MS: i64 = 2 * 60_000;
/// Resume threads from this many hours back are candidates.
const THREAD_HOURS: u32 = 72;

/// Words in a meeting title that say nothing about its subject.
const GENERIC_WORDS: &[&str] = &[
    "the",
    "and",
    "for",
    "with",
    "meeting",
    "detected",
    "call",
    "sync",
    "weekly",
    "daily",
    "chat",
    "untitled",
    "new",
    "zoom",
    "meet",
    "teams",
    "discussion",
    "review",
    "about",
];

#[derive(Debug, Clone, PartialEq)]
pub struct UpcomingMeeting {
    pub title: String,
    pub starts_at: i64,
}

pub fn prep_due(meeting: &UpcomingMeeting, now_ms: i64) -> bool {
    (meeting.starts_at - LEAD_MS..=meeting.starts_at + GRACE_MS).contains(&now_ms)
}

fn subject_words(text: &str) -> HashSet<String> {
    text.split(|ch: char| !ch.is_alphanumeric())
        .map(str::to_lowercase)
        .filter(|word| word.chars().count() >= 3 && !GENERIC_WORDS.contains(&word.as_str()))
        .collect()
}

/// The thread, given as `(key, title)` most recent first, sharing the most
/// subject words with the meeting title. A tie goes to the more recent one.
pub fn match_thread<'a>(
    meeting_title: &str,
    threads: &'a [(String, String)],
) -> Option<&'a (String, String)> {
    let wanted = subject_words(meeting_title);
    if wanted.is_empty() {
        return None;
    }
    threads
        .iter()
        .enumerate()
        .map(|(rank, thread)| {
            let words = subject_words(&format!("{} {}", thread.0, thread.1));
            (wanted.intersection(&words).count(), rank, thread)
        })
        .filter(|(shared, _, _)| *shared > 0)
        .max_by(|a, b| a.0.cmp(&b.0).then_with(|| b.1.cmp(&a.1)))
        .map(|(_, _, thread)| thread)
}

/// The meeting FNDR knows about: one being recorded, with its id.
pub fn from_recording(status: &MeetingRecorderStatus) -> Option<(String, UpcomingMeeting)> {
    if !status.is_recording {
        return None;
    }
    Some((
        status.current_meeting_id.clone()?,
        UpcomingMeeting {
            title: status.current_title.clone()?,
            starts_at: status.started_at?,
        },
    ))
}

pub fn prep_toast(thread_title: &str) -> (String, String) {
    (
        "Meeting prep".to_string(),
        format!("Related work: {thread_title}"),
    )
}

/// The Resume Work thread for a meeting: by shared words in the title, or
/// else the thread holding the strongest hybrid search hit for the title.
pub async fn related_thread(state: &AppState, meeting_title: &str) -> Option<ResumeThread> {
    if subject_words(meeting_title).is_empty() {
        return None;
    }
    let blocklist = state.config.read().blocklist.clone();
    let mut threads =
        crate::resume::build_resume_threads(&state.store, THREAD_HOURS, 400, &blocklist)
            .await
            .ok()?;
    let pairs: Vec<(String, String)> = threads
        .iter()
        .map(|thread| (thread.key.clone(), thread.title.clone()))
        .collect();
    let key = match match_thread(meeting_title, &pairs) {
        Some((key, _)) => key.clone(),
        None => {
            let request = crate::context_runtime::RetrieveRequest {
                query: meeting_title.to_string(),
                time: None,
                app: None,
                limit: 5,
            };
            let (retrieved, results) =
                crate::context_runtime::retrieve_search_results(state, &request)
                    .await
                    .ok()?;
            if !retrieved.strong_match {
                return None;
            }
            let top = results.first()?.id.clone();
            threads
                .iter()
                .find(|thread| thread.evidence.contains(&top))?
                .key
                .clone()
        }
    };
    let index = threads.iter().position(|thread| thread.key == key)?;
    Some(threads.swap_remove(index))
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_800_000_000_000;
    const MIN: i64 = 60_000;

    fn threads() -> Vec<(String, String)> {
        vec![
            ("Groceries".into(), "Weekly groceries list".into()),
            ("FNDR".into(), "FNDR".into()),
            (
                "google_chrome:title:q4_hiring_plan".into(),
                "Q4 hiring plan for the platform team".into(),
            ),
        ]
    }

    #[test]
    fn prep_is_due_from_a_few_minutes_before_until_just_after_the_start() {
        let meeting = UpcomingMeeting {
            title: "Hiring sync".into(),
            starts_at: NOW,
        };
        assert!(!prep_due(&meeting, NOW - 6 * MIN));
        assert!(prep_due(&meeting, NOW - 5 * MIN));
        assert!(prep_due(&meeting, NOW));
        assert!(prep_due(&meeting, NOW + 2 * MIN));
        assert!(!prep_due(&meeting, NOW + 3 * MIN));
    }

    #[test]
    fn a_meeting_matches_the_thread_sharing_its_words() {
        let threads = threads();
        let hit = match_thread("Platform hiring sync", &threads).expect("match");
        assert_eq!(hit.0, "google_chrome:title:q4_hiring_plan");
        assert_eq!(match_thread("fndr standup", &threads).unwrap().0, "FNDR");
    }

    #[test]
    fn a_generic_title_matches_nothing() {
        let threads = threads();
        assert_eq!(match_thread("Detected Meeting", &threads), None);
        assert_eq!(match_thread("Weekly sync call", &threads), None);
        assert_eq!(match_thread("", &threads), None);
    }

    #[test]
    fn a_recording_in_progress_is_the_meeting_fndr_knows_about() {
        let mut status = crate::meeting::MeetingRecorderStatus {
            is_recording: true,
            is_analyzing: false,
            current_meeting_id: Some("m-1".into()),
            current_title: Some("Hiring sync".into()),
            model: None,
            started_at: Some(NOW),
            segment_count: 0,
            consent_state: "n/a".into(),
            consent_evidence: None,
            consent_checked_segments: 0,
            ffmpeg_available: true,
            transcription_backend: String::new(),
            last_error: None,
        };
        let (id, meeting) = from_recording(&status).expect("meeting");
        assert_eq!(id, "m-1");
        assert_eq!(
            meeting,
            UpcomingMeeting {
                title: "Hiring sync".into(),
                starts_at: NOW
            }
        );
        status.is_recording = false;
        assert_eq!(from_recording(&status), None);
    }

    #[test]
    fn the_toast_names_the_related_work() {
        let (title, body) = prep_toast("Q4 hiring plan");
        assert!(!title.is_empty());
        assert_eq!(body, "Related work: Q4 hiring plan");
    }
}
