use crate::import::*;
use mailparse::{DispositionType, MailHeaderMap, ParsedMail};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub const SKIP: &[&str] = &[
    "Trash, Junk, Spam, Drafts, and other non-selected mailboxes by default",
    "bulk promotional mail by default",
    "attachment bodies, quoted replies, and symlinked paths",
];

#[derive(Clone)]
pub enum MailInput {
    AppleMail(std::path::PathBuf),
    Mbox(std::path::PathBuf),
    Eml(std::path::PathBuf),
}

pub struct MailImport {
    pub inputs: Vec<MailInput>,
    pub selected_mailboxes: Vec<String>,
    pub include_bulk: bool,
}

impl Default for MailImport {
    fn default() -> Self {
        Self::new(Vec::new(), Vec::new(), false)
    }
}

pub struct MailboxInfo {
    pub name: String,
    pub count: usize,
}

impl MailImport {
    pub fn new(
        inputs: Vec<MailInput>,
        selected_mailboxes: Vec<String>,
        include_bulk: bool,
    ) -> Self {
        Self {
            inputs,
            selected_mailboxes,
            include_bulk,
        }
    }

    pub fn discover_mailboxes(&self) -> Result<Vec<MailboxInfo>, ImportError> {
        let mut counts = std::collections::BTreeMap::<String, usize>::new();
        for input in &self.inputs {
            if let MailInput::AppleMail(root) = input {
                for (name, _) in apple_mail_files(root)? {
                    *counts.entry(name).or_default() += 1;
                }
            }
        }
        Ok(counts
            .into_iter()
            .map(|(name, count)| MailboxInfo { name, count })
            .collect())
    }
}

impl ImportSource for MailImport {
    fn kind(&self) -> ImportKind {
        ImportKind::Mail
    }
    fn label(&self) -> &'static str {
        "Mail"
    }
    fn permissions_needed(&self) -> Vec<Permission> {
        if self
            .inputs
            .iter()
            .any(|input| matches!(input, MailInput::AppleMail(_)))
        {
            vec![Permission::FullDiskAccess]
        } else {
            vec![Permission::Files]
        }
    }
    fn scan(
        &self,
        since: Option<Cursor>,
        window_days: u32,
        limit: usize,
    ) -> Result<ScanBatch, ImportError> {
        if limit == 0 {
            return Err(ImportError::InvalidData(
                "scan limit must be positive".into(),
            ));
        }
        let mut messages = Vec::<(PathBuf, bool)>::new();
        for input in &self.inputs {
            match input {
                MailInput::AppleMail(root) => {
                    for (name, path) in apple_mail_files(root)? {
                        let selected = name.eq_ignore_ascii_case("Inbox")
                            || name.eq_ignore_ascii_case("Sent")
                            || name.eq_ignore_ascii_case("Sent Messages")
                            || self
                                .selected_mailboxes
                                .iter()
                                .any(|selected| selected == &name);
                        if selected {
                            messages.push((path, true));
                        }
                    }
                }
                MailInput::Eml(path) | MailInput::Mbox(path) => {
                    messages.push((path.clone(), false))
                }
            }
        }
        messages.sort_by(|a, b| a.0.cmp(&b.0));
        let earliest = chrono::Utc::now()
            .timestamp_millis()
            .saturating_sub(i64::from(window_days).saturating_mul(86_400_000));
        let mut items = Vec::new();
        for (path, apple_mail) in messages {
            let metadata =
                fs::symlink_metadata(&path).map_err(|error| map_mail_io(error, apple_mail))?;
            if metadata.file_type().is_symlink() {
                continue;
            }
            if metadata.len() > 16 * 1024 * 1024 {
                continue;
            }
            let bytes = fs::read(&path).map_err(|error| map_mail_io(error, apple_mail))?;
            if path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("mbox"))
            {
                for message in split_mbox(&bytes) {
                    if let Some(item) = parse_message(message, earliest, self.include_bulk) {
                        items.push(item);
                    }
                }
            } else {
                let message = if path
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("emlx"))
                {
                    emlx_message(&bytes)
                } else {
                    Some(bytes.as_slice())
                };
                if let Some(item) =
                    message.and_then(|message| parse_message(message, earliest, self.include_bulk))
                {
                    items.push(item);
                }
            }
        }
        items.sort_by(|a, b| {
            (a.timestamp_ms, &a.external_id).cmp(&(b.timestamp_ms, &b.external_id))
        });
        let offset = since
            .as_ref()
            .and_then(|cursor| cursor.0.parse::<usize>().ok())
            .unwrap_or(0);
        let total = items.len();
        let page = items
            .into_iter()
            .skip(offset)
            .take(limit)
            .collect::<Vec<_>>();
        let next_offset = offset.saturating_add(page.len());
        Ok(ScanBatch {
            items: page,
            next: (next_offset < total).then(|| Cursor(next_offset.to_string())),
            done: next_offset >= total,
        })
    }
}

