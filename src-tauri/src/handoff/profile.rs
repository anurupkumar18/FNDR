//! The Codex configuration a hand-off worker runs under, and the check of what
//! Codex reports back (ADR 032 items 2 and 3, spikes S1 to S3).
//!
//! Measured on Codex 0.151.0 (`docs/evidence/W05/hand-off-spikes.md`):
//! - `mcp_servers={}` and `features.plugins=false` detach nothing: each of the
//!   person's servers, plugins and skills is switched off by name.
//! - Codex keeps the quotes of a quoted `-c` key as part of the name, so a name
//!   is written bare, and a name that cannot be written bare refuses the job.
//! - An ephemeral thread still logs its prompt under `~/.codex`, so Codex's
//!   state and logs for a job go to a folder FNDR owns.

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::ipc::commands::codex_account::{
    codex_knows_feature, is_plain_config_key, known_feature_args, READ_ONLY_DISABLED_FEATURES,
};

/// The worker's permission profile name in Codex's configuration.
pub const PROFILE: &str = "fndr_hand_off";

/// The worker runs code inside its sandbox; everything else that acts stays
/// off, the same list Screen Guide and voice use.
const WORKER_KEEPS: &[&str] = &["shell_tool", "unified_exec"];

/// Features switched off for every worker process.
pub fn worker_disabled_features() -> Vec<&'static str> {
    READ_ONLY_DISABLED_FEATURES
        .iter()
        .copied()
        .filter(|feature| !WORKER_KEEPS.contains(feature))
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebSearchMode {
    Disabled,
    Cached,
    Live,
}

impl WebSearchMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::Cached => "cached",
            Self::Live => "live",
        }
    }
}

/// Where one job runs.
#[derive(Debug, Clone)]
pub struct ProfileInputs {
    pub job_folder: PathBuf,
    pub toolchain: PathBuf,
    /// Codex's state and logs for this job, deleted when the job ends.
    pub codex_state: PathBuf,
    pub web_search: WebSearchMode,
}

/// What the person's own Codex setup would attach to a worker.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PersonCodexConfig {
    pub mcp_servers: Vec<String>,
    pub plugins: Vec<String>,
    /// User and system skills; never the job's own `.agents/skills`.
    pub skill_paths: Vec<PathBuf>,
}

impl PersonCodexConfig {
    /// Read from a throwaway app-server's `config/read` and `skills/list`.
    pub fn from_codex(config_read: &Value, skills_list: &Value, job_folder: &Path) -> Self {
        let config = config_of(config_read);
        let names = |key: &str| -> Vec<String> {
            let mut names: Vec<String> = config
                .get(key)
                .and_then(Value::as_object)
                .map(|table| table.keys().cloned().collect())
                .unwrap_or_default();
            names.sort();
            names
        };
        let mut skill_paths: Vec<PathBuf> = listed_skills(skills_list)
            .filter_map(|skill| skill.get("path").and_then(Value::as_str))
            .map(PathBuf::from)
            .filter(|path| !path.starts_with(job_folder))
            .collect();
        skill_paths.sort();
        skill_paths.dedup();
        Self {
            mcp_servers: names("mcp_servers"),
            plugins: names("plugins"),
            skill_paths,
        }
    }
}

