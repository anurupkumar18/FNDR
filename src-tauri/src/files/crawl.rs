//! Walks the chosen folders and applies [`rules`]. Reads metadata only, never
//! file contents (except the 600-byte sniff that tells a real MPEG-TS video
//! from a TypeScript file). Ported from DigUp's `Crawler`; the code index's
//! git-aware walk comes with code search.

use super::rules::{self, Classification, DatasetEntry};
use super::store::FileFacts;
use super::{FileKind, IndexOptions};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Default)]
pub struct CrawlReport {
    pub candidates: Vec<FileFacts>,
    /// Reason, and how many files it left out.
    pub skipped: BTreeMap<&'static str, usize>,
    /// Reason, and how many folders the walk did not go into.
    pub skipped_folders: BTreeMap<&'static str, usize>,
    /// Folder, and how many files were left out as a dataset.
    pub datasets: HashMap<String, usize>,
}

pub fn crawl(roots: &[PathBuf], options: &IndexOptions, home: &Path) -> CrawlReport {
    let mut report = CrawlReport::default();
    let mut project_roots: HashSet<PathBuf> = HashSet::new();
    let mut found: Vec<FileFacts> = Vec::new();
    let mut found_paths: HashSet<String> = HashSet::new();
    let root_paths: HashSet<String> = roots
        .iter()
        .map(|root| root.to_string_lossy().to_string())
        .collect();

    for root in roots {
        if rules::is_project_root(root) {
            project_roots.insert(root.clone());
        }
        // An excluded folder applies inside the chosen folders that contain it.
        let skips: Vec<String> = options
            .excluded_folders
            .iter()
            .filter(|folder| Path::new(folder).starts_with(root))
            .cloned()
            .collect();
        let mut stack = vec![root.clone()];
        while let Some(folder) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&folder) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                let name = entry.file_name();
                if name.to_string_lossy().starts_with('.') {
                    continue; // hidden, as the walk on macOS skips them
                }
                let Ok(metadata) = std::fs::symlink_metadata(&path) else {
                    continue;
                };
                if metadata.is_dir() {
                    if rules::is_package(&path) {
                        *report.skipped.entry("app or bundle").or_insert(0) += 1;
                    } else if let Some(reason) = rules::skip_reason_for_folder(&path, home, &skips)
                    {
                        *report.skipped_folders.entry(reason).or_insert(0) += 1;
                    } else {
                        if rules::is_project_root(&path) {
                            project_roots.insert(path.clone());
                        }
                        stack.push(path);
                    }
                    continue;
                }
                match rules::classify(&path, options) {
                    Classification::Ignore => *report.skipped.entry("other type").or_insert(0) += 1,
                    Classification::Skip(reason) => *report.skipped.entry(reason).or_insert(0) += 1,
                    Classification::Index(kind) => {
                        if !metadata.is_file() {
                            *report.skipped.entry("link or special file").or_insert(0) += 1;
                            continue;
                        }
                        // Reading a placeholder ("Optimize Mac Storage", File Providers) would download it.
                        if is_dataless(&metadata) {
                            *report
                                .skipped
                                .entry("in the cloud, not downloaded")
                                .or_insert(0) += 1;
                            continue;
                        }
                        let text = path.to_string_lossy().to_string();
                        if found_paths.insert(text.clone()) {
                            found.push(facts(text, kind, &metadata));
                        }
                    }
                }
            }
        }
    }

    // Inside a code project only screenshots count (assets, fixtures and datasets do not).
    let in_project = |path: &str| {
        Path::new(path)
            .ancestors()
            .skip(1)
            .any(|folder| project_roots.contains(folder))
    };
    let mut kept = Vec::with_capacity(found.len());
    for file in found {
        if file.kind != FileKind::Screenshot && in_project(&file.path) {
            *report.skipped.entry("inside a code project").or_insert(0) += 1;
        } else {
            kept.push(file);
        }
    }

    if !options.include_datasets {
        let split: Vec<(String, String)> = kept
            .iter()
            .map(|file| {
                let path = Path::new(&file.path);
                let folder = path
                    .parent()
                    .map(|parent| parent.to_string_lossy().to_string())
                    .unwrap_or_default();
                let stem = path
                    .file_stem()
                    .map(|stem| stem.to_string_lossy().to_string())
                    .unwrap_or_default();
                (folder, stem)
            })
            .collect();
        let entries: Vec<DatasetEntry<'_>> = kept
            .iter()
            .zip(&split)
            .map(|(file, (folder, stem))| DatasetEntry {
                folder,
                stem,
                kind: file.kind,
            })
            .collect();
        let datasets = rules::dataset_folders(&entries, &root_paths, 300);
        if !datasets.is_empty() {
            let before = kept.len();
            let folders: Vec<String> = split.into_iter().map(|(folder, _)| folder).collect();
            let mut index = 0;
            kept.retain(|file| {
                let drop = file.kind.is_media() && datasets.contains_key(&folders[index]);
                index += 1;
                !drop
            });
            *report.skipped.entry("dataset").or_insert(0) += before - kept.len();
            report.datasets = datasets;
        }
    }
    report.candidates = kept;
    report
}

