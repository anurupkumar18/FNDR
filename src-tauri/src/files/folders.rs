//! The folders the person chooses, as Settings and onboarding show them
//! (DigUp's `FolderChoices` and `CodeFolders`): whether a folder can be read
//! now, its folders one level down to tick off, and where code lives.

use serde::Serialize;
use std::path::{Path, PathBuf};

/// Whether a chosen folder can be read now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Reach {
    Ok,
    /// On a drive that is not plugged in.
    NotConnected,
    /// Gone (moved, renamed or deleted).
    Missing,
    /// macOS keeps FNDR out until the person allows it in System Settings.
    NoAccess,
}

pub fn reach(folder: &Path) -> Reach {
    if !folder.exists() {
        return if folder.starts_with("/Volumes") {
            Reach::NotConnected
        } else {
            Reach::Missing
        };
    }
    match std::fs::read_dir(folder) {
        Ok(_) => Reach::Ok,
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => Reach::NoAccess,
        Err(_) => Reach::Missing,
    }
}

/// The folders one level down that a search of `folder` would walk, by name.
pub fn subfolders(folder: &Path, home: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut found: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir() && !path.is_symlink())
        .filter(|path| {
            !path
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with('.'))
        })
        .filter(|path| !super::rules::is_package(path))
        .filter(|path| super::rules::skip_reason_for_folder(path, home, &[]).is_none())
        .collect();
    found.sort_by_key(|path| {
        path.file_name()
            .map(|name| name.to_string_lossy().to_lowercase())
    });
    found
}

/// The home folder's folders that hold a repo, themselves or one level down
/// (~/Developer, ~/Projects...), and the usual names for one inside Documents
/// or Desktop when those are searched already (macOS would ask first).
pub fn code_candidates(home: &Path, searched: &[PathBuf]) -> Vec<PathBuf> {
    const ASKED: [&str; 10] = [
        "Desktop",
        "Documents",
        "Downloads",
        "Library",
        "Pictures",
        "Movies",
        "Music",
        "Public",
        "Applications",
        "Sites",
    ];
    const INSIDE: [&str; 8] = [
        "GitHub",
        "Code",
        "Projects",
        "Developer",
        "dev",
        "src",
        "repos",
        "Repositories",
    ];
    let mut found = Vec::new();
    if let Ok(entries) = std::fs::read_dir(home) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            let path = entry.path();
            if !name.starts_with('.')
                && !ASKED.contains(&name.as_str())
                && path.is_dir()
                && holds_code(&path)
            {
                found.push(path);
            }
        }
    }
    for parent in ["Documents", "Desktop"] {
        let folder = home.join(parent);
        if !searched.iter().any(|root| folder.starts_with(root)) {
            continue;
        }
        for name in INSIDE {
            let inner = folder.join(name);
            if inner.is_dir() && holds_code(&inner) {
                found.push(inner);
            }
        }
    }
    found.sort_by_key(|path| {
        path.file_name()
            .map(|name| name.to_string_lossy().to_lowercase())
    });
    found
}

/// A repo, or a folder with one in it (one level down).
pub fn holds_code(folder: &Path) -> bool {
    if folder.join(".git").exists() {
        return true;
    }
    std::fs::read_dir(folder).is_ok_and(|entries| {
        entries.flatten().take(500).any(|entry| {
            !entry.file_name().to_string_lossy().starts_with('.')
                && entry.path().join(".git").exists()
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_folder_says_whether_its_drive_is_gone() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(reach(dir.path()), Reach::Ok);
        assert_eq!(reach(&dir.path().join("gone")), Reach::Missing);
        assert_eq!(
            reach(Path::new("/Volumes/Backup-not-here/Photos")),
            Reach::NotConnected
        );
    }

    #[test]
    fn subfolders_are_the_ones_a_search_walks() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        let docs = home.join("Documents");
        for name in [
            "Taxes",
            "archive",
            ".hidden",
            "node_modules",
            "Trip.photoslibrary",
        ] {
            std::fs::create_dir_all(docs.join(name)).unwrap();
        }
        std::fs::write(docs.join("note.md"), "a file").unwrap();
        let names: Vec<String> = subfolders(&docs, home)
            .iter()
            .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["archive", "Taxes"]);
    }

    #[test]
    fn code_is_offered_where_repos_are() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        std::fs::create_dir_all(home.join("Developer/app/.git")).unwrap();
        std::fs::create_dir_all(home.join("notes")).unwrap();
        std::fs::create_dir_all(home.join("Documents/GitHub/site/.git")).unwrap();
        std::fs::create_dir_all(home.join("Downloads/repo/.git")).unwrap();
        let names = |found: Vec<PathBuf>| -> Vec<String> {
            found
                .iter()
                .map(|path| {
                    path.strip_prefix(home)
                        .unwrap()
                        .to_string_lossy()
                        .into_owned()
                })
                .collect()
        };
        assert_eq!(names(code_candidates(home, &[])), ["Developer"]);
        assert_eq!(
            names(code_candidates(home, &[home.join("Documents")])),
            ["Developer", "Documents/GitHub"]
        );
    }
}
