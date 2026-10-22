//! The person's own details, kept as facts because memory keeps only summaries.
//!
//! A saved memory holds a short summary and keyword hints; the page text is
//! dropped (`memory_compaction.rs`). A summary of a page of personal details
//! says "a document of personal details" and states none of them, so nothing
//! that fills a form could find a phone number or a passport number in memory.
//!
//! While Smart Fill is on, the capture pipeline hands each captured page to
//! `observe` after the privacy gate has let it through and before its text is
//! compacted away. Lines that state a detail with a label ("Phone: ...",
//! "Date of birth: ...") become facts, by fixed rules and not by a model, so a
//! fact is always text that was on screen. Facts live in one small file in the
//! data folder, each with where and when it was seen.
//!
//! A labelled line may belong to someone else (a signature in an email, a
//! company's contact page). A fact is used only when the page says it is the
//! person's own ("my details", "about me", "profile") or the same value has
//! been seen in three different windows.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};

const FILE_NAME: &str = "profile_facts.json";
/// Distinct windows that must show the same value before an unmarked page's
/// detail counts as the person's own.
const SIGHTINGS_NEEDED: usize = 3;
const MAX_FACTS: usize = 400;

static WRITE_LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FactKind {
    FullName,
    FirstName,
    LastName,
    Phone,
    Email,
    DateOfBirth,
    PassportNumber,
    Nationality,
    University,
    Degree,
    Gpa,
    GraduationDate,
    Address,
    City,
    State,
    PostalCode,
    Country,
    PreferredName,
    Pronouns,
    LinkedIn,
    GitHub,
    Website,
    CurrentEmployer,
    JobTitle,
    YearsExperience,
    WorkAuthorization,
    NeedsSponsorship,
    StartDate,
    Skills,
}

impl FactKind {
    /// How the fact is named when it is shown to the model.
    pub fn label(self) -> &'static str {
        match self {
            Self::FullName => "Full name",
            Self::FirstName => "First name",
            Self::LastName => "Last name",
            Self::Phone => "Phone",
            Self::Email => "Email",
            Self::DateOfBirth => "Date of birth",
            Self::PassportNumber => "Passport number",
            Self::Nationality => "Nationality",
            Self::University => "University",
            Self::Degree => "Degree",
            Self::Gpa => "GPA",
            Self::GraduationDate => "Graduation date",
            Self::Address => "Home address",
            Self::City => "City",
            Self::State => "State",
            Self::PostalCode => "ZIP / postal code",
            Self::Country => "Country",
            Self::PreferredName => "Preferred name",
            Self::Pronouns => "Pronouns",
            Self::LinkedIn => "LinkedIn",
            Self::GitHub => "GitHub",
            Self::Website => "Website",
            Self::CurrentEmployer => "Current employer",
            Self::JobTitle => "Job title",
            Self::YearsExperience => "Years of experience",
            Self::WorkAuthorization => "Work authorization",
            Self::NeedsSponsorship => "Needs visa sponsorship",
            Self::StartDate => "Available to start",
            Self::Skills => "Skills",
        }
    }
}

/// One detail, with where and when it was seen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fact {
    pub kind: FactKind,
    pub value: String,
    pub source_app: String,
    pub source_title: String,
    pub seen_at_ms: i64,
    /// The page said these details are the person's own.
    pub owned: bool,
    /// Windows (by title) that showed this value, for the unmarked case.
    #[serde(default)]
    pub windows: Vec<String>,
}

