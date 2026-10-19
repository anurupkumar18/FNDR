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

/// Walks the code folders for the code index: code files only
/// ([`rules::classify_code`]), in a git repo only those git keeps, none in
/// vendored or built folders, none too big to be written by hand.
pub fn crawl_code(roots: &[PathBuf], options: &IndexOptions, home: &Path) -> CrawlReport {
    let mut report = CrawlReport::default();
    let mut found_paths: HashSet<String> = HashSet::new();
    let mut repos = GitRepos::default();
    for root in roots {
        repos.enter_root(root);
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
                if entry.file_name().to_string_lossy().starts_with('.') {
                    continue;
                }
                let Ok(metadata) = std::fs::symlink_metadata(&path) else {
                    continue;
                };
                if metadata.is_dir() {
                    let reason = rules::skip_reason_for_folder(&path, home, &skips)
                        .or_else(|| rules::code_skip_reason_for_folder(&path))
                        .or_else(|| rules::is_package(&path).then_some("app or bundle"));
                    match reason {
                        Some(reason) => *report.skipped_folders.entry(reason).or_insert(0) += 1,
                        None => {
                            repos.enter(&path);
                            stack.push(path);
                        }
                    }
                    continue;
                }
                match rules::classify_code(&path, options) {
                    Classification::Ignore => *report.skipped.entry("other type").or_insert(0) += 1,
                    Classification::Skip(reason) => *report.skipped.entry(reason).or_insert(0) += 1,
                    Classification::Index(kind) => {
                        if !metadata.is_file() || is_dataless(&metadata) {
                            *report
                                .skipped
                                .entry("link, special or cloud-only file")
                                .or_insert(0) += 1;
                            continue;
                        }
                        let notebook = path
                            .extension()
                            .is_some_and(|ext| ext.eq_ignore_ascii_case("ipynb"));
                        let limit = if notebook {
                            super::code::MAX_NOTEBOOK_BYTES
                        } else {
                            super::code::MAX_CODE_BYTES
                        };
                        if metadata.len() > limit {
                            *report
                                .skipped
                                .entry("too big to be written by hand")
                                .or_insert(0) += 1;
                            continue;
                        }
                        if repos.ignores(&path) {
                            *report.skipped.entry("ignored by git").or_insert(0) += 1;
                            continue;
                        }
                        let text = path.to_string_lossy().to_string();
                        if found_paths.insert(text.clone()) {
                            report.candidates.push(facts(text, kind, &metadata));
                        }
                    }
                }
            }
        }
    }
    report
}

/// The git repos a code crawl walks through, and the files each keeps: a file
/// git ignores (a build, an environment, data, a secret in .gitignore) is not
/// code search's to read. A folder in no repo, or a Mac without git, has no
/// list: everything there counts.
#[derive(Default)]
struct GitRepos {
    /// Repo folder, and the files git keeps in it (relative paths), or `None` when git could not say.
    files: HashMap<PathBuf, Option<HashSet<String>>>,
}

impl GitRepos {
    fn enter_root(&mut self, root: &Path) {
        if let Some(repo) = root.ancestors().find(|folder| folder.join(".git").exists()) {
            self.files
                .entry(repo.to_path_buf())
                .or_insert_with(|| git_files(repo));
        }
    }

    fn enter(&mut self, folder: &Path) {
        if folder.join(".git").is_dir() {
            self.files
                .entry(folder.to_path_buf())
                .or_insert_with(|| git_files(folder));
        }
    }

    fn ignores(&self, path: &Path) -> bool {
        let Some((repo, kept)) = path
            .ancestors()
            .skip(1)
            .find_map(|folder| self.files.get(folder).map(|kept| (folder, kept)))
        else {
            return false;
        };
        match kept {
            Some(kept) => !kept.contains(
                &path
                    .strip_prefix(repo)
                    .unwrap_or(path)
                    .to_string_lossy()
                    .to_string(),
            ),
            None => false,
        }
    }
}

/// The files git keeps in a repo: tracked ones, and new ones its .gitignore
/// does not leave out. A copied repo's own config could name a program
/// (core.fsmonitor, hooks): those are overridden and the system's and the
/// person's git config are left out, so nothing git reads here runs anything.
/// Read-only: never waits for or takes the repo's lock.
fn git_files(repo: &Path) -> Option<HashSet<String>> {
    let git = [
        "/usr/bin/git",
        "/opt/homebrew/bin/git",
        "/usr/local/bin/git",
    ]
    .into_iter()
    .map(Path::new)
    .find(|path| path.is_file())?;
    // /usr/bin/git without the developer tools only asks to install them.
    if git == Path::new("/usr/bin/git")
        && cfg!(target_os = "macos")
        && !Path::new("/Library/Developer/CommandLineTools").exists()
        && !Path::new("/Applications/Xcode.app").exists()
    {
        return None;
    }
    let output = std::process::Command::new(git)
        .args([
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.hooksPath=/dev/null",
            "-C",
        ])
        .arg(repo)
        .args([
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
        ])
        .env_clear()
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", std::env::var_os("HOME").unwrap_or_default())
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(
        output
            .stdout
            .split(|byte| *byte == 0)
            .filter(|part| !part.is_empty())
            .map(|part| String::from_utf8_lossy(part).to_string())
            .collect(),
    )
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
    fn code_search_reads_what_git_keeps() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("billing");
        write(&repo, "src/retry.py");
        write(&repo, "README.md");
        write(&repo, "build/out.py");
        write(&repo, "vendor/lib.py");
        write(&repo, "secrets.env.py");
        write(&repo, "data.json");
        write(&repo, "notes/scratch.py");
        std::fs::write(repo.join(".gitignore"), "notes/\n").unwrap();
        let git = std::process::Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["init", "-q"])
            .status();
        let have_git = git.is_ok_and(|status| status.success());
        let report = crawl_code(
            std::slice::from_ref(&repo),
            &IndexOptions::default(),
            Path::new("/nonexistent-home"),
        );
        let mut names: Vec<String> = report
            .candidates
            .iter()
            .map(|file| {
                Path::new(&file.path)
                    .strip_prefix(&repo)
                    .unwrap()
                    .to_string_lossy()
                    .to_string()
            })
            .collect();
        names.sort();
        let mut expected = vec!["README.md", "src/retry.py"];
        if !have_git {
            expected.push("notes/scratch.py");
            expected.sort();
        }
        assert_eq!(names, expected);
        assert_eq!(report.skipped.get("data file"), Some(&1));
        assert_eq!(report.skipped.get("key or password file"), Some(&1));
        assert_eq!(
            report.skipped_folders.get("vendored or built code"),
            Some(&2)
        );
        if have_git {
            assert_eq!(report.skipped.get("ignored by git"), Some(&1));
        }
        assert!(report
            .candidates
            .iter()
            .all(|file| file.kind == FileKind::Code));
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
