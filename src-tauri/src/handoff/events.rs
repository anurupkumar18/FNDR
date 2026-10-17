//! What a hand-off worker did, read from the app-server's messages. Typed
//! events feed the runner (when the turn ended, what it answered), the trace
//! the person sees and the egress count, never the model's own account.
//!
//! Shapes measured on Codex 0.151.0 in spike S6
//! (`docs/evidence/W05/hand-off-spikes.md`, fixture
//! `tests/fixtures/hand-off/s6-turn.jsonl`):
//! - a turn ends with an `agentMessage` whose `phase` is `final_answer`, then
//!   the thread goes `idle`; `turn/completed` never arrived;
//! - `commentary` messages can be JSON-shaped like the answer;
//! - Python reading an input is labelled `unknown`, so reads are found by the
//!   command naming `inputs/<file>`.

use std::path::Path;

use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub enum JobEvent {
    /// The worker read an input file, by its name in `inputs/`.
    ReadFile {
        name: String,
    },
    /// A command other than a plain read ran.
    RanCode {
        ok: bool,
        ms: u64,
    },
    /// A file in the job folder was written, relative to the folder.
    WroteFile {
        path: String,
    },
    /// An image went to the model; the runner counts its bytes as egress.
    ViewedImage {
        path: String,
    },
    WebSearched {
        query: String,
        urls: Vec<String>,
    },
    /// The final answer: the manifest, as text.
    Answered {
        text: String,
    },
    /// The thread has nothing more to do this turn.
    Idle,
    /// Tokens used by the turn so far.
    Tokens {
        total: u64,
    },
    /// Percent used of the plan's windows.
    RateLimits {
        primary: f64,
        secondary: f64,
    },
}

/// Events in one app-server message. `job_root` makes paths relative.
pub fn map_message(message: &Value, job_root: &Path) -> Vec<JobEvent> {
    let params = &message["params"];
    match message.get("method").and_then(Value::as_str) {
        Some("item/completed") => map_item(&params["item"], job_root),
        Some("thread/status/changed") if params["status"]["type"] == "idle" => vec![JobEvent::Idle],
        Some("thread/tokenUsage/updated") => params
            .pointer("/tokenUsage/total/totalTokens")
            .and_then(Value::as_u64)
            .map(|total| vec![JobEvent::Tokens { total }])
            .unwrap_or_default(),
        Some("account/rateLimits/updated") => {
            let used = |window: &str| {
                params
                    .pointer(&format!("/rateLimits/{window}/usedPercent"))
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0)
            };
            vec![JobEvent::RateLimits {
                primary: used("primary"),
                secondary: used("secondary"),
            }]
        }
        _ => Vec::new(),
    }
}

fn map_item(item: &Value, job_root: &Path) -> Vec<JobEvent> {
    let text = |key: &str| item.get(key).and_then(Value::as_str).unwrap_or_default();
    match text("type") {
        "agentMessage" if text("phase") == "final_answer" => {
            vec![JobEvent::Answered {
                text: text("text").to_string(),
            }]
        }
        "commandExecution" => map_command(item),
        "fileChange" => item
            .get("changes")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|change| change.get("path").and_then(Value::as_str))
            .map(|path| JobEvent::WroteFile {
                path: relative(path, job_root),
            })
            .collect(),
        "imageView" => vec![JobEvent::ViewedImage {
            path: relative(text("path"), job_root),
        }],
        "webSearch" => {
            let urls = item
                .get("results")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|result| result.get("url").and_then(Value::as_str))
                .map(str::to_string)
                .collect();
            vec![JobEvent::WebSearched {
                query: text("query").to_string(),
                urls,
            }]
        }
        _ => Vec::new(),
    }
}

