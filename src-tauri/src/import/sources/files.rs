use crate::import::*;
use quick_xml::events::Event;
use quick_xml::Reader;
use std::fs::{self, File, OpenOptions};
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::time::UNIX_EPOCH;

const MAX_FILE_BYTES: u64 = 5 * 1024 * 1024;
const MAX_TEXT_CHARS: usize = 20_000;

// Names and trees that must never be read, including when a custom root includes them.
pub const SKIPPED_PATH_PATTERNS: &[&str] = &[
    "Library",
    ".*",
    ".env*",
    "*.pem",
    "*.key",
    "id_rsa*",
    "*.keychain*",
    "node_modules",
    "target",
    ".git",
    "build",
    "dist",
    "out",
    "DerivedData",
    ".next",
];

pub struct RecentFilesSource {
    roots: Vec<PathBuf>,
    now_ms: Option<i64>,
}

impl RecentFilesSource {
    pub fn new(roots: Vec<PathBuf>, now_ms: i64) -> Self {
        Self {
            roots,
            now_ms: Some(now_ms),
        }
    }

    pub fn live(roots: Vec<PathBuf>) -> Self {
        Self {
            roots,
            now_ms: None,
        }
    }

    pub fn for_current_user() -> Option<Self> {
        let home = dirs::home_dir()?;
        let roots = ["Documents", "Desktop", "Downloads"]
            .map(|name| home.join(name))
            .to_vec();
        Some(Self::live(roots))
    }
}

impl Default for RecentFilesSource {
    fn default() -> Self {
        Self::for_current_user().unwrap_or_else(|| Self::live(Vec::new()))
    }
}

impl ImportSource for RecentFilesSource {
    fn kind(&self) -> ImportKind {
        ImportKind::File
    }
    fn label(&self) -> &'static str {
        "Recent files"
    }
    fn permissions_needed(&self) -> Vec<Permission> {
        vec![Permission::Files]
    }

    fn scan(
        &self,
        since: Option<Cursor>,
        window_days: u32,
        limit: usize,
    ) -> Result<ScanBatch, ImportError> {
        if limit == 0 {
            return Ok(ScanBatch {
                items: vec![],
                next: None,
                done: true,
            });
        }
        let now_ms = self
            .now_ms
            .unwrap_or_else(|| chrono::Utc::now().timestamp_millis());
        let cutoff = now_ms.saturating_sub(i64::from(window_days).saturating_mul(86_400_000));
        let mut items = Vec::new();
        for root in &self.roots {
            if !root.exists() {
                continue;
            }
            let root = root
                .canonicalize()
                .map_err(|error| permission_or_io(error))?;
            if blocked_path(&root) {
                continue;
            }
            let mut pending = vec![root];
            while let Some(dir) = pending.pop() {
                let entries = fs::read_dir(&dir).map_err(permission_or_io)?;
                for entry in entries {
                    let entry = entry.map_err(permission_or_io)?;
                    let path = entry.path();
                    let kind = entry.file_type().map_err(permission_or_io)?;
                    if kind.is_symlink() || blocked_name(&entry.file_name().to_string_lossy()) {
                        continue;
                    }
                    if kind.is_dir() {
                        pending.push(path);
                        continue;
                    }
                    if !kind.is_file() {
                        continue;
                    }
                    let metadata = match entry.metadata() {
                        Ok(value) => value,
                        Err(_) => continue,
                    };
                    if metadata.len() > MAX_FILE_BYTES {
                        continue;
                    }
                    let Ok(modified) = metadata.modified() else {
                        continue;
                    };
                    let Ok(duration) = modified.duration_since(UNIX_EPOCH) else {
                        continue;
                    };
                    let timestamp_ms = duration.as_millis().min(i64::MAX as u128) as i64;
                    if timestamp_ms < cutoff || timestamp_ms > now_ms {
                        continue;
                    }
                    let extension = path
                        .extension()
                        .and_then(|value| value.to_str())
                        .unwrap_or("")
                        .to_ascii_lowercase();
                    if !matches!(
                        extension.as_str(),
                        "txt"
                            | "md"
                            | "rs"
                            | "ts"
                            | "tsx"
                            | "js"
                            | "jsx"
                            | "py"
                            | "go"
                            | "swift"
                            | "c"
                            | "h"
                            | "cpp"
                            | "cc"
                            | "cxx"
                            | "hpp"
                            | "hxx"
                            | "java"
                            | "kt"
                            | "rb"
                            | "php"
                            | "m"
                            | "mm"
                            | "sh"
                            | "html"
                            | "css"
                            | "scss"
                            | "toml"
                            | "yaml"
                            | "yml"
                            | "xml"
                            | "sql"
                            | "csv"
                            | "json"
                            | "rtf"
                            | "docx"
                            | "pdf"
                    ) {
                        continue;
                    }
                    let title = entry.file_name().to_string_lossy().to_string();
                    let text = match extract_text(&path, &extension) {
                        Ok(text) => text.chars().take(MAX_TEXT_CHARS).collect(),
                        Err(_) => continue,
                    };
                    let absolute = path.to_string_lossy().to_string();
                    items.push(ImportItem {
                        kind: ImportKind::File,
                        external_id: absolute.clone(),
                        timestamp_ms,
                        title,
                        text,
                        reopen_target: Some(absolute),
                        app_label: "Files".into(),
                        people: vec![],
                        url: None,
                    });
                }
            }
        }
        items.sort_by(|a, b| {
            a.timestamp_ms
                .cmp(&b.timestamp_ms)
                .then_with(|| a.external_id.cmp(&b.external_id))
        });
        items.dedup_by(|a, b| a.external_id == b.external_id);
        if let Some(Cursor(cursor)) = since {
            items.retain(|item| cursor_key(item) > cursor);
        }
        let done = items.len() <= limit;
        items.truncate(limit);
        let next = if done {
            None
        } else {
            items.last().map(|item| Cursor(cursor_key(item)))
        };
        Ok(ScanBatch { items, next, done })
    }
}

