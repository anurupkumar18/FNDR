use crate::import::*;
use chrono::DateTime;
use serde_json::Value;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const MAX_TEXT_CHARS: usize = 20_000;
const SKIP: &[&str] = &["node_modules", "Library", ".Trash", "target"];

pub struct GitSource {
    pub roots: Vec<PathBuf>,
    pub global_config_override: Option<PathBuf>,
    pub scan_gh: bool,
}

impl GitSource {
    pub fn default_home() -> Self {
        Self {
            roots: dirs::home_dir().into_iter().collect(),
            global_config_override: None,
            scan_gh: false,
        }
    }

    fn git(&self, repo: &Path, args: &[&str]) -> Result<Output, ImportError> {
        let mut command = Command::new("git");
        command
            .arg("-C")
            .arg(repo)
            .args(args)
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_CONFIG_NOSYSTEM", "1");
        if let Some(path) = &self.global_config_override {
            command.env("GIT_CONFIG_GLOBAL", path);
        }
        command
            .output()
            .map_err(|error| ImportError::Command(error.to_string()))
    }

    fn git_text(&self, repo: &Path, args: &[&str]) -> Result<String, ImportError> {
        let output = self.git(repo, args)?;
        if !output.status.success() {
            return Err(ImportError::Command(
                String::from_utf8_lossy(&output.stderr).into_owned(),
            ));
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    fn emails(&self, repo: &Path) -> HashSet<String> {
        let mut emails = HashSet::new();
        for scope in ["--global", "--local"] {
            if let Ok(output) = self.git(repo, &["config", scope, "--get-all", "user.email"]) {
                if output.status.success() {
                    for value in String::from_utf8_lossy(&output.stdout).lines() {
                        let value = value.trim().to_ascii_lowercase();
                        if !value.is_empty() {
                            emails.insert(value);
                        }
                    }
                }
            }
        }
        emails
    }

    fn repositories(&self) -> Result<Vec<PathBuf>, ImportError> {
        let mut repos = Vec::new();
        for root in &self.roots {
            discover(root, 0, &mut repos)?;
        }
        repos.sort();
        repos.dedup();
        Ok(repos)
    }
}

impl ImportSource for GitSource {
    fn kind(&self) -> ImportKind {
        ImportKind::Git
    }
    fn label(&self) -> &'static str {
        "git"
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
        let mut items = Vec::new();
        for repo in self.repositories()? {
            let emails = self.emails(&repo);
            let repo_name = repo
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("repository");
            let branch = self
                .git_text(&repo, &["branch", "--show-current"])
                .unwrap_or_default()
                .trim()
                .to_string();
            let remote = self
                .git_text(&repo, &["config", "--local", "--get", "remote.origin.url"])
                .ok();
            let since_arg = format!("--since={} days ago", window_days);
            let log_args = [
                "log",
                "--all",
                &since_arg,
                "--format=%x1e%H%x1f%ae%x1f%ct%x1f%s%x1f%b",
                "--numstat",
            ];
            let log_output = self.git(&repo, &log_args)?;
            let log = if log_output.status.success() {
                String::from_utf8_lossy(&log_output.stdout).into_owned()
            } else if !self
                .git(&repo, &["rev-parse", "--verify", "HEAD"])?
                .status
                .success()
            {
                String::new()
            } else {
                return Err(ImportError::Command(
                    String::from_utf8_lossy(&log_output.stderr).into_owned(),
                ));
            };
            for block in log.split('\x1e').skip(1) {
                let mut fields = block.splitn(5, '\x1f');
                let (Some(hash), Some(email), Some(seconds), Some(subject), Some(rest)) = (
                    fields.next(),
                    fields.next(),
                    fields.next(),
                    fields.next(),
                    fields.next(),
                ) else {
                    continue;
                };
                if !emails.contains(&email.to_ascii_lowercase()) {
                    continue;
                }
                let hash = hash.trim();
                let (body, stats) = rest.split_once("\n\n").unwrap_or((rest, ""));
                let mut text = format!(
                    "Repository: {repo_name}\nBranch: {branch}\n{subject}\n{}",
                    body.trim()
                );
                for line in stats.lines() {
                    let mut parts = line.splitn(3, '\t');
                    if let (Some(add), Some(del), Some(path)) =
                        (parts.next(), parts.next(), parts.next())
                    {
                        if add.chars().all(|c| c.is_ascii_digit() || c == '-')
                            && del.chars().all(|c| c.is_ascii_digit() || c == '-')
                        {
                            text.push_str(&format!("\n{path}: +{add} -{del}"));
                        }
                    }
                    if text.chars().count() >= MAX_TEXT_CHARS {
                        break;
                    }
                }
                let target = remote
                    .as_deref()
                    .and_then(|r| commit_url(r.trim(), hash))
                    .unwrap_or_else(|| format!("{}#{}", repo.display(), hash));
                items.push(ImportItem {
                    kind: ImportKind::Git,
                    external_id: format!("{}:{hash}", repo.display()),
                    timestamp_ms: seconds.parse::<i64>().unwrap_or(0) * 1000,
                    title: subject.to_string(),
                    text: text.chars().take(MAX_TEXT_CHARS).collect(),
                    reopen_target: Some(target.clone()),
                    app_label: "git".to_string(),
                    people: vec![email.to_string()],
                    url: target.starts_with("https://").then_some(target),
                });
            }
            let checkouts = self
                .git_text(&repo, &["reflog", "--format=%gs"])
                .unwrap_or_default();
            let lines: Vec<_> = checkouts
                .lines()
                .filter(|line| line.starts_with("checkout: moving from "))
                .take(20)
                .collect();
            let text = format!(
                "Repository: {repo_name}\nCurrent branch: {branch}\n{}",
                lines.join("\n")
            );
            let head_modified_ms = fs::metadata(repo.join(".git/HEAD"))
                .and_then(|meta| meta.modified())
                .ok()
                .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|duration| duration.as_millis() as i64)
                .unwrap_or(0);
            let latest_commit_ms = self
                .git_text(&repo, &["log", "-1", "--format=%ct"])
                .ok()
                .and_then(|value| value.trim().parse::<i64>().ok())
                .unwrap_or(0)
                .saturating_mul(1000);
            items.push(ImportItem {
                kind: ImportKind::Git,
                external_id: format!("{}:branches", repo.display()),
                timestamp_ms: head_modified_ms.max(latest_commit_ms),
                title: format!("Branches in {repo_name}"),
                text: text.chars().take(MAX_TEXT_CHARS).collect(),
                reopen_target: Some(repo.display().to_string()),
                app_label: "git".to_string(),
                people: Vec::new(),
                url: None,
            });
        }
        if self.scan_gh {
            match scan_gh(true, None) {
                Ok(result) => {
                    tracing::info!(commands = ?result.commands_run, "GitHub import commands run");
                    items.extend(result.items);
                }
                Err(error) => {
                    tracing::info!(error = ?error, "GitHub import command failed");
                    let detail = format!("{error:?}");
                    return Err(if matches!(error, GhError::InvalidJson { .. }) {
                        ImportError::InvalidData(detail)
                    } else {
                        ImportError::Command(detail)
                    });
                }
            }
        }
        items.sort_by(|a, b| a.external_id.cmp(&b.external_id));
        let offset = since
            .and_then(|c| c.0.parse::<usize>().ok())
            .unwrap_or(0)
            .min(items.len());
        let end = offset.saturating_add(limit).min(items.len());
        let done = end == items.len();
        Ok(ScanBatch {
            items: items.drain(offset..end).collect(),
            next: (!done).then(|| Cursor(end.to_string())),
            done,
        })
    }
}

