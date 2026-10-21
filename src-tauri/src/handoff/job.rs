//! One hand-off from a reviewed draft to a checked result (ADR 032): the
//! draft is rebuilt and compared, the folder made, the worker configured and
//! checked, the turn run, the result verified and the record kept. Nothing
//! leaves the Mac before `prepare` has confirmed the plan and the profile.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde_json::json;

use super::draft::{self, DraftItemKind, OutputGoal, Selection, TaskSeed};
use super::events::JobEvent;
use super::folder::{self, Caps, JobFolder};
use super::manifest;
use super::profile::{worker_config_args, PersonCodexConfig, ProfileInputs, WebSearchMode};
use super::session::{self, Ended, Limits, NotStarted, Turn};
use super::store::{self, JobRecord, JobStatus};
use super::verify::{self, ResultStatus};
use crate::ipc::commands::codex_account::{ready_executable, refresh_codex_features, AppServer};
use crate::privacy_proof::{record_model_request_including, Feature};
use crate::AppState;

/// The folder hand-offs are made in (PRD open question 1, default).
pub fn hand_offs_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join("Documents")
        .join("FNDR Hand-offs")
}

/// FNDR's own Python and document libraries for workers (Part 7).
pub fn toolchain_dir(state: &AppState) -> PathBuf {
    state.app_data_dir.join("hand-off").join("toolchain")
}

/// The TOOLS section of the first message.
pub fn tools_line(toolchain: &Path) -> String {
    format!(
        "Python 3.12 with pandas, matplotlib, python-docx, openpyxl, python-pptx and pypdf: {}",
        toolchain.join("venv").join("bin").join("python").display()
    )
}

/// What the person reviewed and tapped Hand off on.
#[derive(Debug, Clone)]
pub struct JobRequest {
    pub seed: TaskSeed,
    pub goal: OutputGoal,
    pub selection: Selection,
    /// The draft's `rendered` text as the card showed it.
    pub reviewed: String,
}

/// Runs one hand-off to the end. `stop` is the person's Stop; Private Mode
/// turning on sets it too. Returns the final record.
pub async fn start_job(
    state: &AppState,
    request: JobRequest,
    stop: Arc<AtomicBool>,
    on_event: impl FnMut(&JobEvent),
) -> Result<JobRecord, String> {
    let codex = ready_executable()?;
    start_job_with(
        state,
        &codex,
        &hand_offs_dir(),
        request,
        stop,
        Limits::default(),
        on_event,
    )
    .await
}

pub(crate) async fn start_job_with(
    state: &AppState,
    codex: &Path,
    base: &Path,
    request: JobRequest,
    stop: Arc<AtomicBool>,
    limits: Limits,
    mut on_event: impl FnMut(&JobEvent),
) -> Result<JobRecord, String> {
    let toolchain = toolchain_dir(state);
    let tools = tools_line(&toolchain);
    let draft = draft::recheck(
        state,
        &request.seed,
        request.goal,
        &request.selection,
        &tools,
    )
    .await?;
    if draft.rendered != request.reviewed {
        return Err(
            "The draft changed since it was shown. Review it again before handing it off.".into(),
        );
    }

    let folder = folder::create(base, &request.seed.text, chrono::Local::now().date_naive())
        .map_err(|e| format!("FNDR could not make the job folder: {e}"))?;
    // FNDR's own skills, so formatting comes from tested code (Part 7).
    if let Some(skills) = crate::speech::resolve_sidecar("hand-off/skills") {
        copy_dir(&skills, &folder.root.join(".agents").join("skills"))
            .map_err(|e| format!("FNDR could not add its skills to the job folder: {e}"))?;
    }
    for item in draft.ticked(DraftItemKind::File) {
        let path = item
            .path
            .as_deref()
            .ok_or("A file on the draft has no path.")?;
        folder::copy_input(&folder, path, &Caps::default())
            .map_err(|refusal| format!("{} was not attached: {refusal}.", item.label))?;
    }

    let ids = |kind| {
        draft
            .ticked(kind)
            .map(|i| i.memory_id.clone())
            .collect::<Vec<_>>()
    };
    let job = store::begin(
        &state.state_store,
        JobRecord::starting(
            &request.seed.text,
            request.goal,
            ids(DraftItemKind::Memory),
            ids(DraftItemKind::File),
            &folder.root.display().to_string(),
            chrono::Utc::now().timestamp_millis(),
        ),
    )?;
    let codex_state = state
        .app_data_dir
        .join("hand-off")
        .join("jobs")
        .join(&job.id);
    let inputs = ProfileInputs {
        job_folder: folder.root.clone(),
        toolchain,
        codex_state: codex_state.clone(),
        web_search: WebSearchMode::Cached,
    };

    let result = run(
        state,
        codex,
        &inputs,
        &folder,
        &draft,
        &job,
        stop,
        limits,
        &mut on_event,
    )
    .await;
    // Codex's state for the job held the prompt (spike S2); it goes with the job.
    let _ = std::fs::remove_dir_all(&codex_state);

    let (status, reason, outcome, verdict) = match result {
        Ok((outcome, verdict)) => {
            let (status, reason) = settle(&outcome.ended, verdict.as_ref());
            (status, reason, Some(outcome), verdict)
        }
        Err(reason) => (JobStatus::Failed, Some(reason), None, None),
    };
    store::update(&state.state_store, &job.id, |record| {
        record.status = status;
        record.reason = reason;
        record.finished_at_ms = Some(chrono::Utc::now().timestamp_millis());
        if let Some(outcome) = &outcome {
            record.bytes_sent = outcome.bytes_sent;
        }
        if let Some(verdict) = &verdict {
            record.checked = verdict.checked;
            record.unsupported = verdict.unsupported;
            record.not_checked = verdict.not_checked;
            record.deliverables = verdict
                .deliverables
                .iter()
                .filter(|f| matches!(f.check, verify::FileCheck::Ok { .. }))
                .map(|f| f.path.clone())
                .collect();
        }
    })
}