fn cursor_key(item: &ImportItem) -> String {
    format!("{:019}:{}", item.timestamp_ms, item.external_id)
}

fn permission_or_io(error: std::io::Error) -> ImportError {
    if error.kind() == std::io::ErrorKind::PermissionDenied {
        ImportError::PermissionDenied(Permission::Files)
    } else {
        ImportError::from(error)
    }
}

fn blocked_path(path: &Path) -> bool {
    path.components().any(|component| match component {
        Component::Normal(name) => blocked_name(&name.to_string_lossy()),
        _ => false,
    })
}

fn blocked_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.starts_with('.')
        || lower == "library"
        || lower == "node_modules"
        || lower == "target"
        || lower == "build"
        || lower == "dist"
        || lower == "out"
        || lower == "deriveddata"
        || lower.starts_with("cmake-build")
        || lower == "keychains"
        || lower.starts_with("id_rsa")
        || lower.ends_with(".pem")
        || lower.ends_with(".key")
        || lower.ends_with(".keychain")
        || lower.ends_with(".keychain-db")
}

fn extract_text(path: &Path, extension: &str) -> Result<String, ImportError> {
    if extension == "pdf" {
        return Ok(path.to_string_lossy().into_owned());
    }
    if extension == "docx" {
        return extract_docx(path);
    }
    let mut bytes = Vec::new();
    open_regular_file(path)?
        .take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(ImportError::InvalidData("File too large".into()));
    }
    let raw =
        String::from_utf8(bytes).map_err(|error| ImportError::InvalidData(error.to_string()))?;
    if extension == "rtf" {
        Ok(strip_rtf(&raw))
    } else {
        Ok(raw)
    }
}

