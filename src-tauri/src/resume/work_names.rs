//! What a piece of work is when it does not live in a code repository
//! (ADR 031). A window title carries the name of the work beside the name of
//! the place: "Smart Reminders PRD - Google Docs", "#smart-reminders-launch -
//! Lumen Ledger - Slack". A phrase or an identifier that two or more windows
//! share is a name; windows that go by the same names are one thread.
//!
//! Nothing here reads a model's output. Names come from window titles only,
//! so every grouping can be shown to the person as the words it rests on. A
//! window with no name in its title joins a thread only when its on-screen
//! text uses two of that thread's names and no other thread's.

use super::thread_key;
use crate::storage::MemoryRecord;
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// Threads are named over this much history whatever window a surface shows,
/// so Home, work sets and the digest agree on a thread's key.
pub(crate) const NAMING_WINDOW_MS: i64 = 14 * 24 * 60 * 60 * 1000;

/// Words that join other words and never name anything.
const STOP_WORDS: &[&str] = &[
    "a", "an", "and", "are", "at", "by", "for", "from", "fw", "fwd", "in", "is", "my", "of", "on",
    "or", "our", "page", "re", "the", "to", "vs", "with", "your",
];

/// A file's extension says what kind of file it is, not what the work is.
const FILE_EXTENSIONS: &[&str] = &[
    "csv", "doc", "docx", "js", "json", "key", "md", "pdf", "ppt", "pptx", "py", "rs", "ts", "tsx",
    "txt", "xls", "xlsx",
];

/// More shared names than this in one window of history and only the most
/// widely shared are compared.
const MAX_NAMES: usize = 1500;

/// The thread a group of windows was put in.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Named {
    key: String,
    title: String,
    names: Vec<String>,
}

/// Every thread found in a stretch of history.
#[derive(Debug, Default)]
pub struct WorkThreads {
    by_base: HashMap<String, Named>,
}

impl WorkThreads {
    /// The thread a memory belongs to: the thread its windows were named
    /// into, or the grouping it had before (`thread_key`).
    pub(crate) fn key(&self, record: &MemoryRecord) -> String {
        let base = thread_key(record);
        match self.by_base.get(&base) {
            Some(named) => named.key.clone(),
            None => base,
        }
    }

    /// The name a named thread goes by, as it was written in a window title.
    pub(crate) fn title(&self, record: &MemoryRecord) -> Option<&str> {
        self.by_base
            .get(&thread_key(record))
            .map(|named| named.title.as_str())
    }

    /// The shared words that put this memory's windows in its thread, most
    /// widely shared first. Empty for a thread that was not named.
    pub(crate) fn names(&self, record: &MemoryRecord) -> Vec<String> {
        self.by_base
            .get(&thread_key(record))
            .map(|named| named.names.clone())
            .unwrap_or_default()
    }
}

/// One alphanumeric run of a title: lowercased for matching, and as written.
struct Word {
    lower: String,
    written: String,
}

fn words(segment: &str) -> Vec<Word> {
    let mut out = Vec::new();
    for raw in segment.split_whitespace() {
        // "Review.key" is the file "Review".
        let stem = match raw.rsplit_once('.') {
            Some((stem, ext)) if FILE_EXTENSIONS.contains(&ext.to_lowercase().as_str()) => stem,
            _ => raw,
        };
        for run in stem.split(|ch: char| !ch.is_alphanumeric()) {
            let lower = run.to_lowercase();
            if !run.is_empty() && !STOP_WORDS.contains(&lower.as_str()) {
                out.push(Word {
                    lower,
                    written: run.to_string(),
                });
            }
        }
    }
    out
}

fn all_digits(word: &str) -> bool {
    word.chars().all(|ch| ch.is_ascii_digit())
}

/// "v3" and "v12" count versions of one thing; they do not name it.
fn is_version(word: &str) -> bool {
    word.strip_prefix('v')
        .is_some_and(|rest| !rest.is_empty() && all_digits(rest))
}

