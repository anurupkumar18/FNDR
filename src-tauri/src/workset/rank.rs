//! Which thread of work a request means, and what it reopens. Pure: the same
//! inputs always give the same `Resolution`.

use std::collections::{HashMap, HashSet};

use super::{ItemKind, Resolution, WorkItem, WorkSet, MAX_ITEMS};
use crate::ipc::commands::memory::{resolve_reopen_target, ResolvedReopenTarget};
use crate::memory::reopen::{reopen_rank, ReopenTarget};
use crate::storage::{MemoryRecord, Task};

/// A Resume thread: its title and the memories in it.
#[derive(Debug, Clone, Default)]
pub struct ThreadInput {
    pub title: String,
    pub member_ids: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Hit {
    pub memory_id: String,
    pub score: f32,
}

/// Everything `rank` reads. `records` holds every memory the threads, hits
/// and tasks name; an id missing from it is ignored.
#[derive(Debug, Clone, Default)]
pub struct Inputs {
    pub query: String,
    pub now_ms: i64,
    pub threads: Vec<ThreadInput>,
    pub hits: Vec<Hit>,
    pub tasks: Vec<Task>,
    pub records: HashMap<String, MemoryRecord>,
    pub blocklist: Vec<String>,
}

/// Two sets whose scores are closer than this are not told apart: the
/// person picks.
pub const AMBIGUITY_MARGIN: f32 = 0.1;
const MAX_OPTIONS: usize = 3;
/// A search hit at or above this counts as a match (`STRONG_MATCH_SCORE`).
const MIN_HIT_SCORE: f32 = crate::context_runtime::retrieve::STRONG_MATCH_SCORE;
/// The Vault's session-link window (`sessionLinks.ts`, `LINK_GAP_MS`).
const LINK_GAP_MS: i64 = 10 * 60 * 1000;
/// A thread that itself matches the request joins within this window.
const MATCH_GAP_MS: i64 = 30 * 60 * 1000;
const HOUR_MS: i64 = 60 * 60 * 1000;
const DUE_SOON_MS: i64 = 72 * HOUR_MS;

/// Words that say how to ask, not what the work is.
const PHRASING: &[&str] = &[
    "a",
    "about",
    "all",
    "an",
    "and",
    "again",
    "am",
    "back",
    "bring",
    "can",
    "could",
    "did",
    "doing",
    "everything",
    "for",
    "from",
    "get",
    "gather",
    "had",
    "have",
    "i",
    "im",
    "in",
    "is",
    "it",
    "load",
    "me",
    "my",
    "of",
    "on",
    "open",
    "out",
    "please",
    "pull",
    "related",
    "relating",
    "reopen",
    "set",
    "show",
    "stuff",
    "that",
    "the",
    "these",
    "things",
    "this",
    "those",
    "to",
    "up",
    "was",
    "we",
    "were",
    "what",
    "with",
    "work",
    "worked",
    "working",
    "you",
    "your",
    "earlier",
    "yesterday",
    "today",
    "morning",
    "afternoon",
    "last",
    "week",
    "whole",
    "thing",
    "everything's",
    "one",
    "ones",
    "just",
    "lets",
    "let",
    "s",
];

/// Words in window titles whatever the work is (`sessionLinks.ts`).
const COMMON_TITLE_WORDS: &[&str] = &[
    "about",
    "chrome",
    "document",
    "google",
    "https",
    "inbox",
    "microsoft",
    "safari",
    "search",
    "settings",
    "untitled",
    "window",
    "workspace",
];

fn split_words(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split(|c: char| !c.is_alphanumeric() && c != '\'')
        .map(|word| word.trim_matches('\'').to_lowercase())
        .filter(|word| !word.is_empty())
}

/// One spelling for a word and its plural, so "assignments" finds "Assignment 3".
fn stem(word: &str) -> String {
    if word.chars().count() > 4 && word.ends_with('s') && !word.ends_with("ss") {
        word[..word.len() - 1].to_string()
    } else {
        word.to_string()
    }
}

/// The words of a request that name the work: "the gene expression
/// assignment I was working on" gives `gene`, `expression`, `assignment`.
pub fn topic_words(query: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    split_words(query)
        .filter(|word| !PHRASING.contains(&word.as_str()))
        .filter(|word| seen.insert(word.clone()))
        .collect()
}

/// Whether a memory may ever become an item: never a private or incognito
/// window, a blocklisted app or site, FNDR itself, a soft-deleted memory or
/// an agent note.
pub(crate) fn eligible(record: &MemoryRecord, blocklist: &[String]) -> bool {
    use crate::privacy::safety_gate::{evaluate, SafetyDecision};
    let gate = |url: Option<&str>| {
        evaluate(
            Some(&record.app_name),
            record.bundle_id.as_deref(),
            url,
            Some(&record.window_title),
            None,
            blocklist,
        ) != SafetyDecision::SkipStorage
    };
    !record.is_agent_note()
        && crate::context_runtime::retrieve::memory_is_permitted(record, blocklist)
        && gate(record.url.as_deref())
        && gate(record.reopen_url.as_deref())
        && !crate::privacy::Blocklist::is_context_blocked(
            record.reopen_url.as_deref(),
            None,
            blocklist,
        )
}

fn host_of(url: &str) -> String {
    let rest = url.split_once("://").map(|(_, tail)| tail).unwrap_or(url);
    let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let host = host.rsplit('@').next().unwrap_or(host);
    let host = host.split(':').next().unwrap_or(host).to_lowercase();
    host.strip_prefix("www.").unwrap_or(&host).to_string()
}

fn clip(text: &str, max: usize) -> String {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.chars().count() <= max {
        return text;
    }
    let clipped: String = text.chars().take(max).collect();
    let cut = clipped.rfind(' ').unwrap_or(clipped.len());
    format!("{}…", clipped[..cut].trim_end())
}

fn file_name(path: &std::path::Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_string()
}

fn typed_rank(record: &MemoryRecord) -> u8 {
    reopen_rank(&ReopenTarget {
        kind: record.reopen_kind.clone(),
        url: record.reopen_url.clone(),
        file_path: record.reopen_file_path.clone(),
        app_bundle_id: record.reopen_app_bundle_id.clone(),
        app_deep_link: record.reopen_app_deep_link.clone(),
        page: record.reopen_page,
        text_anchor: record.reopen_text_anchor.clone(),
        ..Default::default()
    })
}

/// The item a memory reopens to, with the key that makes two memories the
/// same target: a link without its fragment, a path, an app, a deep link.
pub(crate) fn item_of(record: &MemoryRecord) -> Option<(String, WorkItem)> {
    let target = resolve_reopen_target(record)?;
    let app_name = record
        .reopen_app_name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or(record.app_name.trim())
        .to_string();
    let title = clip(record.window_title.trim(), 80);
    let page = record.reopen_page;
    let (key, kind, label, host, fallback_rank) = match target {
        ResolvedReopenTarget::BrowserUrl(url) => {
            let bare = url.split('#').next().unwrap_or(&url).to_string();
            let host = host_of(&bare);
            let is_pdf = bare
                .split(['?', '#'])
                .next()
                .unwrap_or_default()
                .to_lowercase()
                .ends_with(".pdf");
            let kind = if is_pdf && page.is_some() {
                ItemKind::PdfPage
            } else {
                ItemKind::Url
            };
            let label = if title.is_empty() {
                host.clone()
            } else {
                title
            };
            (
                format!("url:{}", bare.to_lowercase()),
                kind,
                label,
                Some(host),
                4,
            )
        }
        ResolvedReopenTarget::FilePath(path) => {
            let name = file_name(&path);
            let extension = path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(str::to_lowercase);
            let (kind, label) = match extension.as_deref() {
                Some("pdf") if page.is_some() => (
                    ItemKind::PdfPage,
                    format!("{name}, page {}", page.unwrap_or_default()),
                ),
                None => (ItemKind::Folder, name),
                Some(_) => (ItemKind::File, name),
            };
            (
                format!("file:{}", path.display()),
                kind,
                label,
                None,
                if path.is_absolute() { 6 } else { 1 },
            )
        }
        ResolvedReopenTarget::AppBundle(bundle) => (
            format!("app:{}", bundle.to_lowercase()),
            ItemKind::App,
            app_name.clone(),
            None,
            2,
        ),
        ResolvedReopenTarget::AppDeepLink(link) => (
            format!("link:{link}"),
            ItemKind::App,
            if title.is_empty() {
                app_name.clone()
            } else {
                title
            },
            None,
            3,
        ),
    };
    let rank = match typed_rank(record) {
        0 => fallback_rank,
        rank => rank,
    };
    Some((
        key,
        WorkItem {
            memory_id: record.id.clone(),
            label,
            kind,
            reopen_rank: rank,
            app_name,
            host,
            page: if kind == ItemKind::PdfPage {
                page
            } else {
                None
            },
            captured_at: record.timestamp,
        },
    ))
}

/// One item per distinct target: the highest reopen rank wins, then the
/// newest capture. Listed newest first, an app-only item is dropped when
/// another item already opens that app, and at most `MAX_ITEMS` remain.
fn items_of(members: &[&MemoryRecord]) -> Vec<WorkItem> {
    let mut best: HashMap<String, WorkItem> = HashMap::new();
    for (key, item) in members.iter().filter_map(|record| item_of(record)) {
        let replace = match best.get(&key) {
            None => true,
            Some(kept) => {
                (item.reopen_rank, item.captured_at, &kept.memory_id)
                    > (kept.reopen_rank, kept.captured_at, &item.memory_id)
            }
        };
        if replace {
            best.insert(key, item);
        }
    }
    let mut items: Vec<WorkItem> = best.into_values().collect();
    items.sort_by(|a, b| {
        b.captured_at
            .cmp(&a.captured_at)
            .then_with(|| a.memory_id.cmp(&b.memory_id))
    });
    let apps_with_a_place: HashSet<String> = items
        .iter()
        .filter(|item| item.kind != ItemKind::App)
        .map(|item| item.app_name.to_lowercase())
        .collect();
    items.retain(|item| {
        !(item.kind == ItemKind::App
            && item.reopen_rank <= 2
            && apps_with_a_place.contains(&item.app_name.to_lowercase()))
    });
    items.truncate(MAX_ITEMS);
    items
}

struct Group<'a> {
    key: String,
    title: String,
    members: Vec<&'a MemoryRecord>,
    start: i64,
    end: i64,
    files: HashSet<String>,
    title_words: HashSet<String>,
    /// Sites of its links and names of its other apps.
    places: HashSet<String>,
    relevance: f32,
    hit_count: usize,
    overlap: f32,
    task_boost: f32,
    due_soon: bool,
    score: f32,
    matches: bool,
}

