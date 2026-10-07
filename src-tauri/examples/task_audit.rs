//! Audits the task list of a COPY of a profile: where tasks came from, how
//! many repeat each other, and how many are supported by the memory they
//! cite. Prints counts only, never task or memory text, unless `--titles`
//! is given. Refuses the real profile; pass a copy.
//! Usage: cargo run --example task_audit -- --data-dir <profile copy> [--titles]

use fndr_lib::storage::{Store, Task, TaskType};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;

fn arg(name: &str) -> Option<String> {
    let mut args = std::env::args();
    while let Some(current) = args.next() {
        if current == name {
            return args.next();
        }
    }
    None
}

fn words(text: &str) -> BTreeSet<String> {
    const STOP: &[&str] = &[
        "the", "and", "for", "with", "that", "this", "from", "into", "are", "was", "will", "any",
        "all", "has", "have", "not", "its", "their", "your", "you", "about", "after", "before",
        "then", "them", "they", "been", "being", "need", "needed",
    ];
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| word.len() > 2 && !STOP.contains(word))
        .map(str::to_string)
        .collect()
}

fn overlap(a: &BTreeSet<String>, b: &BTreeSet<String>) -> f32 {
    let shared = a.intersection(b).count() as f32;
    let all = a.union(b).count() as f32;
    if all == 0.0 {
        0.0
    } else {
        shared / all
    }
}

fn tally(map: &mut BTreeMap<String, usize>, key: impl Into<String>) {
    *map.entry(key.into()).or_insert(0) += 1;
}

fn type_name(task: &Task) -> &'static str {
    match task.task_type {
        TaskType::Todo => "todo",
        TaskType::Reminder => "reminder",
        TaskType::Followup => "followup",
    }
}