pub fn map_mail_io(error: io::Error, apple_mail: bool) -> ImportError {
    if apple_mail && error.kind() == io::ErrorKind::PermissionDenied {
        ImportError::PermissionDenied(Permission::FullDiskAccess)
    } else {
        ImportError::from(error)
    }
}

fn apple_mail_files(root: &Path) -> Result<Vec<(String, PathBuf)>, ImportError> {
    let mut files = Vec::new();
    for version in fs::read_dir(root).map_err(|error| map_mail_io(error, true))? {
        let version = version.map_err(|error| map_mail_io(error, true))?;
        if !version.file_name().to_string_lossy().starts_with('V')
            || !version
                .file_type()
                .map_err(|error| map_mail_io(error, true))?
                .is_dir()
        {
            continue;
        }
        let mailboxes = version.path().join("Mailboxes");
        match fs::symlink_metadata(&mailboxes) {
            Ok(metadata) if metadata.file_type().is_dir() => {
                collect_emlx(&mailboxes, &mailboxes, &mut files)?;
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(map_mail_io(error, true)),
        }
    }
    Ok(files)
}

fn collect_emlx(
    base: &Path,
    dir: &Path,
    out: &mut Vec<(String, PathBuf)>,
) -> Result<(), ImportError> {
    for entry in fs::read_dir(dir).map_err(|error| map_mail_io(error, true))? {
        let entry = entry.map_err(|error| map_mail_io(error, true))?;
        let kind = entry
            .file_type()
            .map_err(|error| map_mail_io(error, true))?;
        if kind.is_symlink() {
            continue;
        }
        let path = entry.path();
        if kind.is_dir() {
            collect_emlx(base, &path, out)?;
        } else if kind.is_file()
            && path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("emlx"))
        {
            let name = path
                .strip_prefix(base)
                .ok()
                .and_then(|relative| {
                    relative.components().find_map(|component| {
                        let text = component.as_os_str().to_string_lossy();
                        text.strip_suffix(".mbox").map(str::to_owned)
                    })
                })
                .unwrap_or_default();
            if !name.is_empty() {
                out.push((name, path));
            }
        }
    }
    Ok(())
}

fn emlx_message(bytes: &[u8]) -> Option<&[u8]> {
    let end = bytes.iter().position(|byte| *byte == b'\n')?;
    let size = std::str::from_utf8(&bytes[..end])
        .ok()?
        .trim()
        .parse::<usize>()
        .ok()?;
    bytes.get(end + 1..end + 1 + size)
}

fn split_mbox(bytes: &[u8]) -> Vec<&[u8]> {
    let mut starts = Vec::new();
    let mut pos = 0;
    for line in bytes.split_inclusive(|byte| *byte == b'\n') {
        if line.starts_with(b"From ") {
            starts.push((pos, pos + line.len()));
        }
        pos += line.len();
    }
    starts
        .iter()
        .enumerate()
        .filter_map(|(index, (_, start))| {
            let end = starts
                .get(index + 1)
                .map(|(next, _)| *next)
                .unwrap_or(bytes.len());
            bytes.get(*start..end)
        })
        .collect()
}

