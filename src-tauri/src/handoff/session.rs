//! Running one hand-off turn on a `codex app-server` worker (Part 5).
//!
//! - `prepare` checks the plan's limit and that Codex reports the worker
//!   profile FNDR asked for, before any thread starts (PRD FR5, FR16).
//! - `drive` starts the thread and the turn and reads the one stream until
//!   the turn ends, a limit is reached or the person stops it (FR8, FR9).
//!
//! A turn has ended when the final answer arrived and the thread went idle
//! (spike S6: `turn/completed` never came on Codex 0.151.0).

use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::events::{map_message, JobEvent};
use super::profile::{check_effective, Mismatch, ProfileInputs};
use crate::inference::prompts::HAND_OFF_WORKER_SYSTEM;
use crate::ipc::commands::codex_account::{account_model, with_model, AppServer};

/// The limits of one turn (PRD FR8).
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub wall: Duration,
    pub idle: Duration,
    /// How long Stop waits for the thread to go idle before the process is killed.
    pub stop_grace: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            wall: Duration::from_secs(15 * 60),
            idle: Duration::from_secs(3 * 60),
            stop_grace: Duration::from_secs(5),
        }
    }
}

/// Why a turn ended.
#[derive(Debug, Clone, PartialEq)]
pub enum Ended {
    /// The final answer arrived and the thread went idle.
    Answered,
    /// The thread went idle with no final answer.
    NoAnswer,
    Stopped,
    WallTimeout,
    IdleTimeout,
    /// A plan window reached 100 percent; the turn was interrupted so no
    /// purchased credits are spent (FR16).
    PlanLimit,
    Failed(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    pub ended: Ended,
    /// The `final_answer` text: the manifest, still unchecked.
    pub answer: Option<String>,
    /// What reached the model: the first message, every command's output and
    /// every viewed image (S6). An upper bound; Codex may truncate output.
    pub bytes_sent: u64,
    pub tokens: u64,
    /// Server requests FNDR declined (sandbox escapes, approvals).
    pub declined: Vec<String>,
}

/// Why a job did not start.
#[derive(Debug, Clone, PartialEq)]
pub enum NotStarted {
    PlanFull { resets_at: Option<i64> },
    Profile(Vec<Mismatch>),
    Codex(String),
}

/// Before a thread starts: the plan has room, and Codex reports exactly the
/// worker profile `inputs` describes.
pub(crate) async fn prepare(
    server: &mut AppServer,
    inputs: &ProfileInputs,
) -> Result<(), NotStarted> {
    let limits = server
        .request("account/rateLimits/read", Value::Null)
        .await
        .map_err(NotStarted::Codex)?;
    let limits = &limits["rateLimits"];
    if plan_is_full(limits) {
        let resets_at = ["primary", "secondary"]
            .iter()
            .filter(|w| limits[**w]["usedPercent"].as_f64().unwrap_or(0.0) >= 100.0)
            .filter_map(|w| limits[*w]["resetsAt"].as_i64())
            .max();
        return Err(NotStarted::PlanFull { resets_at });
    }
    let cwd = inputs.job_folder.display().to_string();
    let config = server
        .request("config/read", json!({ "cwd": cwd }))
        .await
        .map_err(NotStarted::Codex)?;
    let skills = server
        .request("skills/list", json!({ "cwds": [cwd], "forceReload": true }))
        .await
        .map_err(NotStarted::Codex)?;
    check_effective(inputs, &config, &skills).map_err(NotStarted::Profile)
}

fn plan_is_full(limits: &Value) -> bool {
    !limits["rateLimitReachedType"].is_null()
        || ["primary", "secondary"]
            .iter()
            .any(|w| limits[*w]["usedPercent"].as_f64().unwrap_or(0.0) >= 100.0)
}

/// What `drive` needs besides the server.
#[derive(Debug, Clone)]
pub struct Turn {
    pub job_folder: PathBuf,
    /// The reviewed draft, sent as the first message unchanged.
    pub rendered: String,
    pub output_schema: Value,
    pub limits: Limits,
}

/// Starts the thread and the turn, then reads until the turn ends. Every
/// event goes to `on_event` as it arrives. `should_stop` is checked at
/// least twice a second: the person's Stop, or Private Mode turning on.
pub(crate) async fn drive(
    server: &mut AppServer,
    turn: &Turn,
    should_stop: impl Fn() -> bool,
    mut on_event: impl FnMut(&JobEvent),
) -> Outcome {
    let mut outcome = Outcome {
        ended: Ended::Failed(String::new()),
        answer: None,
        bytes_sent: turn.rendered.len() as u64,
        tokens: 0,
        declined: Vec::new(),
    };
    let model = account_model(server).await;
    let thread = server
        .request(
            "thread/start",
            with_model(
                json!({
                    "cwd": turn.job_folder,
                    "ephemeral": true,
                    "approvalPolicy": "never",
                    "developerInstructions": HAND_OFF_WORKER_SYSTEM,
                    "serviceName": "fndr_hand_off",
                }),
                model.as_deref(),
            ),
        )
        .await;
    let thread_id = match thread.map(|t| {
        t.pointer("/thread/id")
            .and_then(Value::as_str)
            .map(str::to_string)
    }) {
        Ok(Some(id)) => id,
        Ok(None) => return failed(outcome, "Codex did not start a thread."),
        Err(error) => return failed(outcome, &error),
    };
    let input = json!([{ "type": "text", "text": turn.rendered, "text_elements": [] }]);
    let turn_request = match server
        .send(
            "turn/start",
            json!({
                "threadId": thread_id,
                "input": input,
                "outputSchema": turn.output_schema,
                "approvalPolicy": "never",
            }),
        )
        .await
    {
        Ok(id) => id,
        Err(error) => return failed(outcome, &error),
    };

    let started = Instant::now();
    let mut last_event = Instant::now();
    let mut turn_id: Option<String> = None;
    let mut stopping: Option<(Instant, Ended)> = None;
    loop {
        if stopping.is_none() && should_stop() {
            stopping = Some((Instant::now(), Ended::Stopped));
            interrupt(server, &thread_id, turn_id.as_deref()).await;
        }
        if stopping.is_none() && started.elapsed() >= turn.limits.wall {
            stopping = Some((Instant::now(), Ended::WallTimeout));
            interrupt(server, &thread_id, turn_id.as_deref()).await;
        }
        if stopping.is_none() && last_event.elapsed() >= turn.limits.idle {
            stopping = Some((Instant::now(), Ended::IdleTimeout));
            interrupt(server, &thread_id, turn_id.as_deref()).await;
        }
        if let Some((since, why)) = &stopping {
            if since.elapsed() >= turn.limits.stop_grace {
                outcome.ended = why.clone();
                return outcome;
            }
        }
        // Wake at least every half second to see Stop and the limits.
        let message =
            match tokio::time::timeout(Duration::from_millis(500), server.read_raw()).await {
                Err(_) => continue,
                Ok(Err(error)) => {
                    return match stopping {
                        Some((_, why)) => Outcome {
                            ended: why,
                            ..outcome
                        },
                        None => failed(outcome, &error),
                    }
                }
                Ok(Ok(message)) => message,
            };
        last_event = Instant::now();

        if let (Some(id), Some(method)) = (message.get("id").cloned(), message.get("method")) {
            // Approvals, sandbox escapes, elicitations: never granted.
            outcome
                .declined
                .push(method.as_str().unwrap_or_default().to_string());
            let _ = server
                .write(json!({ "id": id, "error": { "code": -32000, "message": "FNDR does not grant this request." } }))
                .await;
            continue;
        }
        if message.get("id").and_then(Value::as_u64) == Some(turn_request) {
            match message.get("error") {
                Some(error) => {
                    let detail = error["message"]
                        .as_str()
                        .unwrap_or("Codex refused the turn.");
                    return failed(outcome, detail);
                }
                None => {
                    turn_id = message
                        .pointer("/result/turn/id")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                }
            }
            continue;
        }
        if message["method"] == "item/completed" {
            outcome.bytes_sent += sent_by_item(&message["params"]["item"]);
        }
        if message["method"] == "turn/started" && turn_id.is_none() {
            turn_id = message
                .pointer("/params/turn/id")
                .and_then(Value::as_str)
                .map(str::to_string);
        }
        for event in map_message(&message, &turn.job_folder) {
            on_event(&event);
            match event {
                JobEvent::Answered { text } => outcome.answer = Some(text),
                JobEvent::Tokens { total } => outcome.tokens = total,
                JobEvent::RateLimits { primary, secondary }
                    if primary >= 100.0 || secondary >= 100.0 =>
                {
                    if stopping.is_none() {
                        stopping = Some((Instant::now(), Ended::PlanLimit));
                        interrupt(server, &thread_id, turn_id.as_deref()).await;
                    }
                }
                JobEvent::Idle => {
                    outcome.ended = match (&stopping, &outcome.answer) {
                        (Some((_, why)), _) => why.clone(),
                        (None, Some(_)) => Ended::Answered,
                        (None, None) => Ended::NoAnswer,
                    };
                    return outcome;
                }
                _ => {}
            }
        }
    }
}

/// Bytes an item put in front of the model: a command's output, or the image
/// it viewed.
fn sent_by_item(item: &Value) -> u64 {
    match item["type"].as_str() {
        Some("commandExecution") => item["aggregatedOutput"]
            .as_str()
            .map_or(0, |o| o.len() as u64),
        Some("imageView") => item["path"]
            .as_str()
            .and_then(|path| std::fs::metadata(path).ok())
            .map_or(0, |meta| meta.len()),
        _ => 0,
    }
}

async fn interrupt(server: &mut AppServer, thread_id: &str, turn_id: Option<&str>) {
    let _ = server
        .send(
            "turn/interrupt",
            json!({ "threadId": thread_id, "turnId": turn_id }),
        )
        .await;
}

fn failed(outcome: Outcome, why: &str) -> Outcome {
    Outcome {
        ended: Ended::Failed(why.to_string()),
        ..outcome
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    const ROOT: &str = "/Users/sam/Documents/FNDR Hand-offs/2026-10-16 Q4 onboarding readout";

    /// A stand-in `codex app-server`: answers the setup requests, and on
    /// `turn/start` plays a scene. `replay` sends the S6 stream; `hang` sends
    /// nothing until `turn/interrupt`, then goes idle; `escape` asks for an
    /// approval and then answers; `full` reports a full plan mid-turn; `quiet`
    /// goes idle with no answer.
    fn fake_codex(dir: &Path, scene: &str) -> PathBuf {
        let fixture = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/hand-off/s6-turn.jsonl"
        );
        let script = format!(
            r#"#!/usr/bin/env python3
import json, sys
SCENE = {scene:?}
FIXTURE = {fixture:?}
def out(m): print(json.dumps(m), flush=True)
def idle(): out({{"method": "thread/status/changed", "params": {{"threadId": "th", "status": {{"type": "idle"}}}}}})
for line in sys.stdin:
    m = json.loads(line)
    method, mid = m.get("method"), m.get("id")
    if mid is None:
        continue
    if method == "model/list":
        out({{"id": mid, "result": {{"data": [{{"model": "gpt-test", "isDefault": True}}]}}}})
    elif method == "thread/start":
        out({{"id": mid, "result": {{"thread": {{"id": "th"}}}}}})
    elif method == "turn/start":
        out({{"id": mid, "result": {{"turn": {{"id": "tu"}}}}}})
        if SCENE == "replay":
            for row in open(FIXTURE):
                print(row.strip(), flush=True)
        elif SCENE == "escape":
            out({{"id": 900, "method": "item/commandExecution/requestApproval", "params": {{}}}})
            out({{"method": "item/completed", "params": {{"item": {{"type": "agentMessage", "phase": "final_answer", "text": "{{}}"}}}}}})
            idle()
        elif SCENE == "full":
            out({{"method": "account/rateLimits/updated", "params": {{"rateLimits": {{"primary": {{"usedPercent": 100}}, "secondary": {{"usedPercent": 80}}}}}}}})
        elif SCENE == "quiet":
            idle()
    elif method == "turn/interrupt":
        out({{"id": mid, "result": {{}}}})
        if SCENE != "deaf":
            idle()
    else:
        out({{"id": mid, "result": {{}}}})
"#
        );
        let path = dir.join("codex");
        std::fs::write(&path, script).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        path
    }

    fn turn(limits: Limits) -> Turn {
        Turn {
            job_folder: PathBuf::from(ROOT),
            rendered: "TASK\n\"x\"".into(),
            output_schema: super::super::manifest::output_schema(),
            limits,
        }
    }

    async fn run(
        scene: &str,
        limits: Limits,
        stop_after: Option<Duration>,
    ) -> (Outcome, Vec<JobEvent>) {
        let dir = tempfile::tempdir().unwrap();
        let mut server = AppServer::spawn_with(&fake_codex(dir.path(), scene), &[])
            .await
            .unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        if let Some(after) = stop_after {
            let flag = Arc::clone(&stop);
            tokio::spawn(async move {
                tokio::time::sleep(after).await;
                flag.store(true, Ordering::SeqCst);
            });
        }
        let mut events = Vec::new();
        let outcome = drive(
            &mut server,
            &turn(limits),
            || stop.load(Ordering::SeqCst),
            |e| events.push(e.clone()),
        )
        .await;
        server.shutdown().await;
        (outcome, events)
    }

    #[tokio::test]
    async fn the_s6_stream_ends_answered_with_the_manifest() {
        let (outcome, events) = run("replay", Limits::default(), None).await;
        assert_eq!(outcome.ended, Ended::Answered);
        let manifest = super::super::manifest::parse(outcome.answer.as_deref().unwrap()).unwrap();
        assert_eq!(manifest.claims.len(), 4);
        assert!(outcome.tokens > 200_000);
        // The first message plus every command output in the stream; the chart
        // it viewed is not on this machine, so it adds nothing here.
        let fixture = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/hand-off/s6-turn.jsonl"
        );
        let output: u64 = std::fs::read_to_string(fixture)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .filter(|m| m["method"] == "item/completed")
            .map(|m| sent_by_item(&m["params"]["item"]))
            .sum();
        assert!(output > 10_000);
        assert_eq!(outcome.bytes_sent, "TASK\n\"x\"".len() as u64 + output);
        assert!(events.contains(&JobEvent::ReadFile {
            name: "q4-onboarding-survey.csv".into()
        }));
        assert!(outcome.declined.is_empty());
    }