fn base_name(path: &str) -> String {
    path.trim()
        .to_lowercase()
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default()
        .to_string()
}

fn task_memory_ids(task: &Task) -> impl Iterator<Item = &String> {
    task.source_memory_id
        .iter()
        .chain(task.linked_memory_ids.iter())
}

fn describe<'a>(
    key: String,
    title: String,
    members: Vec<&'a MemoryRecord>,
    inputs: &Inputs,
    topic: &[String],
    hits: &HashMap<&str, f32>,
) -> Group<'a> {
    let start = members
        .iter()
        .map(|m| m.timestamp)
        .min()
        .unwrap_or_default();
    let end = members
        .iter()
        .map(|m| m.timestamp)
        .max()
        .unwrap_or_default();
    let mut files = HashSet::new();
    let mut words = HashSet::new();
    let mut title_words = HashSet::new();
    let mut places = HashSet::new();
    let ids: HashSet<&str> = members.iter().map(|m| m.id.as_str()).collect();
    for record in &members {
        let app_words: HashSet<String> = split_words(&record.app_name).collect();
        for path in record
            .files_touched
            .iter()
            .chain(record.reopen_file_path.iter())
        {
            let name = base_name(path);
            if !name.is_empty() {
                words.extend(split_words(&name).map(|w| stem(&w)));
                files.insert(name);
            }
        }
        for word in split_words(&format!("{} {}", record.window_title, record.project)) {
            if word.chars().count() >= 5
                && !app_words.contains(&word)
                && !COMMON_TITLE_WORDS.contains(&word.as_str())
            {
                title_words.insert(word.clone());
            }
            words.insert(stem(&word));
        }
        match record.url.as_deref().or(record.reopen_url.as_deref()) {
            Some(url) => {
                let host = host_of(url);
                words.extend(split_words(&host).map(|w| stem(&w)));
                places.insert(host);
            }
            None => {
                places.insert(record.app_name.trim().to_lowercase());
            }
        }
    }
    words.extend(split_words(&title).map(|w| stem(&w)));
    let mut task_boost: f32 = 0.0;
    let mut due_soon = false;
    for task in &inputs.tasks {
        if !task_memory_ids(task).any(|id| ids.contains(id.as_str())) {
            continue;
        }
        words.extend(split_words(&task.title).map(|w| stem(&w)));
        let soon = task.due_date.is_some_and(|due| {
            let ahead = due - inputs.now_ms;
            (-24 * HOUR_MS..=DUE_SOON_MS).contains(&ahead)
        });
        due_soon |= soon;
        task_boost = task_boost.max(if soon { 0.5 } else { 0.2 });
    }
    let member_hits: Vec<f32> = members
        .iter()
        .filter_map(|m| hits.get(m.id.as_str()).copied())
        .collect();
    let best_hit = member_hits.iter().copied().fold(0.0_f32, f32::max);
    let relevance = if member_hits.is_empty() {
        0.0
    } else {
        best_hit + 0.05 * (member_hits.len().saturating_sub(1).min(4) as f32)
    };
    let overlap = if topic.is_empty() {
        0.0
    } else {
        topic
            .iter()
            .filter(|word| words.contains(&stem(word)))
            .count() as f32
            / topic.len() as f32
    };
    let age_hours = (inputs.now_ms - end).max(0) as f32 / HOUR_MS as f32;
    let recency = 0.3 * 0.5_f32.powf(age_hours / 24.0);
    let matches = topic.is_empty() || best_hit >= MIN_HIT_SCORE || overlap > 0.0;
    Group {
        key,
        title,
        members,
        start,
        end,
        files,
        title_words,
        places,
        relevance,
        hit_count: member_hits.len(),
        overlap,
        task_boost,
        due_soon,
        score: relevance + 0.6 * overlap + task_boost + recency,
        matches,
    }
}