/// The parts of a title between " - ", " | " and the like, without the part
/// that names the app and, for a page, without the site name it ends with.
fn subject_segments(record: &MemoryRecord) -> Vec<String> {
    let mut title = record.window_title.clone();
    for separator in [" - ", " – ", " — ", " | ", " · "] {
        title = title.replace(separator, "\u{1}");
    }
    let all: Vec<String> = title
        .split('\u{1}')
        .map(|part| part.trim().to_string())
        .filter(|part| !part.is_empty())
        .collect();
    let app = record.app_name.trim().to_lowercase();
    let mut kept: Vec<String> = all
        .iter()
        .filter(|part| {
            let part = part.to_lowercase();
            app.is_empty() || !(app.contains(&part) || part.contains(&app))
        })
        .cloned()
        .collect();
    if kept.is_empty() {
        kept = all;
    }
    let is_page = record
        .url
        .as_deref()
        .is_some_and(|url| url.trim().starts_with("http"));
    if is_page && kept.len() >= 2 {
        kept.pop();
    }
    kept
}

/// The candidate names in one memory's window title, each with the way it
/// was written. A phrase is two neighbouring words; an identifier is a word
/// with a digit in it ("Q4", "a4") or a number of three digits or more.
fn candidate_names(record: &MemoryRecord) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for segment in subject_segments(record) {
        let words = words(&segment);
        for pair in words.windows(2) {
            if !(all_digits(&pair[0].lower) && all_digits(&pair[1].lower)) {
                out.push((
                    format!("{} {}", pair[0].lower, pair[1].lower),
                    format!("{} {}", pair[0].written, pair[1].written),
                ));
            }
        }
        for word in &words {
            let has_digit = word.lower.chars().any(|ch| ch.is_ascii_digit());
            let has_letter = word.lower.chars().any(|ch| ch.is_alphabetic());
            let identifier = (all_digits(&word.lower) && word.lower.len() >= 3)
                || (has_digit && has_letter && !is_version(&word.lower));
            if identifier {
                out.push((format!("#{}", word.lower), word.written.clone()));
            }
        }
    }
    out
}

/// Where a window lives: its app, and for a page its site.
fn place(record: &MemoryRecord) -> String {
    let site = record
        .url
        .as_deref()
        .and_then(crate::capture::extract_domain)
        .unwrap_or_default();
    format!("{}|{}", record.app_name.trim().to_lowercase(), site)
}

struct Group {
    base: String,
    place: String,
    /// Set when the memories carry an explicit project, which outranks a name.
    project: bool,
    names: BTreeSet<String>,
    /// The words on screen in these windows, lowercased and single-spaced,
    /// with a space at each end, so a name is found as whole words.
    text: String,
}

/// As much on-screen text per group of windows as is searched for names.
const MAX_TEXT_CHARS: usize = 40_000;

/// A window with no name of its own joins a thread when its text uses this
/// many of the thread's names and no name of any other thread.
const NAMES_IN_TEXT_TO_JOIN: usize = 2;

fn find(parent: &mut [usize], mut node: usize) -> usize {
    while parent[node] != node {
        parent[node] = parent[parent[node]];
        node = parent[node];
    }
    node
}

