//! VS-13: turn time and app phrases in a query into retrieval filters.
//! "the hiring debrief from yesterday" becomes the text "the hiring
//! debrief" plus yesterday's time range; "what Dana asked in Slack" becomes
//! "what Dana asked" plus the app filter "Slack". Pure: the caller supplies
//! the current local time and the app names stored in the vault.

use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, NaiveTime, TimeZone, Weekday};
use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashMap;

/// A half-open local time window, `[start_ms, end_ms)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeRange {
    pub start_ms: i64,
    pub end_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedQuery {
    /// The query with the recognized phrases removed (used to report which
    /// words matched; retrieval itself searches the full query).
    pub text: String,
    pub time: Option<TimeRange>,
    /// The stored app name the query asked for.
    pub app: Option<String>,
}

pub fn parse_query_filters(
    query: &str,
    now: DateTime<Local>,
    known_apps: &[String],
) -> ParsedQuery {
    let mut spans: Vec<(usize, usize)> = Vec::new();

    let time_found = find_time(query, now);
    // "notes on Monday" with the monday.com app stored: the same words read
    // as a day and an app. Read them as the day, and never cut them twice.
    let app = find_app(query, known_apps, time_found.map(|(span, _)| span)).map(|(span, app)| {
        spans.push(span);
        app
    });
    let time = time_found.map(|(span, range)| {
        spans.push(span);
        range
    });

    let mut text = query.to_string();
    spans.sort_unstable_by(|a, b| b.0.cmp(&a.0));
    for (start, end) in spans {
        text.replace_range(start..end, " ");
    }
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    ParsedQuery {
        // Nothing left to search for: keep the words as typed.
        text: if text.is_empty() {
            query.trim().to_string()
        } else {
            text
        },
        time,
        app,
    }
}

const NUMBER_WORDS: [&str; 14] = [
    "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten", "eleven",
    "twelve", "thirteen", "fourteen",
];
const WEEKDAYS: [(&str, Weekday); 7] = [
    ("monday", Weekday::Mon),
    ("tuesday", Weekday::Tue),
    ("wednesday", Weekday::Wed),
    ("thursday", Weekday::Thu),
    ("friday", Weekday::Fri),
    ("saturday", Weekday::Sat),
    ("sunday", Weekday::Sun),
];
const MONTHS: [(&str, u32); 21] = [
    ("january", 1),
    ("jan", 1),
    ("february", 2),
    ("feb", 2),
    ("march", 3),
    ("mar", 3),
    ("april", 4),
    ("apr", 4),
    ("may", 5),
    ("june", 6),
    ("jun", 6),
    ("july", 7),
    ("jul", 7),
    ("august", 8),
    ("aug", 8),
    ("september", 9),
    ("sept", 9),
    ("sep", 9),
    ("october", 10),
    ("oct", 10),
    ("november", 11),
];
const MORE_MONTHS: [(&str, u32); 3] = [("nov", 11), ("december", 12), ("dec", 12)];

static DAY_PHRASE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)(?:\bfrom\s+)?(?:(?P<earlier>\bearlier\s+today)|(?P<today>\btoday)|(?P<yesterday>\byesterday)|(?P<morning>\bthis\s+morning)|(?P<afternoon>\bthis\s+afternoon)|(?P<evening>\bthis\s+evening|\btonight)|(?P<lastnight>\blast\s+night)|(?P<lastweek>\blast\s+week)|(?P<weekago>\ba\s+week\s+ago)|(?P<daysago>\b(?P<n>\d{1,2}|one|two|three|four|five|six|seven|eight|nine|ten|eleven|twelve|thirteen|fourteen)\s+days?\s+ago))\b",
    )
    .expect("day phrase regex")
});
static WEEKDAY_PHRASE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)(?:\b(?:on|from)\s+)?(?P<last>\blast\s+)?\b(?P<day>monday|tuesday|wednesday|thursday|friday|saturday|sunday)\b",
    )
    .expect("weekday regex")
});
static DATE_PHRASE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)(?:\b(?:on|from|since)\s+)?\b(?P<month>january|february|march|april|may|june|july|august|september|october|november|december|jan|feb|mar|apr|jun|jul|aug|sept|sep|oct|nov|dec)\.?\s+(?P<day>\d{1,2})(?:st|nd|rd|th)?\b",
    )
    .expect("date regex")
});

