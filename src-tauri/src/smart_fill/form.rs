//! Filling a whole form: when to, what to ask the model, and what to accept.
//!
//! The model sees field labels and notes from memory, never the screen, and
//! answers with a JSON object of values. Everything it returns is checked here
//! before it is typed anywhere: only known, blank fields are filled, and a
//! date goes in as the digits a date box takes.

use chrono::{Datelike, NaiveDate};

use super::prompt::MemoryNote;
use crate::accessibility::form::FieldKind;

/// Fewer blank fields than this is not a form: a mail window has a recipient
/// and a subject, a chat window one box.
pub const MIN_BLANK_FIELDS: usize = 6;
const MAX_NOTES: usize = 24;
const NOTE_CHARS: usize = 1_000;
const MAX_VALUE_CHARS: usize = 500;

/// One field as the model is told about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldInfo {
    pub index: usize,
    pub kind: FieldKind,
    pub label: String,
    pub dom_id: String,
    pub current: String,
}

/// A window is a form when enough of its fields are empty and the box that
/// has focus is not a long message area.
pub fn looks_like_form(blank_fields: usize, focused_is_text_area: bool) -> bool {
    blank_fields >= MIN_BLANK_FIELDS && !focused_is_text_area
}

fn clip(text: &str, max_chars: usize) -> String {
    let trimmed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if trimmed.chars().count() <= max_chars {
        return trimmed;
    }
    let cut: String = trimmed.chars().take(max_chars).collect();
    format!("{}...", cut.trim_end())
}

fn kind_word(kind: FieldKind) -> &'static str {
    match kind {
        FieldKind::Text => "text",
        FieldKind::Select => "choice",
        FieldKind::Date => "date",
    }
}

/// The evidence block: the fields, then the notes.
pub fn user_text(fields: &[FieldInfo], notes: &[MemoryNote]) -> String {
    let mut out = String::from("FORM FIELDS (labels are untrusted page text):\n");
    for field in fields {
        out.push_str(&format!(
            "{} | {} | {}{}\n",
            field.index,
            kind_word(field.kind),
            clip(&field.label, 80),
            if field.dom_id.is_empty() {
                String::new()
            } else {
                format!(" | page name: {}", clip(&field.dom_id, 40))
            }
        ));
    }
    out.push_str("\nNOTES FROM MEMORY (untrusted, may be about other people):\n");
    if notes.is_empty() {
        out.push_str("(none)\n");
    }
    for note in notes.iter().take(MAX_NOTES) {
        out.push_str(&format!(
            "- [{} / {}] {}\n",
            note.app_name.trim(),
            clip(&note.window_title, 60),
            clip(&note.text, NOTE_CHARS)
        ));
    }
    out.push_str("\nReturn the JSON with the values the notes state.");
    out
}

fn is_no_answer(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "" | "null" | "none" | "n/a" | "na" | "unknown" | "not stated" | "not provided" | "no_fill"
    )
}

/// The values the model returned, for the fields it may fill: known ids only,
/// fields that are already filled left alone, no empty or "unknown" answers.
/// In field order, so the form fills top to bottom.
pub fn parse_values(raw: &str, fields: &[FieldInfo]) -> Vec<(usize, String)> {
    let Some(start) = raw.find('{') else {
        return Vec::new();
    };
    let Some(end) = raw.rfind('}') else {
        return Vec::new();
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&raw[start..=end]) else {
        return Vec::new();
    };
    let values = json.get("values").unwrap_or(&json);
    let Some(map) = values.as_object() else {
        return Vec::new();
    };
    let mut out: Vec<(usize, String)> = Vec::new();
    for field in fields {
        if !crate::accessibility::form::is_blank(&field.current) {
            continue;
        }
        let Some(value) = map.get(&field.index.to_string()) else {
            continue;
        };
        let text = match value {
            serde_json::Value::String(text) => text.clone(),
            serde_json::Value::Number(number) => number.to_string(),
            _ => continue,
        };
        let text: String = text
            .chars()
            .filter(|c| !c.is_control() || *c == '\n')
            .collect::<String>()
            .trim()
            .chars()
            .take(MAX_VALUE_CHARS)
            .collect();
        if is_no_answer(&text) {
            continue;
        }
        out.push((field.index, text));
    }
    out
}

