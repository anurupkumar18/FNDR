//! What Smart Fill sends to the model, and what it accepts back.
//!
//! The system prompt is `inference::SMART_FILL_SYSTEM`. This file builds the
//! evidence block that follows it and cleans the model's answer before any of
//! it is typed into another app.

/// Longest reply Smart Fill will type. A longer answer is cut at a sentence
/// or word end.
pub const MAX_REPLY_CHARS: usize = 2_000;

const SCREEN_TEXT_CHARS: usize = 6_000;
const NOTE_CHARS: usize = 320;
const MAX_NOTES: usize = 4;
/// How much of a memory's stored page text a note carries (ADR 036).
const NOTE_FULL_TEXT_CHARS: usize = 1_500;

/// The model's marker for "I cannot tell what belongs here".
const NO_FILL: &str = "NO_FILL";

/// One memory offered as context for the reply.
pub struct MemoryNote {
    pub app_name: String,
    pub window_title: String,
    pub text: String,
    /// The page text FNDR kept for this memory, when "Keep the full text" is
    /// on and the memory is visible. Exact values (a phone number, a line to
    /// quote) live here; `text` is only the summary.
    pub full_text: Option<String>,
}

/// Everything known about the focused field and its surroundings.
pub struct FillContext<'a> {
    pub app_name: &'a str,
    pub window_title: &'a str,
    pub field_label: &'a str,
    pub current_value: &'a str,
    pub screen_text: &'a str,
    pub notes: &'a [MemoryNote],
}

fn clip(text: &str, max_chars: usize) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }
    let cut: String = trimmed.chars().take(max_chars).collect();
    format!("{}...", cut.trim_end())
}

/// The evidence block, labelled so the model can tell the field, the screen
/// and the memories apart.
pub fn user_text(context: &FillContext<'_>) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "FOCUSED FIELD (untrusted):\napp: {}\nwindow: {}\nlabel: {}\ncurrent value: {}\n",
        context.app_name,
        context.window_title,
        if context.field_label.trim().is_empty() {
            "(none)"
        } else {
            context.field_label.trim()
        },
        if context.current_value.trim().is_empty() {
            "(empty)".to_string()
        } else {
            clip(context.current_value, 600)
        },
    ));
    out.push_str(&format!(
        "\nTEXT ON SCREEN (untrusted, reading order):\n---\n{}\n---\n",
        if context.screen_text.trim().is_empty() {
            "(none read; use the screenshot)".to_string()
        } else {
            clip(context.screen_text, SCREEN_TEXT_CHARS)
        }
    ));
    let notes: Vec<&MemoryNote> = context.notes.iter().take(MAX_NOTES).collect();
    if !notes.is_empty() {
        out.push_str("\nNOTES FROM MEMORY (untrusted, may be unrelated):\n");
        for note in notes {
            out.push_str(&format!(
                "- [{} / {}] {}\n",
                note.app_name.trim(),
                clip(&note.window_title, 80),
                clip(&note.text, NOTE_CHARS)
            ));
            if let Some(full) = note.full_text.as_deref().filter(|t| !t.trim().is_empty()) {
                out.push_str(&format!(
                    "  page text (untrusted): {}\n",
                    clip(full, NOTE_FULL_TEXT_CHARS)
                ));
            }
        }
    }
    out.push_str("\nWrite the text for the focused field.");
    out
}

