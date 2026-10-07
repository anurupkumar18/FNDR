//! What the daily briefing is built from. The model call and its prompt live
//! in `inference`; this decides which part of the day it is, which activity
//! counts, and whether there is enough to say anything.

use chrono::{DateTime, Duration, Local, TimeZone};

/// Fewer activity lines than this and a briefing would only restate one or
/// two captures, so none is written.
pub const MIN_ACTIVITY_LINES: usize = 3;
const MAX_ACTIVITY_LINES: usize = 8;
const MAX_TASK_LINES: usize = 3;

/// Before noon the briefing looks ahead from recent work; from noon it looks
/// back on today. A "morning briefing" at 3 PM was the old behavior.
pub fn mode_for_hour(hour: u32) -> &'static str {
    if hour < 12 {
        "morning"
    } else {
        "evening"
    }
}

/// The earliest capture the briefing may use: today for a recap, and since
/// the start of yesterday for a morning briefing.
pub fn window_start_ms(mode: &str, now: DateTime<Local>) -> i64 {
    let day = if mode == "morning" {
        now.date_naive() - Duration::days(1)
    } else {
        now.date_naive()
    };
    day.and_hms_opt(0, 0, 0)
        .and_then(|start| Local.from_local_datetime(&start).earliest())
        .map_or(0, |start| start.timestamp_millis())
}

/// The lines handed to the model: the person's open tasks first, then
/// activity as `(app, title, summary)`. `None` when there is too little
/// activity to brief on.
pub fn briefing_lines(
    activity: &[(String, String, String)],
    open_tasks: &[String],
) -> Option<Vec<String>> {
    let activity: Vec<String> = activity
        .iter()
        .filter(|(app, _, _)| !crate::tasks::suggest::is_system_surface(app))
        .take(MAX_ACTIVITY_LINES)
        .map(|(app, title, summary)| format!("- [{app}] {title}: {summary}"))
        .collect();
    if activity.len() < MIN_ACTIVITY_LINES {
        return None;
    }
    let mut lines: Vec<String> = open_tasks
        .iter()
        .take(MAX_TASK_LINES)
        .map(|title| format!("- [Open task] {title}"))
        .collect();
    lines.extend(activity);
    Some(lines)
}
