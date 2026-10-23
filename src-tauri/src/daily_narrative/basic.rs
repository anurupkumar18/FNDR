//! The basic summary: a short paragraph per theme, composed in code from the
//! fields the model would have been shown. Nothing here is generated, so it
//! cannot say more than the captures do. Used when no model is available,
//! when too little happened to ask one, and when no model sentence passes
//! `check::verify`.

use super::{
    date_text, duration_text, item_id, DailyNarrative, Group, NarrativeItem, NarrativeParagraph,
    Package, Todo, TodoKind,
};
use once_cell::sync::Lazy;
use regex::Regex;

const MAX_THEMES: usize = 5;
const MAX_SENTENCES: usize = 4;
const MAX_ACTION_ITEMS: usize = 6;
const MAX_REMINDERS: usize = 4;

static TIME_SIGNAL: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)\b(?:due|deadline|today|tomorrow|tonight|next week|by (?:mon|tue|wed|thu|fri|sat|sun|end of|eod|noon|\d)|(?:mon|tues|wednes|thurs|fri|satur|sun)day|\d{1,2}(?::\d{2})?\s?(?:am|pm)|\d{1,2}/\d{1,2})\b",
    )
    .expect("valid time regex")
});

fn join_names(names: &[String], take: usize) -> String {
    let shown: Vec<&str> = names.iter().take(take).map(String::as_str).collect();
    match shown.as_slice() {
        [] => "an app".to_string(),
        [one] => (*one).to_string(),
        [first, second] => format!("{first} and {second}"),
        _ => shown.join(", "),
    }
}

fn sentence(text: &str) -> String {
    let trimmed = text.trim().trim_end_matches(['.', ' ']);
    format!("{trimmed}.")
}

fn group_paragraph(group: &Group) -> String {
    let place = join_names(&group.apps, 2);
    let on = if group.project.is_empty() {
        String::new()
    } else {
        format!(" on {}", group.project)
    };
    let when = group.time_text();
    let mut sentences = vec![if group.span_minutes() >= 10 {
        format!(
            "Spent {} in {place}{on} ({when}).",
            duration_text(group.span_minutes())
        )
    } else if when.contains(" to ") {
        format!("Used {place}{on} from {when}.")
    } else {
        format!("Used {place}{on} at {when}.")
    }];
    for note in group.notes.iter().take(2) {
        sentences.push(sentence(note));
    }
    if let Some(decision) = group.decisions.first() {
        sentences.push(sentence(&format!("Decision noted: {decision}")));
    }
    if let Some(error) = group.errors.first() {
        sentences.push(sentence(&format!("Error seen: {error}")));
    }
    sentences.truncate(MAX_SENTENCES);
    sentences.join(" ")
}

/// One short paragraph per theme, the strongest five, in time order.
pub fn paragraphs(package: &Package) -> Vec<NarrativeParagraph> {
    if package.groups.is_empty() {
        if package.captures_total == 0 {
            return Vec::new();
        }
        let count = package.captures_total;
        return vec![NarrativeParagraph {
            text: format!(
                "{count} {} recorded, none with enough detail to summarize.",
                if count == 1 {
                    "capture was"
                } else {
                    "captures were"
                }
            ),
            memory_ids: Vec::new(),
        }];
    }
    let mut ranked: Vec<&Group> = package.groups.iter().collect();
    ranked.sort_by_key(|g| {
        std::cmp::Reverse(
            g.captures.min(12) + 3 * g.open_todos.len() + 2 * g.decisions.len() + g.notes.len(),
        )
    });
    ranked.truncate(MAX_THEMES);
    ranked.sort_by_key(|g| g.start_ms);
    ranked
        .into_iter()
        .map(|group| NarrativeParagraph {
            text: group_paragraph(group),
            memory_ids: group.memory_ids.iter().take(3).cloned().collect(),
        })
        .collect()
}