fn discover(path: &Path, depth: usize, out: &mut Vec<PathBuf>) -> Result<(), ImportError> {
    if depth > 0
        && path
            .file_name()
            .and_then(|s| s.to_str())
            .is_some_and(|name| name.starts_with('.') || SKIP.contains(&name))
    {
        return Ok(());
    }
    if depth > 4
        || path
            .symlink_metadata()
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(true)
    {
        return Ok(());
    }
    if path.join(".git").exists() {
        out.push(path.to_path_buf());
        return Ok(());
    }
    if !path.is_dir() {
        return Ok(());
    }
    let entries = match fs::read_dir(path) {
        Ok(entries) => entries,
        Err(error) if depth > 0 && error.kind() == std::io::ErrorKind::PermissionDenied => {
            return Ok(())
        }
        Err(error) => return Err(error.into()),
    };
    for child in entries {
        let child = child?;
        let name = child.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') || SKIP.contains(&name.as_ref()) {
            continue;
        }
        if child.file_type()?.is_dir() {
            discover(&child.path(), depth + 1, out)?;
        }
    }
    Ok(())
}

fn commit_url(remote: &str, hash: &str) -> Option<String> {
    let path = if let Some(rest) = remote.strip_prefix("git@github.com:") {
        rest
    } else if let Some(rest) = remote.strip_prefix("git@gitlab.com:") {
        rest
    } else if let Some(rest) = remote.strip_prefix("https://github.com/") {
        rest
    } else if let Some(rest) = remote.strip_prefix("https://gitlab.com/") {
        rest
    } else {
        return None;
    };
    let host = if remote.contains("github.com") {
        "github.com"
    } else {
        "gitlab.com"
    };
    let repo = path.trim_end_matches(".git").trim_end_matches('/');
    if repo.is_empty() || hash.is_empty() {
        return None;
    }
    Some(format!("https://{host}/{repo}/commit/{hash}"))
}

