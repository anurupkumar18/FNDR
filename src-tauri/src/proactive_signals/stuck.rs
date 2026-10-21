//! The stuck detector: the same error line, or the same unchanging page,
//! on screen across ten minutes of captures. When an older memory matches
//! it, FNDR points there.
//!
//! An error line is read from the captured screen text, never from a
//! model's list of errors: the evidence rule in AGENTS.md.

use crate::storage::MemoryRecord;
use crate::AppState;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

/// An episode lasts at least this long, first capture to last.
pub const MIN_STUCK_MS: i64 = 10 * 60_000;
/// How far back the detector reads.
pub const LOOKBACK_MS: i64 = 30 * 60_000;
/// An episode is current only while its newest capture is this recent.
pub const STILL_ON_SCREEN_MS: i64 = 3 * 60_000;
/// A past match predates the episode by this much, so the episode does not
/// find itself or the minutes just before it.
pub const PAST_GAP_MS: i64 = 60 * 60_000;
/// Forced captures come once a minute on a still screen, so ten minutes
/// leaves several.
const MIN_CAPTURES: usize = 4;
/// Of all captures across the episode's span, the share that must show the
/// error. Searching the error in a browser between builds is part of being
/// stuck, so this is lower than for a page.
const MIN_ERROR_SHARE: f32 = 0.5;
const MIN_PAGE_SHARE: f32 = 0.8;
/// Word overlap between the first and last capture of a page episode. A page
/// being read or edited changes more than this.
const MIN_PAGE_TEXT_OVERLAP: f32 = 0.8;
const MAX_ERROR_LINES_PER_CAPTURE: usize = 5;
const MAX_QUERY_CHARS: usize = 200;

