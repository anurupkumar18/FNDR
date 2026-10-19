//! Reading files into text for the index. Documents now; PDFs, pictures,
//! sound and video come with their parts of ADR 033. Ported from DigUp's
//! `DocExtractor` and its portable docx and odt readers.

use quick_xml::events::Event;
use quick_xml::Reader;
use regex::Regex;
use std::io::Read;
use std::path::Path;
use std::sync::LazyLock;

/// Bigger documents than this are not read as text (a log or a dump saved as .txt).
pub const MAX_DOC_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ExtractError {
    #[error("unreadable: {0}")]
    Unreadable(String),
    #[error("{0}")]
    Empty(&'static str),
    #[error("too big to read as text")]
    TooBig,
}

/// The text of a document: plain text as is, HTML with its tags stripped,
/// docx and odt from their XML, rtf and old .doc through macOS's `textutil`.
pub fn document_text(path: &Path) -> Result<String, ExtractError> {
    let size = std::fs::metadata(path)
        .map_err(|error| ExtractError::Unreadable(error.to_string()))?
        .len();
    if size > MAX_DOC_BYTES {
        return Err(ExtractError::TooBig);
    }
    let ext = path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_lowercase();
    let text = match ext.as_str() {
        "docx" => zipped_xml_text(path, "word/document.xml", b"t", b"p")?,
        "odt" => zipped_xml_text(path, "content.xml", b"__none__", b"p")?,
        "html" | "htm" => plain_text_from_html(&read_text(path)?),
        "rtf" | "doc" => textutil(path)?,
        _ => read_text(path)?,
    };
    if text.trim().is_empty() {
        return Err(ExtractError::Empty("no text"));
    }
    Ok(text)
}

/// UTF-8 (with or without a byte order mark), else Latin-1.
pub fn read_text(path: &Path) -> Result<String, ExtractError> {
    let bytes = std::fs::read(path).map_err(|error| ExtractError::Unreadable(error.to_string()))?;
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(&bytes);
    Ok(match std::str::from_utf8(bytes) {
        Ok(text) => text.to_string(),
        Err(_) => bytes.iter().map(|byte| *byte as char).collect(),
    })
}

/// Text from one XML part of a zip (docx, odt). Text nodes inside `text_tag`
/// elements (any namespace) are kept; or, when the format keeps text directly
/// in paragraphs (odt), all text. Each `paragraph_tag` ends a line.
fn zipped_xml_text(
    path: &Path,
    part: &str,
    text_tag: &[u8],
    paragraph_tag: &[u8],
) -> Result<String, ExtractError> {
    let file =
        std::fs::File::open(path).map_err(|error| ExtractError::Unreadable(error.to_string()))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|error| ExtractError::Unreadable(error.to_string()))?;
    let mut xml = String::new();
    archive
        .by_name(part)
        .map_err(|error| ExtractError::Unreadable(error.to_string()))?
        .take(MAX_DOC_BYTES)
        .read_to_string(&mut xml)
        .map_err(|error| ExtractError::Unreadable(error.to_string()))?;
    let all_text = text_tag == b"__none__";
    let mut reader = Reader::from_str(&xml);
    let mut out = String::new();
    let mut inside = 0_usize;
    loop {
        match reader.read_event() {
            Ok(Event::Start(element)) if element.local_name().as_ref() == text_tag => inside += 1,
            Ok(Event::End(element)) if element.local_name().as_ref() == text_tag => {
                inside = inside.saturating_sub(1)
            }
            Ok(Event::End(element))
                if element.local_name().as_ref() == paragraph_tag
                    || element.local_name().as_ref() == b"h" =>
            {
                out.push('\n')
            }
            Ok(Event::Empty(element)) if matches!(element.local_name().as_ref(), b"tab" | b"s") => {
                out.push(' ')
            }
            Ok(Event::Empty(element)) if element.local_name().as_ref() == b"br" => out.push('\n'),
            Ok(Event::Text(text)) if inside > 0 || all_text => {
                if let Ok(text) = text.decode() {
                    out.push_str(&text);
                }
            }
            Ok(Event::GeneralRef(reference)) if inside > 0 || all_text => {
                let name = reference.decode().unwrap_or_default();
                out.push_str(match name.as_ref() {
                    "amp" => "&",
                    "lt" => "<",
                    "gt" => ">",
                    "quot" => "\"",
                    "apos" => "'",
                    _ => " ",
                });
            }
            Ok(Event::Eof) => break,
            Err(error) => return Err(ExtractError::Unreadable(error.to_string())),
            _ => {}
        }
    }
    Ok(out)
}

