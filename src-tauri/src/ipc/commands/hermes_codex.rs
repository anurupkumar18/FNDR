//! Hermes on the person's one ChatGPT sign-in (ADR-018 amendment 2026-10-06).
//!
//! - Sign-in happens once, through the official `codex app-server`, which
//!   writes `~/.codex/auth.json`. FNDR never reads or writes token values.
//! - Hermes imports that login itself. Its `openai-codex` provider re-imports
//!   from `$CODEX_HOME` when its own entry has no tokens, so FNDR writes an
//!   entry with an empty `tokens` object and Hermes fills it.
//! - The Codex app-server is the only refresher. FNDR asks it to refresh before
//!   Hermes starts and every 30 minutes while Hermes runs; whenever the login
//!   rotated, Hermes's copy is emptied again so Hermes re-imports the fresh one
//!   instead of ever spending the shared refresh token itself.

use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::codex_account::{codex_home_dir, parse_account, ready_executable, AppServer};

/// Hermes commit FNDR installs into its app data (v0.18.2, upstream tag
/// v2026.7.7.2 plus 256 commits). Its Codex import path was read at this commit.
pub(crate) const HERMES_PINNED_COMMIT: &str = "b8880f124537acc5a6215718dd154eadc5af1515";
pub(crate) const HERMES_REPO_URL: &str = "https://github.com/NousResearch/hermes-agent.git";

/// How often the app-server is asked to refresh while Hermes runs.
pub(crate) const REFRESH_INTERVAL: Duration = Duration::from_secs(30 * 60);

const PROVIDER: &str = "openai-codex";

/// Holds Hermes's cross-process lock on its auth store (`auth.lock`, flock).
struct AuthLock {
    file: std::fs::File,
}

impl AuthLock {
    fn acquire(hermes_home: &Path) -> Result<Self, String> {
        std::fs::create_dir_all(hermes_home).map_err(|e| e.to_string())?;
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(hermes_home.join("auth.lock"))
            .map_err(|e| format!("Could not open Hermes's auth lock: {e}"))?;
        #[cfg(unix)]
        {
            use std::os::unix::io::AsRawFd;
            if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) } != 0 {
                return Err("Could not lock Hermes's auth store.".to_string());
            }
        }
        Ok(Self { file })
    }
}

impl Drop for AuthLock {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            use std::os::unix::io::AsRawFd;
            unsafe {
                libc::flock(self.file.as_raw_fd(), libc::LOCK_UN);
            }
        }
    }
}

