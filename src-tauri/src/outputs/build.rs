//! The documents, composed from evidence. Pure: no store, no clock, no model.
//!
//! Wording comes from summaries that already passed the voice rules
//! (`briefing::quotable`). A line is a past-tense statement of what happened;
//! none addresses a reader or names one.

use super::timesheet::{self, format_duration, Timesheet};
use super::{
    Citation, Evidence, Line, MemoryEvidence, OutputDoc, OutputKind, Section, TaskEvidence,
};
use crate::briefing::quotable;
use crate::storage::ActivityEvent;
use chrono::{Local, TimeZone};
use std::collections::HashSet;

const MAX_DONE: usize = 8;
const MAX_WEEK_DONE: usize = 12;
const MAX_TODAY: usize = 5;
const MAX_BLOCKERS: usize = 4;
const MAX_DECISIONS: usize = 6;
const MAX_TOP_PROJECTS: usize = 5;
const MAX_ERROR_CHARS: usize = 140;

fn local_date(ms: i64, pattern: &str) -> String {
    Local
        .timestamp_millis_opt(ms)
        .earliest()
        .map_or_else(String::new, |at| at.format(pattern).to_string())
}

fn range_label(start_ms: i64, end_ms: i64) -> String {
    let first = local_date(start_ms, "%b %-d");
    // The range ends at the end of its last day; name that day.
    let last = local_date((end_ms - 1).max(start_ms), "%b %-d");
    if first == last {
        first
    } else {
        format!("{first} to {last}")
    }
}

fn new_doc(
    kind: OutputKind,
    title: String,
    start_ms: i64,
    end_ms: i64,
    now_ms: i64,
    sections: Vec<Section>,
) -> OutputDoc {
    OutputDoc {
        id: uuid::Uuid::new_v4().to_string(),
        kind,
        title,
        start_ms,
        end_ms,
        generated_at_ms: now_ms,
        model_polished: false,
        sections,
    }
}

fn project_of(event: &ActivityEvent) -> Option<String> {
    event
        .project
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
}

fn overlaps(event: &ActivityEvent, start_ms: i64, end_ms: i64) -> bool {
    event.end_time >= start_ms && event.start_time <= end_ms
}

fn event_text(event: &ActivityEvent) -> Option<String> {
    if super::has_narrator(&event.summary) {
        return None;
    }
    quotable(&event.summary).or_else(|| quotable(&event.title))
}

/// One cleaned sentence with no trailing period, so sections can add their own.
fn sentence(text: &str) -> Option<String> {
    quotable(text)
}

fn cut_words(text: &str, max_chars: usize) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= max_chars {
        return flat;
    }
    let mut cut: String = flat.chars().take(max_chars).collect();
    if let Some(space) = cut.rfind(' ') {
        cut.truncate(space);
    }
    format!("{}...", cut.trim_end_matches([',', ';', ':']))
}

/// Work done in the range: events first in the order they happened, then the
/// windows no event covers, each told by its newest quotable summary.
fn done_lines(evidence: &Evidence, start_ms: i64, end_ms: i64, limit: usize) -> Vec<Line> {
    let mut events: Vec<&ActivityEvent> = evidence
        .events
        .iter()
        .filter(|event| overlaps(event, start_ms, end_ms))
        .collect();
    events.sort_by_key(|event| (event.start_time, event.id.clone()));

    let covered: HashSet<&str> = events
        .iter()
        .flat_map(|event| {
            std::iter::once(event.memory_id.as_str())
                .chain(event.source_memory_ids.iter().map(String::as_str))
        })
        .collect();

    let mut seen = HashSet::new();
    let mut lines = Vec::new();
    for event in events {
        let Some(text) = event_text(event) else {
            continue;
        };
        if !seen.insert(text.to_lowercase()) {
            continue;
        }
        lines.push(Line {
            text,
            project: project_of(event),
            citations: vec![Citation::event(event)],
        });
    }

    let mut uncovered: Vec<&MemoryEvidence> = evidence
        .memories
        .iter()
        .filter(|memory| memory.timestamp_ms >= start_ms && memory.timestamp_ms <= end_ms)
        .filter(|memory| !covered.contains(memory.id.as_str()))
        .filter(|memory| !crate::tasks::suggest::is_system_surface(&memory.app))
        .collect();
    uncovered.sort_by_key(|memory| std::cmp::Reverse(memory.timestamp_ms));
    let mut windows = HashSet::new();
    let mut from_memories = Vec::new();
    for memory in uncovered {
        let window = (
            memory.app.to_lowercase(),
            memory.window_title.to_lowercase(),
        );
        if !windows.insert(window) {
            continue;
        }
        let Some(text) = quotable(&memory.summary) else {
            continue;
        };
        if !seen.insert(text.to_lowercase()) {
            continue;
        }
        from_memories.push((
            memory.timestamp_ms,
            Line {
                text,
                project: None,
                citations: vec![Citation::memory(&memory.id)],
            },
        ));
    }
    from_memories.sort_by_key(|(at, _)| *at);
    lines.extend(from_memories.into_iter().map(|(_, line)| line));
    lines.truncate(limit);
    lines
}