#[cfg(target_os = "macos")]
fn textutil(path: &Path) -> Result<String, ExtractError> {
    let output = std::process::Command::new("/usr/bin/textutil")
        .args(["-convert", "txt", "-stdout"])
        .arg(path)
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .map_err(|error| ExtractError::Unreadable(error.to_string()))?;
    if !output.status.success() {
        return Err(ExtractError::Unreadable(
            "textutil could not read it".into(),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(not(target_os = "macos"))]
fn textutil(_path: &Path) -> Result<String, ExtractError> {
    Err(ExtractError::Unreadable(
        "rtf and doc are read only on a Mac".into(),
    ))
}

static HTML_DROPS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    [
        r"(?is)<script\b.*?</script\s*>",
        r"(?is)<style\b.*?</style\s*>",
        r"(?s)<!--.*?-->",
        r"(?is)<head\b.*?</head\s*>",
    ]
    .into_iter()
    .map(|pattern| Regex::new(pattern).unwrap())
    .collect()
});
static HTML_BREAKS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)<(br|/p|/div|/li|/h[1-6]|/tr|/section|/article)\b[^>]*>").unwrap()
});
static HTML_TAGS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<[^>]+>").unwrap());
static NUMERIC_ENTITY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"&#(x[0-9a-fA-F]+|[0-9]+);").unwrap());
static SPACES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[ \t]+").unwrap());
static LINE_BREAKS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s*\n\s*").unwrap());

/// Good enough for search: drops scripts, styles and comments, turns block ends
/// into line breaks, strips the remaining tags, decodes common entities.
pub fn plain_text_from_html(html: &str) -> String {
    let mut text = html.to_string();
    for pattern in HTML_DROPS.iter() {
        text = pattern.replace_all(&text, " ").into_owned();
    }
    text = HTML_BREAKS.replace_all(&text, "\n").into_owned();
    text = HTML_TAGS.replace_all(&text, " ").into_owned();
    for (entity, character) in [
        ("&nbsp;", " "),
        ("&lt;", "<"),
        ("&gt;", ">"),
        ("&quot;", "\""),
        ("&#39;", "'"),
        ("&apos;", "'"),
        ("&mdash;", "\u{2014}"),
        ("&ndash;", "\u{2013}"),
        ("&hellip;", "\u{2026}"),
        ("&amp;", "&"),
    ] {
        text = text.replace(entity, character);
    }
    text = NUMERIC_ENTITY
        .replace_all(&text, |captures: &regex::Captures<'_>| {
            let code = &captures[1];
            let value = match code.strip_prefix('x') {
                Some(hex) => u32::from_str_radix(hex, 16).ok(),
                None => code.parse().ok(),
            };
            value
                .and_then(char::from_u32)
                .map_or(" ".to_string(), String::from)
        })
        .into_owned();
    text = SPACES.replace_all(&text, " ").into_owned();
    LINE_BREAKS.replace_all(&text, "\n").trim().to_string()
}

static CHUNK_SPACES: LazyLock<Regex> = LazyLock::new(|| Regex::new("[ \t\u{00A0}]+").unwrap());
static CHUNK_GAPS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\n\s*\n\s*\n+").unwrap());

