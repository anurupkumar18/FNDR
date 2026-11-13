//! The case file: what an investigation returns. Every line is a field
//! already stored on a memory, or one of the person's own to-dos, with the
//! id it came from. Nothing is reworded and no model is read (ADR 039).

use super::agent::{Case, CaseMemory, StepEvent};
use super::done::done_clause;
use crate::workset::{WorkItem, MAX_ITEMS};
use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Found,
    NothingFound,
    NeedsTopic,
    Private,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    pub memories: u32,
    /// Apps the memories came from, most memories first.
    pub apps: Vec<String>,
    pub first_ms: i64,
    pub last_ms: i64,
    pub ms: u64,
}

/// One memory in the order things happened.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub memory_id: String,
    pub at_ms: i64,
    pub app: String,
    pub title: String,
    pub line: String,
}

/// A decision or a problem, as a memory recorded it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    pub text: String,
    pub at_ms: i64,
    pub memory_id: String,
    pub app: String,
    /// Other memories that record the same line.
    pub also: u32,
}

/// A next step no later memory records as done, or one of the person's own
/// open to-dos.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenItem {
    pub text: String,
    pub at_ms: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_id: Option<String>,
    pub app: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub todo_id: Option<String>,
}

/// A next step with the later clause that records it as done. FNDR shows
/// the clause; it does not say the step is done.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DoneItem {
    pub text: String,
    pub at_ms: i64,
    pub memory_id: String,
    pub clause: String,
    pub done_at_ms: i64,
    pub done_memory_id: String,
    pub done_app: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaseFile {
    pub goal: String,
    pub outcome: Outcome,
    pub title: String,
    pub stats: Stats,
    pub timeline: Vec<Entry>,
    pub decisions: Vec<Finding>,
    pub open: Vec<OpenItem>,
    pub done: Vec<DoneItem>,
    pub problems: Vec<Finding>,
    pub places: Vec<WorkItem>,
    pub steps: Vec<StepEvent>,
}

impl CaseFile {
    /// A file with no findings, for a run that never read a memory.
    pub fn empty(goal: &str, outcome: Outcome) -> Self {
        Self {
            goal: goal.to_string(),
            outcome,
            title: String::new(),
            stats: Stats {
                memories: 0,
                apps: Vec::new(),
                first_ms: 0,
                last_ms: 0,
                ms: 0,
            },
            timeline: Vec::new(),
            decisions: Vec::new(),
            open: Vec::new(),
            done: Vec::new(),
            problems: Vec::new(),
            places: Vec::new(),
            steps: Vec::new(),
        }
    }

    pub fn line_count(&self) -> u32 {
        (self.timeline.len()
            + self.decisions.len()
            + self.open.len()
            + self.done.len()
            + self.problems.len()) as u32
    }
}

/// Two lines are the same line when their words are.
fn fold_key(text: &str) -> String {
    text.split(|ch: char| !ch.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect::<Vec<_>>()
        .join(" ")
}

/// Folds the same line across memories. `keep_latest` cites the newest
/// memory that records it (a next step was still open then); otherwise the
/// oldest (when a decision was first recorded).
fn fold<'a>(
    memories: &'a [CaseMemory],
    lines: impl Fn(&'a CaseMemory) -> &'a [String],
    keep_latest: bool,
) -> Vec<(String, &'a CaseMemory, u32)> {
    let mut order: Vec<String> = Vec::new();
    let mut folded: HashMap<String, (String, &CaseMemory, u32)> = HashMap::new();
    for memory in memories {
        for line in lines(memory) {
            let text = line.trim();
            let key = fold_key(text);
            if key.is_empty() {
                continue;
            }
            match folded.get_mut(&key) {
                Some(entry) => {
                    if entry.1.id != memory.id {
                        entry.2 += 1;
                        if keep_latest {
                            entry.0 = text.to_string();
                            entry.1 = memory;
                        }
                    }
                }
                None => {
                    order.push(key.clone());
                    folded.insert(key, (text.to_string(), memory, 0));
                }
            }
        }
    }
    order
        .into_iter()
        .filter_map(|key| folded.remove(&key))
        .collect()
}

fn findings<'a>(
    memories: &'a [CaseMemory],
    lines: impl Fn(&'a CaseMemory) -> &'a [String],
) -> Vec<Finding> {
    fold(memories, lines, false)
        .into_iter()
        .map(|(text, memory, also)| Finding {
            text,
            at_ms: memory.at_ms,
            memory_id: memory.id.clone(),
            app: memory.app.clone(),
            also,
        })
        .collect()
}

/// The first later memory with a clause that records `step` as done.
fn recorded_done<'a>(
    step: &str,
    after: &CaseMemory,
    memories: &'a [CaseMemory],
) -> Option<(&'a CaseMemory, String)> {
    memories
        .iter()
        .filter(|memory| memory.at_ms > after.at_ms && memory.id != after.id)
        .find_map(|memory| {
            std::iter::once(&memory.line)
                .chain(memory.decisions.iter())
                .find_map(|text| done_clause(step, text))
                .map(|clause| (memory, clause))
        })
}