fn task_citations(task: &TaskEvidence) -> Vec<Citation> {
    task.memory_ids
        .iter()
        .filter(|id| !id.trim().is_empty())
        .map(|id| Citation::memory(id))
        .collect()
}

fn is_overdue(task: &TaskEvidence, now_ms: i64) -> bool {
    task.due_ms.is_some_and(|due| due < now_ms)
}

fn task_line(task: &TaskEvidence, with_due: bool) -> Option<Line> {
    let title = task.title.trim().trim_end_matches('.');
    if title.is_empty() {
        return None;
    }
    let text = match task.due_ms.filter(|_| with_due) {
        Some(due) => format!("{title} (due {})", local_date(due, "%b %-d")),
        None => title.to_string(),
    };
    Some(Line {
        text,
        project: None,
        citations: task_citations(task),
    })
}

fn sorted_open_tasks(tasks: &[TaskEvidence]) -> Vec<&TaskEvidence> {
    let mut open: Vec<&TaskEvidence> = tasks.iter().collect();
    open.sort_by(|a, b| {
        let due = |task: &TaskEvidence| task.due_ms.unwrap_or(i64::MAX);
        due(a)
            .cmp(&due(b))
            .then(b.created_at_ms.cmp(&a.created_at_ms))
    });
    open
}

/// What is next: open tasks that are not overdue, then next steps the events
/// recorded, newest first.
fn next_lines(
    evidence: &Evidence,
    start_ms: i64,
    end_ms: i64,
    now_ms: i64,
    limit: usize,
) -> Vec<Line> {
    let mut seen = HashSet::new();
    let mut lines = Vec::new();
    for task in sorted_open_tasks(&evidence.tasks) {
        if is_overdue(task, now_ms) {
            continue;
        }
        if let Some(line) = task_line(task, true) {
            seen.insert(task.title.trim().trim_end_matches('.').to_lowercase());
            lines.push(line);
        }
    }
    let mut events: Vec<&ActivityEvent> = evidence
        .events
        .iter()
        .filter(|event| overlaps(event, start_ms, end_ms))
        .collect();
    events.sort_by_key(|event| std::cmp::Reverse((event.end_time, event.id.clone())));
    for event in events {
        for step in &event.next_steps {
            let Some(text) = sentence(step) else {
                continue;
            };
            if seen.insert(text.to_lowercase()) {
                lines.push(Line {
                    text,
                    project: project_of(event),
                    citations: vec![Citation::event(event)],
                });
            }
        }
    }
    lines.truncate(limit);
    lines
}

fn resolved(event: &ActivityEvent) -> bool {
    let outcome = event.outcome.to_lowercase();
    ["resolved", "fixed", "success", "completed", "passed"]
        .iter()
        .any(|word| outcome.contains(word))
}

