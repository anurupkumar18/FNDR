//! PDF export helpers and commands.

use std::path::{Path, PathBuf};
use std::process::Command;

use genpdf::Element;

/// Path to the macOS Supplemental font directory where Arial ships.
const MACOS_SUPPLEMENTAL_FONT_DIR: &str = "/System/Library/Fonts/Supplemental";

/// PDF page margins applied uniformly to FNDR exports (millimetres).
const PDF_PAGE_MARGIN: u8 = 18;

/// Load the macOS Arial font family used by FNDR PDF exports.
fn load_pdf_font_family() -> Result<genpdf::fonts::FontFamily<genpdf::fonts::FontData>, String> {
    let font_dir = std::path::Path::new(MACOS_SUPPLEMENTAL_FONT_DIR);
    let load = |name: &str| {
        genpdf::fonts::FontData::load(font_dir.join(name), None)
            .map_err(|err| format!("Failed to load '{name}' from {font_dir:?}: {err}"))
    };
    Ok(genpdf::fonts::FontFamily {
        regular: load("Arial.ttf")?,
        bold: load("Arial Bold.ttf")?,
        italic: load("Arial Italic.ttf")?,
        bold_italic: load("Arial Bold Italic.ttf")?,
    })
}
/// The Daily Summary sections printed above the per-capture list.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct PdfNarrative {
    pub basic: bool,
    pub paragraphs: Vec<String>,
    pub action_items: Vec<String>,
    pub reminders: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
enum PdfBlock {
    Heading(String),
    Paragraph(String),
    Bullet(String),
}

fn strip_bullet(line: &str) -> &str {
    if line.starts_with("- ") || line.starts_with("* ") || line.starts_with("• ") {
        line[2..].trim()
    } else if line.starts_with('-') || line.starts_with('*') || line.starts_with('•') {
        line[1..].trim()
    } else {
        line
    }
}

/// The page content in reading order: the narrative sections, then the old
/// per-capture list under "All activity". Pure, so the layout is testable.
fn pdf_blocks(summary_text: &str, narrative: Option<&PdfNarrative>) -> Vec<PdfBlock> {
    let mut blocks = Vec::new();
    let activity: Vec<&str> = summary_text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    if let Some(narrative) = narrative {
        blocks.push(PdfBlock::Heading(if narrative.basic {
            "Your day (basic summary)".to_string()
        } else {
            "Your day".to_string()
        }));
        blocks.extend(
            narrative
                .paragraphs
                .iter()
                .cloned()
                .map(PdfBlock::Paragraph),
        );
        for (title, items) in [
            ("Action items", &narrative.action_items),
            ("Reminders", &narrative.reminders),
        ] {
            if !items.is_empty() {
                blocks.push(PdfBlock::Heading(title.to_string()));
                blocks.extend(items.iter().cloned().map(PdfBlock::Bullet));
            }
        }
        if !activity.is_empty() {
            blocks.push(PdfBlock::Heading("All activity".to_string()));
        }
    }
    if activity.is_empty() {
        if narrative.is_none() {
            blocks.push(PdfBlock::Paragraph(
                "No activity captured for this date.".to_string(),
            ));
        }
    } else {
        blocks.extend(
            activity
                .into_iter()
                .map(|line| PdfBlock::Bullet(strip_bullet(line).to_string())),
        );
    }
    blocks
}