    #[tokio::test]
    async fn stop_interrupts_and_ends_stopped() {
        let (outcome, _) = run("hang", Limits::default(), Some(Duration::from_millis(300))).await;
        assert_eq!(outcome.ended, Ended::Stopped);
        assert_eq!(outcome.answer, None);
    }

    #[tokio::test]
    async fn a_silent_worker_hits_the_idle_limit() {
        let limits = Limits {
            idle: Duration::from_millis(600),
            ..Limits::default()
        };
        let (outcome, _) = run("hang", limits, None).await;
        assert_eq!(outcome.ended, Ended::IdleTimeout);
    }

    #[tokio::test]
    async fn a_worker_that_never_goes_idle_is_left_after_the_grace() {
        let limits = Limits {
            wall: Duration::from_millis(400),
            stop_grace: Duration::from_millis(600),
            ..Limits::default()
        };
        let (outcome, _) = run("deaf", limits, None).await;
        assert_eq!(outcome.ended, Ended::WallTimeout);
    }

    #[tokio::test]
    async fn a_full_plan_interrupts_the_turn() {
        let (outcome, events) = run("full", Limits::default(), None).await;
        assert_eq!(outcome.ended, Ended::PlanLimit);
        assert!(events.contains(&JobEvent::RateLimits {
            primary: 100.0,
            secondary: 80.0
        }));
    }

    #[tokio::test]
    async fn requests_from_the_worker_are_declined_and_recorded() {
        let (outcome, _) = run("escape", Limits::default(), None).await;
        assert_eq!(outcome.declined, ["item/commandExecution/requestApproval"]);
        assert_eq!(outcome.ended, Ended::Answered);
    }

    #[tokio::test]
    async fn idle_with_no_answer_is_not_a_result() {
        let (outcome, _) = run("quiet", Limits::default(), None).await;
        assert_eq!(outcome.ended, Ended::NoAnswer);
    }

    #[test]
    fn a_plan_is_full_at_100_percent_or_when_codex_says_so() {
        assert!(plan_is_full(
            &json!({ "primary": { "usedPercent": 100 }, "secondary": { "usedPercent": 1 } })
        ));
        assert!(plan_is_full(&json!({ "rateLimitReachedType": "primary" })));
        assert!(!plan_is_full(
            &json!({ "primary": { "usedPercent": 99.5 }, "secondary": { "usedPercent": 78 }, "rateLimitReachedType": null })
        ));
    }
}
