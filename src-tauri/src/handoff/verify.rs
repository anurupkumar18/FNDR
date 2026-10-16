//! FNDR's check of a finished job. The result card's counts come from here
//! only, never from the worker's own account (PRD FR11, ADR 032 item 6).

use std::path::{Component, Path, PathBuf};

use super::folder::JobFolder;
use super::manifest::{Manifest, OutputKind, Source, SourceKind};

/// What FNDR itself observed about the job.
#[derive(Debug, Clone, Default)]
pub struct Evidence {
    /// Memory ids on the reviewed draft.
    pub attached_memories: Vec<String>,
    /// URLs from this job's `webSearch` events, as Codex reported them.
    pub searched_urls: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileCheck {
    Ok { bytes: u64 },
    OutsideOut,
    Missing,
    Link,
    WrongType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceCheck {
    Ok,
    MemoryNotAttached,
    FileOutsideFolder,
    FileMissing,
    UrlNotSearched,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaimStatus {
    Checked,
    Unsupported,
    NotChecked,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResultStatus {
    Ready,
    ReadyWithGaps,
    Failed,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CheckedFile {
    pub path: String,
    pub kind: OutputKind,
    pub title: String,
    pub check: FileCheck,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CheckedClaim {
    pub text: String,
    pub sources: Vec<(Source, SourceCheck)>,
    pub status: ClaimStatus,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Verdict {
    pub deliverables: Vec<CheckedFile>,
    pub claims: Vec<CheckedClaim>,
    pub checked: usize,
    pub unsupported: usize,
    pub not_checked: usize,
    pub status: ResultStatus,
}

pub fn verify(manifest: &Manifest, folder: &JobFolder, evidence: &Evidence) -> Verdict {
    let deliverables: Vec<CheckedFile> = manifest
        .deliverables
        .iter()
        .map(|d| CheckedFile {
            path: d.path.clone(),
            kind: d.kind,
            title: d.title.clone(),
            check: check_deliverable(folder, &d.path, d.kind),
        })
        .collect();

    let searched: Vec<String> = evidence
        .searched_urls
        .iter()
        .map(|u| normalize_url(u))
        .collect();
    let claims: Vec<CheckedClaim> = manifest
        .claims
        .iter()
        .map(|claim| {
            let sources: Vec<(Source, SourceCheck)> = claim
                .sources
                .iter()
                .map(|s| (s.clone(), check_source(folder, evidence, &searched, s)))
                .collect();
            let status = if sources.is_empty() {
                ClaimStatus::Unsupported
            } else if sources.iter().all(|(_, c)| *c == SourceCheck::Ok) {
                ClaimStatus::Checked
            } else {
                ClaimStatus::NotChecked
            };
            CheckedClaim {
                text: claim.text.clone(),
                sources,
                status,
            }
        })
        .collect();

    let count = |status| claims.iter().filter(|c| c.status == status).count();
    let (checked, unsupported, not_checked) = (
        count(ClaimStatus::Checked),
        count(ClaimStatus::Unsupported),
        count(ClaimStatus::NotChecked),
    );
    let good = |f: &&CheckedFile| matches!(f.check, FileCheck::Ok { .. });
    let status = if !deliverables.iter().any(|f| good(&f)) {
        ResultStatus::Failed
    } else if !manifest.not_done.is_empty()
        || claims.is_empty()
        || unsupported + not_checked > 0
        || deliverables.iter().any(|f| !good(&f))
    {
        ResultStatus::ReadyWithGaps
    } else {
        ResultStatus::Ready
    };
    Verdict {
        deliverables,
        claims,
        checked,
        unsupported,
        not_checked,
        status,
    }
}

fn check_deliverable(folder: &JobFolder, path: &str, kind: OutputKind) -> FileCheck {
    let Some(full) = inside(&folder.root, "out", path) else {
        return FileCheck::OutsideOut;
    };
    let named_as_kind = full
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case(kind.extension()));
    if !named_as_kind {
        return FileCheck::WrongType;
    }
    let meta = match std::fs::symlink_metadata(&full) {
        Ok(meta) => meta,
        Err(_) => return FileCheck::Missing,
    };
    if meta.file_type().is_symlink() {
        return FileCheck::Link;
    }
    // A linked folder between `out/` and the file would also leave the job.
    let stays = match (full.canonicalize(), folder.out.canonicalize()) {
        (Ok(file), Ok(out)) => file.starts_with(out),
        _ => false,
    };
    if !stays {
        return FileCheck::OutsideOut;
    }
    if !meta.is_file() {
        return FileCheck::Missing;
    }
    if is_kind(&full, kind) {
        FileCheck::Ok { bytes: meta.len() }
    } else {
        FileCheck::WrongType
    }
}

/// The file's bytes are what its kind says, read the way the app opening it
/// would start: the Office part, the PDF and PNG signatures, UTF-8 text.
fn is_kind(path: &Path, kind: OutputKind) -> bool {
    let office_part = match kind {
        OutputKind::Docx => Some("word/document.xml"),
        OutputKind::Xlsx => Some("xl/workbook.xml"),
        OutputKind::Pptx => Some("ppt/presentation.xml"),
        _ => None,
    };
    if let Some(part) = office_part {
        return std::fs::File::open(path)
            .ok()
            .and_then(|file| zip::ZipArchive::new(file).ok())
            .is_some_and(|mut archive| archive.by_name(part).is_ok());
    }
    let Ok(bytes) = std::fs::read(path) else {
        return false;
    };
    match kind {
        OutputKind::Pdf => bytes.starts_with(b"%PDF-"),
        OutputKind::Png => bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        _ => std::str::from_utf8(&bytes).is_ok(),
    }
}

fn check_source(
    folder: &JobFolder,
    evidence: &Evidence,
    searched: &[String],
    source: &Source,
) -> SourceCheck {
    match source.kind {
        SourceKind::Memory if evidence.attached_memories.contains(&source.reference) => {
            SourceCheck::Ok
        }
        SourceKind::Memory => SourceCheck::MemoryNotAttached,
        SourceKind::Web if searched.contains(&normalize_url(&source.reference)) => SourceCheck::Ok,
        SourceKind::Web => SourceCheck::UrlNotSearched,
        SourceKind::File => {
            let Some(full) = inside(&folder.root, "inputs", &source.reference)
                .or_else(|| inside(&folder.root, "out", &source.reference))
            else {
                return SourceCheck::FileOutsideFolder;
            };
            match std::fs::symlink_metadata(&full) {
                Ok(meta) if meta.file_type().is_symlink() => SourceCheck::FileOutsideFolder,
                Ok(meta) if meta.is_file() => SourceCheck::Ok,
                _ => SourceCheck::FileMissing,
            }
        }
    }
}

/// `root/<part>/...` for a job-relative path that stays inside `part`; `None`
/// for an absolute path, `..`, or a path that is `part` itself.
fn inside(root: &Path, part: &str, relative: &str) -> Option<PathBuf> {
    let mut names = Vec::new();
    for component in Path::new(relative).components() {
        match component {
            Component::Normal(name) => names.push(name),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    if names.len() < 2 || names[0] != part {
        return None;
    }
    Some(
        names
            .iter()
            .fold(root.to_path_buf(), |path, name| path.join(name)),
    )
}

/// Scheme and host in lower case, no fragment, no trailing slash.
pub fn normalize_url(url: &str) -> String {
    let url = url.trim();
    let url = url.split('#').next().unwrap_or(url);
    let Some((scheme, rest)) = url.split_once("://") else {
        return url.trim_end_matches('/').to_string();
    };
    let host_end = rest.find(['/', '?']).unwrap_or(rest.len());
    let (host, tail) = rest.split_at(host_end);
    let (path, query) = match tail.split_once('?') {
        Some((path, query)) => (path, Some(query)),
        None => (tail, None),
    };
    let mut out = format!(
        "{}://{}{}",
        scheme.to_ascii_lowercase(),
        host.to_ascii_lowercase(),
        path.trim_end_matches('/')
    );
    if let Some(query) = query {
        out.push('?');
        out.push_str(query);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handoff::folder;
    use crate::handoff::manifest::{Claim, Deliverable};
    use std::fs;
    use std::io::Write;

    fn job() -> (tempfile::TempDir, JobFolder) {
        let base = tempfile::tempdir().unwrap();
        let day = chrono::NaiveDate::from_ymd_opt(2026, 10, 16).unwrap();
        let job = folder::create(base.path(), "t", day).unwrap();
        (base, job)
    }

    fn docx(path: &Path) {
        let mut zip = zip::ZipWriter::new(fs::File::create(path).unwrap());
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file("[Content_Types].xml", options).unwrap();
        zip.write_all(b"<Types/>").unwrap();
        zip.start_file("word/document.xml", options).unwrap();
        zip.write_all(b"<w:document/>").unwrap();
        zip.finish().unwrap();
    }

    fn source(kind: SourceKind, reference: &str) -> Source {
        Source {
            kind,
            reference: reference.into(),
            locator: String::new(),
        }
    }

    fn claim(text: &str, sources: Vec<Source>) -> Claim {
        Claim {
            text: text.into(),
            sources,
        }
    }

    fn manifest(deliverables: Vec<Deliverable>, claims: Vec<Claim>) -> Manifest {
        Manifest {
            summary: "s".into(),
            deliverables,
            claims,
            not_done: vec![],
            assumptions: vec![],
        }
    }

    fn readout() -> Deliverable {
        Deliverable {
            path: "out/readout.docx".into(),
            kind: OutputKind::Docx,
            title: "Readout".into(),
        }
    }

    fn evidence() -> Evidence {
        Evidence {
            attached_memories: vec!["m_mail".into()],
            searched_urls: vec!["https://Example.com/Report/#top".into()],
        }
    }

    #[test]
    fn a_real_document_with_checked_sources_is_ready() {
        let (_b, job) = job();
        docx(&job.out.join("readout.docx"));
        fs::write(job.inputs.join("survey.csv"), "a,b\n").unwrap();
        let m = manifest(
            vec![readout()],
            vec![
                claim(
                    "78 churned",
                    vec![source(SourceKind::File, "inputs/survey.csv")],
                ),
                claim("Dana asked", vec![source(SourceKind::Memory, "m_mail")]),
                claim(
                    "Benchmark",
                    vec![source(SourceKind::Web, "https://example.com/Report")],
                ),
            ],
        );
        let v = verify(&m, &job, &evidence());
        assert!(matches!(v.deliverables[0].check, FileCheck::Ok { .. }));
        assert_eq!((v.checked, v.unsupported, v.not_checked), (3, 0, 0));
        assert_eq!(v.status, ResultStatus::Ready);
    }

    #[test]
    fn sources_fndr_did_not_see_are_not_checked() {
        let (_b, job) = job();
        docx(&job.out.join("readout.docx"));
        let m = manifest(
            vec![readout()],
            vec![
                claim("a", vec![source(SourceKind::Memory, "m_other")]),
                claim("b", vec![source(SourceKind::File, "inputs/missing.csv")]),
                claim("c", vec![source(SourceKind::File, "../../.ssh/id_rsa")]),
                claim("d", vec![source(SourceKind::File, "/etc/hosts")]),
                claim(
                    "e",
                    vec![source(SourceKind::Web, "https://fake.example/cite")],
                ),
                claim("f", vec![]),
            ],
        );
        let v = verify(&m, &job, &evidence());
        let checks: Vec<SourceCheck> = v
            .claims
            .iter()
            .flat_map(|c| c.sources.iter().map(|(_, s)| s.clone()))
            .collect();
        assert_eq!(
            checks,
            [
                SourceCheck::MemoryNotAttached,
                SourceCheck::FileMissing,
                SourceCheck::FileOutsideFolder,
                SourceCheck::FileOutsideFolder,
                SourceCheck::UrlNotSearched,
            ]
        );
        assert_eq!((v.checked, v.unsupported, v.not_checked), (0, 1, 5));
        assert_eq!(v.claims[5].status, ClaimStatus::Unsupported);
        assert_eq!(v.status, ResultStatus::ReadyWithGaps);
    }

    #[test]
    fn one_bad_source_makes_the_claim_not_checked() {
        let (_b, job) = job();
        docx(&job.out.join("readout.docx"));
        let m = manifest(
            vec![readout()],
            vec![claim(
                "x",
                vec![
                    source(SourceKind::Memory, "m_mail"),
                    source(SourceKind::Memory, "m_other"),
                ],
            )],
        );
        assert_eq!(
            verify(&m, &job, &evidence()).claims[0].status,
            ClaimStatus::NotChecked
        );
    }

    #[test]
    fn deliverables_must_be_real_files_of_their_kind_inside_out() {
        let (_b, job) = job();
        fs::write(job.out.join("fake.docx"), "just text").unwrap();
        fs::write(job.out.join("notes.md"), [0xff, 0xfe, 0x00]).unwrap();
        fs::write(job.work.join("spec.json"), "{}").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink("/etc/hosts", job.out.join("hosts.txt")).unwrap();
        let d = |path: &str, kind| Deliverable {
            path: path.into(),
            kind,
            title: "t".into(),
        };
        let m = manifest(
            vec![
                d("out/fake.docx", OutputKind::Docx),
                d("out/notes.md", OutputKind::Md),
                d("work/spec.json", OutputKind::Txt),
                d("out/../../escape.txt", OutputKind::Txt),
                d("out/none.pdf", OutputKind::Pdf),
                d("out/hosts.txt", OutputKind::Txt),
                d("out/readout.pdf", OutputKind::Docx),
            ],
            vec![],
        );
        let checks: Vec<FileCheck> = verify(&m, &job, &evidence())
            .deliverables
            .into_iter()
            .map(|f| f.check)
            .collect();
        assert_eq!(
            checks,
            [
                FileCheck::WrongType,
                FileCheck::WrongType,
                FileCheck::OutsideOut,
                FileCheck::OutsideOut,
                FileCheck::Missing,
                FileCheck::Link,
                FileCheck::WrongType,
            ]
        );
    }

    #[test]
    fn no_good_deliverable_fails_the_job() {
        let (_b, job) = job();
        let v = verify(&manifest(vec![readout()], vec![]), &job, &evidence());
        assert_eq!(v.status, ResultStatus::Failed);
        let v = verify(&manifest(vec![], vec![]), &job, &evidence());
        assert_eq!(v.status, ResultStatus::Failed);
    }

    #[test]
    fn not_done_or_no_claims_is_a_gap() {
        let (_b, job) = job();
        docx(&job.out.join("readout.docx"));
        let mut m = manifest(
            vec![readout()],
            vec![claim("x", vec![source(SourceKind::Memory, "m_mail")])],
        );
        assert_eq!(verify(&m, &job, &evidence()).status, ResultStatus::Ready);
        m.not_done = vec!["Deadline not confirmed".into()];
        assert_eq!(
            verify(&m, &job, &evidence()).status,
            ResultStatus::ReadyWithGaps
        );
        let empty = manifest(vec![readout()], vec![]);
        assert_eq!(
            verify(&empty, &job, &evidence()).status,
            ResultStatus::ReadyWithGaps
        );
    }

    #[test]
    fn png_pdf_text_and_office_kinds_are_recognised() {
        let (_b, job) = job();
        fs::write(job.out.join("c.png"), b"\x89PNG\r\n\x1a\nrest").unwrap();
        fs::write(job.out.join("r.pdf"), b"%PDF-1.7\n").unwrap();
        fs::write(job.out.join("t.csv"), "a,b\n1,2\n").unwrap();
        let d = |path: &str, kind| Deliverable {
            path: path.into(),
            kind,
            title: "t".into(),
        };
        let m = manifest(
            vec![
                d("out/c.png", OutputKind::Png),
                d("out/r.pdf", OutputKind::Pdf),
                d("out/t.csv", OutputKind::Csv),
            ],
            vec![],
        );
        assert!(verify(&m, &job, &evidence())
            .deliverables
            .iter()
            .all(|f| matches!(f.check, FileCheck::Ok { .. })));
    }

    #[test]
    fn urls_compare_without_case_fragment_or_trailing_slash() {
        assert_eq!(
            normalize_url("HTTPS://Example.COM/Path/?q=1#x"),
            "https://example.com/Path?q=1"
        );
        assert_eq!(normalize_url("https://example.com/"), "https://example.com");
        assert_ne!(
            normalize_url("https://example.com/a"),
            normalize_url("https://example.com/A")
        );
    }

    #[test]
    fn job_relative_paths_never_leave_their_part() {
        let root = PathBuf::from("/j");
        assert_eq!(
            inside(&root, "out", "out/a/b.docx"),
            Some(PathBuf::from("/j/out/a/b.docx"))
        );
        assert_eq!(inside(&root, "out", "out/../inputs/a"), None);
        assert_eq!(
            inside(&root, "out", "./out/a"),
            Some(PathBuf::from("/j/out/a"))
        );
        assert_eq!(inside(&root, "out", "/j/out/a"), None);
        assert_eq!(inside(&root, "out", "outside/a"), None);
        assert_eq!(inside(&root, "out", "out"), None);
    }
}