fn places(memories: &[CaseMemory]) -> Vec<WorkItem> {
    let mut by_key: HashMap<&str, &WorkItem> = HashMap::new();
    for memory in memories {
        if let Some((key, item)) = &memory.place {
            let better = by_key.get(key.as_str()).is_none_or(|held| {
                (item.reopen_rank, item.captured_at) > (held.reopen_rank, held.captured_at)
            });
            if better {
                by_key.insert(key, item);
            }
        }
    }
    let mut items: Vec<WorkItem> = by_key.into_values().cloned().collect();
    items.sort_by(|a, b| {
        b.reopen_rank
            .cmp(&a.reopen_rank)
            .then(b.captured_at.cmp(&a.captured_at))
            .then(a.memory_id.cmp(&b.memory_id))
    });
    items.truncate(MAX_ITEMS);
    items
}

pub fn compose(case: &Case, steps: &[StepEvent], ms: u64) -> CaseFile {
    let mut memories = case.memories.clone();
    memories.sort_by(|a, b| a.at_ms.cmp(&b.at_ms).then_with(|| a.id.cmp(&b.id)));

    let mut app_counts: Vec<(String, u32)> = Vec::new();
    for memory in &memories {
        match app_counts.iter_mut().find(|(app, _)| *app == memory.app) {
            Some(entry) => entry.1 += 1,
            None => app_counts.push((memory.app.clone(), 1)),
        }
    }
    app_counts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    let timeline = memories
        .iter()
        .filter(|memory| !memory.line.trim().is_empty())
        .map(|memory| Entry {
            memory_id: memory.id.clone(),
            at_ms: memory.at_ms,
            app: memory.app.clone(),
            title: memory.title.clone(),
            line: memory.line.trim().to_string(),
        })
        .collect();

    // The person's own to-dos come first; a next step that says the same as
    // one of them is not listed twice.
    let mut open: Vec<OpenItem> = case
        .todos
        .iter()
        .map(|todo| {
            let source = todo
                .memory_ids
                .iter()
                .find_map(|id| memories.iter().find(|memory| &memory.id == id));
            OpenItem {
                text: todo.title.trim().to_string(),
                at_ms: source.map_or(0, |memory| memory.at_ms),
                memory_id: source.map(|memory| memory.id.clone()),
                app: source.map(|memory| memory.app.clone()).unwrap_or_default(),
                todo_id: Some(todo.id.clone()),
            }
        })
        .collect();
    let todo_keys: Vec<String> = open.iter().map(|item| fold_key(&item.text)).collect();

    let mut done = Vec::new();
    let mut steps_open = Vec::new();
    for (text, memory, _) in fold(&memories, |memory| &memory.next_steps, true) {
        if todo_keys.contains(&fold_key(&text)) {
            continue;
        }
        match recorded_done(&text, memory, &memories) {
            Some((later, clause)) => done.push(DoneItem {
                text,
                at_ms: memory.at_ms,
                memory_id: memory.id.clone(),
                clause,
                done_at_ms: later.at_ms,
                done_memory_id: later.id.clone(),
                done_app: later.app.clone(),
            }),
            None => steps_open.push(OpenItem {
                text,
                at_ms: memory.at_ms,
                memory_id: Some(memory.id.clone()),
                app: memory.app.clone(),
                todo_id: None,
            }),
        }
    }
    // Newest first: the step recorded last is the likeliest to still stand.
    steps_open.sort_by(|a, b| b.at_ms.cmp(&a.at_ms).then_with(|| a.text.cmp(&b.text)));
    open.extend(steps_open);
    done.sort_by(|a, b| {
        a.done_at_ms
            .cmp(&b.done_at_ms)
            .then_with(|| a.text.cmp(&b.text))
    });

    CaseFile {
        goal: case.plan.goal.clone(),
        outcome: if memories.is_empty() {
            Outcome::NothingFound
        } else {
            Outcome::Found
        },
        title: case
            .thread
            .as_ref()
            .map(|thread| thread.title.trim().to_string())
            .filter(|title| !title.is_empty())
            .unwrap_or_else(|| case.plan.query()),
        stats: Stats {
            memories: memories.len() as u32,
            apps: app_counts.into_iter().map(|(app, _)| app).collect(),
            first_ms: memories.first().map_or(0, |memory| memory.at_ms),
            last_ms: memories.last().map_or(0, |memory| memory.at_ms),
            ms,
        },
        timeline,
        decisions: findings(&memories, |memory| &memory.decisions),
        open,
        done,
        problems: findings(&memories, |memory| &memory.problems),
        places: places(&memories),
        steps: steps.to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::investigate::agent::Todo;
    use crate::investigate::plan::plan;
    use crate::workset::ItemKind;

    fn memory(id: &str, at_ms: i64, app: &str, line: &str) -> CaseMemory {
        CaseMemory {
            id: id.to_string(),
            at_ms,
            app: app.to_string(),
            title: format!("Title {id}"),
            line: line.to_string(),
            ..Default::default()
        }
    }

    fn place(memory_id: &str, key: &str, rank: u8, at: i64) -> Option<(String, WorkItem)> {
        Some((
            key.to_string(),
            WorkItem {
                memory_id: memory_id.to_string(),
                label: key.to_string(),
                kind: ItemKind::Url,
                reopen_rank: rank,
                app_name: "Chrome".to_string(),
                host: None,
                page: None,
                captured_at: at,
            },
        ))
    }

    fn case_with(memories: Vec<CaseMemory>) -> Case {
        let mut case = Case::new(plan("the launch").unwrap());
        case.memories = memories;
        case
    }

    #[test]
    fn the_timeline_is_in_the_order_things_happened() {
        let case = case_with(vec![
            memory("late", 300, "Slack", "Closed the ticket."),
            memory("early", 100, "Chrome", "Drafted the PRD."),
            memory("blank", 200, "Chrome", "  "),
        ]);
        let file = compose(&case, &[], 7);
        let ids: Vec<&str> = file
            .timeline
            .iter()
            .map(|entry| entry.memory_id.as_str())
            .collect();
        assert_eq!(ids, vec!["early", "late"]);
        assert_eq!(file.stats.memories, 3);
        assert_eq!(file.stats.apps, vec!["Chrome", "Slack"]);
        assert_eq!(
            (file.stats.first_ms, file.stats.last_ms, file.stats.ms),
            (100, 300, 7)
        );
        assert_eq!(file.title, "launch");
    }

    #[test]
    fn a_decision_recorded_twice_is_one_line_citing_the_first_memory() {
        let mut first = memory("a", 100, "Docs", "Drafted.");
        first.decisions = vec!["Keep SMS out of v1".to_string()];
        let mut second = memory("b", 200, "Slack", "Agreed.");
        second.decisions = vec![
            "keep SMS out of v1.".to_string(),
            "Roll out to 10%".to_string(),
        ];
        let file = compose(&case_with(vec![second, first]), &[], 0);
        assert_eq!(file.decisions.len(), 2);
        assert_eq!(file.decisions[0].text, "Keep SMS out of v1");
        assert_eq!(file.decisions[0].memory_id, "a");
        assert_eq!(file.decisions[0].also, 1);
        assert_eq!(file.decisions[1].memory_id, "b");
    }

    #[test]
    fn a_step_a_later_memory_records_as_done_moves_out_of_open() {
        let mut v3 = memory("v3", 100, "Notion", "Updated the checklist v3.");
        v3.next_steps = vec![
            "Close LL-1482".to_string(),
            "Book the go/no-go meeting".to_string(),
        ];
        let mut v4 = memory(
            "v4",
            200,
            "Notion",
            "Updated the checklist v4: the go/no-go meeting is now booked, while LL-1482 is still open.",
        );
        v4.next_steps = vec![
            "Close LL-1482".to_string(),
            "Confirm the email schedule".to_string(),
        ];
        let live = memory(
            "live",
            300,
            "Slack",
            "Rafael O. confirmed the LL-1482 fix is live in production and closed the ticket.",
        );
        let file = compose(&case_with(vec![v3, v4, live]), &[], 0);

        let done: Vec<(&str, &str, &str)> = file
            .done
            .iter()
            .map(|item| {
                (
                    item.text.as_str(),
                    item.memory_id.as_str(),
                    item.done_memory_id.as_str(),
                )
            })
            .collect();
        assert_eq!(
            done,
            vec![
                ("Book the go/no-go meeting", "v3", "v4"),
                ("Close LL-1482", "v4", "live")
            ]
        );
        assert_eq!(file.done[0].clause, "the go/no-go meeting is now booked");
        let open: Vec<&str> = file.open.iter().map(|item| item.text.as_str()).collect();
        assert_eq!(open, vec!["Confirm the email schedule"]);
    }

    #[test]
    fn a_step_is_never_marked_by_the_memory_that_lists_it_or_an_earlier_one() {
        let earlier = memory("earlier", 50, "Slack", "The go/no-go meeting is booked.");
        let mut same = memory("same", 100, "Slack", "Booked the go/no-go meeting.");
        same.next_steps = vec!["Book the go/no-go meeting".to_string()];
        let file = compose(&case_with(vec![earlier, same]), &[], 0);
        assert!(file.done.is_empty());
        assert_eq!(file.open.len(), 1);
    }

    #[test]
    fn todos_come_first_and_a_step_that_repeats_one_is_dropped() {
        let mut a = memory("a", 100, "Docs", "Drafted.");
        a.next_steps = vec!["Send the deck".to_string(), "Rehearse".to_string()];
        let mut case = case_with(vec![a]);
        case.todos = vec![
            Todo {
                id: "t1".to_string(),
                title: "Send the deck".to_string(),
                memory_ids: vec!["a".to_string()],
            },
            Todo {
                id: "t2".to_string(),
                title: "Plan the launch party".to_string(),
                memory_ids: Vec::new(),
            },
        ];
        let file = compose(&case, &[], 0);
        let open: Vec<(&str, Option<&str>, Option<&str>)> = file
            .open
            .iter()
            .map(|item| {
                (
                    item.text.as_str(),
                    item.todo_id.as_deref(),
                    item.memory_id.as_deref(),
                )
            })
            .collect();
        assert_eq!(
            open,
            vec![
                ("Send the deck", Some("t1"), Some("a")),
                ("Plan the launch party", Some("t2"), None),
                ("Rehearse", None, Some("a")),
            ]
        );
        // Every line names where it came from.
        assert!(file
            .open
            .iter()
            .all(|item| item.memory_id.is_some() || item.todo_id.is_some()));
    }

    #[test]
    fn places_are_one_per_target_most_precise_first_and_capped() {
        let mut memories = Vec::new();
        for index in 0..8 {
            let mut item = memory(&format!("m{index}"), index, "Chrome", "Looked.");
            item.place = place(&item.id, &format!("url:{index}"), (index % 3) as u8, index);
            memories.push(item);
        }
        // The same target seen again later, more precisely.
        let mut again = memory("again", 99, "Chrome", "Looked again.");
        again.place = place("again", "url:0", 5, 99);
        memories.push(again);

        let file = compose(&case_with(memories), &[], 0);
        assert_eq!(file.places.len(), MAX_ITEMS);
        assert_eq!(file.places[0].memory_id, "again");
        assert!(file
            .places
            .windows(2)
            .all(|pair| pair[0].reopen_rank >= pair[1].reopen_rank));
        assert!(!file.places.iter().any(|item| item.memory_id == "m0"));
    }

    #[test]
    fn a_case_with_no_memories_is_nothing_found_with_no_lines() {
        let file = compose(&case_with(Vec::new()), &[], 0);
        assert_eq!(file.outcome, Outcome::NothingFound);
        assert_eq!(file.line_count(), 0);
        assert!(file.places.is_empty());
    }
}
