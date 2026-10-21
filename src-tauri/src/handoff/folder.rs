//! A hand-off's job folder: `inputs/` (copies of the files the person
//! attached), `work/` (the worker's scratch) and `out/` (deliverables).
//! The worker may write only here (`profile`); an original file is copied in,
//! never opened for writing.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use chrono::NaiveDate;

/// File types a person may attach (PRD FR4).
pub const INPUT_TYPES: &[&str] = &[
    "csv", "tsv", "xlsx", "docx", "pptx", "pdf", "md", "txt", "json",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Caps {
    pub file_bytes: u64,
    pub total_bytes: u64,
    pub files: usize,
}

impl Default for Caps {
    fn default() -> Self {
        Self {
            file_bytes: 25 * 1024 * 1024,
            total_bytes: 60 * 1024 * 1024,
            files: 5,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobFolder {
    pub root: PathBuf,
    pub inputs: PathBuf,
    pub work: PathBuf,
    pub out: PathBuf,
}

/// Why an attached file was not copied in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputRefusal {
    NotAFile,
    Link,
    Type(String),
    TooLarge { bytes: u64, cap: u64 },
    TotalTooLarge { bytes: u64, cap: u64 },
    TooMany { cap: usize },
    ChangedWhileCopying,
    Io(String),
}

impl std::fmt::Display for InputRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        const MB: u64 = 1024 * 1024;
        match self {
            Self::NotAFile => write!(f, "it is not a file"),
            Self::Link => write!(f, "it is a link to another place"),
            Self::Type(ext) if ext.is_empty() => write!(f, "files without a type are not attached"),
            Self::Type(ext) => write!(f, ".{ext} files are not attached"),
            Self::TooLarge { bytes, cap } => {
                write!(f, "it is {} MB, over the {} MB limit", bytes / MB, cap / MB)
            }
            Self::TotalTooLarge { cap, .. } => {
                write!(f, "the files together would pass {} MB", cap / MB)
            }
            Self::TooMany { cap } => write!(f, "at most {cap} files are attached"),
            Self::ChangedWhileCopying => write!(f, "it changed while FNDR copied it"),
            Self::Io(error) => write!(f, "it could not be read ({error})"),
        }
    }
}

/// What a job folder holds now, for the limits in PRD FR8.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Usage {
    pub bytes: u64,
    pub out_files: usize,
}

/// A new folder `<base>/<yyyy-mm-dd> <title>`, with " 2", " 3" when taken.
pub fn create(base: &Path, title: &str, today: NaiveDate) -> io::Result<JobFolder> {
    fs::create_dir_all(base)?;
    let stem = format!("{} {}", today.format("%Y-%m-%d"), folder_title(title));
    let mut n = 1;
    let root = loop {
        let name = if n == 1 {
            stem.clone()
        } else {
            format!("{stem} {n}")
        };
        let root = base.join(name);
        // `create_dir` claims the name or fails, so two jobs never share one.
        match fs::create_dir(&root) {
            Ok(()) => break root,
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => n += 1,
            Err(e) => return Err(e),
        }
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700))?;
    }
    let folder = JobFolder {
        inputs: root.join("inputs"),
        work: root.join("work"),
        out: root.join("out"),
        root,
    };
    for part in [&folder.inputs, &folder.work, &folder.out] {
        fs::create_dir(part)?;
    }
    Ok(folder)
}

/// The folder name's title part: at most 60 characters, nothing a path or
/// Finder treats specially.
pub fn folder_title(title: &str) -> String {
    let spaced: String = title
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '/' | ':' | '\\') {
                ' '
            } else {
                c
            }
        })
        .collect();
    let words = spaced.split_whitespace().collect::<Vec<_>>().join(" ");
    let title: String = words.trim_start_matches('.').trim().chars().collect();
    // At most 60 characters, ending on a whole word when one fits.
    let title = if title.chars().count() <= 60 {
        title
    } else {
        let cut: String = title.chars().take(60).collect();
        match cut.rfind(' ') {
            Some(space) if space >= 20 => cut[..space].to_string(),
            _ => cut,
        }
    };
    let title = title.trim_end().to_string();
    if title.is_empty() {
        "Hand-off".to_string()
    } else {
        title
    }
}