fn find_time(query: &str, now: DateTime<Local>) -> Option<((usize, usize), TimeRange)> {
    let today = now.date_naive();
    if let Some(found) = DATE_PHRASE
        .captures_iter(query)
        .find(|found| !after_deadline_word(query, found.get(0).unwrap().start()))
    {
        let whole = found.get(0)?;
        let month_name = found["month"].to_lowercase();
        let month = MONTHS
            .iter()
            .chain(MORE_MONTHS.iter())
            .find(|(name, _)| *name == month_name)
            .map(|(_, month)| *month)?;
        let day = found["day"].parse::<u32>().ok()?;
        let this_year = NaiveDate::from_ymd_opt(today.year(), month, day)?;
        let date = if this_year > today {
            NaiveDate::from_ymd_opt(today.year() - 1, month, day)?
        } else {
            this_year
        };
        return Some(((whole.start(), whole.end()), day_window(date, 1)));
    }
    if let Some(found) = DAY_PHRASE.captures_iter(query).find(|found| {
        !after_article(query, found.get(0).unwrap().start())
            && !after_deadline_word(query, found.get(0).unwrap().start())
            && !is_dotted(query, found.get(0).unwrap().end())
    }) {
        let whole = found.get(0)?;
        let span = (whole.start(), whole.end());
        let range = if found.name("earlier").is_some() || found.name("today").is_some() {
            day_window(today, 1)
        } else if found.name("yesterday").is_some() {
            day_window(today - Duration::days(1), 1)
        } else if found.name("morning").is_some() {
            hours_window(today, 5, 12)
        } else if found.name("afternoon").is_some() {
            hours_window(today, 12, 17)
        } else if found.name("evening").is_some() {
            hours_window(today, 17, 24)
        } else if found.name("lastnight").is_some() {
            TimeRange {
                start_ms: at_hour(today - Duration::days(1), 18),
                end_ms: at_hour(today, 5),
            }
        } else if found.name("lastweek").is_some() {
            day_window(today - Duration::days(7), 7)
        } else if found.name("weekago").is_some() {
            day_window(today - Duration::days(7), 1)
        } else {
            let n = found.name("n")?.as_str().to_lowercase();
            let days = n.parse::<i64>().ok().or_else(|| {
                NUMBER_WORDS
                    .iter()
                    .position(|word| *word == n)
                    .map(|index| index as i64 + 1)
            })?;
            day_window(today - Duration::days(days), 1)
        };
        return Some((span, range));
    }
    let found = WEEKDAY_PHRASE.captures_iter(query).find(|found| {
        !is_dotted(query, found.get(0).unwrap().end())
            && !after_deadline_word(query, found.get(0).unwrap().start())
    })?;
    let whole = found.get(0)?;
    let name = found["day"].to_lowercase();
    let weekday = WEEKDAYS.iter().find(|(day, _)| *day == name)?.1;
    let back = (7 + today.weekday().num_days_from_monday() as i64
        - weekday.num_days_from_monday() as i64)
        % 7;
    // "Saturday" on a Saturday is today; "last Saturday" is a week back.
    let back = if found.name("last").is_some() && back == 0 {
        7
    } else {
        back
    };
    Some((
        (whole.start(), whole.end()),
        day_window(today - Duration::days(back), 1),
    ))
}

fn find_app(
    query: &str,
    known_apps: &[String],
    time_span: Option<(usize, usize)>,
) -> Option<((usize, usize), String)> {
    let mut aliases = known_apps
        .iter()
        .flat_map(|app| app_aliases(app).into_iter().map(move |alias| (alias, app)))
        .collect::<Vec<_>>();
    // Longest alias first, so "VS Code" wins over a shorter overlapping name.
    aliases.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then_with(|| a.0.cmp(&b.0)));
    for (alias, app) in aliases {
        let Some(pattern) = app_phrase_pattern(&alias) else {
            continue;
        };
        if let Some(found) = pattern.find_iter(query).find(|found| {
            !is_dotted(query, found.end())
                && time_span.is_none_or(|(start, end)| found.end() <= start || end <= found.start())
        }) {
            return Some(((found.start(), found.end()), app.clone()));
        };
    }
    None
}

/// "in Slack", "using VS Code": the pattern for one app alias. Every Search
/// and Ask parses its query against every stored app, so each pattern is
/// compiled once per process instead of once per query.
fn app_phrase_pattern(alias: &str) -> Option<Regex> {
    static PATTERNS: Lazy<parking_lot::Mutex<HashMap<String, Option<Regex>>>> =
        Lazy::new(Default::default);
    PATTERNS
        .lock()
        .entry(alias.to_string())
        .or_insert_with(|| {
            Regex::new(&format!(
                r"(?i)\b(?:in|on|from|using)\s+{}\b",
                regex::escape(alias)
            ))
            .ok()
        })
        .clone()
}