/// Same stretch of work: the Vault's link rule (ten minutes, and a file or
/// two distinctive title words in common), or, within half an hour, a thread
/// the search found for the request on its own in another app or site. A
/// second page of the same site is a second piece of work, not a companion.
fn tied(seed: &Group, other: &Group) -> bool {
    let apart = seed.start.max(other.start) - seed.end.min(other.end);
    let shared_files = seed.files.intersection(&other.files).count();
    let shared_words = seed.title_words.intersection(&other.title_words).count();
    let companion = other.relevance >= MIN_HIT_SCORE && seed.places.is_disjoint(&other.places);
    (apart <= LINK_GAP_MS && (shared_files >= 1 || shared_words >= 2))
        || (apart <= MATCH_GAP_MS && companion)
}

fn age_phrase(now_ms: i64, at_ms: i64) -> String {
    let minutes = (now_ms - at_ms).max(0) / 60_000;
    match minutes {
        0..=1 => "just now".to_string(),
        2..=59 => format!("{minutes} minutes ago"),
        60..=119 => "an hour ago".to_string(),
        120..=1439 => format!("{} hours ago", minutes / 60),
        1440..=2879 => "yesterday".to_string(),
        _ => format!("{} days ago", minutes / 1440),
    }
}

fn reason(seed: &Group, topic: &[String], now_ms: i64) -> String {
    let mut parts = Vec::new();
    let quoted = format!("\u{201c}{}\u{201d}", topic.join(" "));
    if seed.hit_count > 0 && seed.relevance >= MIN_HIT_SCORE {
        parts.push(format!(
            "{} matching {quoted}",
            if seed.hit_count == 1 {
                "1 memory".to_string()
            } else {
                format!("{} memories", seed.hit_count)
            }
        ));
    } else if seed.overlap > 0.0 {
        parts.push(format!("its titles mention {quoted}"));
    }
    if seed.due_soon {
        parts.push("a task from it is due soon".to_string());
    } else if seed.task_boost > 0.0 {
        parts.push("it has an open task".to_string());
    }
    parts.push(format!("last seen {}", age_phrase(now_ms, seed.end)));
    let text = parts.join("; ");
    let mut chars = text.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// Ranks candidate threads for a request and returns the set to open, the
/// sets to choose between, or why there is none.
pub fn rank(inputs: &Inputs) -> Resolution {
    let topic = topic_words(&inputs.query);
    let hits: HashMap<&str, f32> = inputs
        .hits
        .iter()
        .map(|hit| (hit.memory_id.as_str(), hit.score))
        .collect();
    let usable = |id: &String| {
        inputs
            .records
            .get(id)
            .filter(|record| eligible(record, &inputs.blocklist))
    };

    let mut groups: Vec<Group> = Vec::new();
    let mut grouped: HashSet<&str> = HashSet::new();
    for thread in &inputs.threads {
        let mut members: Vec<&MemoryRecord> = thread
            .member_ids
            .iter()
            .filter_map(usable)
            .filter(|record| grouped.insert(record.id.as_str()))
            .collect();
        if members.is_empty() {
            continue;
        }
        members.sort_by(|a, b| a.timestamp.cmp(&b.timestamp).then(a.id.cmp(&b.id)));
        let newest = members.last().map(|m| m.id.clone()).unwrap_or_default();
        groups.push(describe(
            format!("thread:{newest}"),
            thread.title.clone(),
            members,
            inputs,
            &topic,
            &hits,
        ));
    }

    // A hit or a task's memory outside every recent thread is its own
    // candidate, grouped by project or session.
    let mut loose: Vec<&String> = inputs
        .hits
        .iter()
        .map(|hit| &hit.memory_id)
        .chain(inputs.tasks.iter().flat_map(task_memory_ids))
        .collect();
    loose.sort();
    loose.dedup();
    let mut orphans: HashMap<String, Vec<&MemoryRecord>> = HashMap::new();
    for record in loose.into_iter().filter_map(usable) {
        if grouped.insert(record.id.as_str()) {
            let key = [record.project.trim(), record.session_key.trim()]
                .into_iter()
                .find(|key| !key.is_empty())
                .unwrap_or(record.id.as_str())
                .to_string();
            orphans.entry(key).or_default().push(record);
        }
    }
    let mut orphan_keys: Vec<String> = orphans.keys().cloned().collect();
    orphan_keys.sort();
    for key in orphan_keys {
        let mut members = orphans.remove(&key).unwrap_or_default();
        members.sort_by(|a, b| a.timestamp.cmp(&b.timestamp).then(a.id.cmp(&b.id)));
        let newest = members.last().copied();
        let title = newest
            .map(|record| {
                [
                    record.project.trim(),
                    record.window_title.trim(),
                    record.app_name.trim(),
                ]
                .into_iter()
                .find(|text| !text.is_empty())
                .map(|text| clip(text, 80))
                .unwrap_or_default()
            })
            .unwrap_or_default();
        groups.push(describe(
            format!("loose:{key}"),
            title,
            members,
            inputs,
            &topic,
            &hits,
        ));
    }

    if groups.is_empty() {
        return Resolution::None {
            why: "FNDR has no recent work to reopen.".to_string(),
        };
    }

    let mut order: Vec<usize> = (0..groups.len()).filter(|&i| groups[i].matches).collect();
    if order.is_empty() {
        return Resolution::None {
            why: format!(
                "Nothing FNDR remembers matches \u{201c}{}\u{201d}.",
                topic.join(" ")
            ),
        };
    }
    order.sort_by(|&a, &b| {
        let (a, b) = (&groups[a], &groups[b]);
        b.score
            .total_cmp(&a.score)
            .then(b.end.cmp(&a.end))
            .then(a.key.cmp(&b.key))
    });

    let mut absorbed: HashSet<usize> = HashSet::new();
    let mut sets: Vec<WorkSet> = Vec::new();
    for &seed_index in &order {
        if absorbed.contains(&seed_index) {
            continue;
        }
        absorbed.insert(seed_index);
        let seed = &groups[seed_index];
        let mut members: Vec<&MemoryRecord> = seed.members.clone();
        for (index, other) in groups.iter().enumerate() {
            if index != seed_index && !absorbed.contains(&index) && tied(seed, other) {
                absorbed.insert(index);
                members.extend(other.members.iter().copied());
            }
        }
        let items = items_of(&members);
        if items.is_empty() {
            continue;
        }
        sets.push(WorkSet {
            id: format!("ws:{}", seed.key),
            title: seed.title.clone(),
            reason: reason(seed, &topic, inputs.now_ms),
            score: (seed.score * 1000.0).round() / 1000.0,
            items,
        });
    }

    let Some(best) = sets.first() else {
        return Resolution::None {
            why: "FNDR found that work but has no page, file or app it can reopen for it."
                .to_string(),
        };
    };
    let close: Vec<WorkSet> = sets
        .iter()
        .take(MAX_OPTIONS)
        .filter(|set| best.score - set.score < AMBIGUITY_MARGIN)
        .cloned()
        .collect();
    if close.len() > 1 {
        Resolution::Ambiguous(close)
    } else {
        Resolution::Best(best.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::reopen::ReopenKind;

    const NOW: i64 = 1_800_000_000_000;
    const MIN: i64 = 60_000;

    fn rec(id: &str, app: &str, title: &str, minutes_ago: i64) -> MemoryRecord {
        MemoryRecord {
            id: id.into(),
            timestamp: NOW - minutes_ago * MIN,
            app_name: app.into(),
            window_title: title.into(),
            ..Default::default()
        }
    }

    fn page(mut record: MemoryRecord, url: &str) -> MemoryRecord {
        record.reopen_kind = ReopenKind::BrowserUrl;
        record.reopen_url = Some(url.into());
        record.url = Some(url.into());
        record
    }

    fn file(mut record: MemoryRecord, path: &str, at_page: Option<u32>) -> MemoryRecord {
        record.reopen_kind = ReopenKind::FilePath;
        record.reopen_file_path = Some(path.into());
        record.reopen_page = at_page;
        record
    }

    fn app(mut record: MemoryRecord, bundle: &str) -> MemoryRecord {
        record.reopen_kind = ReopenKind::AppBundle;
        record.reopen_app_bundle_id = Some(bundle.into());
        record
    }

    fn thread(title: &str, ids: &[&str]) -> ThreadInput {
        ThreadInput {
            title: title.into(),
            member_ids: ids.iter().map(|id| id.to_string()).collect(),
        }
    }

    fn hit(id: &str, score: f32) -> Hit {
        Hit {
            memory_id: id.into(),
            score,
        }
    }

    fn inputs(query: &str, records: Vec<MemoryRecord>, threads: Vec<ThreadInput>) -> Inputs {
        Inputs {
            query: query.into(),
            now_ms: NOW,
            threads,
            records: records.into_iter().map(|r| (r.id.clone(), r)).collect(),
            ..Default::default()
        }
    }

    fn best(resolution: Resolution) -> WorkSet {
        match resolution {
            Resolution::Best(set) => set,
            other => panic!("expected one set, got {other:?}"),
        }
    }

    fn ids(set: &WorkSet) -> Vec<&str> {
        set.items
            .iter()
            .map(|item| item.memory_id.as_str())
            .collect()
    }

    /// The owner's flow: the Canvas page, the PDF at its page and the doc
    /// from one stretch of work, and nothing from the music playing beside it.
    fn assignment_evening() -> Inputs {
        let canvas = page(
            rec(
                "canvas",
                "Google Chrome",
                "Assignment 3: Gene Expression Lab Report",
                50,
            ),
            "https://canvas.utah.edu/courses/1/assignments/3",
        );
        let pdf = file(
            rec("pdf", "Preview", "gene_expression_reading.pdf", 40),
            "/Users/k/Downloads/gene_expression_reading.pdf",
            Some(4),
        );
        let doc = page(
            rec(
                "doc",
                "Google Chrome",
                "Gene Expression Lab Report draft - Google Docs",
                30,
            ),
            "https://docs.google.com/document/d/abc/edit",
        );
        let spotify = app(
            rec("spotify", "Spotify", "Spotify Premium", 20),
            "com.spotify.client",
        );
        let mut inputs = inputs(
            "pull up everything related to the gene expression assignment",
            vec![canvas, pdf, doc, spotify],
            vec![
                thread("Assignment 3: Gene Expression Lab Report", &["canvas"]),
                thread("gene_expression_reading.pdf", &["pdf"]),
                thread("Gene Expression Lab Report draft - Google Docs", &["doc"]),
                thread("Spotify Premium", &["spotify"]),
            ],
        );
        inputs.hits = vec![hit("canvas", 0.62), hit("pdf", 0.41), hit("doc", 0.35)];
        inputs
    }

    #[test]
    fn the_request_opens_every_place_of_one_stretch_of_work() {
        let set = best(rank(&assignment_evening()));
        assert_eq!(ids(&set), ["doc", "pdf", "canvas"], "newest first");
        assert_eq!(set.title, "Assignment 3: Gene Expression Lab Report");
        let pdf = &set.items[1];
        assert_eq!(pdf.kind, ItemKind::PdfPage);
        assert_eq!(pdf.page, Some(4));
        assert_eq!(pdf.label, "gene_expression_reading.pdf, page 4");
        assert_eq!(set.items[0].host.as_deref(), Some("docs.google.com"));
        assert!(
            set.reason.contains("gene expression assignment"),
            "{}",
            set.reason
        );
    }

    #[test]
    fn a_thread_from_another_afternoon_is_not_pulled_in() {
        let mut inputs = assignment_evening();
        let old = inputs.records.get_mut("doc").unwrap();
        old.timestamp = NOW - 26 * 60 * MIN;
        let set = best(rank(&inputs));
        assert_eq!(ids(&set), ["pdf", "canvas"]);
    }

    #[test]
    fn one_item_per_target_keeps_the_most_precise_then_the_newest() {
        let mut anchored = page(
            rec("anchored", "Safari", "Week 5 reading", 30),
            "https://site.edu/r",
        );
        anchored.reopen_text_anchor = Some("the passage that was on screen at the time".into());
        let plain_newer = page(
            rec("plain", "Safari", "Week 5 reading", 10),
            "https://site.edu/r#top",
        );
        let old_copy = page(
            rec("old", "Safari", "Week 5 reading", 50),
            "https://site.edu/r",
        );
        let other = page(
            rec("other", "Safari", "Week 5 quiz", 20),
            "https://site.edu/quiz",
        );
        let mut inputs = inputs(
            "open everything for week 5 reading",
            vec![anchored, plain_newer, old_copy, other],
            vec![thread("Week 5", &["anchored", "plain", "old", "other"])],
        );
        inputs.hits = vec![hit("plain", 0.5)];
        let set = best(rank(&inputs));
        assert_eq!(ids(&set), ["other", "anchored"]);
        assert_eq!(set.items[1].reopen_rank, 5);
    }

    #[test]
    fn a_set_holds_at_most_six_places_the_newest_ones() {
        let records: Vec<MemoryRecord> = (0..9)
            .map(|n| {
                page(
                    rec(
                        &format!("m{n}"),
                        "Safari",
                        &format!("Thesis chapter {n}"),
                        10 * n,
                    ),
                    &format!("https://thesis.example/{n}"),
                )
            })
            .collect();
        let all: Vec<String> = records.iter().map(|r| r.id.clone()).collect();
        let all: Vec<&str> = all.iter().map(String::as_str).collect();
        let set = best(rank(&inputs(
            "pull up my thesis",
            records,
            vec![thread("Thesis", &all)],
        )));
        assert_eq!(ids(&set), ["m0", "m1", "m2", "m3", "m4", "m5"]);
    }

    #[test]
    fn two_equally_likely_threads_are_put_to_the_person() {
        let bio = page(
            rec("bio", "Safari", "Biology assignment 2", 30),
            "https://canvas.edu/bio",
        );
        let chem = page(
            rec("chem", "Safari", "Chemistry assignment 4", 32),
            "https://canvas.edu/chem",
        );
        let mut close = inputs(
            "the assignment I was working on",
            vec![bio.clone(), chem.clone()],
            vec![
                thread("Biology assignment 2", &["bio"]),
                thread("Chemistry assignment 4", &["chem"]),
            ],
        );
        match rank(&close) {
            Resolution::Ambiguous(options) => {
                let titles: Vec<&str> = options.iter().map(|o| o.title.as_str()).collect();
                assert_eq!(titles, ["Biology assignment 2", "Chemistry assignment 4"]);
            }
            other => panic!("expected a choice, got {other:?}"),
        }
        close.hits = vec![hit("chem", 0.7)];
        assert_eq!(
            best(rank(&close)).title,
            "Chemistry assignment 4",
            "a clear margin decides"
        );
    }

    #[test]
    fn a_task_due_soon_decides_between_two_matching_threads() {
        let bio = page(
            rec("bio", "Safari", "Biology assignment 2", 30),
            "https://canvas.edu/bio",
        );
        let chem = page(
            rec("chem", "Safari", "Chemistry assignment 4", 30),
            "https://canvas.edu/chem",
        );
        let mut inputs = inputs(
            "pull up the assignment",
            vec![bio, chem],
            vec![
                thread("Biology assignment 2", &["bio"]),
                thread("Chemistry assignment 4", &["chem"]),
            ],
        );
        inputs.tasks = vec![Task {
            id: "t".into(),
            title: "Submit the chemistry write-up".into(),
            description: String::new(),
            source_app: "Safari".into(),
            source_memory_id: Some("chem".into()),
            created_at: NOW - 60 * MIN,
            due_date: Some(NOW + 20 * 60 * MIN),
            is_completed: false,
            is_dismissed: false,
            task_type: crate::storage::TaskType::Todo,
            linked_urls: Vec::new(),
            linked_memory_ids: Vec::new(),
        }];
        let set = best(rank(&inputs));
        assert_eq!(set.title, "Chemistry assignment 4");
        assert!(set.reason.contains("due soon"), "{}", set.reason);
    }

    #[test]
    fn private_incognito_blocklisted_and_fndr_memories_never_become_items() {
        let ok = page(
            rec("ok", "Safari", "Essay outline", 10),
            "https://essay.example/outline",
        );
        let private = page(
            rec("private", "Safari", "Private Browsing", 11),
            "https://essay.example/p",
        );
        let incognito = page(
            rec(
                "incognito",
                "Google Chrome",
                "Essay sources - Incognito",
                12,
            ),
            "https://essay.example/i",
        );
        let blocked = page(
            rec("blocked", "Slack", "Essay feedback", 13),
            "https://slack.example/e",
        );
        let fndr = app(rec("fndr", "FNDR", "Essay search", 14), "com.fndr.app");
        let mut note = page(
            rec("note", "Safari", "Essay note", 15),
            "https://essay.example/n",
        );
        note.source_type = crate::storage::AGENT_NOTE_SOURCE_TYPE.into();
        let mut deleted = page(
            rec("deleted", "Safari", "Essay draft", 16),
            "https://essay.example/d",
        );
        deleted.is_soft_deleted = true;
        let all = [
            "ok",
            "private",
            "incognito",
            "blocked",
            "fndr",
            "note",
            "deleted",
        ];
        let mut inputs = inputs(
            "pull up the essay",
            vec![ok, private, incognito, blocked, fndr, note, deleted],
            vec![thread("Essay", &all)],
        );
        inputs.blocklist = vec!["Slack".into()];
        inputs.hits = all.iter().map(|id| hit(id, 0.6)).collect();
        assert_eq!(ids(&best(rank(&inputs))), ["ok"]);
    }

    #[test]
    fn nothing_matching_the_request_says_so() {
        let inputs = assignment_evening();
        let mut other = inputs.clone();
        other.query = "pull up everything about the tax return".into();
        other.hits.clear();
        match rank(&other) {
            Resolution::None { why } => assert!(why.contains("tax return"), "{why}"),
            got => panic!("expected none, got {got:?}"),
        }
        match rank(&Inputs::default()) {
            Resolution::None { why } => assert!(!why.is_empty()),
            got => panic!("expected none, got {got:?}"),
        }
    }

    #[test]
    fn an_app_only_item_gives_way_to_a_place_in_the_same_app() {
        let doc = page(
            rec("doc", "Google Chrome", "Budget sheet", 10),
            "https://sheets.example/b",
        );
        let chrome = app(
            rec("chrome", "Google Chrome", "Budget sheet", 5),
            "com.google.Chrome",
        );
        let mut inputs = inputs(
            "open the budget",
            vec![doc, chrome],
            vec![thread("Budget", &["doc", "chrome"])],
        );
        inputs.hits = vec![hit("doc", 0.5)];
        assert_eq!(ids(&best(rank(&inputs))), ["doc"]);
    }

    #[test]
    fn the_same_evidence_in_another_order_gives_the_same_answer() {
        let first = rank(&assignment_evening());
        let mut shuffled = assignment_evening();
        shuffled.threads.reverse();
        shuffled.hits.reverse();
        assert_eq!(rank(&shuffled), first);
    }

    #[test]
    fn the_topic_is_what_is_left_after_the_way_of_asking() {
        assert_eq!(
            topic_words("pull up everything related to the assignment I was working on"),
            ["assignment"]
        );
        assert_eq!(
            topic_words("Bring up all the stuff from the biology lab"),
            ["biology", "lab"]
        );
        assert!(topic_words("open everything I was working on").is_empty());
    }
}
