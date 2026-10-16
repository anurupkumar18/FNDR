//! A document as markdown, plain text or HTML.
//!
//! Citations are numbered in order of first use, shown after each line as
//! `[1]`, and listed in full under "Sources" with the id of the memory or
//! activity event. With citations off the body is clean for pasting; the
//! document itself still carries them.

use super::{Citation, CitationKind, OutputDoc, OutputFormat};
use std::collections::HashMap;

/// The numbered source list shared by all three formats.
struct Sources<'a> {
    order: Vec<&'a Citation>,
    number: HashMap<(&'a str, CitationKind), usize>,
}

impl<'a> Sources<'a> {
    fn of(doc: &'a OutputDoc) -> Self {
        let mut sources = Sources {
            order: Vec::new(),
            number: HashMap::new(),
        };
        for citation in doc
            .sections
            .iter()
            .flat_map(|section| &section.lines)
            .flat_map(|line| &line.citations)
        {
            let key = (citation.id.as_str(), citation.kind);
            if !sources.number.contains_key(&key) {
                sources.order.push(citation);
                sources.number.insert(key, sources.order.len());
            }
        }
        sources
    }

    fn marks(&self, citations: &[Citation]) -> String {
        let mut numbers: Vec<usize> = citations
            .iter()
            .filter_map(|c| self.number.get(&(c.id.as_str(), c.kind)).copied())
            .collect();
        numbers.sort_unstable();
        numbers.dedup();
        numbers
            .iter()
            .map(|n| format!("[{n}]"))
            .collect::<Vec<_>>()
            .join("")
    }
}

fn kind_label(kind: CitationKind) -> &'static str {
    match kind {
        CitationKind::Memory => "memory",
        CitationKind::ActivityEvent => "activity event",
    }
}

fn markdown_source(citation: &Citation) -> String {
    let mut text = format!("{} `{}`", kind_label(citation.kind), citation.id);
    if citation.kind == CitationKind::ActivityEvent {
        if let Some(memory) = citation.memory_id.as_deref() {
            text.push_str(&format!(", memory `{memory}`"));
        }
    }
    text
}

fn plain_source(citation: &Citation) -> String {
    let mut text = format!("{} {}", kind_label(citation.kind), citation.id);
    if citation.kind == CitationKind::ActivityEvent {
        if let Some(memory) = citation.memory_id.as_deref() {
            text.push_str(&format!(", memory {memory}"));
        }
    }
    text
}

fn date_line(doc: &OutputDoc) -> String {
    use chrono::{Local, TimeZone};
    let stamp = Local
        .timestamp_millis_opt(doc.generated_at_ms)
        .earliest()
        .map_or_else(String::new, |at| at.format("%Y-%m-%d %H:%M").to_string());
    format!("Generated {stamp} from FNDR memory")
}

pub fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn line_text(text: &str, project: Option<&str>, bold: &str) -> String {
    match project {
        Some(name) if !text.starts_with(name) => format!("{bold}{name}{bold}: {text}"),
        _ => text.to_string(),
    }
}

const EMPTY_SECTION: &str = "Nothing recorded.";

pub fn render(doc: &OutputDoc, format: OutputFormat, show_citations: bool) -> String {
    let sources = Sources::of(doc);
    match format {
        OutputFormat::Markdown => markdown(doc, &sources, show_citations),
        OutputFormat::PlainText => plain(doc, &sources, show_citations),
        OutputFormat::Html => html(doc, &sources, show_citations),
    }
}

fn markdown(doc: &OutputDoc, sources: &Sources, show: bool) -> String {
    let mut out = format!("# {}\n\n_{}_\n", doc.title, date_line(doc));
    for section in &doc.sections {
        out.push_str(&format!("\n## {}\n\n", section.heading));
        if section.lines.is_empty() {
            out.push_str(&format!("_{EMPTY_SECTION}_\n"));
        }
        for line in &section.lines {
            let marks = if show {
                sources.marks(&line.citations)
            } else {
                String::new()
            };
            let text = line_text(&line.text, line.project.as_deref(), "**");
            out.push_str(&format!(
                "- {text}{}\n",
                if marks.is_empty() {
                    String::new()
                } else {
                    format!(" {marks}")
                }
            ));
        }
    }
    if show && !sources.order.is_empty() {
        out.push_str("\n## Sources\n\n");
        for (index, citation) in sources.order.iter().enumerate() {
            out.push_str(&format!("{}. {}\n", index + 1, markdown_source(citation)));
        }
    }
    out
}

fn plain(doc: &OutputDoc, sources: &Sources, show: bool) -> String {
    let mut out = format!("{}\n{}\n", doc.title, date_line(doc));
    for section in &doc.sections {
        out.push_str(&format!("\n{}\n", section.heading));
        if section.lines.is_empty() {
            out.push_str(&format!("  {EMPTY_SECTION}\n"));
        }
        for line in &section.lines {
            let marks = if show {
                sources.marks(&line.citations)
            } else {
                String::new()
            };
            out.push_str(&format!(
                "- {}{}\n",
                line_text(&line.text, line.project.as_deref(), ""),
                if marks.is_empty() {
                    String::new()
                } else {
                    format!(" {marks}")
                }
            ));
        }
    }
    if show && !sources.order.is_empty() {
        out.push_str("\nSources\n");
        for (index, citation) in sources.order.iter().enumerate() {
            out.push_str(&format!("[{}] {}\n", index + 1, plain_source(citation)));
        }
    }
    out
}