/// Export a daily summary to a PDF in the Downloads folder
#[tauri::command]
pub async fn export_daily_summary_pdf(
    _app: tauri::AppHandle,
    date_str: String,
    summary_text: String,
    narrative: Option<PdfNarrative>,
) -> Result<String, String> {
    // 1. Resolve Downloads folder
    let downloads_dir = dirs::download_dir()
        .ok_or_else(|| "Could not find Downloads folder on this system.".to_string())?;

    // 2. Prepare filename
    let safe_date = date_str.replace('/', "-").replace(' ', "_");
    let filename = format!("FNDR_Daily_Summary_{}.pdf", safe_date);
    let target_path = downloads_dir.join(filename);

    let mut doc = genpdf::Document::new(load_pdf_font_family()?);
    doc.set_title(format!("FNDR Daily Summary: {}", date_str));

    let mut decorator = genpdf::SimplePageDecorator::new();
    decorator.set_margins(PDF_PAGE_MARGIN);
    doc.set_page_decorator(decorator);

    // Title & Header
    doc.push(
        genpdf::elements::Text::new("FNDR Daily Summary")
            .styled(genpdf::style::Style::new().bold().with_font_size(20)),
    );
    doc.push(
        genpdf::elements::Text::new(format!("Date: {}", date_str))
            .styled(genpdf::style::Style::new().with_font_size(10)),
    );
    doc.push(genpdf::elements::Break::new(1.5));

    let mut list: Option<genpdf::elements::UnorderedList> = None;
    for block in pdf_blocks(&summary_text, narrative.as_ref()) {
        if !matches!(block, PdfBlock::Bullet(_)) {
            if let Some(done) = list.take() {
                doc.push(done);
            }
        }
        match block {
            PdfBlock::Heading(text) => {
                doc.push(genpdf::elements::Break::new(0.8));
                doc.push(
                    genpdf::elements::Text::new(text)
                        .styled(genpdf::style::Style::new().bold().with_font_size(13)),
                );
                doc.push(genpdf::elements::Break::new(0.4));
            }
            PdfBlock::Paragraph(text) => {
                doc.push(genpdf::elements::Paragraph::new(text));
                doc.push(genpdf::elements::Break::new(0.5));
            }
            PdfBlock::Bullet(text) => list
                .get_or_insert_with(genpdf::elements::UnorderedList::new)
                .push(genpdf::elements::Paragraph::new(text)),
        }
    }
    if let Some(done) = list.take() {
        doc.push(done);
    }

    // Save
    doc.render_to_file(&target_path)
        .map_err(|e| format!("Failed to generate PDF file: {}", e))?;

    Ok(target_path.to_string_lossy().to_string())
}

/// Export a weekly FNDR Wrapped recap to a PDF in the Downloads folder.
#[tauri::command]
pub async fn export_weekly_wrapped_pdf(
    _app: tauri::AppHandle,
    start_date: String,
    end_date: String,
    recap_text: String,
) -> Result<String, String> {
    let downloads_dir = dirs::download_dir()
        .ok_or_else(|| "Could not find Downloads folder on this system.".to_string())?;
    let safe_start = start_date.replace('/', "-").replace(' ', "_");
    let safe_end = end_date.replace('/', "-").replace(' ', "_");
    let target_path = downloads_dir.join(format!("FNDR_Wrapped_{safe_start}_to_{safe_end}.pdf"));

    let mut doc = genpdf::Document::new(load_pdf_font_family()?);
    doc.set_title(format!("FNDR Wrapped: {start_date} to {end_date}"));
    let mut decorator = genpdf::SimplePageDecorator::new();
    decorator.set_margins(PDF_PAGE_MARGIN);
    doc.set_page_decorator(decorator);

    doc.push(
        genpdf::elements::Text::new("FNDR Wrapped")
            .styled(genpdf::style::Style::new().bold().with_font_size(20)),
    );
    doc.push(
        genpdf::elements::Text::new(format!("Data collected from {start_date} to {end_date}"))
            .styled(genpdf::style::Style::new().with_font_size(10)),
    );
    doc.push(genpdf::elements::Break::new(1.5));

    for line in recap_text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        doc.push(genpdf::elements::Paragraph::new(line.to_string()));
        doc.push(genpdf::elements::Break::new(0.5));
    }

    doc.render_to_file(&target_path)
        .map_err(|err| format!("Failed to generate PDF file: {err}"))?;
    Ok(target_path.to_string_lossy().to_string())
}