/// Names a person types for a stored app: the full name, the name without a
/// domain suffix ("zoom.us" as "zoom"), the brand-less last word of a
/// two-word name ("Microsoft Excel" as "excel"), and the initials form of a
/// longer name ("Visual Studio Code" as "vs code" and "vscode").
fn app_aliases(app: &str) -> Vec<String> {
    let name = app.trim().to_lowercase();
    if name.is_empty() {
        return Vec::new();
    }
    let mut aliases = vec![name.clone()];
    for suffix in [".us", ".app", ".com"] {
        if let Some(stripped) = name.strip_suffix(suffix) {
            aliases.push(stripped.to_string());
        }
    }
    let words = name.split_whitespace().collect::<Vec<_>>();
    if words.len() == 2 && words[1].len() >= 4 {
        aliases.push(words[1].to_string());
    }
    if words.len() >= 3 {
        let initials = words[..words.len() - 1]
            .iter()
            .filter_map(|word| word.chars().next())
            .collect::<String>();
        let last = words[words.len() - 1];
        aliases.push(format!("{initials} {last}"));
        aliases.push(format!("{initials}{last}"));
    }
    aliases.dedup();
    aliases
}

/// "the today show": a day word right after an article is a name, not a time.
fn after_article(query: &str, start: usize) -> bool {
    let before = query[..start].trim_end().to_lowercase();
    ["the", "a", "an"]
        .iter()
        .any(|article| before == *article || before.ends_with(&format!(" {article}")))
}

/// "due Thursday", "by Friday", "due October 9", "due today": a deadline the
/// memory mentions, not the day it was captured.
fn after_deadline_word(query: &str, start: usize) -> bool {
    let before = query[..start].trim_end().to_lowercase();
    ["due", "by", "until", "before", "till", "next"]
        .iter()
        .any(|word| before == *word || before.ends_with(&format!(" {word}")))
}

/// "Monday.com" or "Slack.app": a word followed by a dot and a letter is a name.
fn is_dotted(query: &str, end: usize) -> bool {
    let mut rest = query[end..].chars();
    rest.next() == Some('.') && rest.next().is_some_and(char::is_alphanumeric)
}

fn at_hour(date: NaiveDate, hour: u32) -> i64 {
    let date = if hour >= 24 {
        date + Duration::days(1)
    } else {
        date
    };
    let time = NaiveTime::from_hms_opt(hour % 24, 0, 0).expect("valid hour");
    Local
        .from_local_datetime(&date.and_time(time))
        .earliest()
        .map(|moment| moment.timestamp_millis())
        .unwrap_or_else(|| {
            Local
                .from_utc_datetime(&date.and_time(time))
                .timestamp_millis()
        })
}

fn day_window(first_day: NaiveDate, days: i64) -> TimeRange {
    TimeRange {
        start_ms: at_hour(first_day, 0),
        end_ms: at_hour(first_day + Duration::days(days), 0),
    }
}