fn extract_docx(path: &Path) -> Result<String, ImportError> {
    let file = open_regular_file(path)?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|error| ImportError::InvalidData(error.to_string()))?;
    let document = archive
        .by_name("word/document.xml")
        .map_err(|error| ImportError::InvalidData(error.to_string()))?;
    if document.size() > MAX_FILE_BYTES {
        return Err(ImportError::InvalidData("DOCX text too large".into()));
    }
    let mut xml = String::new();
    document.take(MAX_FILE_BYTES + 1).read_to_string(&mut xml)?;
    if xml.len() as u64 > MAX_FILE_BYTES {
        return Err(ImportError::InvalidData("DOCX text too large".into()));
    }
    let mut reader = Reader::from_str(&xml);
    let mut output = String::new();
    let mut in_text = false;
    loop {
        match reader.read_event() {
            Ok(Event::Start(tag)) if tag.local_name().as_ref() == b"t" => in_text = true,
            Ok(Event::End(tag)) if tag.local_name().as_ref() == b"t" => in_text = false,
            Ok(Event::End(tag)) if tag.local_name().as_ref() == b"p" => output.push('\n'),
            Ok(Event::Text(text)) if in_text => {
                let decoded = text
                    .decode()
                    .map_err(|error| ImportError::InvalidData(error.to_string()))?;
                let plain = quick_xml::escape::unescape(&decoded)
                    .map_err(|error| ImportError::InvalidData(error.to_string()))?;
                output.push_str(&plain);
            }
            Ok(Event::GeneralRef(reference)) if in_text => {
                if let Some(character) = reference
                    .resolve_char_ref()
                    .map_err(|error| ImportError::InvalidData(error.to_string()))?
                {
                    output.push(character);
                } else {
                    let name = reference
                        .decode()
                        .map_err(|error| ImportError::InvalidData(error.to_string()))?;
                    if let Some(value) = quick_xml::escape::resolve_predefined_entity(&name) {
                        output.push_str(value);
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(error) => return Err(ImportError::InvalidData(error.to_string())),
            _ => {}
        }
        if output.chars().count() >= MAX_TEXT_CHARS {
            break;
        }
    }
    Ok(output)
}

fn open_regular_file(path: &Path) -> Result<File, ImportError> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(target_os = "macos")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    let file = options.open(path)?;
    if !file.metadata()?.is_file() {
        return Err(ImportError::InvalidData("Not a regular file".into()));
    }
    Ok(file)
}

fn strip_rtf(raw: &str) -> String {
    let mut output = String::new();
    let mut chars = raw.chars().peekable();
    while let Some(character) = chars.next() {
        match character {
            '{' | '}' => {}
            '\\' => match chars.peek().copied() {
                Some('\\' | '{' | '}') => output.push(chars.next().unwrap()),
                Some('\'') => {
                    chars.next();
                    let hex: String = chars.by_ref().take(2).collect();
                    if let Ok(value) = u8::from_str_radix(&hex, 16) {
                        output.push(char::from(value));
                    }
                }
                Some(_) => {
                    let word: String = chars
                        .by_ref()
                        .take_while(|value| value.is_ascii_alphabetic())
                        .collect();
                    while chars
                        .peek()
                        .is_some_and(|value| value.is_ascii_digit() || *value == '-')
                    {
                        chars.next();
                    }
                    if chars.peek() == Some(&' ') {
                        chars.next();
                    }
                    if word == "par" || word == "line" {
                        output.push('\n');
                    }
                }
                None => {}
            },
            _ => output.push(character),
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn source(root: &Path) -> RecentFilesSource {
        RecentFilesSource::new(
            vec![root.to_path_buf()],
            chrono::Utc::now().timestamp_millis() + 10_000,
        )
    }

    #[test]
    fn reads_unicode_text_and_skips_secrets_large_files_and_symlink_loop() {
        let root = tempfile::Builder::new()
            .prefix("fndr_fixture")
            .tempdir()
            .unwrap();
        fs::write(root.path().join("notes.md"), "Résumé for café").unwrap();
        fs::write(root.path().join(".env"), "SECRET").unwrap();
        fs::write(root.path().join("private.pem"), "SECRET").unwrap();
        fs::write(root.path().join("id_rsa_backup"), "SECRET").unwrap();
        fs::write(
            root.path().join("huge.txt"),
            vec![b'x'; MAX_FILE_BYTES as usize + 1],
        )
        .unwrap();
        fs::create_dir(root.path().join("nested")).unwrap();
        fs::create_dir(root.path().join("node_modules")).unwrap();
        fs::write(root.path().join("node_modules/private.ts"), "SECRET").unwrap();
        fs::create_dir(root.path().join(".hidden")).unwrap();
        fs::write(root.path().join(".hidden/private.txt"), "SECRET").unwrap();
        std::os::unix::fs::symlink(root.path(), root.path().join("nested/loop")).unwrap();
        let batch = source(root.path()).scan(None, 30, 100).unwrap();
        assert_eq!(batch.items.len(), 1);
        assert_eq!(batch.items[0].title, "notes.md");
        assert_eq!(batch.items[0].text, "Résumé for café");
        assert!(batch.items[0]
            .reopen_target
            .as_ref()
            .unwrap()
            .starts_with('/'));
    }

    #[test]
    fn docx_and_rtf_extract_text_and_pdf_stays_metadata_only() {
        let root = tempfile::Builder::new()
            .prefix("fndr_fixture")
            .tempdir()
            .unwrap();
        let path = root.path().join("draft.docx");
        let file = File::create(&path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        writer
            .start_file(
                "word/document.xml",
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Deflated),
            )
            .unwrap();
        writer.write_all(br#"<w:document xmlns:w="w"><w:p><w:r><w:t>Tom &amp; Jerry</w:t></w:r></w:p></w:document>"#).unwrap();
        writer.finish().unwrap();
        fs::write(root.path().join("memo.rtf"), r"{\rtf1 Hello\par world}").unwrap();
        fs::write(root.path().join("report.pdf"), b"%PDF-1.4 synthetic").unwrap();
        let batch = source(root.path()).scan(None, 30, 100).unwrap();
        assert_eq!(batch.items.len(), 3);
        assert!(batch
            .items
            .iter()
            .any(|item| item.title == "draft.docx" && item.text.contains("Tom & Jerry")));
        assert!(batch
            .items
            .iter()
            .any(|item| item.title == "memo.rtf" && item.text.contains("Hello\nworld")));
        assert!(batch
            .items
            .iter()
            .any(|item| item.title == "report.pdf" && item.text.ends_with("report.pdf")));
    }

    #[test]
    fn corrupt_docx_is_skipped_and_text_is_capped() {
        let root = tempfile::Builder::new()
            .prefix("fndr_fixture")
            .tempdir()
            .unwrap();
        fs::write(root.path().join("bad.docx"), b"not a zip").unwrap();
        fs::write(root.path().join("long.txt"), "é".repeat(30_000)).unwrap();
        let batch = source(root.path()).scan(None, 30, 100).unwrap();
        assert_eq!(batch.items.len(), 1);
        assert_eq!(batch.items[0].text.chars().count(), MAX_TEXT_CHARS);
    }

    #[test]
    fn library_root_is_never_walked() {
        let root = tempfile::Builder::new()
            .prefix("fndr_fixture")
            .tempdir()
            .unwrap();
        let library = root.path().join("Library");
        fs::create_dir(&library).unwrap();
        fs::write(library.join("data.txt"), "private").unwrap();
        assert!(source(&library)
            .scan(None, 30, 100)
            .unwrap()
            .items
            .is_empty());
    }

    #[test]
    fn files_permission_error_is_typed() {
        let error = permission_or_io(std::io::Error::from(std::io::ErrorKind::PermissionDenied));
        assert!(matches!(
            error,
            ImportError::PermissionDenied(Permission::Files)
        ));
    }

    #[test]
    fn empty_root_window_and_cursor_are_stable() {
        let root = tempfile::Builder::new()
            .prefix("fndr_fixture")
            .tempdir()
            .unwrap();
        assert!(source(root.path())
            .scan(None, 30, 10)
            .unwrap()
            .items
            .is_empty());
        fs::write(root.path().join("one.txt"), "one").unwrap();
        fs::write(root.path().join("two.txt"), "two").unwrap();
        let source = source(root.path());
        let empty = source.scan(None, 30, 0).unwrap();
        assert!(empty.done && empty.items.is_empty());
        let first = source.scan(None, 30, 1).unwrap();
        assert!(!first.done);
        let second = source.scan(first.next, 30, 1).unwrap();
        assert!(second.done);
        assert_ne!(first.items[0].external_id, second.items[0].external_id);
        let outside_window = RecentFilesSource::new(
            vec![root.path().to_path_buf()],
            chrono::Utc::now().timestamp_millis() + 40 * 86_400_000,
        );
        assert!(outside_window.scan(None, 30, 10).unwrap().items.is_empty());
    }

    #[test]
    fn cursor_resume_includes_a_newer_file_change() {
        use std::time::{Duration, SystemTime};

        let root = tempfile::Builder::new()
            .prefix("fndr_fixture")
            .tempdir()
            .unwrap();
        let now = 1_760_000_000_000i64;
        let base = UNIX_EPOCH + Duration::from_millis(now as u64);
        for id in 1..=3u64 {
            let path = root.path().join(format!("{id}.txt"));
            let file = File::create(path).unwrap();
            file.set_modified(base - Duration::from_secs(id * 60))
                .unwrap();
        }
        let source = RecentFilesSource::new(vec![root.path().to_path_buf()], now);
        let first = source.scan(None, 30, 1).unwrap();
        let second = source.scan(first.next, 30, 1).unwrap();
        let saved_cursor = second.next;
        let file = File::create(root.path().join("new.txt")).unwrap();
        file.set_modified(SystemTime::UNIX_EPOCH + Duration::from_millis(now as u64))
            .unwrap();
        let resumed = source.scan(saved_cursor, 30, 10).unwrap();
        assert!(resumed.items.iter().any(|item| item.title == "new.txt"));
    }

    #[test]
    fn live_source_uses_scan_time_for_new_file_changes() {
        let root = tempfile::Builder::new()
            .prefix("fndr_fixture")
            .tempdir()
            .unwrap();
        let source = RecentFilesSource::live(vec![root.path().to_path_buf()]);
        assert!(source.scan(None, 30, 10).unwrap().items.is_empty());
        std::thread::sleep(std::time::Duration::from_millis(50));
        fs::write(root.path().join("later.txt"), "later").unwrap();
        assert_eq!(source.scan(None, 30, 10).unwrap().items.len(), 1);
    }
}
