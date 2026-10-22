//! The person's own details, kept verbatim because memory keeps only summaries.
//!
//! A saved memory holds a short summary and keyword hints; the page text is
//! dropped (`memory_compaction.rs`). A summary of a page of personal details
//! says "a document of personal details" and states none of them, so nothing
//! that fills a form could find a phone number in memory.
//!
//! While Smart Fill is on, the capture pipeline hands each captured page to
//! `observe` after the privacy gate has let it through and before its text is
//! compacted away. Two things are kept, both as the page wrote them:
//!
//! - **Details**: lines of the form `Label: value`, with whatever label the
//!   page used. There is no list of known labels; which saved label answers a
//!   form's field is for the model to judge, so a form nobody planned for is
//!   filled the same way as any other.
//! - **Documents**: a page that is plainly the person's own, as an excerpt.
//!
//! Whose page it is decides everything. A labelled line may belong to someone
//! else (an email signature, a company's contact page), so a page counts as the
//! person's own only when it says so ("my details", "about me") or carries the
//! name of this Mac's user. Lines from any other page are not kept until the
//! same line has been seen in three different windows; until then only a hash
//! of it is remembered, never its text.
//!
//! Everything lives in one file in the data folder, readable only by the owner.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};

const FILE_NAME: &str = "profile_facts.json";
/// Distinct windows that must show the same line before another page's detail
/// counts as the person's own.
const SIGHTINGS_NEEDED: usize = 3;
const MAX_DETAILS: usize = 300;
const MAX_DOCUMENTS: usize = 3;
const MAX_PENDING: usize = 2_000;
const DOCUMENT_CHARS: usize = 3_000;
const MIN_DOCUMENT_CHARS: usize = 120;
const MAX_LABEL_CHARS: usize = 40;
const MAX_LABEL_WORDS: usize = 5;
const MAX_VALUE_CHARS: usize = 200;
const MAX_DETAILS_PER_PAGE: usize = 60;

static WRITE_LOCK: Mutex<()> = Mutex::new(());

/// One labelled line the person's page stated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Detail {
    pub label: String,
    pub value: String,
    pub source_app: String,
    pub source_title: String,
    pub seen_at_ms: i64,
}

/// A page that is plainly the person's own, as an excerpt of its text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Document {
    pub title: String,
    pub excerpt: String,
    pub source_app: String,
    pub seen_at_ms: i64,
}

/// A line seen on a page that is not known to be the person's own: a hash of
/// the line and of each window that showed it, no text.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Sighting {
    line: u64,
    windows: Vec<u64>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Store {
    version: u32,
    details: Vec<Detail>,
    documents: Vec<Document>,
    pending: Vec<Sighting>,
}

fn path(dir: &Path) -> PathBuf {
    dir.join(FILE_NAME)
}

fn load(dir: &Path) -> Store {
    std::fs::read(path(dir))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn save(dir: &Path, store: &Store) -> std::io::Result<()> {
    use std::io::Write;
    let target = path(dir);
    let temp = dir.join(format!("{FILE_NAME}.tmp"));
    {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temp)?;
        file.write_all(&serde_json::to_vec_pretty(store)?)?;
        file.sync_all()?;
    }
    std::fs::rename(temp, target)
}

fn fnv1a(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

// ---------------------------------------------------------------------------
// Reading labelled lines
// ---------------------------------------------------------------------------

/// A line label made of words: short, no sentence, nothing but letters, digits
/// and a few marks.
fn clean_label(raw: &str) -> Option<String> {
    let label = raw
        .trim()
        .trim_start_matches(|c: char| !c.is_alphanumeric())
        .trim_end()
        .to_string();
    let words = label.split_whitespace().count();
    let ok = !label.is_empty()
        && label.chars().count() <= MAX_LABEL_CHARS
        && (1..=MAX_LABEL_WORDS).contains(&words)
        && label.chars().any(|c| c.is_alphabetic())
        && label
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '/' | '&' | '.' | '\'' | '#'));
    ok.then_some(label)
}

fn clean_value(raw: &str) -> Option<String> {
    let value: String = raw
        .chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .trim()
        .trim_matches(|c: char| matches!(c, '"' | ';'))
        .trim()
        .to_string();
    (!value.is_empty() && value.chars().count() <= MAX_VALUE_CHARS).then_some(value)
}

