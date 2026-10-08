//! The daily briefing is written without a model: the right part of the
//! day, activity from that window only, each piece of work said once, the
//! person's open tasks, and nothing at all when there is too little to say.

use chrono::{Local, TimeZone};
use fndr_lib::briefing::{mode_for_hour, plain_briefing, window_start_ms, MIN_ACTIVITY_LINES};

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

fn note(app: &str, title: &str, summary: &str) -> (String, String, String) {
    (app.to_string(), title.to_string(), summary.to_string())
}

/// Newest first, as the store returns them.
fn a_day() -> Vec<(String, String, String)> {
    vec![
        note("Mail", "Inbox", "Replied to the lab report thread."),
        note(
            "Google Chrome",
            "Gamete Chromosome Count",
            "Answered questions on meiosis and gamete chromosome counts",
        ),
        note(
            "UserNotificationCenter",
            "Notification",
            "A banner appeared.",
        ),
        note(
            "Claude",
            "FNDR",
            "Reviewed the app's development status, including commits and UI rendering.",
        ),
        note(
            "Claude",
            "FNDR",
            "Completed a repair of the FNDR database and verified the result.",
        ),
    ]
}

#[test]
fn too_little_activity_means_no_briefing() {
    let day = a_day();
    assert_eq!(plain_briefing(&day[..1], &[]), "");
    assert_eq!(MIN_ACTIVITY_LINES, 3);
    // Two captures of one thing and a notification banner are one piece of work.
    let thin = vec![day[4].clone(), day[4].clone(), day[2].clone()];
    assert_eq!(plain_briefing(&thin, &[]), "");
}

#[test]
fn the_briefing_tells_the_day_in_order_one_sentence_per_piece_of_work() {
    assert_eq!(
        plain_briefing(&a_day(), &[]),
        // The two Claude captures are one piece of work; its latest state is told.
        "Reviewed the app's development status, including commits and UI rendering. \
         Answered questions on meiosis and gamete chromosome counts. \
         Replied to the lab report thread."
    );
}

#[test]
fn open_tasks_close_the_briefing_and_the_rest_are_counted() {
    let one = vec!["Submit the peer review".to_string()];
    assert!(plain_briefing(&a_day(), &one)
        .ends_with("Replied to the lab report thread. Still open: Submit the peer review."));
    let three = vec![
        "Submit the peer review.".to_string(),
        "Book the room".to_string(),
        "Renew the lease".to_string(),
    ];
    assert!(plain_briefing(&a_day(), &three)
        .ends_with("Still open: Submit the peer review, and 2 more."));
}

#[test]
fn a_narrated_or_placeholder_summary_is_never_quoted() {
    let mut day = a_day();
    day[0] = note("Mail", "Inbox", "The user is viewing the inbox.");
    day[1] = note(
        "Google Chrome",
        "Canvas",
        "Screen capture (visual): Google_Chrome_1790643908348.png",
    );
    day.push(note(
        "Notes",
        "Ideas",
        "Listed demo ideas for the capstone.",
    ));
    let briefing = plain_briefing(&day, &[]);
    assert!(!briefing.to_lowercase().contains("the user"), "{briefing}");
    assert!(!briefing.contains("Screen capture"), "{briefing}");
    assert!(
        briefing.contains("Listed demo ideas for the capstone."),
        "{briefing}"
    );
}
