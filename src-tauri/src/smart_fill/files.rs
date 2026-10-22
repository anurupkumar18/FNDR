//! Suggesting a file for a form's upload box.
//!
//! A form asks for a resume, a cover letter, a transcript, a photo, a permit.
//! Smart Fill cannot attach a file for the person (a page's file chooser is a
//! system dialog), but it can say which file is probably the one. There is no
//! list of document types here: the model is shown what each upload box asks
//! for and the names of the person's recent files, and picks. File names only;
//! nothing is opened or read.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, UNIX_EPOCH};

/// A file the model may pick: its name, folder and when it last changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub path: PathBuf,
    pub modified_ms: i64,
}

const EXTENSIONS: [&str; 14] = [
    "pdf", "docx", "doc", "pages", "rtf", "txt", "md", "png", "jpg", "jpeg", "heic", "xlsx",
    "pptx", "zip",
];
const MAX_VISITED: usize = 30_000;
const MAX_DEPTH: usize = 3;
/// How many names the model is shown.
pub const MAX_CANDIDATES: usize = 60;

fn skip_directory(name: &str) -> bool {
    name.starts_with('.')
        || matches!(
            name,
            "node_modules" | "Library" | "target" | "venv" | "__pycache__" | "Applications"
        )
}

fn has_known_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .is_some_and(|ext| EXTENSIONS.contains(&ext.as_str()))
}

/// Every document-like file under `dirs`, a few levels deep, bounded in count
/// and time. Hidden files and folders, `Library`, build folders and apps are
/// skipped.
pub fn walk(dirs: &[PathBuf], budget: Duration) -> Vec<Candidate> {
    let started = Instant::now();
    let mut found = Vec::new();
    let mut visited = 0_usize;
    let mut stack: Vec<(PathBuf, usize)> = dirs.iter().map(|dir| (dir.clone(), 0)).collect();
    while let Some((dir, depth)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            visited += 1;
            if visited > MAX_VISITED || started.elapsed() > budget {
                return found;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                if depth < MAX_DEPTH && !skip_directory(&name) {
                    stack.push((entry.path(), depth + 1));
                }
            } else if kind.is_file() && !name.starts_with('.') && has_known_extension(&entry.path())
            {
                let modified_ms = entry
                    .metadata()
                    .ok()
                    .and_then(|meta| meta.modified().ok())
                    .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                    .map_or(0, |d| d.as_millis() as i64);
                found.push(Candidate {
                    path: entry.path(),
                    modified_ms,
                });
            }
        }
    }
    found
}

/// The person's most recently changed document-like files in Documents,
/// Desktop and Downloads, newest first.
pub fn candidates(home: &Path, limit: usize) -> Vec<Candidate> {
    let dirs: Vec<PathBuf> = ["Documents", "Desktop", "Downloads"]
        .iter()
        .map(|name| home.join(name))
        .filter(|dir| dir.is_dir())
        .collect();
    let mut files = walk(&dirs, Duration::from_millis(500));
    files.sort_by_key(|file| std::cmp::Reverse(file.modified_ms));
    files.truncate(limit);
    files
}

/// "Sam_Example_Resume.pdf (Documents/jobs)": what the model reads for a file.
pub fn label(home: &Path, candidate: &Candidate) -> String {
    let name = candidate
        .path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file");
    let folder = candidate
        .path
        .parent()
        .and_then(|parent| parent.strip_prefix(home).ok())
        .and_then(|relative| relative.to_str())
        .unwrap_or("");
    if folder.is_empty() {
        name.to_string()
    } else {
        format!("{name} ({folder})")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_walk_lists_document_like_files_newest_first_and_skips_hidden_and_library() {
        let home = std::env::temp_dir().join(format!("fndr-suggest-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        for dir in [
            "Documents/jobs",
            "Downloads",
            "Library/Caches",
            "Documents/.hidden",
            "Documents/node_modules",
        ] {
            std::fs::create_dir_all(home.join(dir)).unwrap();
        }
        let files = [
            "Documents/jobs/Sam_Example_Resume.pdf",
            "Downloads/cover letter draft.docx",
            "Downloads/binary.exe",
            "Library/Caches/resume.pdf",
            "Documents/.hidden/resume.pdf",
            "Documents/node_modules/readme.md",
            "Documents/.secret.pdf",
        ];
        for (n, file) in files.iter().enumerate() {
            let path = home.join(file);
            std::fs::write(&path, b"x").unwrap();
            let time = std::time::SystemTime::UNIX_EPOCH + Duration::from_secs(1_000 + n as u64);
            let handle = std::fs::File::options().write(true).open(&path).unwrap();
            handle.set_modified(time).unwrap();
        }
        let found = candidates(&home, 10);
        let names: Vec<String> = found
            .iter()
            .map(|c| c.path.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert_eq!(
            names,
            vec!["cover letter draft.docx", "Sam_Example_Resume.pdf"],
            "newest first; exe, hidden, Library and node_modules left out"
        );
        assert_eq!(
            label(&home, &found[1]),
            "Sam_Example_Resume.pdf (Documents/jobs)"
        );
        assert_eq!(candidates(&home, 1).len(), 1);
        let _ = std::fs::remove_dir_all(&home);
    }
}
