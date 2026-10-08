//! The daily briefing, written without a model.
//!
//! Three prompt versions were run on the local 2B model on 2026-10-07. It
//! invented advice, then copied its notes back word for word, then stated
//! that an open task had been submitted. The briefing is therefore composed
//! from summaries that already pass the voice rules: it cannot say anything
//! the captures do not.

use chrono::{DateTime, Duration, Local, TimeZone};

/// With fewer usable captures than this, or only one piece of work among
/// them, a briefing would restate a capture, so none is written.
pub const MIN_ACTIVITY_LINES: usize = 3;
const MIN_PIECES_OF_WORK: usize = 2;
const MAX_SENTENCES: usize = 3;

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

/// A summary fit to quote: neutral voice, and not a placeholder or narration.
fn quotable(summary: &str) -> Option<String> {
    use crate::summariser::narration_filter::{
        is_placeholder_summary, narration_filter_hits, neutral_voice,
    };
    let sentence = neutral_voice(summary.trim());
    let sentence = sentence.trim().trim_end_matches('.').to_string();
    let lower = sentence.to_lowercase();
    let usable = !is_placeholder_summary(&sentence)
        && !narration_filter_hits(&sentence)
        && !lower.starts_with("the user")
        && !lower.starts_with("you ")
        && sentence.split_whitespace().count() >= 3;
    usable.then_some(sentence)
}

/// The briefing for a window of activity given newest first as
/// `(app, window title, summary)`: up to three pieces of work in the order
/// they happened, each told by its latest summary, then what the person has
/// open. Empty when there is too little to say.
pub fn plain_briefing(activity: &[(String, String, String)], open_tasks: &[String]) -> String {
    let usable: Vec<(&String, &String, String)> = activity
        .iter()
        .filter(|(app, _, _)| !crate::tasks::suggest::is_system_surface(app))
        .filter_map(|(app, title, summary)| quotable(summary).map(|s| (app, title, s)))
        .collect();
    // Captures of the same window are one piece of work; the newest speaks.
    let mut seen = std::collections::HashSet::new();
    let mut pieces: Vec<String> = usable
        .iter()
        .filter(|(app, title, _)| seen.insert((app.to_lowercase(), title.to_lowercase())))
        .map(|(_, _, sentence)| sentence.clone())
        .collect();
    let mut said = std::collections::HashSet::new();
    pieces.retain(|sentence| said.insert(sentence.to_lowercase()));
    if usable.len() < MIN_ACTIVITY_LINES || pieces.len() < MIN_PIECES_OF_WORK {
        return String::new();
    }
    pieces.truncate(MAX_SENTENCES);
    pieces.reverse();
    let mut out: Vec<String> = pieces
        .into_iter()
        .map(|sentence| format!("{sentence}."))
        .collect();
    if let Some(first) = open_tasks.first() {
        let first = first.trim().trim_end_matches('.');
        out.push(match open_tasks.len() - 1 {
            0 => format!("Still open: {first}."),
            more => format!("Still open: {first}, and {more} more."),
        });
    }
    out.join(" ")
}

/// The briefing for `mode` as of `now`: activity from that window that a
/// person can see elsewhere in the app, and the tasks they have open.
pub async fn briefing_for(state: &crate::AppState, mode: &str, now: DateTime<Local>) -> String {
    let results = state
        .store
        .get_search_results_in_range(window_start_ms(mode, now), now.timestamp_millis())
        .await
        .unwrap_or_default();
    let (mut shown, _hidden) = crate::memory_quality::partition_surfaceable(results);
    shown.retain(|result| {
        !crate::privacy::Blocklist::is_internal_app(&result.app_name, result.bundle_id.as_deref())
    });
    shown.sort_by_key(|result| std::cmp::Reverse(result.timestamp));
    let activity: Vec<(String, String, String)> = shown
        .into_iter()
        .map(|result| {
            let summary = if result.display_summary.trim().is_empty() {
                result.snippet
            } else {
                result.display_summary
            };
            (result.app_name, result.window_title, summary)
        })
        .collect();
    let open_tasks: Vec<String> =
        crate::tasks::suggest::open_commitments(state.store.list_tasks().await.unwrap_or_default())
            .into_iter()
            .map(|task| task.title)
            .collect();
    plain_briefing(&activity, &open_tasks)
}