/// Errors an event hit and did not clear, then tasks past their due date.
fn blocker_lines(
    evidence: &Evidence,
    start_ms: i64,
    end_ms: i64,
    now_ms: i64,
    limit: usize,
) -> Vec<Line> {
    let mut events: Vec<&ActivityEvent> = evidence
        .events
        .iter()
        .filter(|event| overlaps(event, start_ms, end_ms) && !resolved(event))
        .collect();
    events.sort_by_key(|event| std::cmp::Reverse((event.end_time, event.id.clone())));
    let mut seen = HashSet::new();
    let mut lines = Vec::new();
    for event in events {
        let Some(error) = event
            .errors
            .iter()
            .map(|error| cut_words(error, MAX_ERROR_CHARS))
            .find(|error| !error.is_empty())
        else {
            continue;
        };
        let place = event_text(event)
            .or_else(|| project_of(event))
            .unwrap_or_else(|| "a work session".to_string());
        let text = format!("Error while working on {place}: {error}");
        if seen.insert(text.to_lowercase()) {
            lines.push(Line {
                text,
                project: project_of(event),
                citations: vec![Citation::event(event)],
            });
        }
    }
    for task in sorted_open_tasks(&evidence.tasks) {
        if !is_overdue(task, now_ms) {
            continue;
        }
        if let Some(mut line) = task_line(task, false) {
            line.text = format!("Overdue: {}", line.text);
            lines.push(line);
        }
    }
    lines.truncate(limit);
    lines
}

pub fn standup(evidence: &Evidence, start_ms: i64, end_ms: i64, now_ms: i64) -> OutputDoc {
    new_doc(
        OutputKind::Standup,
        format!("Standup, {}", range_label(start_ms, end_ms)),
        start_ms,
        end_ms,
        now_ms,
        vec![
            Section {
                heading: "Yesterday".to_string(),
                lines: done_lines(evidence, start_ms, end_ms, MAX_DONE),
            },
            Section {
                heading: "Today".to_string(),
                lines: next_lines(evidence, start_ms, end_ms, now_ms, MAX_TODAY),
            },
            Section {
                heading: "Blockers".to_string(),
                lines: blocker_lines(evidence, start_ms, end_ms, now_ms, MAX_BLOCKERS),
            },
        ],
    )
}

fn decision_lines(evidence: &Evidence, start_ms: i64, end_ms: i64) -> Vec<Line> {
    let mut events: Vec<&ActivityEvent> = evidence
        .events
        .iter()
        .filter(|event| overlaps(event, start_ms, end_ms))
        .collect();
    events.sort_by_key(|event| (event.start_time, event.id.clone()));
    let mut seen = HashSet::new();
    let mut lines = Vec::new();
    for event in events {
        for decision in &event.decisions {
            let Some(text) = sentence(decision) else {
                continue;
            };
            if seen.insert(text.to_lowercase()) {
                lines.push(Line {
                    text,
                    project: project_of(event),
                    citations: vec![Citation::event(event)],
                });
            }
        }
    }
    lines.truncate(MAX_DECISIONS);
    lines
}

fn time_sheet(evidence: &Evidence, start_ms: i64, end_ms: i64) -> Timesheet {
    timesheet::compute(
        &evidence.events,
        &evidence.app_by_memory(),
        start_ms,
        end_ms,
    )
}

pub fn weekly_status(evidence: &Evidence, start_ms: i64, end_ms: i64, now_ms: i64) -> OutputDoc {
    let sheet = time_sheet(evidence, start_ms, end_ms);
    let time_lines = sheet
        .projects
        .iter()
        .take(MAX_TOP_PROJECTS)
        .map(|project| Line {
            text: format!("{}: {}", project.project, format_duration(project.ms)),
            project: Some(project.project.clone()),
            citations: project.citations.clone(),
        })
        .collect();
    let mut open = next_lines(evidence, start_ms, end_ms, now_ms, MAX_TODAY + 3);
    open.truncate(MAX_TODAY + 3);
    new_doc(
        OutputKind::WeeklyStatus,
        format!("Weekly status, {}", range_label(start_ms, end_ms)),
        start_ms,
        end_ms,
        now_ms,
        vec![
            Section {
                heading: "Completed".to_string(),
                lines: done_lines(evidence, start_ms, end_ms, MAX_WEEK_DONE),
            },
            Section {
                heading: "Decisions".to_string(),
                lines: decision_lines(evidence, start_ms, end_ms),
            },
            Section {
                heading: "Time by project".to_string(),
                lines: time_lines,
            },
            Section {
                heading: "Still open".to_string(),
                lines: open,
            },
            Section {
                heading: "Risks".to_string(),
                lines: blocker_lines(evidence, start_ms, end_ms, now_ms, MAX_BLOCKERS + 2),
            },
        ],
    )
}