#[derive(Debug, PartialEq, Eq)]
pub enum GhError {
    Disabled,
    Missing {
        commands_run: Vec<String>,
    },
    Unauthenticated {
        commands_run: Vec<String>,
    },
    CommandFailed {
        commands_run: Vec<String>,
        detail: String,
    },
    InvalidJson {
        commands_run: Vec<String>,
    },
}

pub struct GhScan {
    pub items: Vec<ImportItem>,
    pub commands_run: Vec<String>,
}

pub fn scan_gh(enabled: bool, path_override: Option<&Path>) -> Result<GhScan, GhError> {
    if !enabled {
        return Err(GhError::Disabled);
    }
    let mut items = Vec::new();
    let mut commands_run = Vec::new();
    for kind in ["prs", "issues"] {
        let command_label =
            format!("gh search {kind} --author @me --json title,body,url,createdAt,author,number");
        let mut command = Command::new("gh");
        command
            .args([
                "search",
                kind,
                "--author",
                "@me",
                "--json",
                "title,body,url,createdAt,author,number",
            ])
            .env("GH_PROMPT_DISABLED", "1");
        if let Some(path) = path_override {
            command.env("PATH", path);
        }
        let output = command.output().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                GhError::Missing {
                    commands_run: commands_run.clone(),
                }
            } else {
                GhError::CommandFailed {
                    commands_run: commands_run.clone(),
                    detail: e.to_string(),
                }
            }
        })?;
        commands_run.push(command_label);
        if !output.status.success() {
            let detail = String::from_utf8_lossy(&output.stderr).to_string();
            if detail.contains("auth") || detail.contains("login") || detail.contains("not logged")
            {
                return Err(GhError::Unauthenticated { commands_run });
            }
            return Err(GhError::CommandFailed {
                commands_run,
                detail,
            });
        }
        let records: Vec<Value> =
            serde_json::from_slice(&output.stdout).map_err(|_| GhError::InvalidJson {
                commands_run: commands_run.clone(),
            })?;
        for row in records {
            let Some(url) = row["url"].as_str() else {
                continue;
            };
            let title = row["title"].as_str().unwrap_or("Untitled");
            let body = row["body"].as_str().unwrap_or("");
            let timestamp_ms = row["createdAt"]
                .as_str()
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|date| date.timestamp_millis())
                .unwrap_or(0);
            items.push(ImportItem {
                kind: ImportKind::Git,
                external_id: url.to_string(),
                timestamp_ms,
                title: title.to_string(),
                text: format!("{title}\n{body}")
                    .chars()
                    .take(MAX_TEXT_CHARS)
                    .collect(),
                reopen_target: Some(url.to_string()),
                app_label: "GitHub CLI".to_string(),
                people: row["author"]["login"]
                    .as_str()
                    .map(str::to_string)
                    .into_iter()
                    .collect(),
                url: Some(url.to_string()),
            });
        }
    }
    Ok(GhScan {
        items,
        commands_run,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, process::Command};

    fn git(dir: &Path, args: &[&str]) {
        assert!(Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .output()
            .unwrap()
            .status
            .success());
    }

    #[test]
    fn imports_only_configured_author_and_file_stats() {
        let dir = tempfile::tempdir().unwrap();
        git(dir.path(), &["init", "-q"]);
        git(dir.path(), &["config", "user.email", "me@example.test"]);
        git(dir.path(), &["config", "user.name", "Me"]);
        fs::write(dir.path().join("note.txt"), "hello\n").unwrap();
        git(dir.path(), &["add", "note.txt"]);
        git(dir.path(), &["commit", "-qm", "My work"]);
        git(dir.path(), &["config", "user.email", "other@example.test"]);
        git(dir.path(), &["config", "user.name", "Other"]);
        fs::write(dir.path().join("note.txt"), "secret line\n").unwrap();
        git(dir.path(), &["commit", "-qam", "Other work"]);
        git(dir.path(), &["config", "user.email", "me@example.test"]);
        git(dir.path(), &["config", "user.name", "Me"]);
        let batch = GitSource {
            roots: vec![dir.path().to_owned()],
            global_config_override: Some(PathBuf::from("/dev/null")),
            scan_gh: false,
        }
        .scan(None, 30, 10)
        .unwrap();
        assert!(batch
            .items
            .iter()
            .any(|i| i.title == "My work" && i.text.contains("note.txt")));
        assert!(!batch.items.iter().any(|i| i.title == "Other work"));
        assert!(!batch.items.iter().any(|i| i.text.contains("secret line")));
    }

    #[test]
    fn discovery_skips_hidden_dirs_build_dirs_and_symlink_loops() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let good = dir.path().join("good");
        let skipped = dir.path().join("node_modules/bad");
        fs::create_dir_all(&good).unwrap();
        fs::create_dir_all(&skipped).unwrap();
        fs::create_dir(good.join(".git")).unwrap();
        fs::create_dir(skipped.join(".git")).unwrap();
        let blocked = dir.path().join("blocked");
        fs::create_dir(&blocked).unwrap();
        fs::set_permissions(&blocked, fs::Permissions::from_mode(0o000)).unwrap();
        std::os::unix::fs::symlink(dir.path(), good.join("loop")).unwrap();
        let source = GitSource {
            roots: vec![dir.path().to_owned()],
            global_config_override: Some(PathBuf::from("/dev/null")),
            scan_gh: false,
        };
        assert_eq!(source.repositories().unwrap(), vec![good]);
        fs::set_permissions(&blocked, fs::Permissions::from_mode(0o700)).unwrap();
    }

    #[test]
    fn empty_repository_has_branch_item_without_commits() {
        let dir = tempfile::tempdir().unwrap();
        git(dir.path(), &["init", "-q"]);
        let batch = GitSource {
            roots: vec![dir.path().to_owned()],
            global_config_override: Some(PathBuf::from("/dev/null")),
            scan_gh: false,
        }
        .scan(None, 30, 10)
        .unwrap();
        assert_eq!(batch.items.len(), 1);
        assert!(batch.items[0].title.starts_with("Branches in"));
    }

    #[test]
    fn gh_is_gated_and_reports_fixed_commands() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("gh");
        fs::write(&script, "#!/bin/sh\nprintf '[{\"title\":\"Fix\",\"body\":\"Body\",\"url\":\"https://github.com/a/b/pull/1\",\"createdAt\":\"2026-10-14T00:00:00Z\",\"author\":{\"login\":\"me\"}}]'\n").unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(
            scan_gh(false, Some(dir.path())).err(),
            Some(GhError::Disabled)
        );
        let batch = scan_gh(true, Some(dir.path())).unwrap();
        assert_eq!(batch.items.len(), 2);
        assert_eq!(batch.commands_run.len(), 2);
        assert!(batch.commands_run[0].contains("search prs --author @me"));
        fs::write(
            &script,
            "#!/bin/sh\necho 'gh auth login required' >&2\nexit 1\n",
        )
        .unwrap();
        match scan_gh(true, Some(dir.path())) {
            Err(GhError::Unauthenticated { commands_run }) => assert_eq!(commands_run.len(), 1),
            _ => panic!("expected unauthenticated gh error"),
        }
        fs::remove_file(&script).unwrap();
        assert_eq!(
            scan_gh(true, Some(dir.path())).err(),
            Some(GhError::Missing {
                commands_run: Vec::new()
            })
        );
    }

    #[test]
    fn global_author_remote_url_and_branch_checkouts() {
        let dir = tempfile::tempdir().unwrap();
        let global = dir.path().join("gitconfig");
        fs::write(&global, "[user]\nemail = global@example.test\n").unwrap();
        let repo = dir.path().join("repo");
        fs::create_dir(&repo).unwrap();
        git(&repo, &["init", "-q"]);
        git(&repo, &["config", "user.name", "Global Author"]);
        git(
            &repo,
            &["remote", "add", "origin", "git@github.com:team/repo.git"],
        );
        fs::write(repo.join("note.txt"), "text\n").unwrap();
        git(&repo, &["add", "note.txt"]);
        git(
            &repo,
            &[
                "-c",
                "user.email=global@example.test",
                "commit",
                "-qm",
                "Global work",
            ],
        );
        git(&repo, &["checkout", "-qb", "feature"]);
        let batch = GitSource {
            roots: vec![repo],
            global_config_override: Some(global),
            scan_gh: false,
        }
        .scan(None, 30, 10)
        .unwrap();
        let commit = batch
            .items
            .iter()
            .find(|i| i.title == "Global work")
            .unwrap();
        assert!(commit
            .url
            .as_deref()
            .unwrap()
            .starts_with("https://github.com/team/repo/commit/"));
        let branches = batch
            .items
            .iter()
            .find(|i| i.title.starts_with("Branches in"))
            .unwrap();
        assert!(branches.text.contains("Current branch: feature"));
        assert!(branches.text.contains("checkout: moving from"));
    }
}