/// Points Hermes's `openai-codex` provider at the Codex login: an entry with
/// no tokens, so Hermes imports from `$CODEX_HOME` on its next request. Any
/// copy Hermes already holds is dropped, including its pool entry.
pub(crate) fn mark_hermes_for_codex_import(hermes_home: &Path) -> Result<(), String> {
    let _lock = AuthLock::acquire(hermes_home)?;
    let path = hermes_home.join("auth.json");
    let mut store: Value = std::fs::read_to_string(&path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({ "version": 1, "providers": {} }));
    let object = store.as_object_mut().expect("auth store is an object");
    object.entry("version").or_insert(json!(1));
    let providers = object.entry("providers").or_insert_with(|| json!({}));
    if !providers.is_object() {
        *providers = json!({});
    }
    providers[PROVIDER] = json!({ "tokens": {}, "auth_mode": "chatgpt" });
    if let Some(pool) = object
        .get_mut("credential_pool")
        .and_then(Value::as_object_mut)
    {
        pool.remove(PROVIDER);
    }
    object.insert("active_provider".to_string(), json!(PROVIDER));

    let tmp = hermes_home.join("auth.json.fndr-tmp");
    std::fs::write(
        &tmp,
        serde_json::to_vec_pretty(&store).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("Could not update Hermes's auth store: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
    }
    std::fs::rename(&tmp, &path).map_err(|e| format!("Could not update Hermes's auth store: {e}"))
}

/// The Codex login's `last_refresh` stamp: metadata only, no token values.
pub(crate) fn codex_last_refresh(codex_home: &Path) -> Option<String> {
    let raw = std::fs::read_to_string(codex_home.join("auth.json")).ok()?;
    let value: Value = serde_json::from_str(&raw).ok()?;
    value
        .get("last_refresh")
        .and_then(Value::as_str)
        .map(str::to_string)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LoginState {
    SignedIn,
    /// No ChatGPT sign-in, or it could not be refreshed: show Reconnect.
    SignedOut(String),
}

/// Asks the Codex app-server, the only refresher, to refresh the ChatGPT
/// login if it needs it. Returns whether the login rotated.
pub(crate) async fn refresh_codex_login() -> Result<(LoginState, bool), String> {
    let codex = ready_executable()?;
    let before = codex_last_refresh(&codex_home_dir());
    let mut server = AppServer::spawn_with(&codex, &[]).await?;
    let result = server
        .request("account/read", json!({ "refreshToken": true }))
        .await;
    server.shutdown().await;
    let state = match result {
        Ok(account) if parse_account(&account).is_some_and(|a| a.kind == "chatgpt") => {
            LoginState::SignedIn
        }
        Ok(_) => {
            LoginState::SignedOut("Sign in with ChatGPT to use Hermes and Notch Do.".to_string())
        }
        Err(error) => LoginState::SignedOut(error),
    };
    let rotated = codex_last_refresh(&codex_home_dir()) != before;
    Ok((state, rotated))
}

/// Refreshes through the app-server and, if the login rotated, has Hermes
/// re-import it. Run before every Hermes start and on `REFRESH_INTERVAL`.
pub(crate) async fn sync_hermes_codex_login(hermes_home: &Path) -> LoginState {
    match refresh_codex_login().await {
        Ok((LoginState::SignedIn, rotated)) => {
            let never_imported = !hermes_home.join("auth.json").exists();
            if rotated || never_imported {
                if let Err(error) = mark_hermes_for_codex_import(hermes_home) {
                    tracing::warn!(%error, "hermes_codex:mark_import_failed");
                }
            }
            LoginState::SignedIn
        }
        Ok((signed_out, _)) => signed_out,
        Err(error) => LoginState::SignedOut(error),
    }
}

/// The `mcp_servers` block that lets Hermes query FNDR over its MCP server.
pub(crate) fn hermes_mcp_yaml(endpoint: &str, token: &str) -> String {
    let quote = |value: &str| serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string());
    format!(
        "mcp_servers:\n  fndr:\n    url: {}\n    headers:\n      Authorization: {}\n",
        quote(endpoint),
        quote(&format!("Bearer {token}")),
    )
}

// MARK: - Supervision

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CrashResponse {
    Restart { after: Duration },
    GiveUp,
}

/// One restart per crash streak, with a short backoff. A gateway that stayed
/// up for five minutes starts a new streak.
pub(crate) fn on_crash(restarts_in_streak: u32, uptime: Duration) -> CrashResponse {
    let fresh_streak = uptime >= Duration::from_secs(300);
    if restarts_in_streak == 0 || fresh_streak {
        CrashResponse::Restart {
            after: Duration::from_secs(2),
        }
    } else {
        CrashResponse::GiveUp
    }
}

/// The newest `vYYYY.M.D[.N]` tag in `git ls-remote --tags` output.
/// Pre-releases (anything with a suffix) are skipped.
pub(crate) fn newest_release_tag(ls_remote: &str) -> Option<String> {
    ls_remote
        .lines()
        .filter_map(|line| line.split("refs/tags/").nth(1))
        .filter(|tag| !tag.ends_with("^{}"))
        .filter_map(|tag| {
            let parts: Option<Vec<u64>> = tag
                .strip_prefix('v')?
                .split('.')
                .map(|p| p.parse().ok())
                .collect();
            parts
                .filter(|parts| parts.len() >= 3)
                .map(|parts| (parts, tag.to_string()))
        })
        .max_by(|a, b| a.0.cmp(&b.0))
        .map(|(_, tag)| tag)
}

fn git_in(dir: &Path, args: &[&str]) -> Result<String, String> {
    let output = std::process::Command::new("/usr/bin/git")
        .args(args)
        .current_dir(dir)
        .output()
        .map_err(|e| format!("Could not run git: {e}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

/// The newest Hermes release on GitHub.
pub(crate) fn latest_hermes_release() -> Result<String, String> {
    crate::privacy_proof::record_egress("github.com");
    let output = std::process::Command::new("/usr/bin/git")
        .args(["ls-remote", "--tags", HERMES_REPO_URL])
        .output()
        .map_err(|e| format!("Could not check for a Hermes update: {e}"))?;
    if !output.status.success() {
        return Err("Could not reach GitHub to check for a Hermes update.".to_string());
    }
    newest_release_tag(&String::from_utf8_lossy(&output.stdout))
        .ok_or_else(|| "No Hermes release was found.".to_string())
}

/// The installed Hermes: its release tag when it sits on one, else the short commit.
pub(crate) fn installed_hermes_version(runtime_root: &Path) -> Option<String> {
    let dir = pinned_hermes_dir(runtime_root);
    git_in(&dir, &["describe", "--tags", "--exact-match"])
        .or_else(|_| git_in(&dir, &["rev-parse", "--short", "HEAD"]))
        .ok()
}

/// Checks out `tag` in FNDR's Hermes copy. Returns the commit it replaced, so a
/// failed reinstall can go back.
pub(crate) fn checkout_hermes_release(runtime_root: &Path, tag: &str) -> Result<String, String> {
    let dir = pinned_hermes_dir(runtime_root);
    let previous = git_in(&dir, &["rev-parse", "HEAD"])?;
    crate::privacy_proof::record_egress("github.com");
    git_in(&dir, &["fetch", "-q", "--depth", "1", "origin", "tag", tag])
        .map_err(|e| format!("Downloading Hermes {tag} failed: {e}"))?;
    git_in(&dir, &["checkout", "-q", tag])
        .map_err(|e| format!("Switching to Hermes {tag} failed: {e}"))?;
    Ok(previous)
}

pub(crate) fn checkout_hermes_commit(runtime_root: &Path, commit: &str) -> Result<(), String> {
    git_in(
        &pinned_hermes_dir(runtime_root),
        &["checkout", "-q", commit],
    )
    .map(|_| ())
}

pub(crate) fn pinned_hermes_dir(runtime_root: &Path) -> PathBuf {
    runtime_root.join("src")
}

/// Clones Hermes at the pinned commit into FNDR's app data.
pub(crate) fn clone_pinned_hermes(runtime_root: &Path) -> Result<PathBuf, String> {
    let target = pinned_hermes_dir(runtime_root);
    if target.join("pyproject.toml").exists() {
        return Ok(target);
    }
    std::fs::create_dir_all(&target).map_err(|e| e.to_string())?;
    let git = |args: &[&str]| -> Result<(), String> {
        let output = std::process::Command::new("/usr/bin/git")
            .args(args)
            .current_dir(&target)
            .output()
            .map_err(|e| format!("Could not run git to install Hermes: {e}"))?;
        if output.status.success() {
            Ok(())
        } else {
            Err(format!(
                "Installing Hermes failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ))
        }
    };
    crate::privacy_proof::record_egress("github.com");
    git(&["init", "-q"])?;
    git(&["remote", "add", "origin", HERMES_REPO_URL])?;
    git(&[
        "fetch",
        "-q",
        "--depth",
        "1",
        "origin",
        HERMES_PINNED_COMMIT,
    ])?;
    git(&["checkout", "-q", "FETCH_HEAD"])?;
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(path: &Path) -> Value {
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    #[test]
    fn marks_an_empty_hermes_home_for_import_without_any_token() {
        let home = tempfile::tempdir().unwrap();
        mark_hermes_for_codex_import(home.path()).unwrap();
        let store = read(&home.path().join("auth.json"));
        assert_eq!(store["providers"]["openai-codex"]["tokens"], json!({}));
        assert_eq!(store["active_provider"], "openai-codex");
        assert_eq!(store["version"], 1);
    }

    #[test]
    fn drops_a_stale_copy_and_its_pool_entry_but_keeps_other_providers() {
        let home = tempfile::tempdir().unwrap();
        std::fs::write(
            home.path().join("auth.json"),
            json!({
                "version": 1,
                "providers": {
                    "openai-codex": { "tokens": { "access_token": "old", "refresh_token": "old" } },
                    "nous": { "tokens": { "access_token": "keep" } }
                },
                "credential_pool": { "openai-codex": [{ "access_token": "old" }], "anthropic": [] }
            })
            .to_string(),
        )
        .unwrap();
        mark_hermes_for_codex_import(home.path()).unwrap();
        let store = read(&home.path().join("auth.json"));
        assert_eq!(store["providers"]["openai-codex"]["tokens"], json!({}));
        assert_eq!(store["providers"]["nous"]["tokens"]["access_token"], "keep");
        assert!(store["credential_pool"].get("openai-codex").is_none());
        assert!(store["credential_pool"].get("anthropic").is_some());
        assert!(!std::fs::read_to_string(home.path().join("auth.json"))
            .unwrap()
            .contains("old"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(home.path().join("auth.json"))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }

    #[test]
    fn reads_only_the_refresh_stamp() {
        let home = tempfile::tempdir().unwrap();
        assert_eq!(codex_last_refresh(home.path()), None);
        std::fs::write(
            home.path().join("auth.json"),
            r#"{"tokens":{"access_token":"a"},"last_refresh":"2026-10-02T17:14:49Z"}"#,
        )
        .unwrap();
        assert_eq!(
            codex_last_refresh(home.path()).as_deref(),
            Some("2026-10-02T17:14:49Z")
        );
    }

    #[test]
    fn picks_the_newest_release_tag() {
        let ls_remote = "a1\trefs/tags/v2026.7.7.2\nb2\trefs/tags/v2026.9.24\nb2\trefs/tags/v2026.9.24^{}\nc3\trefs/tags/v2026.9.7\nd4\trefs/tags/nightly\ne5\trefs/tags/v2026.10.1-rc1\n";
        assert_eq!(newest_release_tag(ls_remote).as_deref(), Some("v2026.9.24"));
        assert_eq!(newest_release_tag("").as_deref(), None);
    }

    #[test]
    fn restarts_once_per_crash_streak() {
        assert_eq!(
            on_crash(0, Duration::from_secs(3)),
            CrashResponse::Restart {
                after: Duration::from_secs(2)
            }
        );
        assert_eq!(on_crash(1, Duration::from_secs(3)), CrashResponse::GiveUp);
        assert_eq!(
            on_crash(1, Duration::from_secs(600)),
            CrashResponse::Restart {
                after: Duration::from_secs(2)
            }
        );
    }

    #[test]
    fn mcp_block_quotes_the_endpoint_and_token() {
        let yaml = hermes_mcp_yaml("http://127.0.0.1:5123/mcp", "t0k\"en");
        assert!(yaml.contains("url: \"http://127.0.0.1:5123/mcp\""));
        assert!(yaml.contains("Authorization: \"Bearer t0k\\\"en\""));
    }

    /// Runs Hermes on the person's real ChatGPT login the way FNDR does, makes
    /// one model call, then checks the Codex CLI is still signed in and that
    /// Hermes never rotated the shared login itself.
    /// `cargo test --lib live_hermes -- --ignored --nocapture`
    #[tokio::test]
    #[ignore = "live: uses the real ChatGPT login and a Hermes install"]
    async fn live_hermes_call_keeps_the_codex_cli_signed_in() {
        let hermes = std::env::var("FNDR_LIVE_HERMES")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                PathBuf::from(std::env::var("HOME").unwrap()).join(".local/bin/hermes")
            });
        let model = std::env::var("FNDR_LIVE_MODEL").unwrap_or_else(|_| "gpt-5.6-sol".to_string());
        let home = tempfile::tempdir().unwrap();
        let gateway_dir = tempfile::tempdir().unwrap();

        assert_eq!(
            sync_hermes_codex_login(home.path()).await,
            LoginState::SignedIn
        );
        let refreshed_before_call = codex_last_refresh(&codex_home_dir());
        std::fs::write(
            home.path().join("config.yaml"),
            format!("model:\n  provider: openai-codex\n  default: \"{model}\"\n"),
        )
        .unwrap();
        std::fs::write(
            home.path().join(".env"),
            "API_SERVER_ENABLED=true\nAPI_SERVER_HOST=127.0.0.1\nAPI_SERVER_PORT=8749\nAPI_SERVER_KEY=fndr-live-test-0123456789abcdef\nAPI_SERVER_MODEL_NAME=hermes-agent\n",
        )
        .unwrap();

        let mut gateway = tokio::process::Command::new(&hermes)
            .arg("gateway")
            .env("HERMES_HOME", home.path())
            .env("CODEX_HOME", codex_home_dir())
            .current_dir(gateway_dir.path())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .expect("hermes starts");
        let client = reqwest::Client::new();
        let mut ready = false;
        for _ in 0..120 {
            if client
                .get("http://127.0.0.1:8749/health")
                .send()
                .await
                .is_ok_and(|r| r.status().is_success())
            {
                ready = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        assert!(ready, "Hermes API came up");

        let response = client
            .post("http://127.0.0.1:8749/v1/responses")
            .bearer_auth("fndr-live-test-0123456789abcdef")
            .json(&json!({ "model": "hermes-agent", "input": "Reply with the single word ok.", "store": false }))
            .timeout(Duration::from_secs(180))
            .send()
            .await
            .expect("Hermes answers");
        let status = response.status();
        let body: Value = response.json().await.unwrap_or(Value::Null);
        let _ = gateway.kill().await;
        let text = body.to_string();
        println!(
            "hermes status {status}; reply excerpt: {}",
            &text[..text.len().min(300)]
        );
        assert!(status.is_success(), "Hermes call failed: {text}");

        let imported = read(&home.path().join("auth.json"));
        let access = imported["providers"]["openai-codex"]["tokens"]["access_token"]
            .as_str()
            .unwrap_or_default();
        assert!(!access.is_empty(), "Hermes imported the login itself");
        assert_eq!(
            codex_last_refresh(&codex_home_dir()),
            refreshed_before_call,
            "Hermes did not rotate the shared login"
        );

        let codex = ready_executable().unwrap();
        let mut server = AppServer::spawn_with(&codex, &[]).await.unwrap();
        let account = server
            .request("account/read", json!({ "refreshToken": false }))
            .await
            .unwrap();
        server.shutdown().await;
        assert_eq!(
            parse_account(&account).map(|a| a.kind).as_deref(),
            Some("chatgpt"),
            "Codex CLI still signed in"
        );
        let cli = std::process::Command::new(&codex)
            .args(["login", "status"])
            .output()
            .unwrap();
        println!(
            "codex login status: {}{}",
            String::from_utf8_lossy(&cli.stdout).trim(),
            String::from_utf8_lossy(&cli.stderr).trim()
        );
        assert!(cli.status.success(), "codex login status succeeds");
    }
}