/// The `Label: value` lines of `text`, as the page wrote them. A line whose
/// part before the first colon is not a short label (a sentence, a URL, a
/// time) is skipped.
pub fn extract(text: &str) -> Vec<(String, String)> {
    let mut found: Vec<(String, String)> = Vec::new();
    for line in text.lines() {
        let Some((label, value)) = line.split_once(':') else {
            continue;
        };
        // "https://..." and "10:30" are not labelled lines.
        let is_time = label.ends_with(|c: char| c.is_ascii_digit())
            && value.starts_with(|c: char| c.is_ascii_digit());
        if value.starts_with("//") || value.trim().is_empty() || is_time {
            continue;
        }
        let (Some(label), Some(value)) = (clean_label(label), clean_value(value)) else {
            continue;
        };
        if !found
            .iter()
            .any(|(l, v)| l.eq_ignore_ascii_case(&label) && *v == value)
        {
            found.push((label, value));
        }
        if found.len() >= MAX_DETAILS_PER_PAGE {
            break;
        }
    }
    found
}

// ---------------------------------------------------------------------------
// Whose page is it
// ---------------------------------------------------------------------------

/// The full name of this Mac's user, from the account.
fn account_full_name() -> Option<&'static str> {
    static NAME: OnceLock<Option<String>> = OnceLock::new();
    NAME.get_or_init(|| {
        let output = std::process::Command::new("id").arg("-F").output().ok()?;
        let name = String::from_utf8(output.stdout).ok()?.trim().to_string();
        (name.split_whitespace().count() >= 2).then_some(name)
    })
    .as_deref()
}

/// The page says these details are the person's own, or carries their name in
/// its opening lines.
fn is_theirs(text: &str, title: &str, account_name: Option<&str>) -> bool {
    const MARKERS: [&str; 8] = [
        "my details",
        "my personal",
        "my information",
        "my profile",
        "my contact",
        "about me",
        "my info",
        "my account",
    ];
    let lower = text.to_ascii_lowercase();
    let title_lower = title.to_ascii_lowercase();
    if MARKERS
        .iter()
        .any(|marker| lower.contains(marker) || title_lower.contains(marker))
    {
        return true;
    }
    let Some(name) = account_name else {
        return false;
    };
    let name = name.to_ascii_lowercase();
    let head: String = lower.chars().take(800).collect();
    head.contains(&name)
        || title_lower.contains(&name)
        || title_lower.contains(&name.replace(' ', "_"))
}

// ---------------------------------------------------------------------------
// Keeping and using
// ---------------------------------------------------------------------------

fn excerpt(text: &str) -> String {
    let mut out = String::new();
    let mut blank_run = 0;
    for line in text.lines() {
        let line = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if line.is_empty() {
            blank_run += 1;
            if blank_run > 1 {
                continue;
            }
        } else {
            blank_run = 0;
        }
        out.push_str(&line);
        out.push('\n');
        if out.chars().count() >= DOCUMENT_CHARS {
            break;
        }
    }
    out.trim().chars().take(DOCUMENT_CHARS).collect()
}

/// Reads `text` for the person's details and keeps what it states. Called by
/// capture while Smart Fill is on, after the privacy gate has passed the page.
pub fn observe(dir: &Path, text: &str, app: &str, window_title: &str) {
    observe_as(dir, text, app, window_title, account_full_name());
}