/// Why a job cannot be configured.
#[derive(Debug, Clone, PartialEq)]
pub enum Refusal {
    /// A server, plugin or skill name Codex's `-c` keys cannot address.
    UnaddressableName { kind: &'static str, name: String },
}

/// `codex app-server` arguments for one job, in a stable order.
pub fn worker_config_args(
    inputs: &ProfileInputs,
    person: &PersonCodexConfig,
) -> Result<Vec<String>, Vec<Refusal>> {
    let refusals: Vec<Refusal> = person
        .mcp_servers
        .iter()
        .map(|name| ("MCP server", name))
        .chain(person.plugins.iter().map(|name| ("plugin", name)))
        .filter(|(_, name)| !is_plain_config_key(name))
        .map(|(kind, name)| Refusal::UnaddressableName {
            kind,
            name: name.clone(),
        })
        .collect();
    if !refusals.is_empty() {
        return Err(refusals);
    }

    let mut config = vec![
        format!("default_permissions={}", toml_string(PROFILE)),
        format!(
            "permissions.{PROFILE}.description={}",
            toml_string("FNDR hand-off job")
        ),
        format!(
            "permissions.{PROFILE}.filesystem={{{}=\"read\",{}=\"read\",{}=\"write\"}}",
            toml_string(":minimal"),
            toml_string(&inputs.toolchain.display().to_string()),
            toml_string(&inputs.job_folder.display().to_string()),
        ),
        format!("permissions.{PROFILE}.network={{enabled=false}}"),
        format!("web_search={}", toml_string(inputs.web_search.as_str())),
        format!("sqlite_home={}", toml_string(&state_dir(inputs, "sqlite"))),
        format!("log_dir={}", toml_string(&state_dir(inputs, "logs"))),
        format!("history.persistence={}", toml_string("none")),
    ];
    config.extend(
        person
            .mcp_servers
            .iter()
            .map(|name| format!("mcp_servers.{name}.enabled=false")),
    );
    config.extend(
        person
            .plugins
            .iter()
            .map(|name| format!("plugins.{name}.enabled=false")),
    );
    if !person.skill_paths.is_empty() {
        let entries: Vec<String> = person
            .skill_paths
            .iter()
            .map(|path| {
                format!(
                    "{{path={},enabled=false}}",
                    toml_string(&path.display().to_string())
                )
            })
            .collect();
        config.push(format!("skills.config=[{}]", entries.join(",")));
    }

    let mut args = Vec::new();
    // Profile and state first, then features, then the person's own setup,
    // so a reader of a process listing sees the sandbox first.
    let (head, tail) = config.split_at(8);
    for value in head {
        args.extend(["-c".to_string(), value.clone()]);
    }
    // Only features this Codex knows: it exits on an unknown `--disable`.
    args.extend(known_feature_args(&worker_disabled_features()));
    for value in tail {
        args.extend(["-c".to_string(), value.clone()]);
    }
    Ok(args)
}

/// A TOML basic string.
fn toml_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if c.is_control() => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn state_dir(inputs: &ProfileInputs, leaf: &str) -> String {
    inputs.codex_state.join(leaf).display().to_string()
}

fn config_of(config_read: &Value) -> &Value {
    config_read.get("config").unwrap_or(config_read)
}

fn listed_skills(skills_list: &Value) -> impl Iterator<Item = &Value> {
    skills_list
        .get("data")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.get("skills").and_then(Value::as_array))
        .flatten()
}

fn enabled(entry: &Value) -> bool {
    // An entry without `enabled` is on: that is Codex's default.
    entry
        .get("enabled")
        .and_then(Value::as_bool)
        .unwrap_or(true)
}