impl Fact {
    fn usable(&self) -> bool {
        self.owned || self.windows.len() >= SIGHTINGS_NEEDED
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Store {
    version: u32,
    facts: Vec<Fact>,
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

// ---------------------------------------------------------------------------
// Reading labelled lines
// ---------------------------------------------------------------------------

fn label_kind(label: &str) -> Option<FactKind> {
    let label = label
        .trim()
        .trim_start_matches(|c: char| !c.is_alphanumeric())
        .to_ascii_lowercase();
    let label = label.trim_end_matches('#').trim();
    Some(match label {
        "name" | "full name" | "legal name" | "my name" => FactKind::FullName,
        "first name" | "given name" => FactKind::FirstName,
        "last name" | "family name" | "surname" => FactKind::LastName,
        "phone" | "phone number" | "mobile" | "mobile number" | "cell" | "cell phone"
        | "telephone" | "tel" => FactKind::Phone,
        "email" | "e-mail" | "email address" => FactKind::Email,
        "date of birth" | "dob" | "birthday" | "birth date" | "born" => FactKind::DateOfBirth,
        "passport number" | "passport no" | "passport" | "passport no." => FactKind::PassportNumber,
        "nationality" | "citizenship" => FactKind::Nationality,
        "university" | "college" | "school" => FactKind::University,
        "degree" | "major" | "field of study" => FactKind::Degree,
        "gpa" | "grade point average" => FactKind::Gpa,
        "graduation" | "graduation date" | "expected graduation" | "graduation year" => {
            FactKind::GraduationDate
        }
        "address" | "home address" | "street address" | "mailing address" => FactKind::Address,
        "city" | "town" => FactKind::City,
        "state" | "province" | "state / province" => FactKind::State,
        "zip" | "zip code" | "postal code" | "postcode" | "zip / postal code" => {
            FactKind::PostalCode
        }
        "country" | "country of residence" => FactKind::Country,
        "preferred name" | "nickname" | "goes by" => FactKind::PreferredName,
        "pronouns" => FactKind::Pronouns,
        "linkedin" | "linkedin url" | "linkedin profile" => FactKind::LinkedIn,
        "github" | "github url" | "github profile" => FactKind::GitHub,
        "website" | "portfolio" | "personal website" | "portfolio url" | "homepage" => {
            FactKind::Website
        }
        "current employer" | "current company" | "employer" | "company" => {
            FactKind::CurrentEmployer
        }
        "job title" | "current title" | "title" | "current role" | "role" => FactKind::JobTitle,
        "years of experience" | "experience" | "total experience" => FactKind::YearsExperience,
        "work authorization" | "work authorisation" | "authorized to work" | "visa status" => {
            FactKind::WorkAuthorization
        }
        "sponsorship" | "requires sponsorship" | "needs sponsorship" | "visa sponsorship" => {
            FactKind::NeedsSponsorship
        }
        "available to start" | "start date" | "availability" | "earliest start date" => {
            FactKind::StartDate
        }
        "skills" | "technical skills" | "key skills" => FactKind::Skills,
        _ => return None,
    })
}

fn only_name_characters(text: &str) -> bool {
    text.chars()
        .all(|c| c.is_alphabetic() || matches!(c, ' ' | '\'' | '-' | '.'))
}

fn digits(text: &str) -> usize {
    text.chars().filter(char::is_ascii_digit).count()
}

/// The value as the page wrote it, kept only when it looks like what the label
/// says it is. A trailing parenthetical (a spelled-out repeat) is dropped.
fn clean_value(kind: FactKind, raw: &str) -> Option<String> {
    let value = raw.split(" (").next().unwrap_or(raw).trim();
    let value = value.trim_matches(|c: char| matches!(c, '"' | '\'' | ',' | ';') || c.is_control());
    let value = value.trim();
    if value.is_empty() || value.chars().count() > 120 {
        return None;
    }
    let words = value.split_whitespace().count();
    let ok = match kind {
        FactKind::FullName => (2..=5).contains(&words) && only_name_characters(value),
        FactKind::FirstName | FactKind::LastName => words == 1 && only_name_characters(value),
        FactKind::Phone => (7..=15).contains(&digits(value)) && !value.contains('@'),
        FactKind::Email => {
            !value.contains(char::is_whitespace)
                && value
                    .split_once('@')
                    .is_some_and(|(local, domain)| !local.is_empty() && domain.contains('.'))
        }
        FactKind::DateOfBirth => return normalize_date(value),
        FactKind::PassportNumber => {
            (6..=12).contains(&value.chars().count())
                && value.chars().all(|c| c.is_ascii_alphanumeric())
                && value.chars().any(|c| c.is_ascii_digit())
        }
        FactKind::Nationality => words <= 3 && only_name_characters(value),
        FactKind::University => value.chars().count() >= 3,
        FactKind::Degree => value.chars().count() >= 2,
        FactKind::Gpa => {
            let number: String = value
                .chars()
                .take_while(|c| c.is_ascii_digit() || *c == '.')
                .collect();
            return number
                .parse::<f32>()
                .ok()
                .filter(|gpa| (0.0..=5.0).contains(gpa))
                .map(|_| number);
        }
        FactKind::GraduationDate => {
            value.chars().count() <= 40
                && value
                    .split(|c: char| !c.is_ascii_digit())
                    .any(|part| part.len() == 4 && part.starts_with(['2', '1']))
        }
        FactKind::Address => {
            value.chars().count() >= 8
                && value.chars().any(|c| c.is_ascii_digit())
                && value.chars().any(|c| c.is_alphabetic())
        }
        FactKind::City | FactKind::State | FactKind::Country | FactKind::PreferredName => {
            words <= 4 && only_name_characters(value)
        }
        FactKind::PostalCode => {
            (3..=10).contains(&value.chars().count())
                && value
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == ' ')
                && value.chars().any(|c| c.is_ascii_digit())
        }
        FactKind::Pronouns => value.contains('/') && value.chars().count() <= 24,
        FactKind::LinkedIn => value.to_ascii_lowercase().contains("linkedin.com/"),
        FactKind::GitHub => value.to_ascii_lowercase().contains("github.com/"),
        FactKind::Website => {
            let lower = value.to_ascii_lowercase();
            (lower.starts_with("http://") || lower.starts_with("https://") || lower.contains('.'))
                && !value.contains(char::is_whitespace)
        }
        FactKind::CurrentEmployer | FactKind::JobTitle => {
            value.chars().count() >= 2 && value.chars().count() <= 80
        }
        FactKind::YearsExperience => value
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '.')
            .collect::<String>()
            .parse::<f32>()
            .is_ok_and(|years| (0.0..=60.0).contains(&years)),
        FactKind::WorkAuthorization => value.chars().count() >= 2 && value.chars().count() <= 80,
        FactKind::NeedsSponsorship => {
            matches!(
                value.to_ascii_lowercase().as_str(),
                "yes" | "no" | "true" | "false"
            )
        }
        FactKind::StartDate => value.chars().count() <= 40,
        FactKind::Skills => value.chars().count() >= 2 && value.chars().count() <= 120,
    };
    ok.then(|| value.to_string())
}

/// A date as month/day/year with a four digit year.
fn normalize_date(value: &str) -> Option<String> {
    for format in [
        "%m/%d/%Y",
        "%Y-%m-%d",
        "%B %d, %Y",
        "%b %d, %Y",
        "%d %B %Y",
        "%d %b %Y",
        "%m-%d-%Y",
        "%m.%d.%Y",
    ] {
        if let Ok(date) = NaiveDate::parse_from_str(value, format) {
            // "%Y" reads "05" as the year 5; a date of birth has four digits.
            if (1900..=2200).contains(&date.year()) {
                return Some(date.format("%m/%d/%Y").to_string());
            }
        }
    }
    None
}

/// Labelled lines of `text`, as (kind, value).
pub fn extract(text: &str) -> Vec<(FactKind, String)> {
    let mut found: Vec<(FactKind, String)> = Vec::new();
    for line in text.lines() {
        let Some((label, value)) = line.split_once(':') else {
            continue;
        };
        if label.chars().count() > 30 {
            continue;
        }
        let Some(kind) = label_kind(label) else {
            continue;
        };
        let Some(value) = clean_value(kind, value) else {
            continue;
        };
        if !found.iter().any(|(k, v)| *k == kind && *v == value) {
            found.push((kind, value));
        }
    }
    found
}

/// The page says these details are the person's own.
fn says_it_is_theirs(text: &str, title: &str) -> bool {
    const MARKERS: [&str; 9] = [
        "my details",
        "my personal",
        "my information",
        "my profile",
        "my contact",
        "about me",
        "my resume",
        "my cv",
        "my info",
    ];
    let text = text.to_ascii_lowercase();
    let title = title.to_ascii_lowercase();
    MARKERS
        .iter()
        .any(|marker| text.contains(marker) || title.contains(marker))
}

// ---------------------------------------------------------------------------
// Keeping and using facts
// ---------------------------------------------------------------------------

/// Reads `text` for the person's details and keeps what it states. Called by
/// capture while Smart Fill is on, after the privacy gate has passed the page.
pub fn observe(dir: &Path, text: &str, app: &str, window_title: &str) {
    let found = extract(text);
    if found.is_empty() {
        return;
    }
    observe_found(
        dir,
        found,
        says_it_is_theirs(text, window_title),
        app,
        window_title,
    );
}

fn observe_found(
    dir: &Path,
    found: Vec<(FactKind, String)>,
    owned: bool,
    app: &str,
    window_title: &str,
) {
    let Ok(_guard) = WRITE_LOCK.lock() else {
        return;
    };
    let mut store = load(dir);
    store.version = 1;
    let now = chrono::Utc::now().timestamp_millis();
    for (kind, value) in found {
        let existing = store
            .facts
            .iter_mut()
            .find(|fact| fact.kind == kind && fact.value == value);
        match existing {
            Some(fact) => {
                fact.seen_at_ms = now;
                fact.owned |= owned;
                fact.source_app = app.to_string();
                fact.source_title = window_title.to_string();
                if !fact.windows.iter().any(|w| w == window_title) {
                    fact.windows.push(window_title.to_string());
                }
            }
            None => store.facts.push(Fact {
                kind,
                value,
                source_app: app.to_string(),
                source_title: window_title.to_string(),
                seen_at_ms: now,
                owned,
                windows: vec![window_title.to_string()],
            }),
        }
    }
    if store.facts.len() > MAX_FACTS {
        store
            .facts
            .sort_by_key(|fact| std::cmp::Reverse(fact.seen_at_ms));
        store.facts.truncate(MAX_FACTS);
    }
    if let Err(error) = save(dir, &store) {
        tracing::warn!("profile facts could not be saved: {error}");
    }
}

/// The most recent usable value of each kind, in a fixed order.
pub fn current(dir: &Path) -> Vec<Fact> {
    let store = load(dir);
    let order = [
        FactKind::FullName,
        FactKind::FirstName,
        FactKind::LastName,
        FactKind::Email,
        FactKind::Phone,
        FactKind::DateOfBirth,
        FactKind::Address,
        FactKind::Nationality,
        FactKind::PassportNumber,
        FactKind::University,
        FactKind::Degree,
        FactKind::Gpa,
        FactKind::GraduationDate,
        FactKind::City,
        FactKind::State,
        FactKind::PostalCode,
        FactKind::Country,
        FactKind::PreferredName,
        FactKind::Pronouns,
        FactKind::LinkedIn,
        FactKind::GitHub,
        FactKind::Website,
        FactKind::CurrentEmployer,
        FactKind::JobTitle,
        FactKind::YearsExperience,
        FactKind::WorkAuthorization,
        FactKind::NeedsSponsorship,
        FactKind::StartDate,
        FactKind::Skills,
    ];
    order
        .iter()
        .filter_map(|kind| {
            store
                .facts
                .iter()
                .filter(|fact| fact.kind == *kind && fact.usable())
                .max_by_key(|fact| fact.seen_at_ms)
                .cloned()
        })
        .collect()
}

/// The saved details as one block of lines for the model, or `None` when
/// nothing usable is saved.
pub fn saved_details_text(dir: &Path) -> Option<String> {
    let facts = current(dir);
    if facts.is_empty() {
        return None;
    }
    Some(
        facts
            .iter()
            .map(|fact| format!("{}: {}", fact.kind.label(), fact.value))
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

/// How many details are saved and usable, for Settings.
pub fn usable_count(dir: &Path) -> usize {
    current(dir).len()
}

/// Forgets every saved detail.
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
        Nationality: Canadian\n\
        Date of birth: 01/02/2000 (January 2, 2000)\n\
        University: Example University\n\
        Degree: B.S. in Computer Science\n\
        GPA: 3.50\n\
        Expected graduation: May 2030\n\
        Home address: 100 Example Street, Springfield, Ohio";

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("fndr-profile-facts-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn value_of(facts: &[(FactKind, String)], kind: FactKind) -> Option<&str> {
        facts
            .iter()
            .find(|(k, _)| *k == kind)
            .map(|(_, v)| v.as_str())
    }

    #[test]
    fn a_page_of_details_gives_every_labelled_fact() {
        let facts = extract(DETAILS_PAGE);
        assert_eq!(value_of(&facts, FactKind::FullName), Some("Sam Example"));
        assert_eq!(value_of(&facts, FactKind::Phone), Some("+1 555 010 0199"));
        assert_eq!(value_of(&facts, FactKind::PassportNumber), Some("A1234567"));
        assert_eq!(value_of(&facts, FactKind::Nationality), Some("Canadian"));
        assert_eq!(value_of(&facts, FactKind::DateOfBirth), Some("01/02/2000"));
        assert_eq!(
            value_of(&facts, FactKind::University),
            Some("Example University")
        );
        assert_eq!(
            value_of(&facts, FactKind::Degree),
            Some("B.S. in Computer Science")
        );
        assert_eq!(value_of(&facts, FactKind::Gpa), Some("3.50"));
        assert_eq!(value_of(&facts, FactKind::GraduationDate), Some("May 2030"));
        assert_eq!(
            value_of(&facts, FactKind::Address),
            Some("100 Example Street, Springfield, Ohio")
        );
    }

    #[test]
    fn values_that_do_not_look_like_their_label_are_dropped() {
        let facts = extract(
            "Phone: call me maybe\nEmail: not an email\nGPA: excellent\n\
             Passport number: 12\nDate of birth: someday\nName: X\nGPA: 9.9",
        );
        assert!(facts.is_empty(), "{facts:?}");
    }

    #[test]
    fn dates_are_written_month_day_year() {
        assert_eq!(normalize_date("2000-01-02").as_deref(), Some("01/02/2000"));
        assert_eq!(
            normalize_date("January 2, 2000").as_deref(),
            Some("01/02/2000")
        );
        assert_eq!(normalize_date("2 Jan 2000").as_deref(), Some("01/02/2000"));
        assert_eq!(normalize_date("01/02/00"), None);
    }

    #[test]
    fn details_on_a_page_that_says_they_are_mine_are_used_at_once() {
        let dir = scratch("mine");
        observe(&dir, DETAILS_PAGE, "TextEdit", "details.txt");
        let text = saved_details_text(&dir).expect("saved");
        assert!(text.contains("Phone: +1 555 010 0199"));
        assert!(text.contains("Passport number: A1234567"));
        assert!(text.starts_with("Full name: Sam Example"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn someone_elses_details_are_not_used_until_seen_in_three_windows() {
        let dir = scratch("theirs");
        let page = "From Priya Shah\nPhone: 801 555 0199\nEmail: priya@example.com";
        observe(&dir, page, "Mail", "Re: lease");
        assert_eq!(saved_details_text(&dir), None);
        observe(&dir, page, "Mail", "Re: lease");
        assert_eq!(
            saved_details_text(&dir),
            None,
            "same window twice is one sighting"
        );
        observe(&dir, page, "Safari", "Priya's page");
        observe(&dir, page, "Slack", "Priya");
        let text = saved_details_text(&dir).expect("three windows");
        assert!(text.contains("Phone: 801 555 0199"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn job_application_details_are_read_too() {
        let facts = extract(
            "LinkedIn: https://www.linkedin.com/in/sam-example\nGitHub: https://github.com/sam-example\n\
             Current employer: Example Corp\nJob title: Software Engineer\nYears of experience: 3\n\
             Work authorization: Authorized to work in the US\nSponsorship: No\nAvailable to start: June 1, 2030\n\
             City: Springfield\nState: Ohio\nZIP code: 45501\nCountry: United States\nPronouns: they/them\n\
             Skills: Rust, TypeScript, SQL\nWebsite: https://sam.example.com",
        );
        let get = |kind| value_of(&facts, kind);
        assert_eq!(
            get(FactKind::LinkedIn),
            Some("https://www.linkedin.com/in/sam-example")
        );
        assert_eq!(
            get(FactKind::GitHub),
            Some("https://github.com/sam-example")
        );
        assert_eq!(get(FactKind::CurrentEmployer), Some("Example Corp"));
        assert_eq!(get(FactKind::JobTitle), Some("Software Engineer"));
        assert_eq!(get(FactKind::YearsExperience), Some("3"));
        assert_eq!(get(FactKind::NeedsSponsorship), Some("No"));
        assert_eq!(get(FactKind::City), Some("Springfield"));
        assert_eq!(get(FactKind::State), Some("Ohio"));
        assert_eq!(get(FactKind::PostalCode), Some("45501"));
        assert_eq!(get(FactKind::Country), Some("United States"));
        assert_eq!(get(FactKind::Pronouns), Some("they/them"));
        assert_eq!(get(FactKind::Website), Some("https://sam.example.com"));
        assert_eq!(get(FactKind::Skills), Some("Rust, TypeScript, SQL"));
    }

    #[test]
    fn a_newer_value_replaces_an_older_one_of_the_same_kind() {
        let dir = scratch("newer");
        observe(
            &dir,
            "My details\nPhone: 385 555 0100",
            "Notes",
            "my details",
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
        observe(
            &dir,
            "My details\nPhone: 385 555 0111",
            "Notes",
            "my details",
        );
        let phones: Vec<Fact> = current(&dir)
            .into_iter()
            .filter(|f| f.kind == FactKind::Phone)
            .collect();
        assert_eq!(phones.len(), 1);
        assert_eq!(phones[0].value, "385 555 0111");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn forgetting_removes_everything_and_the_file_is_private() {
        let dir = scratch("forget");
        observe(&dir, DETAILS_PAGE, "TextEdit", "details.txt");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(path(&dir)).unwrap().permissions().mode();
            assert_eq!(mode & 0o077, 0, "only the owner can read it");
        }
        forget_all(&dir).unwrap();
        assert_eq!(saved_details_text(&dir), None);
        forget_all(&dir).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