fn html(doc: &OutputDoc, sources: &Sources, show: bool) -> String {
    let mut out = format!(
        "<h1>{}</h1>\n<p><em>{}</em></p>\n",
        escape_html(&doc.title),
        escape_html(&date_line(doc))
    );
    for section in &doc.sections {
        out.push_str(&format!("<h2>{}</h2>\n", escape_html(&section.heading)));
        if section.lines.is_empty() {
            out.push_str(&format!("<p><em>{EMPTY_SECTION}</em></p>\n"));
            continue;
        }
        out.push_str("<ul>\n");
        for line in &section.lines {
            let mut item = match line
                .project
                .as_deref()
                .filter(|name| !line.text.starts_with(name))
            {
                Some(name) => format!(
                    "<strong>{}</strong>: {}",
                    escape_html(name),
                    escape_html(&line.text)
                ),
                None => escape_html(&line.text),
            };
            if show {
                for citation in &line.citations {
                    if let Some(number) = sources.number.get(&(citation.id.as_str(), citation.kind))
                    {
                        item.push_str(&format!(
                            " <sup><a href=\"#source-{number}\">[{number}]</a></sup>"
                        ));
                    }
                }
            }
            out.push_str(&format!("<li>{item}</li>\n"));
        }
        out.push_str("</ul>\n");
    }
    if show && !sources.order.is_empty() {
        out.push_str("<h2>Sources</h2>\n<ol>\n");
        for (index, citation) in sources.order.iter().enumerate() {
            let id = escape_html(&citation.id);
            let open = citation
                .memory_id
                .as_deref()
                .map(|memory| format!(" data-memory-id=\"{}\"", escape_html(memory)))
                .unwrap_or_default();
            let memory = if citation.kind == CitationKind::ActivityEvent {
                citation
                    .memory_id
                    .as_deref()
                    .map(|memory| format!(", memory <code>{}</code>", escape_html(memory)))
                    .unwrap_or_default()
            } else {
                String::new()
            };
            out.push_str(&format!(
                "<li id=\"source-{}\"{open}>{} <code>{id}</code>{memory}</li>\n",
                index + 1,
                kind_label(citation.kind)
            ));
        }
        out.push_str("</ol>\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outputs::{Line, OutputKind, Section};

    fn doc() -> OutputDoc {
        OutputDoc {
            id: "d1".to_string(),
            kind: OutputKind::Standup,
            title: "Standup, Oct 14".to_string(),
            start_ms: 0,
            end_ms: 1,
            generated_at_ms: 1_760_000_000_000,
            model_polished: false,
            sections: vec![
                Section {
                    heading: "Yesterday".to_string(),
                    lines: vec![
                        Line {
                            text: "Fixed the <export> job & reran it".to_string(),
                            project: Some("Atlas".to_string()),
                            citations: vec![Citation::memory("mem-1"), Citation::memory("mem-2")],
                        },
                        Line {
                            text: "Wrote the plan".to_string(),
                            project: None,
                            citations: vec![Citation::memory("mem-1")],
                        },
                    ],
                },
                Section {
                    heading: "Blockers".to_string(),
                    lines: Vec::new(),
                },
            ],
        }
    }

    #[test]
    fn markdown_numbers_sources_and_lists_every_id() {
        let text = render(&doc(), OutputFormat::Markdown, true);
        assert!(text.starts_with("# Standup, Oct 14\n"));
        assert!(text.contains("## Yesterday"));
        assert!(text.contains("- **Atlas**: Fixed the <export> job & reran it [1][2]"));
        assert!(text.contains("- Wrote the plan [1]"));
        assert!(text.contains("1. memory `mem-1`"));
        assert!(text.contains("2. memory `mem-2`"));
        assert!(text.contains("_Nothing recorded._"));
    }

    #[test]
    fn plain_text_has_no_markup() {
        let text = render(&doc(), OutputFormat::PlainText, true);
        assert!(text.contains("Yesterday\n- Atlas: Fixed the <export> job & reran it [1][2]"));
        assert!(text.contains("[1] memory mem-1"));
        assert!(!text.contains('#') && !text.contains('`') && !text.contains("**"));
    }

    #[test]
    fn html_escapes_text_and_links_sources() {
        let text = render(&doc(), OutputFormat::Html, true);
        assert!(text
            .contains("<li><strong>Atlas</strong>: Fixed the &lt;export&gt; job &amp; reran it"));
        assert!(text.contains("href=\"#source-1\""));
        assert!(text.contains(
            "<li id=\"source-2\" data-memory-id=\"mem-2\">memory <code>mem-2</code></li>"
        ));
        assert!(!text.contains("<export>"));
    }

    #[test]
    fn citations_can_be_left_off_the_body() {
        for format in [
            OutputFormat::Markdown,
            OutputFormat::PlainText,
            OutputFormat::Html,
        ] {
            let text = render(&doc(), format, false);
            assert!(!text.contains("mem-1"), "{format:?} leaked an id");
            assert!(!text.contains("Sources"));
        }
    }

    #[test]
    fn an_activity_event_source_is_labelled_as_one() {
        let mut d = doc();
        d.sections[0].lines[1].citations = vec![Citation {
            kind: CitationKind::ActivityEvent,
            id: "evt-9".to_string(),
            memory_id: None,
        }];
        let text = render(&d, OutputFormat::Markdown, true);
        assert!(text.contains("activity event `evt-9`"));
    }

    #[test]
    fn an_activity_event_source_names_the_memory_it_cites() {
        let mut d = doc();
        d.sections[0].lines[1].citations = vec![Citation {
            kind: CitationKind::ActivityEvent,
            id: "evt-9".to_string(),
            memory_id: Some("mem-9".to_string()),
        }];
        let markdown = render(&d, OutputFormat::Markdown, true);
        assert!(markdown.contains("activity event `evt-9`, memory `mem-9`"));
        let plain = render(&d, OutputFormat::PlainText, true);
        assert!(plain.contains("activity event evt-9, memory mem-9"));
    }
}