/// Copies a folder of plain files, without following links.
fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let target = to.join(entry.file_name());
        if kind.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else if kind.is_file() {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn run(
    state: &AppState,
    codex: &Path,
    inputs: &ProfileInputs,
    folder: &JobFolder,
    draft: &draft::HandOffDraft,
    job: &JobRecord,
    stop: Arc<AtomicBool>,
    limits: Limits,
    on_event: &mut impl FnMut(&JobEvent),
) -> Result<(session::Outcome, Option<verify::Verdict>), String> {
    refresh_codex_features(codex).await?;
    let person = {
        let mut probe = AppServer::spawn_with(codex, &[]).await?;
        let cwd = folder.root.display().to_string();
        let config = probe.request("config/read", json!({ "cwd": cwd })).await;
        let skills = probe
            .request("skills/list", json!({ "cwds": [cwd], "forceReload": true }))
            .await;
        probe.shutdown().await;
        PersonCodexConfig::from_codex(&config?, &skills?, &folder.root)
    };
    let args = worker_config_args(inputs, &person).map_err(|refused| {
        format!("FNDR cannot isolate this Codex setup for a hand-off: {refused:?}")
    })?;
    let mut server = AppServer::spawn_with(codex, &args).await?;
    if let Err(not_started) = session::prepare(&mut server, inputs).await {
        server.shutdown().await;
        return Err(match not_started {
            NotStarted::PlanFull { .. } => {
                "Your ChatGPT plan's limit is reached. Hand off again when it resets.".to_string()
            }
            NotStarted::Profile(found) => format!(
                "Hand-off is off on this Mac: Codex did not confirm the sandbox ({} differences).",
                found.len()
            ),
            NotStarted::Codex(error) => error,
        });
    }
    store::update(&state.state_store, &job.id, |r| {
        r.status = JobStatus::Working
    })?;

    // Logged before the first byte leaves (ADR 024 item 6, PRD FR12).
    let mut included = vec![];
    if draft.ticked(DraftItemKind::Memory).next().is_some() {
        included.push("memory_summaries");
    }
    if draft.ticked(DraftItemKind::File).next().is_some() {
        included.push("files");
    }
    record_model_request_including(
        Feature::HandOff,
        "chatgpt.com",
        draft.estimate_bytes as usize,
        &included,
    );

    let turn = Turn {
        job_folder: folder.root.clone(),
        rendered: draft.rendered.clone(),
        output_schema: manifest::output_schema(),
        limits,
    };
    let mut searched: Vec<String> = Vec::new();
    // Stop, or Private Mode turning on (PRD FR13).
    let should_stop = || stop.load(Ordering::SeqCst) || state.is_incognito.load(Ordering::SeqCst);
    let outcome = session::drive(&mut server, &turn, should_stop, |event| {
        if let JobEvent::WebSearched { urls, .. } = event {
            searched.extend(urls.iter().cloned());
        }
        on_event(event);
    })
    .await;
    server.shutdown().await;
    let worked = outcome
        .bytes_sent
        .saturating_sub(draft.rendered.len() as u64);
    if worked > 0 {
        record_model_request_including(
            Feature::HandOffWork,
            "chatgpt.com",
            worked as usize,
            &["command_output", "images"],
        );
    }

    let verdict = outcome.answer.as_deref().and_then(|answer| {
        let manifest = manifest::parse(answer).ok()?;
        Some(verify::verify(
            &manifest,
            folder,
            &verify::Evidence {
                attached_memories: job.memory_ids.clone(),
                searched_urls: searched,
            },
        ))
    });
    Ok((outcome, verdict))
}

/// The record's status and its one-sentence reason (PRD "States").
fn settle(ended: &Ended, verdict: Option<&verify::Verdict>) -> (JobStatus, Option<String>) {
    let kept = Some("What it wrote is kept in the folder.".to_string());
    match (ended, verdict) {
        (Ended::Answered, Some(v)) => match v.status {
            ResultStatus::Ready => (JobStatus::Ready, None),
            ResultStatus::ReadyWithGaps => (JobStatus::ReadyWithGaps, None),
            ResultStatus::Failed => (
                JobStatus::Failed,
                Some("It finished without a file FNDR could check.".into()),
            ),
        },
        (Ended::Answered, None) => (
            JobStatus::Failed,
            Some("Its final message was not a manifest FNDR could read.".into()),
        ),
        (Ended::NoAnswer, _) => (
            JobStatus::Failed,
            Some("It stopped without a final answer.".into()),
        ),
        (Ended::Stopped, _) => (JobStatus::Stopped, kept),
        (Ended::PlanLimit, _) => (
            JobStatus::Stopped,
            Some(
                "Your ChatGPT plan's limit was reached, so it stopped. What it wrote is kept."
                    .into(),
            ),
        ),
        (Ended::WallTimeout, _) => (
            JobStatus::Stopped,
            Some("Stopped after 15 minutes. What it wrote is kept.".into()),
        ),
        (Ended::IdleTimeout, _) => (
            JobStatus::Stopped,
            Some("Stopped after 3 minutes without progress. What it wrote is kept.".into()),
        ),
        (Ended::Failed(why), _) => (JobStatus::Failed, Some(why.clone())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::reopen::ReopenKind;
    use crate::storage::MemoryRecord;
    use std::collections::HashSet;

    fn state(path: &Path) -> AppState {
        let store = Arc::new(crate::storage::Store::new(path).unwrap());
        let state_store = Arc::new(crate::storage::StateStore::new(path).unwrap());
        let graph = crate::graph::GraphStore::new(store.clone());
        AppState::new(
            path.to_path_buf(),
            crate::config::Config::default(),
            store,
            state_store,
            graph,
            None,
        )
    }

    /// A stand-in `codex`: lists features, reports back exactly the config it
    /// was started with (so the real profile check runs), and on `turn/start`
    /// writes S6's two deliverables into the job folder and replays S6.
    fn fake_codex(dir: &Path) -> PathBuf {
        let fixture = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/hand-off/s6-turn.jsonl"
        );
        let features: Vec<&str> =
            crate::ipc::commands::codex_account::READ_ONLY_DISABLED_FEATURES.to_vec();
        let script = format!(
            r#"#!/usr/bin/env python3
import json, sys, tomllib, zipfile, os
FIXTURE = {fixture:?}
if sys.argv[1:2] == ["features"]:
    for name in {features:?}:
        print(name, "stable", "true")
    sys.exit(0)
args = sys.argv[2:]
toml_lines, off = [], {{}}
i = 0
while i < len(args):
    if args[i] == "-c":
        key, _, value = args[i + 1].partition("=")
        if key != "cli_auth_credentials_store":
            toml_lines.append(key + " = " + value)
        i += 2
    elif args[i] == "--disable":
        off[args[i + 1]] = False
        i += 2
    else:
        i += 1
config = tomllib.loads("\n".join(toml_lines)) if toml_lines else {{}}
config.setdefault("features", {{}}).update(off)
def out(m): print(json.dumps(m), flush=True)
cwd = "."
for line in sys.stdin:
    m = json.loads(line)
    method, mid = m.get("method"), m.get("id")
    if mid is None:
        continue
    result = {{}}
    if method == "config/read":
        result = {{"config": config}}
    elif method == "skills/list":
        result = {{"data": []}}
    elif method == "account/rateLimits/read":
        result = {{"rateLimits": {{"primary": {{"usedPercent": 0}}, "secondary": {{"usedPercent": 78}}, "rateLimitReachedType": None}}}}
    elif method == "model/list":
        result = {{"data": [{{"model": "gpt-test", "isDefault": True}}]}}
    elif method == "thread/start":
        cwd = m["params"]["cwd"]
        result = {{"thread": {{"id": "th"}}}}
    elif method == "turn/start":
        out({{"id": mid, "result": {{"turn": {{"id": "tu"}}}}}})
        os.makedirs(os.path.join(cwd, "out"), exist_ok=True)
        with zipfile.ZipFile(os.path.join(cwd, "out/q4-onboarding-survey-readout.docx"), "w") as z:
            z.writestr("word/document.xml", "<w:document/>")
        with open(os.path.join(cwd, "out/q4-onboarding-survey-readout-chart.png"), "wb") as f:
            f.write(b"\x89PNG\r\n\x1a\n")
        for row in open(FIXTURE):
            print(row.strip(), flush=True)
        continue
    out({{"id": mid, "result": result}})
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

    fn memory(id: &str, app: &str, summary: &str) -> MemoryRecord {
        MemoryRecord {
            id: id.into(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            app_name: app.into(),
            window_title: format!("{app} window"),
            display_summary: summary.into(),
            ..Default::default()
        }
    }

    async fn seeded(state: &AppState, dir: &Path) -> JobRequest {
        let csv = dir.join("Downloads").join("q4-onboarding-survey.csv");
        std::fs::create_dir_all(csv.parent().unwrap()).unwrap();
        std::fs::write(&csv, "respondent_id,churned\n1,yes\n").unwrap();
        let mut file = memory("m_csv", "Finder", "The Q4 survey export was downloaded.");
        file.reopen_kind = ReopenKind::FilePath;
        file.reopen_file_path = Some(csv.display().to_string());
        state
            .store
            .add_batch_preserving_ids(&[
                memory(
                    "m_mail_7f3a",
                    "Mail",
                    "Dana asked for a one-page Q4 onboarding readout.",
                ),
                memory(
                    "m_notes_21c9",
                    "Notes",
                    "Notes guessed that pricing drives churn.",
                ),
                file,
            ])
            .await
            .unwrap();
        let seed = TaskSeed {
            text: "Own a one-page Q4 onboarding survey readout".into(),
            memory_id: Some("m_mail_7f3a".into()),
        };
        let selection = Selection {
            memory_ids: Some(HashSet::from([
                "m_mail_7f3a".to_string(),
                "m_notes_21c9".to_string(),
            ])),
            file_memory_ids: Some(HashSet::from(["m_csv".to_string()])),
        };
        let tools = tools_line(&toolchain_dir(state));
        let draft = draft::recheck(state, &seed, OutputGoal::Doc, &selection, &tools)
            .await
            .unwrap();
        JobRequest {
            seed,
            goal: OutputGoal::Doc,
            selection,
            reviewed: draft.rendered,
        }
    }

    #[test]
    fn a_reviewed_draft_runs_to_a_checked_ready_record() {
        let dir = tempfile::tempdir().unwrap();
        let state = state(&dir.path().join("data"));
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let request = seeded(&state, dir.path()).await;
            let codex = fake_codex(dir.path());
            let base = dir.path().join("Hand-offs");
            let mut events = Vec::new();
            let record = start_job_with(
                &state,
                &codex,
                &base,
                request,
                Arc::new(AtomicBool::new(false)),
                Limits::default(),
                |e| events.push(e.clone()),
            )
            .await
            .unwrap();

            assert_eq!(record.status, JobStatus::Ready, "{:?}", record.reason);
            assert_eq!(
                (record.checked, record.unsupported, record.not_checked),
                (4, 0, 0)
            );
            assert_eq!(
                record.deliverables,
                [
                    "out/q4-onboarding-survey-readout.docx",
                    "out/q4-onboarding-survey-readout-chart.png"
                ]
            );
            let folder = PathBuf::from(&record.folder);
            assert!(folder.starts_with(&base));
            assert!(folder.join("inputs/q4-onboarding-survey.csv").is_file());
            assert!(folder
                .join(".agents/skills/readout-docx/SKILL.md")
                .is_file());
            assert!(
                !state
                    .app_data_dir
                    .join("hand-off/jobs")
                    .join(&record.id)
                    .exists(),
                "Codex state is deleted"
            );
            assert!(record.bytes_sent > 10_000);
            assert!(events.contains(&JobEvent::Idle));
            assert_eq!(store::get(&state.state_store, &record.id).unwrap(), record);
            let logged: Vec<String> =
                crate::privacy_proof::build_privacy_proof(&state.capture_stats)
                    .model_requests
                    .into_iter()
                    .map(|r| r.feature)
                    .collect();
            assert!(
                logged.contains(&"hand_off".to_string())
                    && logged.contains(&"hand_off_work".to_string())
            );
        });
    }

    #[test]
    fn a_draft_that_changed_since_review_never_starts() {
        let dir = tempfile::tempdir().unwrap();
        let state = state(&dir.path().join("data"));
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let mut request = seeded(&state, dir.path()).await;
            request.reviewed.push_str("\nedited");
            let base = dir.path().join("Hand-offs");
            let error = start_job_with(
                &state,
                &fake_codex(dir.path()),
                &base,
                request,
                Arc::new(AtomicBool::new(false)),
                Limits::default(),
                |_| {},
            )
            .await
            .unwrap_err();
            assert!(error.contains("changed since it was shown"));
            assert!(!base.exists(), "no folder is made");
            assert!(store::list(&state.state_store).unwrap().is_empty());
        });
    }

    /// One real hand-off: the made-up survey and memories, the installed
    /// Codex on the person's ChatGPT plan, FNDR's toolchain installed fresh.
    /// The job folder is left in `~/Documents/FNDR Hand-offs/` to open.
    /// `prepare` refuses a full plan and `drive` interrupts a turn whose plan
    /// fills, so no purchased credits are spent.
    #[test]
    #[ignore = "One model call on the ChatGPT plan and a 200 MB install; run: cargo test --lib live_hand_off_runs_end_to_end -- --ignored --nocapture"]
    fn live_hand_off_runs_end_to_end() {
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hand-off");
        let root = dirs::home_dir()
            .unwrap()
            .join("Library/Caches/fndr-hand-off-live")
            .join(chrono::Utc::now().timestamp().to_string());
        std::fs::create_dir_all(&root).unwrap();
        let state = state(&root.join("data"));
        let uv = std::env::split_paths(&std::env::var_os("PATH").unwrap())
            .map(|dir| dir.join("uv"))
            .find(|path| path.is_file())
            .expect("uv on PATH");
        let started = std::time::Instant::now();
        super::super::toolchain::install(&uv, &toolchain_dir(&state)).unwrap();
        eprintln!("toolchain installed in {:?}", started.elapsed());

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let csv = root.join("Downloads/q4-onboarding-survey.csv");
            std::fs::create_dir_all(csv.parent().unwrap()).unwrap();
            std::fs::copy(fixtures.join("q4-onboarding-survey.csv"), &csv).unwrap();
            let mut file = memory("m_csv", "Finder", "The Q4 onboarding survey export was downloaded.");
            file.reopen_kind = ReopenKind::FilePath;
            file.reopen_file_path = Some(csv.display().to_string());
            state
                .store
                .add_batch_preserving_ids(&[
                    memory(
                        "m_mail_7f3a",
                        "Mail",
                        "Dana Reyes asked Sam to own a one-page Q4 onboarding survey readout with the top three churn drivers by Thursday, for the exec sync. The survey export was attached.",
                    ),
                    memory(
                        "m_notes_21c9",
                        "Notes",
                        "Churn hypotheses: pricing page confusion is probably the top driver; the Salesforce integration gap keeps coming up; enterprise churn is low. To check against the survey.",
                    ),
                    file,
                ])
                .await
                .unwrap();
            let seed = TaskSeed {
                text: "Own a one-page Q4 onboarding survey readout with the top three churn drivers by Thursday".into(),
                memory_id: Some("m_mail_7f3a".into()),
            };
            let selection = Selection {
                memory_ids: Some(HashSet::from(["m_mail_7f3a".to_string(), "m_notes_21c9".to_string()])),
                file_memory_ids: Some(HashSet::from(["m_csv".to_string()])),
            };
            let tools = tools_line(&toolchain_dir(&state));
            let draft = draft::recheck(&state, &seed, OutputGoal::Doc, &selection, &tools).await.unwrap();
            eprintln!("draft: {} bytes to send\n{}", draft.estimate_bytes, draft.rendered);
            let request = JobRequest { seed, goal: OutputGoal::Doc, selection, reviewed: draft.rendered };

            let started = std::time::Instant::now();
            let record = start_job(&state, request, Arc::new(AtomicBool::new(false)), |event| {
                eprintln!("{:>6.1}s  {event:?}", started.elapsed().as_secs_f32());
            })
            .await
            .unwrap();
            eprintln!("record: {record:#?}");

            assert_eq!(record.status, JobStatus::Ready, "{:?}", record.reason);
            assert!(record.checked >= 3 && record.unsupported == 0 && record.not_checked == 0);
            let docx = record.deliverables.iter().find(|d| d.ends_with(".docx")).expect("a document");
            let mut archive = zip::ZipArchive::new(std::fs::File::open(Path::new(&record.folder).join(docx)).unwrap()).unwrap();
            let mut xml = String::new();
            std::io::Read::read_to_string(&mut archive.by_name("word/document.xml").unwrap(), &mut xml).unwrap();
            let text = xml.split('<').filter_map(|part| part.split_once('>').map(|(_, t)| t)).collect::<Vec<_>>().join(" ");
            for fact in ["400", "78", "24", "21", "18", "28.2"] {
                assert!(text.contains(fact), "the document states {fact}");
            }
            assert!(!state.app_data_dir.join("hand-off/jobs").join(&record.id).exists());
        });
        let _ = std::fs::remove_dir_all(&root);
    }

    fn verdict(status: ResultStatus) -> verify::Verdict {
        verify::Verdict {
            deliverables: vec![],
            claims: vec![],
            checked: 0,
            unsupported: 0,
            not_checked: 0,
            status,
        }
    }

    #[test]
    fn a_checked_answer_settles_by_the_verdict() {
        assert_eq!(
            settle(&Ended::Answered, Some(&verdict(ResultStatus::Ready))).0,
            JobStatus::Ready
        );
        assert_eq!(
            settle(
                &Ended::Answered,
                Some(&verdict(ResultStatus::ReadyWithGaps))
            )
            .0,
            JobStatus::ReadyWithGaps
        );
        assert_eq!(
            settle(&Ended::Answered, Some(&verdict(ResultStatus::Failed))).0,
            JobStatus::Failed
        );
        assert_eq!(settle(&Ended::Answered, None).0, JobStatus::Failed);
    }

    #[test]
    fn every_stop_keeps_the_folder_and_says_why() {
        for ended in [
            Ended::Stopped,
            Ended::PlanLimit,
            Ended::WallTimeout,
            Ended::IdleTimeout,
        ] {
            let (status, reason) = settle(&ended, None);
            assert_eq!(status, JobStatus::Stopped, "{ended:?}");
            assert!(reason.unwrap().contains("kept"), "{ended:?}");
        }
        assert_eq!(settle(&Ended::NoAnswer, None).0, JobStatus::Failed);
        assert_eq!(
            settle(&Ended::Failed("Codex app-server exited".into()), None),
            (JobStatus::Failed, Some("Codex app-server exited".into()))
        );
    }

    #[test]
    fn the_tools_line_names_the_toolchains_python() {
        let line = tools_line(Path::new("/A/hand-off/toolchain"));
        assert!(line.ends_with("/A/hand-off/toolchain/venv/bin/python"));
        assert!(hand_offs_dir().ends_with("Documents/FNDR Hand-offs"));
    }
}