/// One way Codex's effective configuration differs from what FNDR asked for.
#[derive(Debug, Clone, PartialEq)]
pub enum Mismatch {
    DefaultProfile(Option<String>),
    ProfileMissing,
    Filesystem {
        path: String,
        expected: Option<&'static str>,
        found: Option<String>,
    },
    NetworkEnabled,
    WebSearch(Option<String>),
    FeatureOn(String),
    McpServerEnabled(String),
    PluginEnabled(String),
    SkillEnabled(String),
    StateNotIsolated(&'static str),
    HistoryKept,
}

/// Compare `config/read` and `skills/list` with what was asked; every
/// difference is named. The probe self-test, not this check, proves the
/// sandbox: Codex has reported a policy it did not enforce (S1, `/tmp`).
pub fn check_effective(
    inputs: &ProfileInputs,
    config_read: &Value,
    skills_list: &Value,
) -> Result<(), Vec<Mismatch>> {
    let config = config_of(config_read);
    let mut found = Vec::new();

    let default = config.get("default_permissions").and_then(Value::as_str);
    if default != Some(PROFILE) {
        found.push(Mismatch::DefaultProfile(default.map(str::to_string)));
    }
    match config.pointer(&format!("/permissions/{PROFILE}")) {
        None => found.push(Mismatch::ProfileMissing),
        Some(profile) => {
            check_filesystem(inputs, profile.get("filesystem"), &mut found);
            if profile.pointer("/network/enabled").and_then(Value::as_bool) != Some(false) {
                found.push(Mismatch::NetworkEnabled);
            }
        }
    }

    let web_search = config.get("web_search").and_then(Value::as_str);
    if web_search != Some(inputs.web_search.as_str()) {
        found.push(Mismatch::WebSearch(web_search.map(str::to_string)));
    }
    for feature in worker_disabled_features() {
        if codex_knows_feature(feature) == Some(false) {
            continue;
        }
        if config
            .pointer(&format!("/features/{feature}"))
            .and_then(Value::as_bool)
            != Some(false)
        {
            found.push(Mismatch::FeatureOn(feature.to_string()));
        }
    }
    for (key, mismatch) in [
        (
            "mcp_servers",
            Mismatch::McpServerEnabled as fn(String) -> Mismatch,
        ),
        ("plugins", Mismatch::PluginEnabled),
    ] {
        let mut on: Vec<&String> = config
            .get(key)
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
            .filter(|(_, entry)| enabled(entry))
            .map(|(name, _)| name)
            .collect();
        on.sort();
        found.extend(on.into_iter().map(|name| mismatch(name.clone())));
    }
    let own_skills = inputs.job_folder.join(".agents").join("skills");
    let mut skills_on: Vec<String> = listed_skills(skills_list)
        .filter(|skill| enabled(skill))
        .filter_map(|skill| skill.get("path").and_then(Value::as_str))
        .filter(|path| !Path::new(path).starts_with(&own_skills))
        .map(str::to_string)
        .collect();
    skills_on.sort();
    skills_on.dedup();
    found.extend(skills_on.into_iter().map(Mismatch::SkillEnabled));

    for (key, leaf) in [("sqlite_home", "sqlite"), ("log_dir", "logs")] {
        if config.get(key).and_then(Value::as_str) != Some(state_dir(inputs, leaf).as_str()) {
            found.push(Mismatch::StateNotIsolated(key));
        }
    }
    if config
        .pointer("/history/persistence")
        .and_then(Value::as_str)
        != Some("none")
    {
        found.push(Mismatch::HistoryKept);
    }

    if found.is_empty() {
        Ok(())
    } else {
        Err(found)
    }
}

fn check_filesystem(inputs: &ProfileInputs, filesystem: Option<&Value>, found: &mut Vec<Mismatch>) {
    let entries: Vec<(&String, &str)> = filesystem
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        // Settings such as `glob_scan_max_depth` share the table; only access
        // modes are entries.
        .filter_map(|(path, mode)| mode.as_str().map(|mode| (path, mode)))
        .collect();
    let expected = [
        (":minimal".to_string(), "read"),
        (inputs.toolchain.display().to_string(), "read"),
        (inputs.job_folder.display().to_string(), "write"),
    ];
    for (path, mode) in &expected {
        let actual = entries.iter().find(|(p, _)| *p == path).map(|(_, m)| *m);
        if actual != Some(*mode) {
            found.push(Mismatch::Filesystem {
                path: path.clone(),
                expected: Some(mode),
                found: actual.map(str::to_string),
            });
        }
    }
    let mut extra: Vec<&(&String, &str)> = entries
        .iter()
        .filter(|(path, _)| !expected.iter().any(|(p, _)| p == *path))
        .collect();
    extra.sort();
    for (path, mode) in extra {
        found.push(Mismatch::Filesystem {
            path: (*path).clone(),
            expected: None,
            found: Some((*mode).to_string()),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn inputs() -> ProfileInputs {
        ProfileInputs {
            job_folder: PathBuf::from("/Users/sam/Documents/FNDR Hand-offs/2026-10-16 Q4 readout"),
            toolchain: PathBuf::from(
                "/Users/sam/Library/Application Support/com.fndr.app/hand-off/toolchain",
            ),
            codex_state: PathBuf::from(
                "/Users/sam/Library/Application Support/com.fndr.app/hand-off/jobs/j1/codex",
            ),
            web_search: WebSearchMode::Cached,
        }
    }

    fn person() -> PersonCodexConfig {
        PersonCodexConfig {
            mcp_servers: vec!["computer-use".into(), "notion".into()],
            plugins: vec![
                "browser@openai-bundled".into(),
                "hugging-face@openai-curated".into(),
            ],
            skill_paths: vec![PathBuf::from("/Users/sam/.codex/skills/sentry/SKILL.md")],
        }
    }

    /// `config/read` as Codex 0.151.0 shapes it, for a worker started with
    /// `worker_config_args(inputs(), person())`.
    fn effective() -> Value {
        let i = inputs();
        let mut filesystem = serde_json::Map::new();
        filesystem.insert("glob_scan_max_depth".into(), Value::Null);
        filesystem.insert(":minimal".into(), json!("read"));
        filesystem.insert(i.toolchain.display().to_string(), json!("read"));
        filesystem.insert(i.job_folder.display().to_string(), json!("write"));
        let mut features = serde_json::Map::new();
        for f in worker_disabled_features() {
            features.insert(f.into(), json!(false));
        }
        features.insert("mentions_v2".into(), json!(true));
        json!({ "config": {
            "default_permissions": PROFILE,
            "permissions": { PROFILE: {
                "description": "FNDR hand-off job",
                "filesystem": Value::Object(filesystem),
                "network": { "enabled": false, "mode": null }
            }},
            "web_search": "cached",
            "features": Value::Object(features),
            "mcp_servers": {
                "computer-use": { "command": "x", "enabled": false },
                "notion": { "url": "https://mcp.notion.com/mcp", "enabled": false }
            },
            "plugins": {
                "browser@openai-bundled": { "enabled": false },
                "hugging-face@openai-curated": { "enabled": false }
            },
            "sqlite_home": i.codex_state.join("sqlite").display().to_string(),
            "log_dir": i.codex_state.join("logs").display().to_string(),
            "history": { "persistence": "none", "max_bytes": null }
        }})
    }

    fn skills(job_skill_enabled: bool, person_skill_enabled: bool) -> Value {
        let i = inputs();
        json!({ "data": [{ "cwd": i.job_folder.display().to_string(), "errors": [], "skills": [
            { "name": "readout-docx", "enabled": job_skill_enabled, "scope": "repo",
              "path": i.job_folder.join(".agents/skills/readout-docx/SKILL.md").display().to_string() },
            { "name": "sentry", "enabled": person_skill_enabled, "scope": "user",
              "path": "/Users/sam/.codex/skills/sentry/SKILL.md" }
        ]}]})
    }

    fn value_of<'a>(args: &'a [String], key_prefix: &str) -> Vec<&'a str> {
        args.windows(2)
            .filter(|w| w[0] == "-c" && w[1].starts_with(key_prefix))
            .map(|w| w[1].as_str())
            .collect()
    }

    #[test]
    fn args_name_the_profile_folder_toolchain_and_network() {
        let args = worker_config_args(&inputs(), &person()).unwrap();
        assert_eq!(
            value_of(&args, "default_permissions="),
            [r#"default_permissions="fndr_hand_off""#]
        );
        let fs = value_of(&args, "permissions.fndr_hand_off.filesystem=");
        assert_eq!(
            fs,
            [concat!(
                r#"permissions.fndr_hand_off.filesystem={":minimal"="read","#,
                r#""/Users/sam/Library/Application Support/com.fndr.app/hand-off/toolchain"="read","#,
                r#""/Users/sam/Documents/FNDR Hand-offs/2026-10-16 Q4 readout"="write"}"#
            )]
        );
        assert_eq!(
            value_of(&args, "permissions.fndr_hand_off.network="),
            ["permissions.fndr_hand_off.network={enabled=false}"]
        );
        assert_eq!(value_of(&args, "web_search="), [r#"web_search="cached""#]);
    }

    #[test]
    fn args_switch_off_every_feature() {
        let args = worker_config_args(&inputs(), &person()).unwrap();
        let off: Vec<&str> = args
            .windows(2)
            .filter(|w| w[0] == "--disable")
            .map(|w| w[1].as_str())
            .collect();
        assert_eq!(off, worker_disabled_features());
        assert!(
            !off.contains(&"shell_tool"),
            "the worker runs code in its sandbox"
        );
        assert!(off.contains(&"goals") && off.contains(&"plugins"));
    }

    #[test]
    fn args_detach_each_server_plugin_and_skill_by_bare_name() {
        let args = worker_config_args(&inputs(), &person()).unwrap();
        assert_eq!(
            value_of(&args, "mcp_servers."),
            [
                "mcp_servers.computer-use.enabled=false",
                "mcp_servers.notion.enabled=false"
            ]
        );
        assert_eq!(
            value_of(&args, "plugins."),
            [
                "plugins.browser@openai-bundled.enabled=false",
                "plugins.hugging-face@openai-curated.enabled=false"
            ]
        );
        assert_eq!(
            value_of(&args, "skills.config="),
            [r#"skills.config=[{path="/Users/sam/.codex/skills/sentry/SKILL.md",enabled=false}]"#]
        );
        assert!(
            !args.iter().any(|a| a == "mcp_servers={}"),
            "an empty table detaches nothing (S1)"
        );
    }

    #[test]
    fn args_isolate_codex_state_and_keep_no_history() {
        let args = worker_config_args(&inputs(), &person()).unwrap();
        assert_eq!(
            value_of(&args, "sqlite_home="),
            [
                r#"sqlite_home="/Users/sam/Library/Application Support/com.fndr.app/hand-off/jobs/j1/codex/sqlite""#
            ]
        );
        assert_eq!(
            value_of(&args, "log_dir="),
            [
                r#"log_dir="/Users/sam/Library/Application Support/com.fndr.app/hand-off/jobs/j1/codex/logs""#
            ]
        );
        assert_eq!(
            value_of(&args, "history.persistence="),
            [r#"history.persistence="none""#]
        );
    }

    #[test]
    fn args_are_stable_for_the_same_input() {
        assert_eq!(
            worker_config_args(&inputs(), &person()).unwrap(),
            worker_config_args(&inputs(), &person()).unwrap()
        );
    }

    #[test]
    fn paths_with_quotes_and_backslashes_are_escaped_as_toml() {
        let mut i = inputs();
        i.job_folder = PathBuf::from(r#"/Users/sam/Documents/FNDR Hand-offs/a "quoted" \ name"#);
        let args = worker_config_args(&i, &PersonCodexConfig::default()).unwrap();
        let fs = value_of(&args, "permissions.fndr_hand_off.filesystem=")[0];
        assert!(
            fs.contains(r#""/Users/sam/Documents/FNDR Hand-offs/a \"quoted\" \\ name"="write""#),
            "{fs}"
        );
    }

    #[test]
    fn a_name_codex_cannot_address_refuses_the_job() {
        let mut p = person();
        p.mcp_servers.push("team.tools".into());
        p.plugins.push("has space@x".into());
        p.plugins.push("quote\"d".into());
        let refused = worker_config_args(&inputs(), &p).unwrap_err();
        assert_eq!(
            refused,
            [
                Refusal::UnaddressableName {
                    kind: "MCP server",
                    name: "team.tools".into()
                },
                Refusal::UnaddressableName {
                    kind: "plugin",
                    name: "has space@x".into()
                },
                Refusal::UnaddressableName {
                    kind: "plugin",
                    name: "quote\"d".into()
                },
            ]
        );
    }

    #[test]
    fn person_config_is_read_from_codex_and_skips_the_jobs_own_skills() {
        let found =
            PersonCodexConfig::from_codex(&effective(), &skills(true, true), &inputs().job_folder);
        assert_eq!(found.mcp_servers, ["computer-use", "notion"]);
        assert_eq!(
            found.plugins,
            ["browser@openai-bundled", "hugging-face@openai-curated"]
        );
        assert_eq!(
            found.skill_paths,
            [PathBuf::from("/Users/sam/.codex/skills/sentry/SKILL.md")]
        );
    }

    #[test]
    fn the_asked_configuration_passes() {
        assert_eq!(
            check_effective(&inputs(), &effective(), &skills(true, false)),
            Ok(())
        );
    }

    #[test]
    fn every_difference_is_named() {
        let mut c = effective();
        let cfg = c["config"].as_object_mut().unwrap();
        cfg["permissions"][PROFILE]["network"]["enabled"] = json!(true);
        cfg["permissions"][PROFILE]["filesystem"]["/Users/sam"] = json!("read");
        cfg["web_search"] = json!("live");
        cfg["features"]["computer_use"] = json!(true);
        cfg["mcp_servers"]["notion"]["enabled"] = json!(true);
        cfg["plugins"]["browser@openai-bundled"]["enabled"] = json!(true);
        cfg["sqlite_home"] = Value::Null;
        cfg["history"]["persistence"] = json!("save-all");
        let found = check_effective(&inputs(), &c, &skills(true, true)).unwrap_err();
        assert_eq!(
            found,
            [
                Mismatch::Filesystem {
                    path: "/Users/sam".into(),
                    expected: None,
                    found: Some("read".into())
                },
                Mismatch::NetworkEnabled,
                Mismatch::WebSearch(Some("live".into())),
                Mismatch::FeatureOn("computer_use".into()),
                Mismatch::McpServerEnabled("notion".into()),
                Mismatch::PluginEnabled("browser@openai-bundled".into()),
                Mismatch::SkillEnabled("/Users/sam/.codex/skills/sentry/SKILL.md".into()),
                Mismatch::StateNotIsolated("sqlite_home"),
                Mismatch::HistoryKept,
            ]
        );
    }

    #[test]
    fn a_missing_profile_or_another_default_is_named() {
        let mut c = effective();
        c["config"]["default_permissions"] = json!(":workspace");
        c["config"]["permissions"] = json!({});
        let found = check_effective(&inputs(), &c, &skills(true, false)).unwrap_err();
        assert_eq!(
            found,
            [
                Mismatch::DefaultProfile(Some(":workspace".into())),
                Mismatch::ProfileMissing
            ]
        );
    }

    #[test]
    fn a_missing_job_folder_or_toolchain_entry_is_named() {
        let mut c = effective();
        let job = inputs().job_folder.display().to_string();
        c["config"]["permissions"][PROFILE]["filesystem"]
            .as_object_mut()
            .unwrap()
            .remove(&job);
        let found = check_effective(&inputs(), &c, &skills(true, false)).unwrap_err();
        assert_eq!(
            found,
            [Mismatch::Filesystem {
                path: job,
                expected: Some("write"),
                found: None
            }]
        );
    }

    /// Starts the installed Codex twice: once as the person's own setup, to
    /// learn what it would attach, then as a worker with `worker_config_args`,
    /// and checks what that worker reports. No model turn, no plan usage.
    #[tokio::test]
    #[ignore = "Requires an installed Codex CLI; run: cargo test --lib live_worker_profile_matches_what_codex_reports -- --ignored --nocapture"]
    async fn live_worker_profile_matches_what_codex_reports() {
        use crate::ipc::commands::codex_account::{
            ready_executable, refresh_codex_features, AppServer,
        };

        let codex = ready_executable().expect("codex on PATH");
        refresh_codex_features(&codex).await.unwrap();
        let base = tempfile::tempdir().unwrap();
        let job = base.path().join("job");
        std::fs::create_dir_all(job.join(".agents/skills/readout-docx")).unwrap();
        std::fs::write(
            job.join(".agents/skills/readout-docx/SKILL.md"),
            "---\nname: readout-docx\ndescription: Render a readout.\n---\nRender it.\n",
        )
        .unwrap();
        let inputs = ProfileInputs {
            job_folder: job.clone(),
            toolchain: base.path().join("toolchain"),
            codex_state: base.path().join("codex"),
            web_search: WebSearchMode::Cached,
        };
        let read = |args: Vec<String>| {
            let codex = codex.clone();
            let job = job.clone();
            async move {
                let mut server = AppServer::spawn_with(&codex, &args).await.unwrap();
                let config = server
                    .request("config/read", json!({ "cwd": job }))
                    .await
                    .unwrap();
                let skills = server
                    .request("skills/list", json!({ "cwds": [job], "forceReload": true }))
                    .await
                    .unwrap();
                server.shutdown().await;
                (config, skills)
            }
        };

        let (config, skills) = read(Vec::new()).await;
        let person = PersonCodexConfig::from_codex(&config, &skills, &job);
        eprintln!(
            "person: {} servers, {} plugins, {} skills",
            person.mcp_servers.len(),
            person.plugins.len(),
            person.skill_paths.len()
        );
        let args = worker_config_args(&inputs, &person).unwrap();
        let (config, skills) = read(args).await;
        assert_eq!(check_effective(&inputs, &config, &skills), Ok(()));
    }
}