/// Lowercase markers of an error line. Each is specific to tool output, so
/// prose about errors ("error handling", "with the exception of") is not one.
const ERROR_MARKERS: &[&str] = &[
    "error:",
    "error[",
    "exception:",
    "exception in thread",
    "traceback (most recent call last)",
    "panicked at",
    "fatal:",
    "npm err!",
    "command not found",
    "no such file or directory",
    "segmentation fault",
    "failed to compile",
    "build failed",
    "cannot find module",
    "is not defined",
    "is not a function",
    "permission denied",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StuckKind {
    Error,
    Page,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StuckEpisode {
    pub kind: StuckKind,
    /// The normalized error line, or app and window title: what stays the
    /// same across the episode.
    pub signature: String,
    /// What older memories are searched with: the error line as captured, or
    /// the window title.
    pub query: String,
    pub started_at: i64,
    pub last_seen_at: i64,
    pub captures: usize,
    pub memory_ids: Vec<String>,
}

impl StuckEpisode {
    pub fn minutes(&self) -> i64 {
        (self.last_seen_at - self.started_at) / 60_000
    }

    /// Identifies the issue for the once-a-day limit without holding any
    /// screen text.
    pub fn issue_key(&self) -> String {
        let digest = Sha256::digest(format!("{:?}\n{}", self.kind, self.signature));
        let hex: String = digest.iter().take(12).map(|b| format!("{b:02x}")).collect();
        format!("stuck:{hex}")
    }
}

/// An older memory that matched the episode's text.
#[derive(Debug, Clone, PartialEq)]
pub struct PastMatch {
    pub memory_id: String,
    pub timestamp: i64,
}

/// Error lines in a capture's text, as captured, at most five.
pub fn error_lines(text: &str) -> Vec<&str> {
    text.lines()
        .map(str::trim)
        .filter(|line| (12..=300).contains(&line.chars().count()))
        .filter(|line| {
            let lower = line.to_lowercase();
            ERROR_MARKERS.iter().any(|marker| lower.contains(marker))
        })
        .take(MAX_ERROR_LINES_PER_CAPTURE)
        .collect()
}

/// Lowercase with every run of digits as `#`, so the same error matches as
/// line numbers, counts and addresses move.
pub fn normalize_error(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut in_digits = false;
    for ch in line.to_lowercase().chars() {
        if ch.is_ascii_digit() {
            if !in_digits {
                out.push('#');
            }
            in_digits = true;
            continue;
        }
        in_digits = false;
        out.push(ch);
    }
    out.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(160)
        .collect()
}

fn screen_text(record: &MemoryRecord) -> &str {
    if record.clean_text.trim().is_empty() {
        &record.text
    } else {
        &record.clean_text
    }
}

fn page_signature(record: &MemoryRecord) -> Option<String> {
    let title = record.window_title.trim();
    let app = record.app_name.trim();
    if title.is_empty() || title.eq_ignore_ascii_case(app) {
        return None;
    }
    Some(format!("{}|{}", app.to_lowercase(), normalize_error(title)))
}

fn words(text: &str) -> HashSet<String> {
    text.split(|ch: char| !ch.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect()
}

fn overlap(a: &str, b: &str) -> f32 {
    let (a, b) = (words(a), words(b));
    let union = a.union(&b).count();
    if union == 0 {
        return 0.0;
    }
    a.intersection(&b).count() as f32 / union as f32
}

/// The episode for one signature, when its captures qualify: enough of
/// them, across ten minutes, still on screen, and most of what was captured
/// in that span.
fn episode_from(
    kind: StuckKind,
    signature: &str,
    query: &str,
    rows: &[&MemoryRecord],
    indexes: &[usize],
    now_ms: i64,
    min_share: f32,
) -> Option<StuckEpisode> {
    let (first, last) = (*indexes.first()?, *indexes.last()?);
    let started_at = rows[first].timestamp;
    let last_seen_at = rows[last].timestamp;
    let in_span = last - first + 1;
    let share = indexes.len() as f32 / in_span as f32;
    let qualifies = indexes.len() >= MIN_CAPTURES
        && last_seen_at - started_at >= MIN_STUCK_MS
        && last_seen_at >= now_ms - STILL_ON_SCREEN_MS
        && share >= min_share;
    qualifies.then(|| StuckEpisode {
        kind,
        signature: signature.to_string(),
        query: query.chars().take(MAX_QUERY_CHARS).collect(),
        started_at,
        last_seen_at,
        captures: indexes.len(),
        memory_ids: indexes.iter().map(|&i| rows[i].id.clone()).collect(),
    })
}

/// The episode the person is in now, if any, from captures of the last half
/// hour. Only captures that any surface may show count: hidden, blocklisted
/// and FNDR's own rows are left out. An error episode wins over a page one,
/// then the longer episode.
pub fn detect_stuck(
    records: &[MemoryRecord],
    now_ms: i64,
    blocklist: &[String],
) -> Option<StuckEpisode> {
    let mut rows: Vec<&MemoryRecord> = records
        .iter()
        .filter(|r| (now_ms - LOOKBACK_MS..=now_ms).contains(&r.timestamp))
        .filter(|r| crate::resume::admits(r, blocklist))
        .collect();
    rows.sort_by(|a, b| a.timestamp.cmp(&b.timestamp).then_with(|| a.id.cmp(&b.id)));
    if rows.last()?.timestamp < now_ms - STILL_ON_SCREEN_MS {
        return None;
    }

    let mut errors: HashMap<String, (String, Vec<usize>)> = HashMap::new();
    let mut pages: HashMap<String, Vec<usize>> = HashMap::new();
    for (index, row) in rows.iter().enumerate() {
        for line in error_lines(screen_text(row)) {
            let entry = errors
                .entry(normalize_error(line))
                .or_insert_with(|| (line.to_string(), Vec::new()));
            if entry.1.last() != Some(&index) {
                entry.1.push(index);
            }
        }
        if let Some(signature) = page_signature(row) {
            pages.entry(signature).or_default().push(index);
        }
    }

    let error_episodes = errors.iter().filter_map(|(signature, (line, indexes))| {
        episode_from(
            StuckKind::Error,
            signature,
            line,
            &rows,
            indexes,
            now_ms,
            MIN_ERROR_SHARE,
        )
    });
    let page_episodes = pages.iter().filter_map(|(signature, indexes)| {
        let first = rows[*indexes.first()?];
        let last = rows[*indexes.last()?];
        if overlap(screen_text(first), screen_text(last)) < MIN_PAGE_TEXT_OVERLAP {
            return None;
        }
        episode_from(
            StuckKind::Page,
            signature,
            last.window_title.trim(),
            &rows,
            indexes,
            now_ms,
            MIN_PAGE_SHARE,
        )
    });
    error_episodes.chain(page_episodes).max_by(|a, b| {
        (a.kind == StuckKind::Error)
            .cmp(&(b.kind == StuckKind::Error))
            .then_with(|| (a.last_seen_at - a.started_at).cmp(&(b.last_seen_at - b.started_at)))
            .then_with(|| b.signature.cmp(&a.signature))
    })
}

/// The best-ranked hit, given in rank order as `(memory id, timestamp)`,
/// from at least an hour before the episode began.
pub fn pick_past_match(hits: &[(String, i64)], episode: &StuckEpisode) -> Option<PastMatch> {
    hits.iter()
        .find(|(id, timestamp)| {
            *timestamp < episode.started_at - PAST_GAP_MS && !episode.memory_ids.contains(id)
        })
        .map(|(id, timestamp)| PastMatch {
            memory_id: id.clone(),
            timestamp: *timestamp,
        })
}

/// "Oct 3" for a memory's local date.
pub fn date_label(timestamp_ms: i64) -> String {
    use chrono::TimeZone;
    chrono::Local
        .timestamp_millis_opt(timestamp_ms)
        .single()
        .map(|at| at.format("%b %-d").to_string())
        .unwrap_or_default()
}

/// The toast: how long, and the date of the older memory it opens. It holds
/// no screen text. It says "saw", not "solved": a match shows the same text
/// was on screen before, not that it was fixed then.
pub fn stuck_toast(episode: &StuckEpisode, past_date: &str) -> (String, String) {
    let what = match episode.kind {
        StuckKind::Error => "this error",
        StuckKind::Page => "this screen",
    };
    (
        "Been on this a while?".to_string(),
        format!(
            "You have been on {what} for {} minutes. You saw something similar on {past_date}; open it to see what you did then.",
            episode.minutes()
        ),
    )
}

/// The current episode and an older memory that matches it, from the
/// stored captures and the hybrid search every surface uses.
pub async fn find_stuck(state: &AppState, now_ms: i64) -> Option<(StuckEpisode, PastMatch)> {
    let blocklist = state.config.read().blocklist.clone();
    let records = state
        .store
        .get_memories_in_range(now_ms - LOOKBACK_MS, now_ms)
        .await
        .ok()?;
    let episode = detect_stuck(&records, now_ms, &blocklist)?;
    let past = past_match_for(state, &episode).await?;
    Some((episode, past))
}

/// Searches every memory with the episode's text and keeps a strong match
/// from before it.
pub async fn past_match_for(state: &AppState, episode: &StuckEpisode) -> Option<PastMatch> {
    let request = crate::context_runtime::RetrieveRequest {
        query: episode.query.clone(),
        time: None,
        app: None,
        limit: 20,
    };
    let (retrieved, results) = crate::context_runtime::retrieve_search_results(state, &request)
        .await
        .ok()?;
    if !retrieved.strong_match {
        return None;
    }
    let hits: Vec<(String, i64)> = results
        .into_iter()
        .map(|result| (result.id, result.timestamp))
        .collect();
    pick_past_match(&hits, episode)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_800_000_000_000;
    const MIN: i64 = 60_000;

    fn capture(id: &str, minutes_ago: i64, app: &str, title: &str, text: &str) -> MemoryRecord {
        MemoryRecord {
            id: id.into(),
            timestamp: NOW - minutes_ago * MIN,
            app_name: app.into(),
            window_title: title.into(),
            clean_text: text.into(),
            ocr_block_count: 5,
            ocr_confidence: 0.9,
            ..Default::default()
        }
    }

    fn compile_error(id: &str, minutes_ago: i64, line: i64) -> MemoryRecord {
        capture(
            id,
            minutes_ago,
            "Terminal",
            "fndr: cargo build",
            &format!(
                "Compiling fndr v0.1.0 attempt {minutes_ago}\n\
                 error[E0308]: mismatched types at src/main.rs:{line}:7\n\
                 expected `String`, found `&str` while building the parser"
            ),
        )
    }

    #[test]
    fn the_same_error_for_ten_minutes_is_stuck_even_as_line_numbers_move() {
        let records: Vec<_> = (0..8)
            .map(|i| compile_error(&format!("e{i}"), 15 - i * 2, 40 + i))
            .collect();
        let episode = detect_stuck(&records, NOW, &[]).expect("stuck");
        assert_eq!(episode.kind, StuckKind::Error);
        assert_eq!(episode.captures, 8);
        assert!(episode.minutes() >= 10, "{}", episode.minutes());
        assert!(episode.query.starts_with("error[E0308]: mismatched types"));
        assert_eq!(episode.memory_ids.len(), 8);
    }

    #[test]
    fn an_error_seen_for_a_few_minutes_is_not_stuck() {
        let records: Vec<_> = (0..4)
            .map(|i| compile_error(&format!("e{i}"), 7 - i * 2, 40))
            .collect();
        assert_eq!(detect_stuck(&records, NOW, &[]), None);
    }

    #[test]
    fn an_error_that_left_the_screen_is_not_stuck() {
        let mut records: Vec<_> = (0..6)
            .map(|i| compile_error(&format!("e{i}"), 25 - i * 2, 40))
            .collect();
        for minutes_ago in [9, 6, 3, 1] {
            records.push(capture(
                &format!("ok{minutes_ago}"),
                minutes_ago,
                "Terminal",
                "fndr: cargo build",
                &format!("Finished dev profile target in {minutes_ago}.2s and all tests passed"),
            ));
        }
        assert_eq!(detect_stuck(&records, NOW, &[]), None);
    }

    #[test]
    fn an_error_glimpsed_between_other_work_is_not_stuck() {
        let mut records: Vec<_> = [14, 10, 6, 1]
            .iter()
            .map(|&m| compile_error(&format!("e{m}"), m, 40))
            .collect();
        for m in [13, 12, 11, 9, 8, 7, 5, 4, 3, 2] {
            records.push(capture(
                &format!("w{m}"),
                m,
                "Google Chrome",
                "Quarterly planning doc",
                &format!("Planning notes {m}: topic{m} owner{m} budget{m} hiring{m} quarter{m}"),
            ));
        }
        assert_eq!(detect_stuck(&records, NOW, &[]), None);
    }

    #[test]
    fn a_page_unchanged_for_ten_minutes_is_stuck_but_a_changing_one_is_not() {
        let text = "Configure the OAuth redirect URI in the console before testing the flow";
        let still: Vec<_> = (0..6)
            .map(|i| {
                capture(
                    &format!("p{i}"),
                    13 - i * 2,
                    "Google Chrome",
                    "OAuth setup guide",
                    text,
                )
            })
            .collect();
        let episode = detect_stuck(&still, NOW, &[]).expect("stuck on a page");
        assert_eq!(episode.kind, StuckKind::Page);
        assert_eq!(episode.query, "OAuth setup guide");

        let reading: Vec<_> = (0..6)
            .map(|i| {
                capture(
                    &format!("r{i}"),
                    13 - i * 2,
                    "Google Chrome",
                    "OAuth setup guide",
                    &format!(
                        "Section {i} covers topic{i} alpha{i} beta{i} gamma{i} delta{i} and more"
                    ),
                )
            })
            .collect();
        assert_eq!(detect_stuck(&reading, NOW, &[]), None);
    }

    #[test]
    fn blocklisted_captures_never_make_an_episode() {
        let records: Vec<_> = (0..8)
            .map(|i| compile_error(&format!("e{i}"), 15 - i * 2, 40))
            .collect();
        assert_eq!(detect_stuck(&records, NOW, &["Terminal".to_string()]), None);
    }

    #[test]
    fn prose_about_errors_is_not_an_error_line() {
        assert!(error_lines("Error handling in Rust is explicit and typed").is_empty());
        assert!(error_lines("with the exception of two cases").is_empty());
        assert_eq!(
            error_lines("ok\nTypeError: x.map is not a function\nmore"),
            vec!["TypeError: x.map is not a function"]
        );
        assert_eq!(
            normalize_error("error[E0308]: mismatched types at src/main.rs:42:7"),
            normalize_error("error[E0308]: mismatched types at src/main.rs:97:12")
        );
    }

    #[test]
    fn a_past_match_predates_the_episode_and_is_not_part_of_it() {
        let records: Vec<_> = (0..8)
            .map(|i| compile_error(&format!("e{i}"), 15 - i * 2, 40))
            .collect();
        let episode = detect_stuck(&records, NOW, &[]).unwrap();
        let hits = vec![
            ("e3".to_string(), NOW - 9 * MIN),
            ("earlier-today".to_string(), NOW - 30 * MIN),
            ("last-week".to_string(), NOW - 7 * 24 * 60 * MIN),
            ("older".to_string(), NOW - 9 * 24 * 60 * MIN),
        ];
        let past = pick_past_match(&hits, &episode).expect("past match");
        assert_eq!(past.memory_id, "last-week");
        assert_eq!(pick_past_match(&hits[..2], &episode), None);
    }

    #[test]
    fn the_issue_key_is_stable_and_never_holds_screen_text() {
        let records: Vec<_> = (0..8)
            .map(|i| compile_error(&format!("e{i}"), 15 - i * 2, 40))
            .collect();
        let first = detect_stuck(&records, NOW, &[]).unwrap();
        let shifted: Vec<_> = (0..8)
            .map(|i| compile_error(&format!("x{i}"), 14 - i * 2, 90))
            .collect();
        let second = detect_stuck(&shifted, NOW, &[]).unwrap();
        assert_eq!(first.issue_key(), second.issue_key());
        assert!(!first.issue_key().contains("mismatched"));
    }

    #[test]
    fn the_toast_names_minutes_and_a_date_and_no_screen_text() {
        let records: Vec<_> = (0..8)
            .map(|i| compile_error(&format!("e{i}"), 15 - i * 2, 40))
            .collect();
        let episode = detect_stuck(&records, NOW, &[]).unwrap();
        let (title, body) = stuck_toast(&episode, "Oct 3");
        assert!(!title.is_empty());
        assert!(
            body.contains("14 minutes") && body.contains("Oct 3"),
            "{body}"
        );
        assert!(!body.contains("mismatched") && !title.contains("mismatched"));
    }
}