/// Copy one attached file into `inputs/` under its own name.
pub fn copy_input(folder: &JobFolder, source: &Path, caps: &Caps) -> Result<PathBuf, InputRefusal> {
    let io_err = |e: io::Error| InputRefusal::Io(e.to_string());
    let meta = fs::symlink_metadata(source).map_err(io_err)?;
    if meta.file_type().is_symlink() {
        return Err(InputRefusal::Link);
    }
    if !meta.is_file() {
        return Err(InputRefusal::NotAFile);
    }
    let ext = source
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !INPUT_TYPES.contains(&ext.as_str()) {
        return Err(InputRefusal::Type(ext));
    }
    let bytes = meta.len();
    if bytes > caps.file_bytes {
        return Err(InputRefusal::TooLarge {
            bytes,
            cap: caps.file_bytes,
        });
    }
    let (held_files, held_bytes) = files_and_bytes(&folder.inputs).map_err(io_err)?;
    if held_files >= caps.files {
        return Err(InputRefusal::TooMany { cap: caps.files });
    }
    if held_bytes + bytes > caps.total_bytes {
        return Err(InputRefusal::TotalTooLarge {
            bytes: held_bytes + bytes,
            cap: caps.total_bytes,
        });
    }

    let (dest, mut out) = new_file_for(&folder.inputs, source).map_err(io_err)?;
    let copied = fs::File::open(source)
        .and_then(|mut input| io::copy(&mut input, &mut out))
        .map_err(io_err);
    let after = fs::metadata(source).map(|m| m.len()).ok();
    match copied {
        Ok(n) if n == bytes && after == Some(bytes) => Ok(dest),
        Ok(_) => {
            let _ = fs::remove_file(&dest);
            Err(InputRefusal::ChangedWhileCopying)
        }
        Err(e) => {
            let _ = fs::remove_file(&dest);
            Err(e)
        }
    }
}

/// The names `copy_input` gives these files, in order, in an empty
/// `inputs/`: the file's own name, then "notes 2.md" for a second "notes.md".
/// The draft shows these names, so they must match what the job holds.
pub fn planned_input_names(sources: &[PathBuf]) -> Vec<String> {
    let mut taken: Vec<String> = Vec::new();
    for source in sources {
        let name = (1..)
            .map(|n| numbered_name(source, n))
            .find(|name| !taken.contains(name))
            .unwrap_or_default();
        taken.push(name);
    }
    taken
}

fn numbered_name(source: &Path, n: usize) -> String {
    let stem = source
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("input");
    match (n, source.extension().and_then(|e| e.to_str())) {
        (1, Some(ext)) => format!("{stem}.{ext}"),
        (1, None) => stem.to_string(),
        (n, Some(ext)) => format!("{stem} {n}.{ext}"),
        (n, None) => format!("{stem} {n}"),
    }
}