fn capitalised(text: &str) -> String {
    let trimmed = text.trim().trim_end_matches(['.', ' ']);
    let mut chars = trimmed.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn has_time(todo: &Todo) -> bool {
    todo.due_ms.is_some() || todo.kind == TodoKind::Reminder || TIME_SIGNAL.is_match(&todo.text)
}

/// The open tasks of the day's groups, as they were extracted: action items
/// without a time, and reminders with one.
pub fn items(package: &Package) -> (Vec<NarrativeItem>, Vec<NarrativeItem>) {
    let mut seen = std::collections::HashSet::new();
    let mut actions = Vec::new();
    let mut reminders = Vec::new();
    for todo in package.groups.iter().flat_map(|g| g.open_todos.iter()) {
        let text = capitalised(&todo.text);
        if text.is_empty() || !seen.insert(text.to_lowercase()) {
            continue;
        }
        let item = |kind: &str| NarrativeItem {
            id: item_id(kind, &text),
            text: text.clone(),
            memory_ids: vec![todo.memory_id.clone()],
            task_id: todo.task_id.clone(),
            due_label: todo.due_ms.map(date_text),
        };
        if has_time(todo) {
            if reminders.len() < MAX_REMINDERS {
                reminders.push(item("reminder"));
            }
        } else if actions.len() < MAX_ACTION_ITEMS {
            actions.push(item("action_item"));
        }
    }
    (actions, reminders)
}

pub fn compose(
    package: &Package,
    date: &str,
    reason: &str,
    backend: &str,
    items_dropped: usize,
    model_ms: u64,
) -> DailyNarrative {
    let (action_items, reminders) = items(package);
    DailyNarrative {
        date: date.to_string(),
        mode: "basic".into(),
        backend: backend.to_string(),
        basic_reason: Some(reason.to_string()),
        paragraphs: paragraphs(package),
        action_items,
        reminders,
        groups_used: package.groups.len(),
        items_dropped,
        model_ms,
    }
}

#[cfg(test)]
mod tests {
    use super::super::fixtures::*;
    use super::super::{build_package, clock_text, compose as compose_day};
    use super::*;

    #[test]
    fn one_short_paragraph_per_theme_not_one_bullet_per_capture() {
        let package = build_package(&sam_day(), "Wednesday, 2026-10-21");
        let basic = paragraphs(&package);
        assert_eq!(basic.len(), 3, "five captures, three themes");
        let first = &basic[0].text;
        assert!(
            first.starts_with("Spent about 1 hour 30 minutes in Xcode on Payments ("),
            "{first}"
        );
        assert!(first.contains(&clock_text(at(14, 10))));
        assert!(
            first.contains("Decision noted: Cap payment retries at 5 attempts."),
            "{first}"
        );
        assert!(first.matches(". ").count() <= MAX_SENTENCES);
        assert!(basic.iter().all(|p| !p.memory_ids.is_empty()));
    }

    #[test]
    fn the_basic_text_has_no_narrator_and_no_capture_counts() {
        let package = build_package(&sam_day(), "day");
        for paragraph in paragraphs(&package) {
            let lower = paragraph.text.to_lowercase();
            assert!(
                !lower.contains("you ") && !lower.contains("captures"),
                "{lower}"
            );
        }
    }

    #[test]
    fn open_tasks_become_items_and_the_timed_ones_become_reminders() {
        let mut captures = sam_day();
        captures[2].todos = vec![{
            let mut t = todo("m3", "Ship the retry fix by Friday");
            t.task_id = None;
            t
        }];
        let package = build_package(&captures, "day");
        let (actions, reminders) = items(&package);
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].text, "Send the Q3 budget draft to Priya Shah");
        assert_eq!(actions[0].memory_ids, ["m4"]);
        assert_eq!(reminders.len(), 1);
        assert_eq!(reminders[0].text, "Ship the retry fix by Friday");
    }

    #[test]
    fn an_empty_day_is_empty_and_never_asks_the_model() {
        let package = build_package(&[], "day");
        let result =
            futures::executor::block_on(compose_day(&package, "2026-10-21", true, |_| async {
                panic!("the model must not be asked about an empty day");
                #[allow(unreachable_code)]
                (String::new(), "none")
            }));
        assert_eq!(result.mode, "empty");
        assert!(result.paragraphs.is_empty());
    }

    #[test]
    fn a_single_app_day_with_one_capture_is_basic_without_a_model_call() {
        let mut only = capture("a", 17, 57, "Dia", "Personal 2: Bot Avatars");
        only.summary = Some("Read notes on bot avatar designs".into());
        let package = build_package(&[only], "day");
        let result =
            futures::executor::block_on(compose_day(&package, "2026-10-21", true, |_| async {
                panic!("one capture is not worth a model call");
                #[allow(unreachable_code)]
                (String::new(), "none")
            }));
        assert_eq!(result.mode, "basic");
        assert_eq!(result.paragraphs.len(), 1);
        assert!(
            result.paragraphs[0].text.starts_with("Used Dia at "),
            "{}",
            result.paragraphs[0].text
        );
    }

    #[test]
    fn a_single_app_day_with_enough_captures_asks_once_and_keeps_a_checked_paragraph() {
        let mut captures = Vec::new();
        for (i, note) in [
            "Read the compost guide",
            "Compared two compost bins",
            "Saved the bin list",
        ]
        .iter()
        .enumerate()
        {
            let mut c = capture(
                &format!("m{i}"),
                10,
                i as u32 * 20,
                "Safari",
                "Compost guide",
            );
            c.summary = Some((*note).into());
            captures.push(c);
        }
        let package = build_package(&captures, "Wednesday, 2026-10-21");
        let calls = std::cell::Cell::new(0);
        let result = futures::executor::block_on(compose_day(
            &package,
            "2026-10-21",
            false,
            |_| {
                calls.set(calls.get() + 1);
                async {
                    (
                    r#"{"paragraphs":[{"text":"Spent the morning in Safari reading about compost and comparing two compost bins.","evidence":["g1"]}]}"#.to_string(),
                    "local",
                )
                }
            },
        ));
        assert_eq!(calls.get(), 1);
        assert_eq!(result.mode, "narrative", "{:?}", result.basic_reason);
        assert_eq!(result.backend, "local");
    }

    #[test]
    fn a_model_that_fails_every_check_falls_back_to_basic_and_says_why() {
        let package = build_package(&sam_day(), "Wednesday, 2026-10-21");
        let result = futures::executor::block_on(compose_day(
            &package,
            "2026-10-21",
            false,
            |_| async {
                (
                r#"{"paragraphs":[{"text":"Spent the afternoon with Marcus Webb planning a launch in Figma.","evidence":["g1"]}]}"#.to_string(),
                "local",
            )
            },
        ));
        assert_eq!(result.mode, "basic");
        assert!(result
            .basic_reason
            .as_deref()
            .unwrap()
            .contains("did not pass the checks"));
        assert_eq!(result.items_dropped, 1);
        assert_eq!(result.paragraphs.len(), 3);
    }

    #[test]
    fn apple_output_that_fails_is_retried_once_on_the_local_model() {
        let package = build_package(&sam_day(), "Wednesday, 2026-10-21");
        let seen = std::cell::RefCell::new(Vec::new());
        let result = futures::executor::block_on(compose_day(
            &package,
            "2026-10-21",
            true,
            |local| {
                seen.borrow_mut().push(local);
                async move {
                    if local {
                        (
                        r#"{"paragraphs":[{"text":"Spent the afternoon in Xcode on the Payments project.","evidence":["g1"]}]}"#.to_string(),
                        "local",
                    )
                    } else {
                        ("not json".to_string(), "apple")
                    }
                }
            },
        ));
        assert_eq!(*seen.borrow(), [false, true]);
        assert_eq!(result.mode, "narrative");
        assert_eq!(result.backend, "local");
    }

    #[test]
    fn a_silent_model_gives_the_basic_summary_without_blaming_the_checks() {
        let package = build_package(&sam_day(), "day");
        let result =
            futures::executor::block_on(compose_day(&package, "2026-10-21", false, |_| async {
                (String::new(), "none")
            }));
        assert_eq!(result.mode, "basic");
        assert_eq!(result.backend, "none");
        assert!(result
            .basic_reason
            .as_deref()
            .unwrap()
            .contains("did not answer"));
    }

    #[test]
    fn when_no_model_item_survives_the_open_tasks_are_listed_from_the_extracted_fields() {
        let package = build_package(&sam_day(), "Wednesday, 2026-10-21");
        let result = futures::executor::block_on(compose_day(
            &package,
            "2026-10-21",
            false,
            |_| async {
                (
                r#"{"paragraphs":[{"text":"Spent the afternoon in Xcode on the Payments project.","evidence":["g1"]}],"action_items":[{"text":"Call Marcus Webb tomorrow","evidence":["g2"]}]}"#.to_string(),
                "local",
            )
            },
        ));
        assert_eq!(result.mode, "narrative");
        assert_eq!(result.action_items.len(), 1);
        assert_eq!(
            result.action_items[0].text,
            "Send the Q3 budget draft to Priya Shah"
        );
    }
}