fn parse_message(bytes: &[u8], earliest: i64, include_bulk: bool) -> Option<ImportItem> {
    let parsed = mailparse::parse_mail(bytes).ok()?;
    let header = |name| parsed.headers.get_first_value(name).unwrap_or_default();
    let id = header("Message-ID")
        .trim()
        .trim_start_matches('<')
        .trim_end_matches('>')
        .to_string();
    let title = header("Subject").trim().to_string();
    let timestamp_ms = chrono::DateTime::parse_from_rfc2822(&header("Date"))
        .ok()?
        .timestamp_millis();
    if id.is_empty() || title.is_empty() || timestamp_ms < earliest {
        return None;
    }
    let category =
        format!("{} {}", header("X-Category"), header("X-Gmail-Labels")).to_ascii_lowercase();
    if !include_bulk
        && !header("List-Unsubscribe").is_empty()
        && (category.contains("promotion") || category.contains("marketing"))
    {
        return None;
    }
    let mut plain = None;
    let mut html = None;
    let mut attachments = Vec::new();
    collect_parts(&parsed, &mut plain, &mut html, &mut attachments);
    let body = plain.or_else(|| html.map(|value| html_to_text(&value)))?;
    let body = strip_reply_and_signature(&body);
    let from = header("From");
    let to = header("To");
    let reply = header("In-Reply-To");
    let mut text = format!("From: {from}\nTo: {to}\nDate: {}\n", header("Date"));
    if !reply.is_empty() {
        text.push_str(&format!("In-Reply-To: {reply}\n"));
    }
    text.push_str(&body);
    if !attachments.is_empty() {
        text.push_str(&format!("\nAttachments: {}", attachments.join(", ")));
    }
    let text = text.chars().take(20_000).collect();
    Some(ImportItem {
        kind: ImportKind::Mail,
        external_id: id.clone(),
        timestamp_ms,
        title,
        text,
        reopen_target: Some(format!("message://{id}")),
        app_label: "Mail".into(),
        people: [from, to]
            .into_iter()
            .filter(|person| !person.is_empty())
            .collect(),
        url: None,
    })
}

fn collect_parts(
    part: &ParsedMail<'_>,
    plain: &mut Option<String>,
    html: &mut Option<String>,
    attachments: &mut Vec<String>,
) {
    let disposition = part.get_content_disposition();
    let filename = disposition
        .params
        .get("filename")
        .or_else(|| part.ctype.params.get("name"));
    if disposition.disposition == DispositionType::Attachment || filename.is_some() {
        if let Some(name) = filename {
            attachments.push(name.clone());
        }
        return;
    }
    for subpart in &part.subparts {
        collect_parts(subpart, plain, html, attachments);
    }
    if part.subparts.is_empty() {
        if part.ctype.mimetype.eq_ignore_ascii_case("text/plain") && plain.is_none() {
            *plain = part.get_body().ok();
        }
        if part.ctype.mimetype.eq_ignore_ascii_case("text/html") && html.is_none() {
            *html = part.get_body().ok();
        }
    }
}

fn html_to_text(html: &str) -> String {
    let quoted = regex::Regex::new("(?is)<blockquote\\b[^>]*>.*?</blockquote>").unwrap();
    let scripts = regex::Regex::new("(?is)<(script|style)\\b[^>]*>.*?</(script|style)>").unwrap();
    let breaks = regex::Regex::new("(?i)</?(p|div|br|li|tr|h[1-6])[^>]*>").unwrap();
    let tags = regex::Regex::new("<[^>]*>").unwrap();
    let text = quoted.replace_all(html, "");
    let text = scripts.replace_all(&text, "");
    let text = breaks.replace_all(&text, "\n");
    tags.replace_all(&text, "")
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .to_string()
}