/// Splits text into pieces of about `size` characters (about 450 tokens) that
/// overlap a little, breaking at whitespace. At most `limit` pieces.
pub fn chunks(text: &str, size: usize, overlap: usize, limit: usize) -> Vec<String> {
    let clean = CHUNK_SPACES.replace_all(text, " ");
    let clean = CHUNK_GAPS.replace_all(&clean, "\n\n");
    let chars: Vec<char> = clean.trim().chars().collect();
    let mut out = Vec::new();
    let mut start = 0;
    while start < chars.len() && out.len() < limit {
        let mut end = (start + size).min(chars.len());
        if end < chars.len() {
            if let Some(space) = chars[start..end].iter().rposition(|c| c.is_whitespace()) {
                if space > size / 2 {
                    end = start + space;
                }
            }
        }
        let piece: String = chars[start..end].iter().collect();
        let piece = piece.trim();
        if !piece.is_empty() {
            out.push(piece.to_string());
        }
        if end >= chars.len() {
            break;
        }
        let mut next = end.saturating_sub(overlap).max(start);
        if next <= start {
            next = end;
        }
        // Start the next piece on a word boundary.
        while next < end && !chars[next].is_whitespace() {
            next += 1;
        }
        start = next;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn zip_with(part: &str, xml: &str) -> tempfile::NamedTempFile {
        let file = tempfile::NamedTempFile::new().unwrap();
        let mut writer = zip::ZipWriter::new(file.reopen().unwrap());
        writer
            .start_file(part, zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(xml.as_bytes()).unwrap();
        writer.finish().unwrap();
        file
    }

    #[test]
    fn docx_text_keeps_paragraphs() {
        let xml = r#"<w:document xmlns:w="w"><w:body><w:p><w:r><w:t>Pets need</w:t></w:r><w:r><w:t xml:space="preserve"> consent &amp; a fee.</w:t></w:r></w:p><w:p><w:r><w:t>Second</w:t></w:r></w:p></w:body></w:document>"#;
        let file = zip_with("word/document.xml", xml);
        let text = zipped_xml_text(file.path(), "word/document.xml", b"t", b"p").unwrap();
        assert_eq!(text, "Pets need consent & a fee.\nSecond\n");
    }

    #[test]
    fn odt_text_is_read_from_paragraphs() {
        let xml = r#"<office:document-content xmlns:office="o" xmlns:text="t"><office:body><text:h>Title</text:h><text:p>One<text:s/>two</text:p></office:body></office:document-content>"#;
        let file = zip_with("content.xml", xml);
        let text = zipped_xml_text(file.path(), "content.xml", b"__none__", b"p").unwrap();
        assert_eq!(text, "Title\nOne two\n");
    }

    #[test]
    fn html_becomes_lines_of_text() {
        let html = "<html><head><title>x</title></head><body><script>var a</script><p>Caf&eacute; &amp; bar&#33;</p><div>Second&nbsp;line</div></body></html>";
        assert_eq!(
            plain_text_from_html(html),
            "Caf&eacute; & bar!\nSecond line"
        );
    }

    #[test]
    fn latin1_text_is_read() {
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), [b'c', b'a', b'f', 0xE9]).unwrap();
        assert_eq!(read_text(file.path()).unwrap(), "caf\u{E9}");
    }

    #[test]
    fn empty_documents_are_reported() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("blank.md");
        std::fs::write(&path, "  \n ").unwrap();
        assert_eq!(document_text(&path), Err(ExtractError::Empty("no text")));
    }

    #[test]
    fn chunks_overlap_and_break_at_spaces() {
        let text = (0..1000)
            .map(|n| format!("word{n}"))
            .collect::<Vec<_>>()
            .join(" ");
        let pieces = chunks(&text, 1800, 200, 100);
        assert!(pieces.len() > 1);
        for piece in &pieces {
            assert!(piece.chars().count() <= 1800);
            assert!(piece.starts_with("word"), "{}", &piece[..10]);
        }
        let last_of_first = pieces[0].split(' ').next_back().unwrap();
        assert!(pieces[1].contains(last_of_first));
        assert_eq!(chunks(&text, 1800, 200, 2).len(), 2);
        assert!(chunks("   ", 1800, 200, 5).is_empty());
    }
}