fn observe_as(dir: &Path, text: &str, app: &str, window_title: &str, account_name: Option<&str>) {
    let lines = extract(text);
    let theirs = is_theirs(text, window_title, account_name);
    let document = theirs && text.chars().count() >= MIN_DOCUMENT_CHARS;
    if lines.is_empty() && !document {
        return;
    }
    let Ok(_guard) = WRITE_LOCK.lock() else {
        return;
    };
    let mut store = load(dir);
    store.version = 2;
    let now = chrono::Utc::now().timestamp_millis();
    let window = fnv1a(window_title);

    for (label, value) in lines {
        let keep = if theirs {
            true
        } else {
            let line = fnv1a(&format!("{}\u{1}{}", label.to_ascii_lowercase(), value));
            let at = match store.pending.iter().position(|s| s.line == line) {
                Some(at) => at,
                None => {
                    store.pending.push(Sighting {
                        line,
                        windows: Vec::new(),
                    });
                    store.pending.len() - 1
                }
            };
            let sighting = &mut store.pending[at];
            if !sighting.windows.contains(&window) {
                sighting.windows.push(window);
            }
            sighting.windows.len() >= SIGHTINGS_NEEDED
        };
        if !keep {
            continue;
        }
        match store
            .details
            .iter_mut()
            .find(|d| d.label.eq_ignore_ascii_case(&label) && d.value == value)
        {
            Some(detail) => {
                detail.seen_at_ms = now;
                detail.source_app = app.to_string();
                detail.source_title = window_title.to_string();
            }
            None => store.details.push(Detail {
                label,
                value,
                source_app: app.to_string(),
                source_title: window_title.to_string(),
                seen_at_ms: now,
            }),
        }
    }
    if store.pending.len() > MAX_PENDING {
        let drop = store.pending.len() - MAX_PENDING;
        store.pending.drain(..drop);
    }
    if store.details.len() > MAX_DETAILS {
        store
            .details
            .sort_by_key(|d| std::cmp::Reverse(d.seen_at_ms));
        store.details.truncate(MAX_DETAILS);
    }

    if document {
        let text = excerpt(text);
        match store.documents.iter_mut().find(|d| d.title == window_title) {
            Some(existing) => {
                existing.excerpt = text;
                existing.source_app = app.to_string();
                existing.seen_at_ms = now;
            }
            None => store.documents.push(Document {
                title: window_title.to_string(),
                excerpt: text,
                source_app: app.to_string(),
                seen_at_ms: now,
            }),
        }
        store
            .documents
            .sort_by_key(|d| std::cmp::Reverse(d.seen_at_ms));
        store.documents.truncate(MAX_DOCUMENTS);
    }
    if let Err(error) = save(dir, &store) {
        tracing::warn!("saved details could not be written: {error}");
    }
}

/// The most recent value of each label, newest first.
fn current_details(store: &Store) -> Vec<&Detail> {
    let mut sorted: Vec<&Detail> = store.details.iter().collect();
    sorted.sort_by_key(|d| std::cmp::Reverse(d.seen_at_ms));
    let mut seen: Vec<String> = Vec::new();
    let mut out = Vec::new();
    for detail in sorted {
        let key = detail.label.to_ascii_lowercase();
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        out.push(detail);
    }
    out
}

/// The saved details and documents as text for the model, or `None` when
/// nothing is saved.
pub fn saved_details_text(dir: &Path) -> Option<String> {
    let store = load(dir);
    let details = current_details(&store);
    if details.is_empty() && store.documents.is_empty() {
        return None;
    }
    let mut out = String::new();
    for detail in details {
        out.push_str(&format!("{}: {}\n", detail.label, detail.value));
    }
    for document in &store.documents {
        out.push_str(&format!(
            "\nDOCUMENT \"{}\":\n{}\n",
            document.title, document.excerpt
        ));
    }
    Some(out.trim_end().to_string())
}

/// How many details and documents are saved, for Settings.
pub fn usable_count(dir: &Path) -> usize {
    let store = load(dir);
    current_details(&store).len() + store.documents.len()
}