fn strip_reply_and_signature(body: &str) -> String {
    let mut kept = Vec::new();
    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('>')
            || trimmed == "-- "
            || trimmed.starts_with("Sent from my ")
            || trimmed.starts_with("-----Original Message-----")
            || (trimmed.starts_with("On ") && trimmed.ends_with("wrote:"))
        {
            break;
        }
        kept.push(line);
    }
    kept.join("\n").trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const DATE: &str = "Wed, 14 Oct 2026 10:00:00 +0000";

    #[test]
    fn imports_eml_with_mime_decoding_and_strips_reply_and_signature() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("message.eml");
        fs::write(&path, format!("From: Ana <ana@example.test>\r\nTo: Ben <ben@example.test>\r\nSubject: Update\r\nDate: {DATE}\r\nMessage-ID: <one@example.test>\r\nIn-Reply-To: <prior@example.test>\r\nContent-Type: multipart/mixed; boundary=abc\r\n\r\n--abc\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Transfer-Encoding: quoted-printable\r\n\r\nReady=20today.=0AOn Tue, Ben wrote:=0A> Old text=0A-- =0AAna\r\n--abc\r\nContent-Type: application/pdf; name=plan.pdf\r\nContent-Disposition: attachment; filename=plan.pdf\r\nContent-Transfer-Encoding: base64\r\n\r\nc2VjcmV0\r\n--abc--\r\n")).unwrap();
        let source = MailImport {
            inputs: vec![MailInput::Eml(path)],
            selected_mailboxes: vec![],
            include_bulk: false,
        };
        let batch = source.scan(None, 30, 10).unwrap();
        assert_eq!(batch.items.len(), 1);
        let item = &batch.items[0];
        assert_eq!(item.external_id, "one@example.test");
        assert_eq!(
            item.reopen_target.as_deref(),
            Some("message://one@example.test")
        );
        assert!(item.text.contains("Ready today."));
        assert!(item.text.contains("plan.pdf"));
        assert!(!item.text.contains("Old text"));
        assert!(!item.text.contains("secret"));
    }

    #[test]
    fn emlx_mbox_html_only_base64_and_non_utf8() {
        let dir = tempfile::tempdir().unwrap();
        let emlx = dir.path().join("1.emlx");
        let message = format!("From: Jo <jo@example.test>\nSubject: Cafe\nDate: {DATE}\nMessage-ID: <two@example.test>\nContent-Type: text/html; charset=iso-8859-1\nContent-Transfer-Encoding: base64\n\nPHA+Q2Fm6SB1cGRhdGU8L3A+\n");
        fs::write(
            &emlx,
            format!("{}\n{}<plist>metadata</plist>", message.len(), message),
        )
        .unwrap();
        let mbox = dir.path().join("drop.mbox");
        fs::write(&mbox, format!("From jo@example.test Wed Oct 14 10:00:00 2026\nFrom: Jo <jo@example.test>\nSubject: Mbox\nDate: {DATE}\nMessage-ID: <three@example.test>\n\nMbox body\n")).unwrap();
        let source = MailImport {
            inputs: vec![MailInput::Eml(emlx), MailInput::Mbox(mbox)],
            selected_mailboxes: vec![],
            include_bulk: false,
        };
        let batch = source.scan(None, 30, 10).unwrap();
        assert_eq!(batch.items.len(), 2);
        assert!(batch
            .items
            .iter()
            .any(|item| item.text.contains("Café update")));
        assert!(batch
            .items
            .iter()
            .any(|item| item.text.contains("Mbox body")));
        assert!(!batch
            .items
            .iter()
            .any(|item| item.text.contains("metadata")));
    }

    #[test]
    fn default_mailbox_scope_bulk_filter_and_discovery() {
        let dir = tempfile::tempdir().unwrap();
        let inbox = dir.path().join("V10/Mailboxes/Inbox.mbox/Messages");
        let private = dir.path().join("V10/Mailboxes/Private.mbox/Messages");
        fs::create_dir_all(&inbox).unwrap();
        fs::create_dir_all(&private).unwrap();
        let mail = |id: &str, bulk: bool| {
            format!("From: A <a@example.test>\nSubject: {id}\nDate: {DATE}\nMessage-ID: <{id}@example.test>\n{}\nBody\n", if bulk { "List-Unsubscribe: <mailto:off@example.test>\nX-Category: promotions" } else { "" })
        };
        fs::write(
            inbox.join("1.emlx"),
            format!("{}\n{}", mail("keep", false).len(), mail("keep", false)),
        )
        .unwrap();
        fs::write(
            inbox.join("2.emlx"),
            format!("{}\n{}", mail("bulk", true).len(), mail("bulk", true)),
        )
        .unwrap();
        fs::write(
            private.join("3.emlx"),
            format!(
                "{}\n{}",
                mail("private", false).len(),
                mail("private", false)
            ),
        )
        .unwrap();
        let source = MailImport {
            inputs: vec![MailInput::AppleMail(dir.path().to_path_buf())],
            selected_mailboxes: vec![],
            include_bulk: false,
        };
        let found = source.discover_mailboxes().unwrap();
        assert!(found
            .iter()
            .any(|mailbox| mailbox.name == "Inbox" && mailbox.count == 2));
        assert!(found
            .iter()
            .any(|mailbox| mailbox.name == "Private" && mailbox.count == 1));
        let batch = source.scan(None, 30, 10).unwrap();
        assert_eq!(batch.items.len(), 1);
        assert_eq!(batch.items[0].external_id, "keep@example.test");
        let opted_in = MailImport {
            inputs: vec![MailInput::AppleMail(dir.path().to_path_buf())],
            selected_mailboxes: vec!["Private".into()],
            include_bulk: true,
        };
        let opted_in_batch = opted_in.scan(None, 30, 10).unwrap();
        assert_eq!(opted_in_batch.items.len(), 3);
    }

    #[test]
    fn malformed_empty_huge_and_permission_denied() {
        let dir = tempfile::tempdir().unwrap();
        let malformed = dir.path().join("bad.eml");
        fs::write(&malformed, "not a message").unwrap();
        let empty = dir.path().join("empty.mbox");
        fs::write(&empty, "").unwrap();
        let huge = dir.path().join("huge.eml");
        fs::write(&huge, format!("From: A <a@example.test>\nSubject: Huge\nDate: {DATE}\nMessage-ID: <huge@example.test>\n\n{}", "é".repeat(25_000))).unwrap();
        let source = MailImport {
            inputs: vec![
                MailInput::Eml(malformed),
                MailInput::Mbox(empty),
                MailInput::Eml(huge),
            ],
            selected_mailboxes: vec![],
            include_bulk: false,
        };
        let batch = source.scan(None, 30, 10).unwrap();
        assert_eq!(batch.items.len(), 1);
        assert_eq!(batch.items[0].text.chars().count(), 20_000);
        let error = super::map_mail_io(std::io::Error::from_raw_os_error(13), true);
        assert_eq!(
            error,
            ImportError::PermissionDenied(Permission::FullDiskAccess)
        );
        assert!(error.advice().contains("Full Disk Access"));
    }

    #[test]
    fn multiple_mbox_messages_and_html_quotes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("drop.mbox");
        let first = format!("From: A <a@example.test>\nSubject: First\nDate: {DATE}\nMessage-ID: <first@example.test>\nContent-Type: text/html\n\n<p>Current</p><blockquote>Private old reply</blockquote><script>hidden</script>\n");
        let second = format!("From: B <b@example.test>\nSubject: Second\nDate: {DATE}\nMessage-ID: <second@example.test>\n\nSecond body\n");
        fs::write(&path, format!("From a@example.test Wed Oct 14 10:00:00 2026\n{first}From b@example.test Wed Oct 14 10:00:00 2026\n{second}")).unwrap();
        let source = MailImport {
            inputs: vec![MailInput::Mbox(path)],
            selected_mailboxes: vec![],
            include_bulk: false,
        };
        let first_page = source.scan(None, 30, 1).unwrap();
        assert_eq!(first_page.items.len(), 1);
        assert!(!first_page.items[0].text.contains("Private old reply"));
        assert!(!first_page.items[0].text.contains("hidden"));
        let second_page = source.scan(first_page.next, 30, 1).unwrap();
        assert_eq!(second_page.items[0].external_id, "second@example.test");
        assert!(second_page.done);
    }

    #[test]
    fn corrupt_emlx_is_skipped_without_leaking_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("corrupt.emlx");
        fs::write(&path, "1000\nshort\n<plist>secret</plist>").unwrap();
        let source = MailImport {
            inputs: vec![MailInput::Eml(path)],
            selected_mailboxes: vec![],
            include_bulk: false,
        };
        assert!(source.scan(None, 30, 10).unwrap().items.is_empty());
    }
}