/// Names the threads in `records`. Pure, and the same for any order of the
/// same records.
pub(crate) fn work_threads(records: &[MemoryRecord]) -> WorkThreads {
    // One group per existing thread key, in key order.
    let mut by_base: BTreeMap<String, Group> = BTreeMap::new();
    let mut written: HashMap<String, String> = HashMap::new();
    let mut ordered: Vec<&MemoryRecord> = records.iter().collect();
    ordered.sort_by(|a, b| a.timestamp.cmp(&b.timestamp).then_with(|| a.id.cmp(&b.id)));
    for record in ordered {
        let base = thread_key(record);
        let group = by_base.entry(base.clone()).or_insert_with(|| Group {
            base,
            place: place(record),
            project: !record.project.trim().is_empty(),
            names: BTreeSet::new(),
            text: String::from(" "),
        });
        if group.text.len() < MAX_TEXT_CHARS {
            let screen = if record.clean_text.trim().is_empty() {
                &record.text
            } else {
                &record.clean_text
            };
            for run in screen.split(|ch: char| !ch.is_alphanumeric()) {
                if !run.is_empty() {
                    group.text.push_str(&run.to_lowercase());
                    group.text.push(' ');
                }
            }
        }
        for (name, as_written) in candidate_names(record) {
            written.entry(name.clone()).or_insert(as_written);
            group.names.insert(name);
        }
    }
    let groups: Vec<Group> = by_base.into_values().collect();

    let mut holders: BTreeMap<&str, BTreeSet<usize>> = BTreeMap::new();
    let mut groups_in_place: HashMap<&str, usize> = HashMap::new();
    for (index, group) in groups.iter().enumerate() {
        *groups_in_place.entry(group.place.as_str()).or_default() += 1;
        for name in &group.names {
            holders.entry(name.as_str()).or_default().insert(index);
        }
    }

    // A name is shared by two windows or more. A phrase that sits in most
    // windows of one place and nowhere else names the place: a workspace, a
    // site, a kind of page.
    let names_the_place = |held_by: &BTreeSet<usize>| {
        let mut places = held_by.iter().map(|&index| groups[index].place.as_str());
        let first = places.next().unwrap_or_default();
        if places.any(|other| other != first) {
            return false;
        }
        let in_place = groups_in_place.get(first).copied().unwrap_or(0);
        in_place >= 3 && held_by.len() * 2 > in_place
    };
    let mut shared: Vec<(&str, &BTreeSet<usize>)> = holders
        .iter()
        .filter(|(_, held_by)| held_by.len() >= 2 && !names_the_place(held_by))
        .map(|(name, held_by)| (*name, held_by))
        .collect();
    if shared.len() > MAX_NAMES {
        shared.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then_with(|| a.0.cmp(b.0)));
        shared.truncate(MAX_NAMES);
        shared.sort_by(|a, b| a.0.cmp(b.0));
    }

    // Two names for the same work sit on mostly the same windows.
    let mut parent: Vec<usize> = (0..shared.len()).collect();
    for left in 0..shared.len() {
        for right in left + 1..shared.len() {
            let overlap = shared[left].1.intersection(shared[right].1).count();
            let smaller = shared[left].1.len().min(shared[right].1.len());
            if overlap * 2 > smaller {
                let (a, b) = (find(&mut parent, left), find(&mut parent, right));
                if a != b {
                    parent[a.max(b)] = a.min(b);
                }
            }
        }
    }
    // Each set of names, most widely shared first; a phrase before an
    // identifier, then by spelling, so the choice never depends on order.
    let mut name_sets: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for index in 0..shared.len() {
        let root = find(&mut parent, index);
        name_sets.entry(root).or_default().push(index);
    }
    for members in name_sets.values_mut() {
        members.sort_by(|&a, &b| {
            shared[b]
                .1
                .len()
                .cmp(&shared[a].1.len())
                .then_with(|| {
                    shared[a]
                        .0
                        .starts_with('#')
                        .cmp(&shared[b].0.starts_with('#'))
                })
                .then_with(|| shared[a].0.cmp(shared[b].0))
        });
    }
    let position: HashMap<&str, usize> = shared
        .iter()
        .enumerate()
        .map(|(index, (name, _))| (*name, index))
        .collect();

    // A window joins the set of names it shares most widely.
    let mut members_of: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (index, group) in groups.iter().enumerate() {
        let mut score: BTreeMap<usize, usize> = BTreeMap::new();
        for name in &group.names {
            if let Some(&at) = position.get(name.as_str()) {
                *score.entry(find(&mut parent, at)).or_default() += shared[at].1.len();
            }
        }
        let best = score.iter().max_by(|a, b| {
            a.1.cmp(b.1)
                .then_with(|| shared[name_sets[b.0][0]].0.cmp(shared[name_sets[a.0][0]].0))
        });
        if let Some((&root, _)) = best {
            members_of.entry(root).or_default().push(index);
        }
    }

    let mut threads = WorkThreads::default();
    // Each thread that formed, with every name it goes by.
    let mut formed: Vec<(Named, Vec<&str>)> = Vec::new();
    for (root, members) in members_of {
        let lead = shared[name_sets[&root][0]].0;
        let as_written = |name: &str| written.get(name).cloned().unwrap_or_default();
        let names: Vec<String> = name_sets[&root]
            .iter()
            .take(3)
            .map(|&index| as_written(shared[index].0))
            .collect();
        let projects: Vec<usize> = members
            .iter()
            .copied()
            .filter(|&index| groups[index].project)
            .collect();
        // One project among the windows: the name's windows are that
        // project. Two projects: the person told them apart, so they stay
        // apart and only the unlabelled windows are named.
        let (joined, key, title) = match projects.as_slice() {
            [only] => (
                members.clone(),
                groups[*only].base.clone(),
                groups[*only].base.clone(),
            ),
            [] => (members.clone(), format!("work:{lead}"), as_written(lead)),
            _ => (
                members
                    .iter()
                    .copied()
                    .filter(|&index| !groups[index].project)
                    .collect(),
                format!("work:{lead}"),
                as_written(lead),
            ),
        };
        if joined.len() < 2 {
            continue;
        }
        let named = Named { key, title, names };
        for index in joined {
            threads
                .by_base
                .insert(groups[index].base.clone(), named.clone());
        }
        let every_name = name_sets[&root]
            .iter()
            .map(|&index| shared[index].0)
            .collect();
        formed.push((named, every_name));
    }

    // A window whose title names nothing can still say what it is about: a
    // chat channel, a rubric page. It joins a thread when its text uses two
    // of that thread's names and none of any other thread's. An inbox that
    // mentions everything joins nothing.
    for group in &groups {
        if group.project || threads.by_base.contains_key(&group.base) {
            continue;
        }
        let mut mentioned = formed.iter().filter_map(|(named, every_name)| {
            let used = every_name
                .iter()
                .filter(|name| {
                    let phrase = name.strip_prefix('#').unwrap_or(name);
                    group.text.contains(&format!(" {phrase} "))
                })
                .count();
            (used > 0).then_some((named, used))
        });
        if let (Some((named, used)), None) = (mentioned.next(), mentioned.next()) {
            if used >= NAMES_IN_TEXT_TO_JOIN {
                threads.by_base.insert(group.base.clone(), named.clone());
            }
        }
    }
    threads
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(id: &str, app: &str, title: &str, url: Option<&str>) -> MemoryRecord {
        MemoryRecord {
            id: id.into(),
            app_name: app.into(),
            window_title: title.into(),
            url: url.map(str::to_string),
            session_key: crate::capture::build_session_key(app, title, url),
            ..Default::default()
        }
    }

    fn keys(records: &[MemoryRecord]) -> Vec<String> {
        let threads = work_threads(records);
        records.iter().map(|record| threads.key(record)).collect()
    }

    #[test]
    fn windows_in_different_apps_that_share_a_phrase_are_one_thread() {
        let records = [
            window(
                "doc",
                "Google Chrome",
                "Smart Reminders PRD - Google Docs",
                Some("https://docs.google.com/document/d/1/edit"),
            ),
            window("deck", "Keynote", "Smart Reminders Launch Review.key", None),
            window("mail", "Mail", "Lunch order", None),
        ];
        let threads = work_threads(&records);
        assert_eq!(threads.key(&records[0]), "work:smart reminders");
        assert_eq!(threads.key(&records[1]), "work:smart reminders");
        assert_eq!(threads.title(&records[0]), Some("Smart Reminders"));
        assert_eq!(threads.names(&records[1]), vec!["Smart Reminders"]);
        // A window that shares nothing keeps the grouping it had.
        assert_eq!(threads.key(&records[2]), thread_key(&records[2]));
        assert_eq!(threads.title(&records[2]), None);
    }

    #[test]
    fn a_short_identifier_two_windows_share_is_a_name() {
        let records = [
            window("term", "Terminal", "zsh - a4", None),
            window(
                "code",
                "Visual Studio Code",
                "charts.py - a4 - Visual Studio Code",
                None,
            ),
        ];
        assert_eq!(keys(&records), vec!["work:#a4", "work:#a4"]);
        assert_eq!(work_threads(&records).title(&records[0]), Some("a4"));
    }

    #[test]
    fn the_site_a_page_title_ends_with_is_not_a_name() {
        let records = [
            window(
                "essay",
                "Google Chrome",
                "Essay outline - Google Docs",
                Some("https://docs.google.com/document/d/1/edit"),
            ),
            window(
                "notes",
                "Google Chrome",
                "Meeting notes - Google Docs",
                Some("https://docs.google.com/document/d/2/edit"),
            ),
        ];
        let named = keys(&records);
        assert_ne!(named[0], named[1]);
    }

    #[test]
    fn a_phrase_in_most_windows_of_one_app_names_the_place_not_the_work() {
        let records = [
            window("a", "Slack", "#hiring - Lumen Ledger - Slack", None),
            window("b", "Slack", "#launch - Lumen Ledger - Slack", None),
            window("c", "Slack", "Victor S. (DM) - Lumen Ledger - Slack", None),
        ];
        let named = keys(&records);
        assert_eq!(named.iter().collect::<BTreeSet<_>>().len(), 3, "{named:?}");
    }

    #[test]
    fn a_version_number_is_not_a_name() {
        let records = [
            window("spec", "Notion", "Rate limiting spec v2", None),
            window("deck", "Keynote", "Hiring plan v2.key", None),
        ];
        let named = keys(&records);
        assert_ne!(named[0], named[1]);
    }

    #[test]
    fn one_explicit_project_takes_the_windows_that_share_its_name() {
        let mut filed = window("deck", "Keynote", "Pantry App - Final Demo.key", None);
        filed.project = "Capstone".into();
        let records = [
            filed,
            window(
                "script",
                "Notion",
                "Demo script v1 - Pantry App Capstone",
                None,
            ),
        ];
        let threads = work_threads(&records);
        assert_eq!(keys(&records), vec!["Capstone", "Capstone"]);
        assert_eq!(threads.title(&records[1]), Some("Capstone"));
    }

    #[test]
    fn two_explicit_projects_are_never_merged_by_a_shared_name() {
        let mut first = window("a", "Keynote", "Quarterly Review deck.key", None);
        first.project = "Sales".into();
        let mut second = window("b", "Numbers", "Quarterly Review numbers", None);
        second.project = "Finance".into();
        let records = [first, second];
        assert_eq!(keys(&records), vec!["Sales", "Finance"]);
    }

    #[test]
    fn the_same_records_in_any_order_get_the_same_keys() {
        let mut records = vec![
            window("a", "Keynote", "Q3 Business Review.key", None),
            window(
                "b",
                "Microsoft Excel",
                "Q3 Business Review numbers.xlsx",
                None,
            ),
            window("c", "Notion", "Q3 Business Review notes", None),
        ];
        let forward = keys(&records);
        records.reverse();
        let mut backward = keys(&records);
        backward.reverse();
        assert_eq!(forward, backward);
        assert_eq!(forward[0], "work:business review");
    }

    fn reading(mut record: MemoryRecord, text: &str) -> MemoryRecord {
        record.clean_text = text.into();
        record
    }

    fn two_threads() -> Vec<MemoryRecord> {
        vec![
            window("deck", "Keynote", "Smart Reminders Launch Review.key", None),
            window(
                "list",
                "Notion",
                "Launch Review checklist - Smart Reminders",
                None,
            ),
            window(
                "role",
                "Pages",
                "Senior Product Designer job description",
                None,
            ),
            window("card", "Numbers", "Senior Product Designer scorecard", None),
        ]
    }

    #[test]
    fn a_window_whose_text_uses_two_names_of_one_thread_joins_it() {
        let mut records = two_threads();
        records.push(reading(
            window("chat", "Slack", "#general - Slack", None),
            "Is the launch review moved? Smart Reminders ships Tuesday.",
        ));
        let threads = work_threads(&records);
        assert_eq!(threads.key(&records[4]), threads.key(&records[0]));
        // Two names shared by the same windows: the first by spelling leads.
        assert_eq!(threads.title(&records[4]), Some("Launch Review"));
    }

    #[test]
    fn text_that_mentions_two_threads_or_one_name_once_joins_neither() {
        let mut records = two_threads();
        records.push(reading(
            window("inbox", "Mail", "Inbox", None),
            "Smart Reminders launch review at 3. Senior Product Designer onsite at 4.",
        ));
        records.push(reading(
            window("note", "Notes", "Groceries", None),
            "Ask about Smart Reminders pricing.",
        ));
        let threads = work_threads(&records);
        assert_eq!(threads.key(&records[4]), thread_key(&records[4]));
        assert_eq!(threads.key(&records[5]), thread_key(&records[5]));
    }

    #[test]
    fn text_never_moves_a_window_that_has_its_own_project() {
        let mut records = two_threads();
        let mut filed = reading(
            window("budget", "Numbers", "Budget", None),
            "Smart Reminders launch review costs.",
        );
        filed.project = "Finance".into();
        records.push(filed);
        assert_eq!(work_threads(&records).key(&records[4]), "Finance");
    }

    /// The three persona corpora file every entry under a project. With that
    /// label hidden, how many same-project pairs does the grouping find
    /// (recall), and how many of the pairs it makes are right (precision)?
    fn score(persona: &str, named: bool) -> (f32, f32) {
        let path = format!(
            "{}/../scripts/demo/{persona}-week.json",
            env!("CARGO_MANIFEST_DIR")
        );
        let entries: Vec<serde_json::Value> =
            serde_json::from_slice(&std::fs::read(&path).expect("corpus")).expect("json");
        let text = |entry: &serde_json::Value, field: &str| {
            entry[field].as_str().unwrap_or_default().to_string()
        };
        let records: Vec<MemoryRecord> = entries
            .iter()
            .map(|entry| {
                reading(
                    window(
                        &text(entry, "id"),
                        &text(entry, "app_name"),
                        &text(entry, "window_title"),
                        entry["url"].as_str(),
                    ),
                    &text(entry, "ocr_text"),
                )
            })
            .collect();
        let threads = if named {
            work_threads(&records)
        } else {
            WorkThreads::default()
        };
        let keys: Vec<String> = records.iter().map(|record| threads.key(record)).collect();
        let (mut both, mut made, mut wanted) = (0f32, 0f32, 0f32);
        for a in 0..entries.len() {
            for b in a + 1..entries.len() {
                let project = text(&entries[a], "project");
                let gold = !project.is_empty() && project == text(&entries[b], "project");
                let same = keys[a] == keys[b];
                both += f32::from(u8::from(gold && same));
                made += f32::from(u8::from(same));
                wanted += f32::from(u8::from(gold));
            }
        }
        (both / made.max(1.0), both / wanted.max(1.0))
    }

    #[test]
    fn naming_finds_far_more_of_a_personas_work_and_is_seldom_wrong() {
        // (persona, precision floor, recall floor). Measured 2026-10-13 from
        // titles alone: 1.00 and 0.33, 0.98 and 0.69, 0.93 and 0.37. With the
        // text pass: 1.00 and 0.42, 0.98 and 0.84, 0.94 and 0.46.
        for (persona, precision_floor, recall_floor) in [
            ("knowledge-worker", 0.95, 0.40),
            ("office-pm", 0.95, 0.80),
            ("software-engineer", 0.90, 0.44),
        ] {
            let (_, recall_before) = score(persona, false);
            let (precision, recall) = score(persona, true);
            println!(
                "{persona}: recall {recall_before:.2} -> {recall:.2}, precision {precision:.2}"
            );
            assert!(
                precision >= precision_floor,
                "{persona} precision {precision}"
            );
            assert!(recall >= recall_floor, "{persona} recall {recall}");
            assert!(recall >= recall_before * 2.0, "{persona} recall {recall}");
        }
    }
}