/// Forgets everything saved, including the hashes of lines not yet confirmed.
pub fn forget_all(dir: &Path) -> std::io::Result<()> {
    let Ok(_guard) = WRITE_LOCK.lock() else {
        return Ok(());
    };
    match std::fs::remove_file(path(dir)) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DETAILS_PAGE: &str = "My personal details, for filling in forms about me\n\n\
        Full name: Sam Example (first name Sam, last name Example)\n\
        Phone: +1 555 010 0199\n\
        Passport number: A1234567\n\
        Date of birth: 01/02/2000\n\
        Current employer: Example Corp\n\
        Favorite editor: Helix\n\
        Shirt size: M\n\
        Home address: 100 Example Street, Springfield, Ohio";

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("fndr-saved-details-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn has(facts: &[(String, String)], label: &str, value: &str) -> bool {
        facts
            .iter()
            .any(|(l, v)| l.eq_ignore_ascii_case(label) && v == value)
    }

    #[test]
    fn any_label_is_kept_including_ones_no_form_was_planned_for() {
        let facts = extract(DETAILS_PAGE);
        assert!(has(&facts, "Phone", "+1 555 010 0199"));
        assert!(has(&facts, "Passport number", "A1234567"));
        assert!(has(&facts, "Current employer", "Example Corp"));
        assert!(has(&facts, "Favorite editor", "Helix"));
        assert!(has(&facts, "Shirt size", "M"));
        assert!(has(
            &facts,
            "Full name",
            "Sam Example (first name Sam, last name Example)"
        ));
    }

    #[test]
    fn lines_that_are_not_labelled_details_are_skipped() {
        let facts = extract(
            "Meeting at 10:30 tomorrow\nSee https://example.com/page for more\n\
             This is a long sentence that happens to contain a colon: and then more words after it\n\
             : no label\nLabel with empty value:\nTODO: \n12345: digits only label",
        );
        assert!(facts.is_empty(), "{facts:?}");
    }

    #[test]
    fn a_page_that_says_it_is_mine_is_used_at_once() {
        let dir = scratch("mine");
        observe_as(&dir, DETAILS_PAGE, "TextEdit", "details.txt", None);
        let text = saved_details_text(&dir).expect("saved");
        assert!(text.contains("Phone: +1 555 010 0199"));
        assert!(text.contains("Shirt size: M"));
        assert!(
            text.contains("DOCUMENT \"details.txt\""),
            "the page is kept as a document too"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_page_carrying_the_account_holders_name_is_theirs_without_a_marker() {
        let dir = scratch("account");
        let page = "Sam Example\nSoftware engineer\nSkills: Rust, SQL\nExperience: three years building \
                    services for Example Corp, including the payments platform and its on-call tooling";
        observe_as(&dir, page, "Preview", "Resume.pdf", Some("Sam Example"));
        let text = saved_details_text(&dir).expect("saved");
        assert!(text.contains("Skills: Rust, SQL"));
        assert!(text.contains("DOCUMENT \"Resume.pdf\""));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn someone_elses_details_are_not_kept_until_seen_in_three_windows() {
        let dir = scratch("theirs");
        let page = "From Priya Shah\nPhone: 801 555 0199\nEmail: priya@example.com";
        observe_as(&dir, page, "Mail", "Re: lease", Some("Sam Example"));
        assert_eq!(saved_details_text(&dir), None);
        observe_as(&dir, page, "Mail", "Re: lease", Some("Sam Example"));
        assert_eq!(
            saved_details_text(&dir),
            None,
            "same window twice is one sighting"
        );
        let stored = std::fs::read_to_string(path(&dir)).unwrap();
        assert!(
            !stored.contains("801 555 0199"),
            "an unconfirmed line is stored as a hash only"
        );
        observe_as(&dir, page, "Safari", "Priya's page", Some("Sam Example"));
        observe_as(&dir, page, "Slack", "Priya", Some("Sam Example"));
        let text = saved_details_text(&dir).expect("three windows");
        assert!(text.contains("Phone: 801 555 0199"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_newer_value_replaces_an_older_one_of_the_same_label() {
        let dir = scratch("newer");
        observe_as(
            &dir,
            "My details\nPhone: 555 010 0100",
            "Notes",
            "my details",
            None,
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
        observe_as(
            &dir,
            "My details\nPhone: 555 010 0111",
            "Notes",
            "my details",
            None,
        );
        let text = saved_details_text(&dir).unwrap();
        assert!(text.contains("Phone: 555 010 0111"));
        assert!(!text.contains("Phone: 555 010 0100"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn at_most_three_documents_are_kept_newest_first() {
        let dir = scratch("docs");
        let body = "My details\n".to_string() + &"line of my own text. ".repeat(20);
        for n in 0..5 {
            observe_as(&dir, &body, "Notes", &format!("doc {n}"), None);
            std::thread::sleep(std::time::Duration::from_millis(3));
        }
        let store = load(&dir);
        assert_eq!(store.documents.len(), MAX_DOCUMENTS);
        assert_eq!(store.documents[0].title, "doc 4");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn forgetting_removes_everything_and_the_file_is_private() {
        let dir = scratch("forget");
        observe_as(&dir, DETAILS_PAGE, "TextEdit", "details.txt", None);
        assert!(usable_count(&dir) > 0);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(path(&dir)).unwrap().permissions().mode();
            assert_eq!(mode & 0o077, 0, "only the owner can read it");
        }
        forget_all(&dir).unwrap();
        assert_eq!(saved_details_text(&dir), None);
        assert_eq!(usable_count(&dir), 0);
        forget_all(&dir).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