/// Open a PDF exported by FNDR from the user's Downloads folder.
#[tauri::command]
pub async fn open_exported_pdf(path: String) -> Result<(), String> {
    let downloads_dir = dirs::download_dir()
        .ok_or_else(|| "Could not find Downloads folder on this system.".to_string())?;
    let downloads_dir = downloads_dir
        .canonicalize()
        .map_err(|e| format!("Could not resolve Downloads folder: {}", e))?;
    let target_path = PathBuf::from(path);
    let target_path = target_path
        .canonicalize()
        .map_err(|e| format!("Could not find exported PDF: {}", e))?;

    if !target_path.starts_with(&downloads_dir) {
        return Err("FNDR can only open exported PDFs from your Downloads folder.".to_string());
    }

    let is_pdf = target_path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("pdf"));
    if !is_pdf {
        return Err("FNDR can only open exported PDF files.".to_string());
    }

    open_path_with_system(&target_path)
}

#[cfg(target_os = "macos")]
fn open_path_with_system(path: &Path) -> Result<(), String> {
    Command::new("open")
        .arg(path)
        .spawn()
        .map_err(|e| format!("Failed to open exported PDF: {}", e))?;
    Ok(())
}

#[cfg(target_os = "windows")]
fn open_path_with_system(path: &Path) -> Result<(), String> {
    Command::new("cmd")
        .arg("/C")
        .arg("start")
        .arg("")
        .arg(path)
        .spawn()
        .map_err(|e| format!("Failed to open exported PDF: {}", e))?;
    Ok(())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn open_path_with_system(path: &Path) -> Result<(), String> {
    Command::new("xdg-open")
        .arg(path)
        .spawn()
        .map_err(|e| format!("Failed to open exported PDF: {}", e))?;
    Ok(())
}

#[cfg(test)]
mod daily_pdf_tests {
    use super::*;

    fn narrative() -> PdfNarrative {
        PdfNarrative {
            basic: false,
            paragraphs: vec!["Spent the afternoon on Payments.".into()],
            action_items: vec!["Send the Q3 budget draft".into()],
            reminders: vec![],
        }
    }

    #[test]
    fn the_narrative_sections_come_before_the_activity_list() {
        let blocks = pdf_blocks(
            "- Xcode: 3 captures\n\n- Slack: 1 capture",
            Some(&narrative()),
        );
        assert_eq!(
            blocks,
            vec![
                PdfBlock::Heading("Your day".into()),
                PdfBlock::Paragraph("Spent the afternoon on Payments.".into()),
                PdfBlock::Heading("Action items".into()),
                PdfBlock::Bullet("Send the Q3 budget draft".into()),
                PdfBlock::Heading("All activity".into()),
                PdfBlock::Bullet("Xcode: 3 captures".into()),
                PdfBlock::Bullet("Slack: 1 capture".into()),
            ]
        );
    }

    #[test]
    fn a_basic_summary_is_labelled_and_an_empty_section_is_left_out() {
        let basic = PdfNarrative {
            basic: true,
            ..narrative()
        };
        let blocks = pdf_blocks("", Some(&basic));
        assert_eq!(
            blocks[0],
            PdfBlock::Heading("Your day (basic summary)".into())
        );
        assert!(!blocks.contains(&PdfBlock::Heading("Reminders".into())));
        assert!(!blocks.contains(&PdfBlock::Heading("All activity".into())));
    }

    #[test]
    fn without_a_narrative_the_page_is_the_old_bullet_list() {
        assert_eq!(
            pdf_blocks("- one\n* two", None),
            vec![
                PdfBlock::Bullet("one".into()),
                PdfBlock::Bullet("two".into())
            ]
        );
        assert_eq!(
            pdf_blocks("", None),
            vec![PdfBlock::Paragraph(
                "No activity captured for this date.".into()
            )]
        );
    }
}