/// A file under `dir` named like `source`, created here and now, never an
/// existing one: "notes.md", then "notes 2.md".
fn new_file_for(dir: &Path, source: &Path) -> io::Result<(PathBuf, fs::File)> {
    for n in 1.. {
        let name = numbered_name(source, n);
        let path = dir.join(name);
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(file) => return Ok((path, file)),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    unreachable!("an unbounded range always yields")
}

pub fn usage(folder: &JobFolder) -> io::Result<Usage> {
    let (_, bytes) = files_and_bytes(&folder.root)?;
    let (out_files, _) = files_and_bytes(&folder.out)?;
    Ok(Usage { bytes, out_files })
}

/// Regular files under `dir`, recursively, without following links.
fn files_and_bytes(dir: &Path) -> io::Result<(usize, u64)> {
    let mut files = 0;
    let mut bytes = 0;
    let mut pending = vec![dir.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let meta = fs::symlink_metadata(entry.path())?;
            if meta.is_dir() {
                pending.push(entry.path());
            } else if meta.is_file() {
                files += 1;
                bytes += meta.len();
            }
        }
    }
    Ok((files, bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 16).unwrap()
    }

    fn write(path: &Path, bytes: usize) {
        fs::write(path, vec![b'a'; bytes]).unwrap();
    }

    #[test]
    fn a_folder_is_named_by_day_and_title_with_its_three_parts() {
        let base = tempfile::tempdir().unwrap();
        let job = create(base.path(), "Q4 onboarding readout", day()).unwrap();
        assert_eq!(
            job.root,
            base.path().join("2026-10-16 Q4 onboarding readout")
        );
        for part in [&job.inputs, &job.work, &job.out] {
            assert!(part.is_dir());
            assert_eq!(part.parent(), Some(job.root.as_path()));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&job.root).unwrap().permissions().mode() & 0o777,
                0o700
            );
        }
    }

    #[test]
    fn a_taken_name_gets_a_number() {
        let base = tempfile::tempdir().unwrap();
        create(base.path(), "Readout", day()).unwrap();
        let second = create(base.path(), "Readout", day()).unwrap();
        let third = create(base.path(), "Readout", day()).unwrap();
        assert_eq!(second.root.file_name().unwrap(), "2026-10-16 Readout 2");
        assert_eq!(third.root.file_name().unwrap(), "2026-10-16 Readout 3");
    }

    #[test]
    fn titles_lose_separators_and_leading_dots_and_stay_short() {
        assert_eq!(folder_title("Q4/Q3: readout"), "Q4 Q3 readout");
        assert_eq!(folder_title("..hidden"), "hidden");
        assert_eq!(folder_title("   "), "Hand-off");
        assert_eq!(folder_title("line\nbreak"), "line break");
        assert_eq!(folder_title(&"é".repeat(80)).chars().count(), 60);
        // The live run's request was cut mid-word ("...with the top thr").
        assert_eq!(
            folder_title(
                "Own a one-page Q4 onboarding survey readout with the top three churn drivers by Thursday"
            ),
            "Own a one-page Q4 onboarding survey readout with the top"
        );
    }

    #[test]
    fn an_allowed_file_is_copied_under_its_own_name() {
        let base = tempfile::tempdir().unwrap();
        let job = create(base.path(), "t", day()).unwrap();
        let src = base.path().join("survey.csv");
        write(&src, 100);
        let copied = copy_input(&job, &src, &Caps::default()).unwrap();
        assert_eq!(copied, job.inputs.join("survey.csv"));
        assert_eq!(fs::read(&copied).unwrap().len(), 100);
        assert!(src.exists(), "the original stays where it was");
    }

    #[test]
    fn a_second_file_with_the_same_name_is_not_overwritten() {
        let base = tempfile::tempdir().unwrap();
        let job = create(base.path(), "t", day()).unwrap();
        let a = base.path().join("a");
        let b = base.path().join("b");
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&b).unwrap();
        write(&a.join("notes.md"), 3);
        write(&b.join("notes.md"), 5);
        assert_eq!(
            copy_input(&job, &a.join("notes.md"), &Caps::default()).unwrap(),
            job.inputs.join("notes.md")
        );
        assert_eq!(
            copy_input(&job, &b.join("notes.md"), &Caps::default()).unwrap(),
            job.inputs.join("notes 2.md")
        );
    }

    #[test]
    fn refusals_name_their_reason() {
        let base = tempfile::tempdir().unwrap();
        let job = create(base.path(), "t", day()).unwrap();
        let caps = Caps {
            file_bytes: 10,
            total_bytes: 15,
            files: 2,
        };

        let script = base.path().join("run.sh");
        write(&script, 1);
        assert_eq!(
            copy_input(&job, &script, &caps),
            Err(InputRefusal::Type("sh".into()))
        );

        let dir = base.path().join("folder.csv");
        fs::create_dir_all(&dir).unwrap();
        assert_eq!(copy_input(&job, &dir, &caps), Err(InputRefusal::NotAFile));

        let big = base.path().join("big.csv");
        write(&big, 11);
        assert_eq!(
            copy_input(&job, &big, &caps),
            Err(InputRefusal::TooLarge { bytes: 11, cap: 10 })
        );

        #[cfg(unix)]
        {
            let target = base.path().join("real.csv");
            write(&target, 1);
            let link = base.path().join("link.csv");
            std::os::unix::fs::symlink(&target, &link).unwrap();
            assert_eq!(copy_input(&job, &link, &caps), Err(InputRefusal::Link));
        }

        let a = base.path().join("a.csv");
        let b = base.path().join("b.csv");
        let c = base.path().join("c.csv");
        write(&a, 8);
        write(&b, 8);
        write(&c, 1);
        copy_input(&job, &a, &caps).unwrap();
        assert_eq!(
            copy_input(&job, &b, &caps),
            Err(InputRefusal::TotalTooLarge { bytes: 16, cap: 15 })
        );
        copy_input(&job, &c, &caps).unwrap();
        assert_eq!(
            copy_input(&job, &c, &caps),
            Err(InputRefusal::TooMany { cap: 2 })
        );
    }

    #[test]
    fn planned_names_are_the_names_copy_input_gives() {
        let base = tempfile::tempdir().unwrap();
        let job = create(base.path(), "t", day()).unwrap();
        let dirs = ["a", "b", "c"].map(|d| base.path().join(d));
        let mut sources = Vec::new();
        for (dir, name) in dirs.iter().zip(["notes.md", "notes.md", "survey.csv"]) {
            fs::create_dir_all(dir).unwrap();
            write(&dir.join(name), 2);
            sources.push(dir.join(name));
        }
        let copied: Vec<String> = sources
            .iter()
            .map(|s| copy_input(&job, s, &Caps::default()).unwrap())
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(planned_input_names(&sources), copied);
        assert_eq!(copied, ["notes.md", "notes 2.md", "survey.csv"]);
    }

    #[test]
    fn upper_case_extensions_are_allowed() {
        let base = tempfile::tempdir().unwrap();
        let job = create(base.path(), "t", day()).unwrap();
        let src = base.path().join("Quotes.PDF");
        write(&src, 4);
        assert!(copy_input(&job, &src, &Caps::default()).is_ok());
    }

    #[test]
    fn usage_counts_bytes_everywhere_and_files_in_out() {
        let base = tempfile::tempdir().unwrap();
        let job = create(base.path(), "t", day()).unwrap();
        write(&job.inputs.join("a.csv"), 10);
        write(&job.work.join("spec.json"), 5);
        fs::create_dir_all(job.out.join("charts")).unwrap();
        write(&job.out.join("readout.docx"), 20);
        write(&job.out.join("charts/c.png"), 7);
        assert_eq!(
            usage(&job).unwrap(),
            Usage {
                bytes: 42,
                out_files: 2
            }
        );
    }
}