fn map_command(item: &Value) -> Vec<JobEvent> {
    let command = item
        .get("command")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let mut events: Vec<JobEvent> = input_names(command)
        .into_iter()
        .map(|name| JobEvent::ReadFile { name })
        .collect();
    let actions: Vec<&str> = item
        .get("commandActions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|action| action.get("type").and_then(Value::as_str))
        .collect();
    let only_looked = !actions.is_empty()
        && actions
            .iter()
            .all(|kind| matches!(*kind, "read" | "listFiles" | "search"));
    if !only_looked {
        let ok = item.get("status").and_then(Value::as_str) == Some("completed")
            && item.get("exitCode").and_then(Value::as_i64) == Some(0);
        let ms = item.get("durationMs").and_then(Value::as_u64).unwrap_or(0);
        events.push(JobEvent::RanCode { ok, ms });
    }
    events
}

/// Distinct `inputs/<name>` a command names, in order.
fn input_names(command: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for (start, _) in command.match_indices("inputs/") {
        let before = command[..start].chars().next_back().unwrap_or(' ');
        // A path segment, not `myinputs/` or `inputs/` inside another word.
        if before.is_alphanumeric() || before == '_' || before == '-' {
            continue;
        }
        let rest = &command[start + "inputs/".len()..];
        // A quoted path runs to its closing quote ("notes 2.md" is a name
        // the draft gives); an unquoted one ends at a space or punctuation.
        let name: String = if matches!(before, '\'' | '"') {
            rest.chars().take_while(|c| *c != before).collect()
        } else {
            rest.chars()
                .take_while(|c| {
                    !c.is_whitespace() && !matches!(c, '\'' | '"' | '\\' | ')' | ';' | ',')
                })
                .collect()
        };
        if !name.is_empty() && !name.contains('/') && !names.contains(&name) {
            names.push(name);
        }
    }
    names
}

fn relative(path: &str, job_root: &Path) -> String {
    Path::new(path)
        .strip_prefix(job_root)
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| path.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const ROOT: &str = "/Users/sam/Documents/FNDR Hand-offs/2026-10-16 Q4 onboarding readout";

    fn s6() -> Vec<JobEvent> {
        let fixture = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/hand-off/s6-turn.jsonl"
        );
        std::fs::read_to_string(fixture)
            .unwrap()
            .lines()
            .flat_map(|line| map_message(&serde_json::from_str(line).unwrap(), Path::new(ROOT)))
            .collect()
    }

    #[test]
    fn the_s6_turn_ends_with_one_answer_then_idle() {
        let events = s6();
        let answers: Vec<usize> = events
            .iter()
            .enumerate()
            .filter(|(_, e)| matches!(e, JobEvent::Answered { .. }))
            .map(|(i, _)| i)
            .collect();
        assert_eq!(answers.len(), 1, "commentary messages are not answers");
        assert_eq!(events.last(), Some(&JobEvent::Idle));
        let JobEvent::Answered { text } = &events[answers[0]] else {
            unreachable!()
        };
        let manifest: Value = serde_json::from_str(text).unwrap();
        assert_eq!(
            manifest["deliverables"][0]["path"],
            "out/q4-onboarding-survey-readout.docx"
        );
    }

    #[test]
    fn the_s6_turn_read_the_survey_ran_code_and_wrote_in_its_folder() {
        let events = s6();
        let reads: Vec<&JobEvent> = events
            .iter()
            .filter(|e| matches!(e, JobEvent::ReadFile { .. }))
            .collect();
        assert!(reads.contains(&&JobEvent::ReadFile {
            name: "q4-onboarding-survey.csv".into()
        }));
        assert!(events
            .iter()
            .all(|e| !matches!(e, JobEvent::RanCode { ok: false, .. })));
        assert_eq!(
            events
                .iter()
                .filter(|e| matches!(e, JobEvent::RanCode { .. }))
                .count(),
            7
        );
        let written: Vec<&str> = events
            .iter()
            .filter_map(|e| match e {
                JobEvent::WroteFile { path } => Some(path.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(written, ["work/readout-spec.json"; 3]);
        assert!(events.contains(&JobEvent::ViewedImage {
            path: "out/q4-onboarding-survey-readout-chart.png".into()
        }));
        assert!(events
            .iter()
            .any(|e| matches!(e, JobEvent::Tokens { total } if *total > 200_000)));
    }

    #[test]
    fn a_plain_read_is_not_code_and_a_failed_command_is_not_ok() {
        let read = json!({ "type": "commandExecution", "command": "sed -n 1,20p inputs/a.csv",
            "commandActions": [{ "type": "read" }], "status": "completed", "exitCode": 0 });
        assert_eq!(
            map_item(&read, Path::new(ROOT)),
            [JobEvent::ReadFile {
                name: "a.csv".into()
            }]
        );
        let failed = json!({ "type": "commandExecution", "command": "python3 x.py",
            "commandActions": [{ "type": "unknown" }], "status": "failed", "exitCode": 1, "durationMs": 40 });
        assert_eq!(
            map_item(&failed, Path::new(ROOT)),
            [JobEvent::RanCode { ok: false, ms: 40 }]
        );
    }

    #[test]
    fn input_names_are_whole_path_segments_listed_once() {
        assert_eq!(
            input_names(
                "pd.read_csv('inputs/q4.csv'); open(\"inputs/notes 2.md\"); x='inputs/q4.csv'"
            ),
            ["q4.csv", "notes 2.md"]
        );
        assert!(input_names("cat myinputs/a.csv old_inputs/b.csv").is_empty());
        assert!(input_names("ls inputs/").is_empty());
    }

    #[test]
    fn web_searches_keep_their_query_and_result_urls() {
        let item = json!({ "type": "webSearch", "query": "saas onboarding churn benchmark",
            "results": [{ "url": "https://example.com/a", "title": "A" }, { "title": "no url" }] });
        assert_eq!(
            map_item(&item, Path::new(ROOT)),
            [JobEvent::WebSearched {
                query: "saas onboarding churn benchmark".into(),
                urls: vec!["https://example.com/a".into()]
            }]
        );
    }

    #[test]
    fn rate_limits_and_unknown_messages() {
        let limits = json!({ "method": "account/rateLimits/updated",
            "params": { "rateLimits": { "primary": { "usedPercent": 100 }, "secondary": { "usedPercent": 78 } } } });
        assert_eq!(
            map_message(&limits, Path::new(ROOT)),
            [JobEvent::RateLimits {
                primary: 100.0,
                secondary: 78.0
            }]
        );
        let commentary = json!({ "method": "item/completed",
            "params": { "item": { "type": "agentMessage", "phase": "commentary", "text": "{}" } } });
        assert!(map_message(&commentary, Path::new(ROOT)).is_empty());
        assert!(map_message(&json!({ "method": "turn/diff/updated" }), Path::new(ROOT)).is_empty());
        let active = json!({ "method": "thread/status/changed", "params": { "status": { "type": "active" } } });
        assert!(map_message(&active, Path::new(ROOT)).is_empty());
    }
}