fn facts(path: String, kind: FileKind, metadata: &std::fs::Metadata) -> FileFacts {
    let mtime = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0.0, |duration| duration.as_secs_f64());
    #[cfg(unix)]
    let (inode, device) = {
        use std::os::unix::fs::MetadataExt;
        (metadata.ino(), metadata.dev())
    };
    #[cfg(not(unix))]
    let (inode, device) = (0, 0);
    FileFacts {
        path,
        kind,
        size: metadata.len() as i64,
        mtime,
        inode,
        device,
    }
}

#[cfg(target_os = "macos")]
fn is_dataless(metadata: &std::fs::Metadata) -> bool {
    use std::os::darwin::fs::MetadataExt;
    metadata.st_flags() & 0x4000_0000 != 0 // SF_DATALESS
}

#[cfg(not(target_os = "macos"))]
fn is_dataless(_metadata: &std::fs::Metadata) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(root: &Path, relative: &str) {
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"x").unwrap();
    }

    #[test]
    fn the_walk_keeps_what_the_rules_allow() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("Documents");
        write(&root, "lease.pdf");
        write(&root, "notes/meeting.md");
        write(&root, "Screenshot 1.png");
        write(&root, ".ssh/id_rsa");
        write(&root, "id_rsa");
        write(&root, "node_modules/x/readme.md");
        write(&root, "Thing.app/Contents/info.txt");
        write(&root, "app/package.json");
        write(&root, "app/assets/logo.png");
        write(&root, "app/Screenshot 2.png");
        write(&root, "data.csv");

        let report = crawl(
            std::slice::from_ref(&root),
            &IndexOptions::default(),
            dir.path(),
        );
        let mut names: Vec<String> = report
            .candidates
            .iter()
            .map(|file| {
                Path::new(&file.path)
                    .strip_prefix(&root)
                    .unwrap()
                    .to_string_lossy()
                    .to_string()
            })
            .collect();
        names.sort();
        assert_eq!(
            names,
            [
                "Screenshot 1.png",
                "app/Screenshot 2.png",
                "lease.pdf",
                "notes/meeting.md"
            ]
        );
        assert_eq!(report.skipped.get("key or password file"), Some(&1));
        assert_eq!(report.skipped.get("inside a code project"), Some(&1));
        assert_eq!(report.skipped.get("app or bundle"), Some(&1));
        assert_eq!(report.skipped_folders.get("developer folder"), Some(&1));
    }

    #[test]
    fn excluded_folders_are_not_walked() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        write(&root, "Private/taxes.pdf");
        write(&root, "Public/menu.pdf");
        let options = IndexOptions {
            excluded_folders: vec![root.join("Private").to_string_lossy().to_string()],
            ..IndexOptions::default()
        };
        let report = crawl(
            std::slice::from_ref(&root),
            &options,
            Path::new("/nonexistent-home"),
        );
        assert_eq!(report.candidates.len(), 1);
        assert!(report.candidates[0].path.ends_with("menu.pdf"));
    }

    #[test]
    fn a_dataset_is_left_out_and_reported() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        for n in 0..300 {
            write(&root, &format!("train/clip_{n:06}.jpg"));
        }
        write(&root, "beach.jpg");
        let report = crawl(
            std::slice::from_ref(&root),
            &IndexOptions::default(),
            Path::new("/nonexistent-home"),
        );
        assert_eq!(report.candidates.len(), 1);
        assert_eq!(report.skipped.get("dataset"), Some(&300));
        assert_eq!(report.datasets.values().sum::<usize>(), 300);
    }
}
