//! Time per project and app from activity events.
//!
//! An event is a span of work. Spans can overlap (two windows open at once),
//! so each moment is counted once: an event ends where the next one starts.
//! One event is counted for at most `MAX_EVENT_MS`, because an event left
//! open overnight is a capture gap and not eight hours of work.

use crate::outputs::Citation;
use crate::storage::ActivityEvent;
use std::collections::{BTreeMap, HashMap};

pub const MAX_EVENT_MS: i64 = 4 * 60 * 60 * 1000;
pub const NO_PROJECT: &str = "No project";
pub const UNKNOWN_APP: &str = "Unknown app";
/// A line cites at most this many events; a total over a month would
/// otherwise carry hundreds.
pub const MAX_CITATIONS_PER_LINE: usize = 25;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub project: String,
    pub app: String,
    pub ms: i64,
    pub citations: Vec<Citation>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectTotal {
    pub project: String,
    pub ms: i64,
    pub citations: Vec<Citation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Timesheet {
    /// Longest first, then by name.
    pub rows: Vec<Row>,
    pub projects: Vec<ProjectTotal>,
    pub total_ms: i64,
    pub total_citations: Vec<Citation>,
}

/// Counted time for each event: clipped to `[start_ms, end_ms]`, capped, and
/// ended where the next event starts.
fn counted_spans(
    events: &[ActivityEvent],
    start_ms: i64,
    end_ms: i64,
) -> Vec<(&ActivityEvent, i64)> {
    let mut ordered: Vec<&ActivityEvent> = events
        .iter()
        .filter(|event| event.end_time > event.start_time)
        .filter(|event| event.end_time > start_ms && event.start_time < end_ms)
        .collect();
    ordered.sort_by_key(|event| (event.start_time, event.id.clone()));
    let mut spans = Vec::with_capacity(ordered.len());
    for (index, event) in ordered.iter().enumerate() {
        let begin = event.start_time.max(start_ms);
        let mut end = event.end_time.min(end_ms);
        if let Some(next) = ordered.get(index + 1) {
            end = end.min(next.start_time.max(begin));
        }
        let counted = (end - begin).clamp(0, MAX_EVENT_MS);
        if counted > 0 {
            spans.push((*event, counted));
        }
    }
    spans
}

fn cap(mut citations: Vec<Citation>) -> Vec<Citation> {
    citations.truncate(MAX_CITATIONS_PER_LINE);
    citations
}

pub fn compute(
    events: &[ActivityEvent],
    app_by_memory: &HashMap<String, String>,
    start_ms: i64,
    end_ms: i64,
) -> Timesheet {
    let mut rows: BTreeMap<(String, String), Row> = BTreeMap::new();
    let mut projects: BTreeMap<String, ProjectTotal> = BTreeMap::new();
    let mut total = Timesheet::default();
    for (event, ms) in counted_spans(events, start_ms, end_ms) {
        let project = event
            .project
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .unwrap_or(NO_PROJECT)
            .to_string();
        let app = app_by_memory
            .get(&event.memory_id)
            .map(|name| name.trim())
            .filter(|name| !name.is_empty())
            .unwrap_or(UNKNOWN_APP)
            .to_string();
        let citation = Citation::event(event);
        let row = rows
            .entry((project.clone(), app.clone()))
            .or_insert_with(|| Row {
                project: project.clone(),
                app,
                ms: 0,
                citations: Vec::new(),
            });
        row.ms += ms;
        row.citations.push(citation.clone());
        let project_total = projects
            .entry(project.clone())
            .or_insert_with(|| ProjectTotal {
                project,
                ms: 0,
                citations: Vec::new(),
            });
        project_total.ms += ms;
        project_total.citations.push(citation.clone());
        total.total_ms += ms;
        total.total_citations.push(citation);
    }
    total.rows = rows.into_values().collect();
    total.rows.sort_by(|a, b| {
        b.ms.cmp(&a.ms)
            .then(a.project.cmp(&b.project))
            .then(a.app.cmp(&b.app))
    });
    total.projects = projects.into_values().collect();
    total
        .projects
        .sort_by(|a, b| b.ms.cmp(&a.ms).then(a.project.cmp(&b.project)));
    for row in &mut total.rows {
        row.citations = cap(std::mem::take(&mut row.citations));
    }
    for project in &mut total.projects {
        project.citations = cap(std::mem::take(&mut project.citations));
    }
    total.total_citations = cap(std::mem::take(&mut total.total_citations));
    total
}

/// "1h 05m", "45m", "0m". Rounds to the nearest minute.
pub fn format_duration(ms: i64) -> String {
    let minutes = (ms.max(0) + 30_000) / 60_000;
    if minutes >= 60 {
        format!("{}h {:02}m", minutes / 60, minutes % 60)
    } else {
        format!("{minutes}m")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: i64 = 60_000;
    const DAY0: i64 = 1_760_000_000_000;

    fn event(
        id: &str,
        memory: &str,
        project: Option<&str>,
        start_min: i64,
        end_min: i64,
    ) -> ActivityEvent {
        ActivityEvent {
            id: id.to_string(),
            memory_id: memory.to_string(),
            start_time: DAY0 + start_min * MIN,
            end_time: DAY0 + end_min * MIN,
            project: project.map(str::to_string),
            ..Default::default()
        }
    }

    fn apps() -> HashMap<String, String> {
        [
            ("m1", "Xcode"),
            ("m2", "Safari"),
            ("m3", "Xcode"),
            ("m4", "Slack"),
        ]
        .into_iter()
        .map(|(id, app)| (id.to_string(), app.to_string()))
        .collect()
    }

    fn window() -> (i64, i64) {
        (DAY0 - 60 * MIN, DAY0 + 24 * 60 * MIN)
    }

    #[test]
    fn totals_add_up_per_project_app_and_overall() {
        let events = vec![
            event("e1", "m1", Some("Atlas"), 0, 90),
            event("e2", "m2", Some("Atlas"), 90, 120),
            event("e3", "m3", Some("Birch"), 120, 165),
            event("e4", "m4", None, 165, 180),
        ];
        let (start, end) = window();
        let sheet = compute(&events, &apps(), start, end);
        assert_eq!(sheet.total_ms, 180 * MIN);
        let project = |name: &str| {
            sheet
                .projects
                .iter()
                .find(|p| p.project == name)
                .unwrap()
                .ms
        };
        assert_eq!(project("Atlas"), 120 * MIN);
        assert_eq!(project("Birch"), 45 * MIN);
        assert_eq!(project(NO_PROJECT), 15 * MIN);
        let row = |p: &str, a: &str| {
            sheet
                .rows
                .iter()
                .find(|r| r.project == p && r.app == a)
                .unwrap()
                .ms
        };
        assert_eq!(row("Atlas", "Xcode"), 90 * MIN);
        assert_eq!(row("Atlas", "Safari"), 30 * MIN);
        assert_eq!(row("Birch", "Xcode"), 45 * MIN);
        assert_eq!(row(NO_PROJECT, "Slack"), 15 * MIN);
        assert_eq!(sheet.rows.iter().map(|r| r.ms).sum::<i64>(), sheet.total_ms);
        assert_eq!(sheet.projects[0].project, "Atlas", "longest project first");
    }

    #[test]
    fn overlapping_events_count_each_minute_once() {
        let events = vec![
            event("e1", "m1", Some("Atlas"), 0, 60),
            event("e2", "m2", Some("Birch"), 30, 90),
        ];
        let (start, end) = window();
        let sheet = compute(&events, &apps(), start, end);
        assert_eq!(sheet.total_ms, 90 * MIN);
        let atlas = sheet
            .projects
            .iter()
            .find(|p| p.project == "Atlas")
            .unwrap();
        assert_eq!(atlas.ms, 30 * MIN, "Atlas ends where Birch starts");
    }

    #[test]
    fn an_event_is_clipped_to_the_range_and_capped() {
        let events = vec![
            event("e1", "m1", Some("Atlas"), -30, 30),
            event("e2", "m2", Some("Birch"), 600, 1200),
        ];
        let sheet = compute(&events, &apps(), DAY0, DAY0 + 24 * 60 * MIN);
        let atlas = sheet
            .projects
            .iter()
            .find(|p| p.project == "Atlas")
            .unwrap();
        assert_eq!(atlas.ms, 30 * MIN, "time before the range is not counted");
        let birch = sheet
            .projects
            .iter()
            .find(|p| p.project == "Birch")
            .unwrap();
        assert_eq!(
            birch.ms, MAX_EVENT_MS,
            "ten hours in one event is a capture gap"
        );
    }

    #[test]
    fn events_outside_the_range_or_without_duration_add_nothing() {
        let events = vec![
            event("e1", "m1", Some("Atlas"), 2000, 2060),
            event("e2", "m2", Some("Atlas"), 10, 10),
            event("e3", "m3", Some("Atlas"), 20, 5),
        ];
        let sheet = compute(&events, &apps(), DAY0, DAY0 + 60 * MIN);
        assert_eq!(sheet, Timesheet::default());
    }

    #[test]
    fn every_row_cites_the_events_it_counted() {
        let events = vec![
            event("e1", "m1", Some("Atlas"), 0, 30),
            event("e2", "m1", Some("Atlas"), 40, 70),
        ];
        let (start, end) = window();
        let sheet = compute(&events, &apps(), start, end);
        assert_eq!(sheet.rows.len(), 1);
        let ids: Vec<&str> = sheet.rows[0]
            .citations
            .iter()
            .map(|c| c.id.as_str())
            .collect();
        assert_eq!(ids, ["e1", "e2"]);
        assert_eq!(sheet.rows[0].citations[0].memory_id.as_deref(), Some("m1"));
    }

    #[test]
    fn a_memory_with_no_known_app_is_unknown_not_dropped() {
        let events = vec![event("e1", "gone", Some("Atlas"), 0, 30)];
        let (start, end) = window();
        let sheet = compute(&events, &apps(), start, end);
        assert_eq!(sheet.rows[0].app, UNKNOWN_APP);
    }

    #[test]
    fn durations_read_as_hours_and_minutes() {
        assert_eq!(format_duration(0), "0m");
        assert_eq!(format_duration(45 * MIN), "45m");
        assert_eq!(format_duration(65 * MIN), "1h 05m");
        assert_eq!(format_duration(125 * MIN + 31_000), "2h 06m");
    }
}