/// The model's answer made safe to type: no fences, no wrapping quotes, no
/// "Reply:" label, bounded length. `None` when there is nothing to type.
pub fn clean_reply(raw: &str) -> Option<String> {
    let mut text = raw.trim().to_string();
    if text.is_empty() || text.eq_ignore_ascii_case(NO_FILL) {
        return None;
    }
    if text.starts_with("```") {
        let inner = text.trim_start_matches('`');
        let inner = inner.split_once('\n').map_or(inner, |(_, rest)| rest);
        text = inner.trim_end_matches('`').trim().to_string();
    }
    for label in ["Reply:", "Response:", "Text:", "Answer:"] {
        if let Some(rest) = text.strip_prefix(label) {
            text = rest.trim_start().to_string();
        }
    }
    let wrapped = |open: char, close: char| {
        text.chars().count() >= 2
            && text.starts_with(open)
            && text.ends_with(close)
            && !text[open.len_utf8()..text.len() - close.len_utf8()].contains(open)
    };
    if wrapped('"', '"') || wrapped('\u{201c}', '\u{201d}') {
        let mut chars = text.chars();
        chars.next();
        chars.next_back();
        text = chars.as_str().trim().to_string();
    }
    if text.is_empty() || text.eq_ignore_ascii_case(NO_FILL) {
        return None;
    }
    if text.chars().count() > MAX_REPLY_CHARS {
        let cut: String = text.chars().take(MAX_REPLY_CHARS).collect();
        let end = cut
            .rfind(['.', '!', '?', '\n'])
            .filter(|&i| i > MAX_REPLY_CHARS / 2)
            .map(|i| i + 1)
            .or_else(|| cut.rfind(' '))
            .unwrap_or(cut.len());
        text = cut[..end].trim_end().to_string();
    }
    Some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evidence_block_labels_field_screen_and_notes() {
        let notes = [MemoryNote {
            app_name: "Mail".into(),
            window_title: "Lease".into(),
            text: "Deposit is 1200 dollars".into(),
            full_text: Some("Sam Example, phone +1 555 010 0199, deposit 1200".into()),
        }];
        let text = user_text(&FillContext {
            app_name: "Messages",
            window_title: "Sam",
            field_label: "",
            current_value: "",
            screen_text: "Sam: are you free tonight?",
            notes: &notes,
        });
        assert!(text.contains("app: Messages"));
        assert!(text.contains("label: (none)"));
        assert!(text.contains("current value: (empty)"));
        assert!(text.contains("Sam: are you free tonight?"));
        assert!(text.contains("- [Mail / Lease] Deposit is 1200 dollars"));
        assert!(text.contains("page text (untrusted): Sam Example, phone +1 555 010 0199"));
        assert!(text.ends_with("Write the text for the focused field."));
    }

    #[test]
    fn without_screen_text_the_model_is_pointed_at_the_screenshot() {
        let text = user_text(&FillContext {
            app_name: "Safari",
            window_title: "",
            field_label: "Email",
            current_value: "",
            screen_text: "  ",
            notes: &[],
        });
        assert!(text.contains("(none read; use the screenshot)"));
        assert!(!text.contains("NOTES FROM MEMORY"));
    }

    #[test]
    fn only_four_notes_are_sent_and_long_ones_are_clipped() {
        let notes: Vec<MemoryNote> = (0..6)
            .map(|i| MemoryNote {
                app_name: "Notes".into(),
                window_title: format!("n{i}"),
                text: "x".repeat(1_000),
                full_text: None,
            })
            .collect();
        let text = user_text(&FillContext {
            app_name: "A",
            window_title: "",
            field_label: "",
            current_value: "",
            screen_text: "s",
            notes: &notes,
        });
        assert_eq!(text.matches("\n- [").count(), 4);
        assert!(!text.contains(&"x".repeat(400)));
    }

    #[test]
    fn plain_replies_pass_through() {
        assert_eq!(
            clean_reply("  Sounds good, see you at 7.  ").as_deref(),
            Some("Sounds good, see you at 7.")
        );
    }

    #[test]
    fn no_fill_and_empty_replies_type_nothing() {
        assert_eq!(clean_reply("NO_FILL"), None);
        assert_eq!(clean_reply("no_fill"), None);
        assert_eq!(clean_reply("   "), None);
        assert_eq!(clean_reply("\"NO_FILL\""), None);
    }

    #[test]
    fn fences_labels_and_wrapping_quotes_are_removed() {
        assert_eq!(
            clean_reply("```\nSee you then\n```").as_deref(),
            Some("See you then")
        );
        assert_eq!(
            clean_reply("Reply: Yes please").as_deref(),
            Some("Yes please")
        );
        assert_eq!(
            clean_reply("\"I'll be there\"").as_deref(),
            Some("I'll be there")
        );
        assert_eq!(
            clean_reply("\u{201c}Count me in\u{201d}").as_deref(),
            Some("Count me in")
        );
    }

    #[test]
    fn inner_quotes_are_kept() {
        assert_eq!(
            clean_reply("\"Yes\" and \"no\" both work").as_deref(),
            Some("\"Yes\" and \"no\" both work")
        );
    }

    #[test]
    fn a_long_reply_is_cut_at_a_sentence_end() {
        let sentence = "This is one sentence. ";
        let raw = sentence.repeat(200);
        let out = clean_reply(&raw).unwrap();
        assert!(out.chars().count() <= MAX_REPLY_CHARS);
        assert!(out.ends_with('.'));
    }
}