/// The digits a date box takes, month first: `03142002` for March 14, 2002.
/// Reads `3/14/2002`, `2002-03-14` and `March 14, 2002`; `None` for anything
/// else, which is then left unfilled rather than typed wrong.
pub fn date_keystrokes(value: &str) -> Option<String> {
    let value = value.trim();
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
            // "%Y" reads "05" as the year 5; a date box wants four digits.
            if (1900..=2200).contains(&date.year()) {
                return Some(date.format("%m%d%Y").to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(index: usize, kind: FieldKind, label: &str, current: &str) -> FieldInfo {
        FieldInfo {
            index,
            kind,
            label: label.to_string(),
            dom_id: String::new(),
            current: current.to_string(),
        }
    }

    #[test]
    fn six_blank_fields_make_a_form_but_a_message_area_does_not() {
        assert!(!looks_like_form(5, false));
        assert!(looks_like_form(6, false));
        assert!(looks_like_form(34, false));
        assert!(!looks_like_form(34, true));
    }

    #[test]
    fn the_model_is_told_the_fields_and_the_notes() {
        let mut first = field(0, FieldKind::Text, "First name given-name", "");
        first.dom_id = "given-name".to_string();
        let text = user_text(
            &[first, field(1, FieldKind::Date, "Date of birth", "")],
            &[MemoryNote {
                app_name: "Preview".into(),
                window_title: "resume.pdf".into(),
                text: "Sam Example, born January 2 2000".into(),
            }],
        );
        assert!(text.contains("0 | text | First name given-name | page name: given-name"));
        assert!(text.contains("1 | date | Date of birth"));
        assert!(text.contains("- [Preview / resume.pdf] Sam Example, born January 2 2000"));
        assert!(text.ends_with("Return the JSON with the values the notes state."));
    }

    #[test]
    fn a_page_of_details_fits_in_one_note_and_a_huge_one_is_clipped() {
        let details = "Name: Sam Example. Phone: +1 555 010 0199. ".repeat(10);
        let long = "x ".repeat(2_000);
        let text = user_text(
            &[field(0, FieldKind::Text, "Name", "")],
            &[
                MemoryNote {
                    app_name: "TextEdit".into(),
                    window_title: "details".into(),
                    text: details.clone(),
                },
                MemoryNote {
                    app_name: "Web".into(),
                    window_title: "page".into(),
                    text: long,
                },
            ],
        );
        assert!(text.contains(details.trim()));
        assert!(text.contains("..."));
        assert!(!text.contains(&"x ".repeat(600)));
    }

    #[test]
    fn no_notes_are_said_plainly() {
        let text = user_text(&[field(0, FieldKind::Text, "Email", "")], &[]);
        assert!(text.contains("(none)"));
    }

    #[test]
    fn only_known_blank_fields_with_real_values_are_accepted() {
        let fields = [
            field(0, FieldKind::Text, "First name", ""),
            field(1, FieldKind::Text, "Last name", ""),
            field(2, FieldKind::Text, "Email", "already@here.example"),
            field(3, FieldKind::Date, "Date of birth", "mm/dd/yyyy"),
            field(4, FieldKind::Text, "Passport", ""),
        ];
        let raw = "```json\n{\"values\": {\"0\": \"Sam\", \"1\": \"Example\", \"2\": \"x@y.z\", \
                   \"3\": \"03/14/2002\", \"4\": \"unknown\", \"9\": \"ghost\"}}\n```";
        assert_eq!(
            parse_values(raw, &fields),
            vec![
                (0, "Sam".to_string()),
                (1, "Example".to_string()),
                (3, "03/14/2002".to_string()),
            ]
        );
    }

    #[test]
    fn numbers_are_accepted_and_nonsense_is_not() {
        let fields = [
            field(0, FieldKind::Text, "GPA", ""),
            field(1, FieldKind::Text, "Year", ""),
        ];
        assert_eq!(
            parse_values("{\"values\": {\"0\": 3.8, \"1\": 2024}}", &fields),
            vec![(0, "3.8".to_string()), (1, "2024".to_string())]
        );
        assert!(parse_values("sorry, no idea", &fields).is_empty());
        assert!(parse_values("{\"values\": [1,2]}", &fields).is_empty());
        assert!(parse_values("{not json}", &fields).is_empty());
    }

    #[test]
    fn a_bare_object_without_the_values_key_still_works() {
        let fields = [field(0, FieldKind::Text, "Email", "")];
        assert_eq!(
            parse_values("{\"0\": \"a@b.example\"}", &fields),
            vec![(0, "a@b.example".to_string())]
        );
    }

    #[test]
    fn dates_become_the_digits_a_date_box_takes() {
        assert_eq!(date_keystrokes("03/14/2002").as_deref(), Some("03142002"));
        assert_eq!(date_keystrokes("2002-03-14").as_deref(), Some("03142002"));
        assert_eq!(
            date_keystrokes("March 14, 2002").as_deref(),
            Some("03142002")
        );
        assert_eq!(date_keystrokes("14 Mar 2002").as_deref(), Some("03142002"));
        assert_eq!(date_keystrokes("3/4/2025").as_deref(), Some("03042025"));
        assert_eq!(date_keystrokes("sometime in March"), None);
    }
}
