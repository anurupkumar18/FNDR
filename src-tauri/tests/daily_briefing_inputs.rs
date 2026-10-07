//! What the daily briefing is given: the right part of the day, activity
//! from that window only, the person's open tasks, and nothing at all when
//! there is too little to say.

use chrono::{Local, TimeZone};
use fndr_lib::briefing::{briefing_lines, mode_for_hour, window_start_ms, MIN_ACTIVITY_LINES};

#[test]
fn before_noon_looks_ahead_and_from_noon_looks_back_on_today() {
    assert_eq!(mode_for_hour(8), "morning");
    assert_eq!(mode_for_hour(11), "morning");
    assert_eq!(mode_for_hour(12), "evening");
    assert_eq!(mode_for_hour(14), "evening");
    assert_eq!(mode_for_hour(22), "evening");
}

#[test]
fn a_recap_covers_today_and_a_morning_briefing_reaches_back_one_day() {
    let now = Local.with_ymd_and_hms(2026, 10, 7, 14, 48, 0).unwrap();
    let today = Local
        .with_ymd_and_hms(2026, 10, 7, 0, 0, 0)
        .unwrap()
        .timestamp_millis();
    let yesterday = Local
        .with_ymd_and_hms(2026, 10, 6, 0, 0, 0)
        .unwrap()
        .timestamp_millis();
    assert_eq!(window_start_ms("evening", now), today);
    assert_eq!(window_start_ms("morning", now), yesterday);
}

fn activity(count: usize) -> Vec<(String, String, String)> {
    (0..count)
        .map(|n| {
            (
                "Mail".to_string(),
                format!("Thread {n}"),
                format!("Replied to thread {n}."),
            )
        })
        .collect()
}

#[test]
fn too_little_activity_means_no_briefing() {
    assert!(briefing_lines(&activity(MIN_ACTIVITY_LINES - 1), &[]).is_none());
    assert!(briefing_lines(&activity(MIN_ACTIVITY_LINES), &[]).is_some());
}

#[test]
fn open_tasks_lead_and_system_processes_are_left_out() {
    let mut cards = activity(3);
    cards.push((
        "UserNotificationCenter".to_string(),
        "Notification".to_string(),
        "A banner appeared.".to_string(),
    ));
    let tasks = vec![
        "Send Priya the draft report".to_string(),
        "Book the conference room".to_string(),
        "Renew the lease".to_string(),
        "Pay the invoice".to_string(),
    ];
    let lines = briefing_lines(&cards, &tasks).expect("enough activity");
    assert_eq!(lines[0], "- Open task: Send Priya the draft report");
    assert_eq!(
        lines
            .iter()
            .filter(|line| line.starts_with("- Open task:"))
            .count(),
        3
    );
    assert!(lines
        .iter()
        .all(|line| !line.contains("UserNotificationCenter")));
    assert!(lines
        .iter()
        .any(|line| line == "- Thread 0: Replied to thread 0. (in Mail)"));
}