fn hours_window(day: NaiveDate, from_hour: u32, to_hour: u32) -> TimeRange {
    TimeRange {
        start_ms: at_hour(day, from_hour),
        end_ms: at_hour(day, to_hour),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Saturday 2026-10-03, 15:30 local.
    fn now() -> DateTime<Local> {
        Local
            .from_local_datetime(
                &NaiveDate::from_ymd_opt(2026, 10, 3)
                    .unwrap()
                    .and_hms_opt(15, 30, 0)
                    .unwrap(),
            )
            .single()
            .unwrap()
    }

    fn at(day: u32, hour: u32) -> i64 {
        Local
            .from_local_datetime(
                &NaiveDate::from_ymd_opt(2026, if day > 3 { 9 } else { 10 }, day)
                    .unwrap()
                    .and_hms_opt(hour, 0, 0)
                    .unwrap(),
            )
            .single()
            .unwrap()
            .timestamp_millis()
    }

    fn day_range(month: u32, day: u32) -> TimeRange {
        let start = Local
            .from_local_datetime(
                &NaiveDate::from_ymd_opt(2026, month, day)
                    .unwrap()
                    .and_hms_opt(0, 0, 0)
                    .unwrap(),
            )
            .single()
            .unwrap();
        TimeRange {
            start_ms: start.timestamp_millis(),
            end_ms: (start + Duration::days(1)).timestamp_millis(),
        }
    }

    fn apps() -> Vec<String> {
        [
            "Slack",
            "Google Chrome",
            "zoom.us",
            "Microsoft Excel",
            "Visual Studio Code",
            "Terminal",
            "Notion",
            "Preview",
        ]
        .iter()
        .map(|app| app.to_string())
        .collect()
    }

    fn parse(query: &str) -> ParsedQuery {
        parse_query_filters(query, now(), &apps())
    }

    #[test]
    fn day_phrases_become_whole_local_days() {
        let cases = [
            (
                "the hiring debrief from yesterday",
                "the hiring debrief",
                day_range(10, 2),
            ),
            ("what did I read today", "what did I read", day_range(10, 3)),
            (
                "the essay draft I was working on earlier today",
                "the essay draft I was working on",
                day_range(10, 3),
            ),
            (
                "the launch review deck from two days ago",
                "the launch review deck",
                day_range(10, 1),
            ),
            (
                "the launch checklist from three days ago",
                "the launch checklist",
                day_range(9, 30),
            ),
            (
                "the pandas error from 5 days ago",
                "the pandas error",
                day_range(9, 28),
            ),
            ("notes on Sep 30", "notes", day_range(9, 30)),
            ("notes from September 29", "notes", day_range(9, 29)),
            ("what I read on Thursday", "what I read", day_range(10, 1)),
            (
                "standup notes last Saturday",
                "standup notes",
                day_range(9, 26),
            ),
            ("standup notes Saturday", "standup notes", day_range(10, 3)),
        ];
        for (query, text, range) in cases {
            let parsed = parse(query);
            assert_eq!(parsed.text, text, "{query}");
            assert_eq!(parsed.time, Some(range), "{query}");
            assert_eq!(parsed.app, None, "{query}");
        }
    }

    #[test]
    fn parts_of_a_day_and_weeks_become_windows() {
        let cases = [
            (
                "the Q3 numbers email I sent this morning",
                "the Q3 numbers email I sent",
                at(3, 5),
                at(3, 12),
            ),
            ("the call this afternoon", "the call", at(3, 12), at(3, 17)),
            (
                "the article I read last night",
                "the article I read",
                at(2, 18),
                at(3, 5),
            ),
            (
                "the PRD I drafted last week",
                "the PRD I drafted",
                at(26, 0),
                at(3, 0),
            ),
        ];
        for (query, text, start_ms, end_ms) in cases {
            let parsed = parse(query);
            assert_eq!(parsed.text, text, "{query}");
            assert_eq!(parsed.time, Some(TimeRange { start_ms, end_ms }), "{query}");
        }
    }

    #[test]
    fn app_phrases_name_a_stored_app() {
        let cases = [
            (
                "what Dana asked me to do in Slack",
                "what Dana asked me to do",
                "Slack",
            ),
            (
                "the offsite venue vote on Zoom",
                "the offsite venue vote",
                "zoom.us",
            ),
            (
                "the churn breakdown in Excel",
                "the churn breakdown",
                "Microsoft Excel",
            ),
            (
                "the pandas page I had open in Chrome",
                "the pandas page I had open",
                "Google Chrome",
            ),
            ("charts.py in VS Code", "charts.py", "Visual Studio Code"),
            (
                "the pandas error in Terminal",
                "the pandas error",
                "Terminal",
            ),
        ];
        for (query, text, app) in cases {
            let parsed = parse(query);
            assert_eq!(parsed.text, text, "{query}");
            assert_eq!(parsed.app.as_deref(), Some(app), "{query}");
            assert_eq!(parsed.time, None, "{query}");
        }
    }

    #[test]
    fn time_and_app_combine() {
        let parsed = parse("what Rafael said in Slack yesterday");
        assert_eq!(parsed.text, "what Rafael said");
        assert_eq!(parsed.app.as_deref(), Some("Slack"));
        assert_eq!(parsed.time, Some(day_range(10, 2)));
    }

    #[test]
    fn look_alikes_are_not_filters() {
        for query in [
            "Monday.com board for the launch",
            "weekday vs weekend ridership chart",
            "the survey data in Sheets",
            "the labor contracts passage in the PDF",
            "you may want to check the budget",
            "single sign-on",
            "Field Order 15",
            "what does Dana want for the survey readout",
            "the today show transcript",
            "net new ARR $1.24M",
            "churn drivers due Thursday",
            "send the readout by Friday",
            "the grant report due October 9",
            "the invoice due today",
        ] {
            let parsed = parse(query);
            assert_eq!(parsed.time, None, "{query}");
            assert_eq!(parsed.app, None, "{query}");
            assert_eq!(parsed.text, query, "{query}");
        }
    }

    #[test]
    fn a_query_that_is_only_a_filter_keeps_its_words() {
        // Nothing would be left to search for; keep the text as typed.
        let parsed = parse("yesterday");
        assert_eq!(parsed.text, "yesterday");
        assert_eq!(parsed.time, Some(day_range(10, 2)));
    }

    /// Small seeded generator, so the property tests need no extra crate
    /// and every failure reproduces from its case number.
    struct Lcg(u64);

    impl Lcg {
        fn next(&mut self) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            self.0 >> 33
        }

        fn below(&mut self, n: u64) -> u64 {
            self.next() % n
        }
    }

    /// Words that start or end filter phrases, plus look-alikes, numbers,
    /// punctuation, and non-ASCII text.
    const VOCABULARY: [&str; 44] = [
        "notes",
        "the",
        "report",
        "a",
        "due",
        "by",
        "until",
        "before",
        "next",
        "on",
        "from",
        "in",
        "using",
        "since",
        "last",
        "this",
        "morning",
        "week",
        "two",
        "days",
        "ago",
        "Monday",
        "thursday",
        "Friday",
        "today",
        "yesterday",
        "October",
        "9",
        "Sep",
        "30th",
        "Slack",
        "Chrome",
        "VS",
        "Code",
        "Monday.com",
        "monday",
        "Notion",
        "1.8%",
        "LL-1482",
        ".",
        "naïve",
        "日本語",
        "🙂",
        "earlier",
    ];

    /// Stored app names, including ones whose short names are time words.
    fn tricky_apps() -> Vec<String> {
        let mut apps = apps();
        apps.push("monday.com".to_string());
        apps.push("Day One".to_string());
        apps
    }

    #[test]
    fn parsing_holds_its_invariants_on_random_queries() {
        let apps = tricky_apps();
        for case in 0..3000u64 {
            let mut rng = Lcg(case);
            let words = 1 + rng.below(7) as usize;
            let query = (0..words)
                .map(|_| VOCABULARY[rng.below(VOCABULARY.len() as u64) as usize])
                .collect::<Vec<_>>()
                .join(" ");
            let parsed = parse_query_filters(&query, now(), &apps);
            let normalized = query.split_whitespace().collect::<Vec<_>>().join(" ");

            if parsed.time.is_none() && parsed.app.is_none() {
                assert_eq!(parsed.text, normalized, "case {case}: {query}");
            }
            // The text only ever loses words.
            for word in parsed.text.split_whitespace() {
                assert!(
                    query.contains(word),
                    "case {case}: {query} -> {}",
                    parsed.text
                );
            }
            if let Some(range) = parsed.time {
                assert!(range.start_ms < range.end_ms, "case {case}: {query}");
                assert!(
                    range.end_ms - range.start_ms <= 7 * 86_400_000 + 3_600_000,
                    "case {case}: {query}"
                );
                assert!(
                    range.start_ms <= now().timestamp_millis(),
                    "case {case}: {query}"
                );
            }
        }
    }

    #[test]
    fn no_time_phrase_after_a_deadline_word_is_a_filter() {
        let phrases = [
            "Monday",
            "on Thursday",
            "friday",
            "today",
            "yesterday",
            "October 9",
            "Sep 30",
            "two days ago",
            "last week",
            "this morning",
        ];
        for word in ["due", "by", "until", "before", "till", "next", "Due", "BY"] {
            for phrase in phrases {
                let query = format!("the report {word} {phrase}");
                let parsed = parse(&query);
                assert_eq!(parsed.time, None, "{query}");
                assert_eq!(parsed.text, query, "{query}");
            }
        }
    }

    #[test]
    fn an_app_whose_name_is_a_time_word_does_not_break_parsing() {
        let parsed = parse_query_filters("notes on Monday", now(), &tricky_apps());
        assert!(parsed.time.is_some() || parsed.app.is_some());
        assert!(!parsed.text.is_empty());
    }

    #[test]
    fn overlapping_weekday_alias_does_not_hide_a_separate_app_filter() {
        for query in ["notes on Monday from Slack", "notes from Slack on Monday"] {
            let parsed = parse_query_filters(query, now(), &tricky_apps());
            assert_eq!(parsed.time, Some(day_range(9, 28)), "{query}");
            assert_eq!(parsed.app.as_deref(), Some("Slack"), "{query}");
            assert_eq!(parsed.text, "notes", "{query}");
        }
    }
}