const AI_CHAT_APPS: &[&str] = &["claude", "chatgpt", "codex", "cursor", "gemini", "copilot"];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let data_dir = PathBuf::from(arg("--data-dir").ok_or("--data-dir required")?).canonicalize()?;
    let real = dirs::data_dir().ok_or("no data dir")?.join("com.fndr.app");
    if real
        .canonicalize()
        .is_ok_and(|real| data_dir.starts_with(&real))
    {
        return Err("refusing the real FNDR profile; pass a copy".into());
    }
    let show_titles = std::env::args().any(|current| current == "--titles");

    let store = Store::new(&data_dir)?;
    let runtime = tokio::runtime::Runtime::new()?;
    let report = runtime.block_on(async {
        let tasks = store.list_tasks().await.map_err(|e| e.to_string())?;
        let memories = store.list_all_memories().await.map_err(|e| e.to_string())?;
        let by_id: HashMap<&str, _> = memories.iter().map(|m| (m.id.as_str(), m)).collect();
        let open: Vec<&Task> = tasks
            .iter()
            .filter(|task| !task.is_completed && !task.is_dismissed)
            .collect();

        let (mut status, mut by_type, mut by_source, mut by_day, mut opener) = (
            BTreeMap::new(),
            BTreeMap::new(),
            BTreeMap::new(),
            BTreeMap::new(),
            BTreeMap::new(),
        );
        for task in &tasks {
            tally(
                &mut status,
                if task.is_completed {
                    "completed"
                } else if task.is_dismissed {
                    "dismissed"
                } else {
                    "open"
                },
            );
        }

        let mut per_memory: BTreeMap<&str, usize> = BTreeMap::new();
        let (mut from_ai_chat, mut no_memory, mut memory_gone) = (0usize, 0usize, 0usize);
        let (mut is_first_sentence, mut template_leak, mut names_a_team) = (0usize, 0usize, 0usize);
        let (mut supported, mut weakly_supported, mut unsupported) = (0usize, 0usize, 0usize);
        let (mut memory_hidden, mut has_due) = (0usize, 0usize);
        for task in &open {
            tally(&mut by_type, type_name(task));
            tally(&mut by_source, task.source_app.clone());
            let day = chrono::DateTime::from_timestamp_millis(task.created_at)
                .map(|at| at.with_timezone(&chrono::Local).format("%Y-%m-%d").to_string())
                .unwrap_or_default();
            tally(&mut by_day, day);
            let title_lower = task.title.to_lowercase();
            tally(
                &mut opener,
                title_lower.split_whitespace().next().unwrap_or("").to_string(),
            );
            has_due += usize::from(task.due_date.is_some());
            template_leak += usize::from(
                title_lower.contains(" + ") || title_lower.contains('[') || title_lower.contains("person/team"),
            );
            names_a_team += usize::from(title_lower.contains(" team"));
            if AI_CHAT_APPS
                .iter()
                .any(|app| task.source_app.to_lowercase().contains(app))
            {
                from_ai_chat += 1;
            }

            let Some(memory_id) = task.source_memory_id.as_deref() else {
                no_memory += 1;
                continue;
            };
            *per_memory.entry(memory_id).or_insert(0) += 1;
            let Some(memory) = by_id.get(memory_id) else {
                memory_gone += 1;
                continue;
            };
            memory_hidden +=
                usize::from(fndr_lib::memory_quality::record_low_signal_reason(memory).is_some());
            let evidence = format!(
                "{} {} {} {}",
                memory.window_title, memory.snippet, memory.clean_text, memory.memory_context
            );
            let first = evidence_first_sentence(&memory.snippet);
            is_first_sentence += usize::from(!first.is_empty() && first == title_lower.trim());
            // How much of the task's wording is in the memory it cites?
            let title_words = words(&task.title);
            let evidence_words = words(&evidence);
            let found = title_words.intersection(&evidence_words).count();
            let share = found as f32 / title_words.len().max(1) as f32;
            if share >= 0.8 {
                supported += 1;
            } else if share >= 0.5 {
                weakly_supported += 1;
            } else {
                unsupported += 1;
            }
        }

        // Tasks that say the same thing in different words.
        let word_sets: Vec<BTreeSet<String>> = open.iter().map(|task| words(&task.title)).collect();
        let mut group_of: Vec<usize> = (0..open.len()).collect();
        for i in 0..open.len() {
            for j in (i + 1)..open.len() {
                if overlap(&word_sets[i], &word_sets[j]) >= 0.5 {
                    let (a, b) = (group_of[i], group_of[j]);
                    if a != b {
                        for group in group_of.iter_mut() {
                            if *group == b {
                                *group = a;
                            }
                        }
                    }
                }
            }
        }
        let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for (index, group) in group_of.iter().enumerate() {
            groups.entry(*group).or_default().push(index);
        }
        let repeated: usize = groups.values().filter(|g| g.len() > 1).map(|g| g.len() - 1).sum();
        let largest = groups.values().map(Vec::len).max().unwrap_or(0);

        let mut top_openers: Vec<_> = opener.into_iter().collect();
        top_openers.sort_by(|a, b| b.1.cmp(&a.1));
        top_openers.truncate(12);
        let mut per_memory_counts: BTreeMap<String, usize> = BTreeMap::new();
        for count in per_memory.values() {
            tally(&mut per_memory_counts, format!("{count} tasks"));
        }
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let memories_today = memories
            .iter()
            .filter(|m| {
                chrono::DateTime::from_timestamp_millis(m.timestamp)
                    .is_some_and(|at| at.with_timezone(&chrono::Local).format("%Y-%m-%d").to_string() == today)
            })
            .count();

        let titles = show_titles.then(|| {
            groups
                .values()
                .filter(|g| g.len() > 1)
                .map(|g| g.iter().map(|i| open[*i].title.clone()).collect::<Vec<_>>())
                .collect::<Vec<_>>()
        });

        Ok::<_, String>(json!({
            "all_tasks_ever": tasks.len(),
            "status": status,
            "open": open.len(),
            "open_by_type": by_type,
            "open_by_source": by_source,
            "open_by_day_created": by_day,
            "memories": { "total": memories.len(), "captured_today": memories_today, "cited_by_an_open_task": per_memory.len() },
            "open_tasks_per_cited_memory": per_memory_counts,
            "origin": {
                "from_an_ai_chat_app": from_ai_chat,
                "no_memory_cited": no_memory,
                "cited_memory_deleted": memory_gone,
                "cited_memory_hidden_as_low_signal": memory_hidden,
                "title_is_the_memory_summary_first_sentence": is_first_sentence,
            },
            "wording": {
                "format_placeholder_leaked": template_leak,
                "names_a_team": names_a_team,
                "has_a_due_date": has_due,
                "most_common_first_words": top_openers,
            },
            "support_in_cited_memory": {
                "at_least_80_percent_of_title_words": supported,
                "50_to_80_percent": weakly_supported,
                "under_50_percent": unsupported,
            },
            "repeats": {
                "distinct_after_merging_similar_titles": groups.len(),
                "tasks_that_repeat_another": repeated,
                "largest_group": largest,
                "groups": titles,
            },
        }))
    })?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

fn evidence_first_sentence(snippet: &str) -> String {
    fndr_lib::summariser::sentences::first_sentence(snippet)
        .trim()
        .to_lowercase()
}