pub fn timesheet_doc(evidence: &Evidence, start_ms: i64, end_ms: i64, now_ms: i64) -> OutputDoc {
    let sheet = time_sheet(evidence, start_ms, end_ms);
    let by_project = sheet
        .projects
        .iter()
        .map(|project| Line {
            text: format!("{}: {}", project.project, format_duration(project.ms)),
            project: Some(project.project.clone()),
            citations: project.citations.clone(),
        })
        .collect();
    let by_app = sheet
        .rows
        .iter()
        .map(|row| Line {
            text: format!(
                "{} in {}: {}",
                row.project,
                row.app,
                format_duration(row.ms)
            ),
            project: Some(row.project.clone()),
            citations: row.citations.clone(),
        })
        .collect();
    let total = if sheet.total_ms > 0 {
        vec![Line {
            text: format!("Total: {}", format_duration(sheet.total_ms)),
            project: None,
            citations: sheet.total_citations.clone(),
        }]
    } else {
        Vec::new()
    };
    new_doc(
        OutputKind::Timesheet,
        format!("Timesheet, {}", range_label(start_ms, end_ms)),
        start_ms,
        end_ms,
        now_ms,
        vec![
            Section {
                heading: "By project".to_string(),
                lines: by_project,
            },
            Section {
                heading: "By project and app".to_string(),
                lines: by_app,
            },
            Section {
                heading: "Total".to_string(),
                lines: total,
            },
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outputs::build as build_output;

    const MIN: i64 = 60_000;
    const DAY0: i64 = 1_760_000_000_000;
    const END: i64 = DAY0 + 24 * 60 * MIN;

    fn event(
        id: &str,
        memory: &str,
        project: &str,
        start_min: i64,
        end_min: i64,
        summary: &str,
    ) -> ActivityEvent {
        ActivityEvent {
            id: id.to_string(),
            memory_id: memory.to_string(),
            start_time: DAY0 + start_min * MIN,
            end_time: DAY0 + end_min * MIN,
            project: Some(project.to_string()).filter(|name| !name.is_empty()),
            summary: summary.to_string(),
            ..Default::default()
        }
    }

    fn memory(id: &str, minute: i64, app: &str, title: &str, summary: &str) -> MemoryEvidence {
        MemoryEvidence {
            id: id.to_string(),
            timestamp_ms: DAY0 + minute * MIN,
            app: app.to_string(),
            window_title: title.to_string(),
            summary: summary.to_string(),
        }
    }

    fn texts(doc: &OutputDoc, heading: &str) -> Vec<String> {
        doc.sections
            .iter()
            .find(|section| section.heading == heading)
            .map(|section| section.lines.iter().map(|line| line.text.clone()).collect())
            .unwrap_or_default()
    }

    fn evidence() -> Evidence {
        let mut blocked = event(
            "e3",
            "m3",
            "Atlas",
            200,
            230,
            "Debugged the failing export job in the Atlas pipeline",
        );
        blocked.errors =
            vec!["ConnectionResetError: peer closed the socket while uploading".to_string()];
        blocked.next_steps = vec!["Retry the export with a smaller batch size".to_string()];
        let mut decided = event(
            "e2",
            "m2",
            "Birch",
            100,
            150,
            "Wrote the migration plan for the Birch database",
        );
        decided.decisions = vec!["Keep the legacy table until the cutover is verified".to_string()];
        Evidence {
            events: vec![
                event(
                    "e1",
                    "m1",
                    "Atlas",
                    0,
                    60,
                    "Reviewed the Atlas pull request for the export service",
                ),
                decided,
                blocked,
            ],
            memories: vec![
                memory(
                    "m1",
                    10,
                    "Xcode",
                    "Atlas",
                    "Reviewed the Atlas pull request for the export service",
                ),
                memory(
                    "m2",
                    110,
                    "Notion",
                    "Birch plan",
                    "Wrote the migration plan for the Birch database",
                ),
                memory(
                    "m3",
                    210,
                    "Terminal",
                    "export",
                    "Debugged the failing export job in the Atlas pipeline",
                ),
                memory(
                    "m9",
                    300,
                    "Safari",
                    "Release notes",
                    "Read the release notes for the new Tauri version",
                ),
            ],
            tasks: vec![
                TaskEvidence {
                    title: "Send the Birch plan to the platform team".to_string(),
                    due_ms: Some(END + 60 * MIN),
                    created_at_ms: DAY0,
                    memory_ids: vec!["m2".to_string()],
                },
                TaskEvidence {
                    title: "File the Atlas expense report".to_string(),
                    due_ms: Some(DAY0 + 600 * MIN),
                    created_at_ms: DAY0,
                    memory_ids: vec!["m1".to_string()],
                },
            ],
        }
    }

    #[test]
    fn a_standup_has_yesterday_today_and_blockers_each_cited() {
        let doc = build_output(OutputKind::Standup, &evidence(), DAY0, END, END);
        let yesterday = texts(&doc, "Yesterday");
        assert!(yesterday
            .contains(&"Reviewed the Atlas pull request for the export service".to_string()));
        assert!(yesterday.iter().any(|line| line.contains("migration plan")));
        let today = texts(&doc, "Today");
        assert!(today
            .iter()
            .any(|line| line.starts_with("Send the Birch plan")));
        assert!(today.iter().any(|line| line.contains("smaller batch size")));
        let blockers = texts(&doc, "Blockers");
        assert!(blockers
            .iter()
            .any(|line| line.contains("ConnectionResetError")));
        assert!(blockers
            .iter()
            .any(|line| line == "Overdue: File the Atlas expense report"));
        assert!(
            !today.iter().any(|line| line.contains("expense report")),
            "overdue is a blocker, not a plan"
        );
        for section in &doc.sections {
            for line in &section.lines {
                assert!(!line.citations.is_empty(), "uncited line: {}", line.text);
            }
        }
    }

    #[test]
    fn event_lines_cite_the_event_and_open_its_memory() {
        let doc = build_output(OutputKind::Standup, &evidence(), DAY0, END, END);
        let line = doc.sections[0]
            .lines
            .iter()
            .find(|line| line.text.contains("Atlas pull request"))
            .unwrap();
        assert_eq!(line.citations[0].id, "e1");
        assert_eq!(line.citations[0].memory_id.as_deref(), Some("m1"));
        assert_eq!(line.project.as_deref(), Some("Atlas"));
    }

    #[test]
    fn a_memory_no_event_covers_still_makes_a_line_cited_to_the_memory() {
        let doc = build_output(OutputKind::Standup, &evidence(), DAY0, END, END);
        let line = doc.sections[0]
            .lines
            .iter()
            .find(|line| line.text.contains("release notes"))
            .expect("the uncovered Safari capture is told");
        assert_eq!(line.citations[0].kind, crate::outputs::CitationKind::Memory);
        assert_eq!(line.citations[0].id, "m9");
    }

    #[test]
    fn a_task_with_no_source_memory_is_dropped_not_quoted() {
        let mut ev = evidence();
        ev.tasks.push(TaskEvidence {
            title: "Invented follow up with nobody".to_string(),
            due_ms: None,
            created_at_ms: DAY0,
            memory_ids: Vec::new(),
        });
        let doc = build_output(OutputKind::Standup, &ev, DAY0, END, END);
        assert!(!texts(&doc, "Today")
            .iter()
            .any(|line| line.contains("Invented")));
    }

    #[test]
    fn nothing_recorded_gives_empty_sections_not_made_up_lines() {
        let doc = build_output(OutputKind::Standup, &Evidence::default(), DAY0, END, END);
        assert_eq!(doc.line_count(), 0);
        assert_eq!(doc.sections.len(), 3);
    }

    #[test]
    fn events_outside_the_range_are_left_out() {
        let doc = build_output(
            OutputKind::Standup,
            &evidence(),
            END + MIN,
            END + 60 * MIN,
            END + 60 * MIN,
        );
        assert!(texts(&doc, "Yesterday").is_empty());
    }

    #[test]
    fn a_resolved_event_is_not_a_blocker() {
        let mut ev = evidence();
        ev.events[2].outcome = "Fixed after retrying".to_string();
        let doc = build_output(OutputKind::Standup, &ev, DAY0, END, END);
        assert!(!texts(&doc, "Blockers")
            .iter()
            .any(|line| line.contains("ConnectionResetError")));
    }

    #[test]
    fn repeated_summaries_make_one_line() {
        let mut ev = evidence();
        let mut again = ev.events[0].clone();
        again.id = "e1b".to_string();
        again.start_time += 90 * MIN;
        again.end_time += 90 * MIN;
        ev.events.push(again);
        let doc = build_output(OutputKind::Standup, &ev, DAY0, END, END);
        let count = texts(&doc, "Yesterday")
            .iter()
            .filter(|line| line.contains("Atlas pull request"))
            .count();
        assert_eq!(count, 1);
    }

    #[test]
    fn a_placeholder_or_narrated_summary_is_not_quoted() {
        let ev = Evidence {
            events: vec![event(
                "e1",
                "m1",
                "Atlas",
                0,
                30,
                "The user is looking at a window",
            )],
            ..Default::default()
        };
        let doc = build_output(OutputKind::Standup, &ev, DAY0, END, END);
        assert_eq!(doc.line_count(), 0);
    }

    #[test]
    fn the_weekly_status_adds_decisions_time_and_risks() {
        let doc = build_output(OutputKind::WeeklyStatus, &evidence(), DAY0, END, END);
        assert!(doc.title.starts_with("Weekly status, "));
        assert!(texts(&doc, "Decisions")[0].contains("legacy table"));
        let time = texts(&doc, "Time by project");
        assert_eq!(time.len(), 2);
        assert_eq!(time[0], "Atlas: 1h 30m");
        assert_eq!(time[1], "Birch: 50m");
        assert!(texts(&doc, "Risks")
            .iter()
            .any(|line| line.contains("ConnectionResetError")));
        assert!(texts(&doc, "Still open")
            .iter()
            .any(|line| line.starts_with("Send the Birch plan")));
    }

    #[test]
    fn the_timesheet_lists_project_app_rows_and_a_cited_total() {
        let doc = build_output(OutputKind::Timesheet, &evidence(), DAY0, END, END);
        assert_eq!(texts(&doc, "By project"), ["Atlas: 1h 30m", "Birch: 50m"]);
        let by_app = texts(&doc, "By project and app");
        assert!(by_app.contains(&"Atlas in Xcode: 1h 00m".to_string()));
        assert!(by_app.contains(&"Atlas in Terminal: 30m".to_string()));
        assert!(by_app.contains(&"Birch in Notion: 50m".to_string()));
        assert_eq!(texts(&doc, "Total"), ["Total: 2h 20m"]);
        let total = &doc.sections[2].lines[0];
        assert_eq!(total.citations.len(), 3);
    }

    #[test]
    fn a_title_names_the_day_or_the_span() {
        let midnight = Local
            .timestamp_millis_opt(DAY0)
            .unwrap()
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .and_then(|naive| Local.from_local_datetime(&naive).earliest())
            .unwrap()
            .timestamp_millis();
        let day_ms = 24 * 60 * MIN;
        let day = build_output(
            OutputKind::Standup,
            &Evidence::default(),
            midnight,
            midnight + day_ms - 1,
            END,
        );
        assert!(!day.title.contains(" to "));
        let week = build_output(
            OutputKind::Timesheet,
            &Evidence::default(),
            midnight,
            midnight + 7 * day_ms - 1,
            END,
        );
        assert!(week.title.contains(" to "));
    }
}
