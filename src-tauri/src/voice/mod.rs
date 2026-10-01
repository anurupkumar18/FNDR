use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

pub const VOICE_STATE_EVENT: &str = "voice://state";
const VOICE_EVENT_VERSION: u8 = 1;
const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(30);
const ACTOR_TICK: Duration = Duration::from_millis(25);
const HELPER_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(1);

type EventSink = Arc<dyn Fn(VoiceStateEvent) + Send + Sync>;

#[derive(Debug, Clone)]
struct HelperCommand {
    program: PathBuf,
    args: Vec<OsString>,
}

impl HelperCommand {
    fn new(program: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
        }
    }

    #[cfg(test)]
    fn arg(mut self, arg: impl Into<OsString>) -> Self {
        self.args.push(arg.into());
        self
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VoiceSurface {
    HomeSearch,
    ScreenGuide,
    NotchAsk,
    NotchDo,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VoiceMode {
    Toggle,
    PushToTalk,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VoiceSession {
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VoiceStateEvent {
    pub version: u8,
    pub session_id: Option<String>,
    pub surface: Option<VoiceSurface>,
    pub state: VoiceState,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum VoiceState {
    Idle,
    RequestingPermission {
        permission: VoicePermission,
    },
    PreparingModel,
    Listening {
        level: f64,
    },
    Partial {
        text: String,
    },
    Final {
        text: String,
    },
    Error {
        code: VoiceErrorCode,
        message: String,
    },
    Unavailable {
        reason: VoiceUnavailableReason,
        message: String,
    },
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VoicePermission {
    Microphone,
    SpeechRecognition,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VoiceErrorCode {
    PermissionDenied,
    RecordingFailed,
    RecognitionFailed,
    HelperCrashed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VoiceUnavailableReason {
    PrivateContext,
    SpeechRecognitionUnavailable,
    LanguageAssetMissing,
    PlatformUnsupported,
    PolicyNotEnabled,
}

#[derive(Debug, Clone)]
struct ActiveSession {
    id: String,
    surface: VoiceSurface,
    accepting_results: bool,
}

impl ActiveSession {
    fn event(&self, state: VoiceState) -> VoiceStateEvent {
        VoiceStateEvent {
            version: VOICE_EVENT_VERSION,
            session_id: Some(self.id.clone()),
            surface: Some(self.surface),
            state,
        }
    }
}

enum Control {
    Start {
        session: ActiveSession,
        response: oneshot::Sender<Result<(), String>>,
    },
    Stop {
        session_id: String,
        response: oneshot::Sender<Result<(), String>>,
    },
    Cancel {
        session_id: String,
        response: oneshot::Sender<Result<(), String>>,
    },
    Unavailable {
        session: ActiveSession,
        reason: VoiceUnavailableReason,
        message: String,
        response: oneshot::Sender<Result<(), String>>,
    },
    UnavailableActive {
        reason: VoiceUnavailableReason,
        message: String,
    },
    Shutdown,
}

enum HelperOutput {
    Line { generation: u64, line: String },
    Eof { generation: u64 },
}

struct HelperProcess {
    generation: u64,
    child: Child,
    stdin: ChildStdin,
    reader: JoinHandle<()>,
}

impl HelperProcess {
    async fn write(&mut self, command: &str) -> Result<(), String> {
        self.stdin
            .write_all(format!("{command}\n").as_bytes())
            .await
            .map_err(|error| format!("Speech helper closed its input: {error}"))?;
        self.stdin
            .flush()
            .await
            .map_err(|error| format!("Could not flush speech helper input: {error}"))
    }

    async fn quit(mut self) {
        let _ = self.write("quit").await;
        if tokio::time::timeout(HELPER_SHUTDOWN_TIMEOUT, self.child.wait())
            .await
            .is_err()
        {
            let _ = self.child.kill().await;
        }
        self.reader.abort();
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum HelperEvent {
    Ready,
    PreparingModel,
    Listening { level: Option<f64> },
    Level { level: f64 },
    Partial { text: String },
    Final { text: String },
    Error { code: String, message: String },
}

pub struct VoiceManager {
    control: mpsc::Sender<Control>,
    next_session_id: AtomicU64,
}

impl VoiceManager {
    fn spawn(command: HelperCommand, idle_timeout: Duration, sink: EventSink) -> Self {
        let (control, control_rx) = mpsc::channel(16);
        tokio::spawn(run_actor(command, idle_timeout, sink, control_rx));
        Self {
            control,
            next_session_id: AtomicU64::new(1),
        }
    }

    pub fn for_app<R: tauri::Runtime>(app: AppHandle<R>) -> Self {
        let emit_app = app.clone();
        let sink: EventSink = Arc::new(move |event| {
            if let Err(error) = emit_app.emit(VOICE_STATE_EVENT, event) {
                tracing::warn!(error = %error, "Could not emit voice state");
            }
        });
        Self::spawn(native_helper_command(), DEFAULT_IDLE_TIMEOUT, sink)
    }

    pub async fn start(
        &self,
        surface: VoiceSurface,
        mode: VoiceMode,
    ) -> Result<VoiceSession, String> {
        validate_surface_mode(surface, mode)?;
        let session = self.new_session(surface);

        if matches!(surface, VoiceSurface::NotchAsk | VoiceSurface::NotchDo) {
            self.send_unavailable(
                session.clone(),
                VoiceUnavailableReason::PolicyNotEnabled,
                "Voice is not enabled for this surface yet.".to_string(),
            )
            .await?;
            return Ok(VoiceSession {
                session_id: session.id,
            });
        }

        let (response, result) = oneshot::channel();
        self.control
            .send(Control::Start {
                session: session.clone(),
                response,
            })
            .await
            .map_err(|_| "Voice runtime is unavailable".to_string())?;
        result
            .await
            .map_err(|_| "Voice runtime stopped before start completed".to_string())??;
        Ok(VoiceSession {
            session_id: session.id,
        })
    }

    pub async fn reject_private(&self, surface: VoiceSurface) -> Result<VoiceSession, String> {
        let session = self.new_session(surface);
        self.send_unavailable(
            session.clone(),
            VoiceUnavailableReason::PrivateContext,
            "Voice is unavailable while Private Mode is on.".to_string(),
        )
        .await?;
        Ok(VoiceSession {
            session_id: session.id,
        })
    }

    pub async fn stop(&self, session_id: &str) -> Result<(), String> {
        self.send_session_control(session_id, false).await
    }

    pub async fn cancel(&self, session_id: &str) -> Result<(), String> {
        self.send_session_control(session_id, true).await
    }

    pub fn unavailable_active(&self, reason: VoiceUnavailableReason, message: impl Into<String>) {
        let _ = self.control.try_send(Control::UnavailableActive {
            reason,
            message: message.into(),
        });
    }

    pub fn shutdown_now(&self) {
        let _ = self.control.try_send(Control::Shutdown);
    }

    fn new_session(&self, surface: VoiceSurface) -> ActiveSession {
        let sequence = self.next_session_id.fetch_add(1, Ordering::Relaxed);
        ActiveSession {
            id: format!("{}-{sequence}", uuid::Uuid::new_v4()),
            surface,
            accepting_results: false,
        }
    }

    async fn send_session_control(&self, session_id: &str, cancel: bool) -> Result<(), String> {
        let (response, result) = oneshot::channel();
        let message = if cancel {
            Control::Cancel {
                session_id: session_id.to_string(),
                response,
            }
        } else {
            Control::Stop {
                session_id: session_id.to_string(),
                response,
            }
        };
        self.control
            .send(message)
            .await
            .map_err(|_| "Voice runtime is unavailable".to_string())?;
        result
            .await
            .map_err(|_| "Voice runtime stopped before command completed".to_string())?
    }

    async fn send_unavailable(
        &self,
        session: ActiveSession,
        reason: VoiceUnavailableReason,
        message: String,
    ) -> Result<(), String> {
        let (response, result) = oneshot::channel();
        self.control
            .send(Control::Unavailable {
                session,
                reason,
                message,
                response,
            })
            .await
            .map_err(|_| "Voice runtime is unavailable".to_string())?;
        result
            .await
            .map_err(|_| "Voice runtime stopped before command completed".to_string())?
    }
}

fn validate_surface_mode(surface: VoiceSurface, mode: VoiceMode) -> Result<(), String> {
    let valid = matches!(
        (surface, mode),
        (VoiceSurface::HomeSearch, VoiceMode::Toggle)
            | (VoiceSurface::ScreenGuide, VoiceMode::PushToTalk)
            | (VoiceSurface::NotchAsk, _)
            | (VoiceSurface::NotchDo, _)
    );
    valid
        .then_some(())
        .ok_or_else(|| format!("Voice mode {mode:?} is not valid for surface {surface:?}"))
}

fn native_helper_command() -> HelperCommand {
    if let Some(path) = std::env::var_os("FNDR_SPEECH_HELPER") {
        return HelperCommand::new(path);
    }

    let suffixed_name = if cfg!(target_arch = "aarch64") {
        "fndr-speech-aarch64-apple-darwin"
    } else {
        "fndr-speech-x86_64-apple-darwin"
    };
    let mut candidates = Vec::new();
    if let Ok(executable) = std::env::current_exe() {
        if let Some(parent) = executable.parent() {
            candidates.push(parent.join("fndr-speech"));
            candidates.push(parent.join(suffixed_name));
        }
    }
    candidates.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("binaries")
            .join(suffixed_name),
    );

    let path = candidates
        .iter()
        .find(|candidate| candidate.is_file())
        .cloned()
        .unwrap_or_else(|| candidates.remove(0));
    HelperCommand::new(path)
}

async fn spawn_helper(
    command: &HelperCommand,
    generation: u64,
    output: mpsc::Sender<HelperOutput>,
) -> Result<HelperProcess, String> {
    let mut child = Command::new(&command.program)
        .args(&command.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| {
            format!(
                "Could not start speech helper ({}): {error}",
                command.program.display()
            )
        })?;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| "Speech helper has no stdin".to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Speech helper has no stdout".to_string())?;
    let reader = tokio::spawn(async move {
        let mut lines = BufReader::new(stdout).lines();
        loop {
            match lines.next_line().await {
                Ok(Some(line)) => {
                    if output
                        .send(HelperOutput::Line { generation, line })
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Ok(None) | Err(_) => {
                    let _ = output.send(HelperOutput::Eof { generation }).await;
                    break;
                }
            }
        }
    });
    Ok(HelperProcess {
        generation,
        child,
        stdin,
        reader,
    })
}

fn emit_terminal(sink: &EventSink, session: &ActiveSession, state: VoiceState) {
    sink(session.event(state));
    sink(session.event(VoiceState::Idle));
}

fn helper_error_state(code: &str, message: String) -> VoiceState {
    let normalized = code.to_ascii_lowercase();
    if normalized.contains("unsupported") {
        return VoiceState::Unavailable {
            reason: VoiceUnavailableReason::PlatformUnsupported,
            message,
        };
    }
    if normalized.contains("asset") || normalized.contains("language") {
        return VoiceState::Unavailable {
            reason: VoiceUnavailableReason::LanguageAssetMissing,
            message,
        };
    }
    if normalized.contains("unavailable") {
        return VoiceState::Unavailable {
            reason: VoiceUnavailableReason::SpeechRecognitionUnavailable,
            message,
        };
    }
    let code = if normalized.contains("permission") || normalized.contains("denied") {
        VoiceErrorCode::PermissionDenied
    } else if normalized.contains("microphone") || normalized.contains("record") {
        VoiceErrorCode::RecordingFailed
    } else {
        VoiceErrorCode::RecognitionFailed
    };
    VoiceState::Error { code, message }
}

async fn restart_after_crash(
    command: &HelperCommand,
    helper: &mut Option<HelperProcess>,
    active: &mut Option<ActiveSession>,
    restart_count: &mut u8,
    generation: &mut u64,
    output: &mpsc::Sender<HelperOutput>,
    sink: &EventSink,
) {
    if active.is_none() {
        return;
    }
    if *restart_count >= 1 {
        let session = active.take().expect("active session checked");
        emit_terminal(
            sink,
            &session,
            VoiceState::Unavailable {
                reason: VoiceUnavailableReason::SpeechRecognitionUnavailable,
                message: "The on-device speech helper stopped twice.".to_string(),
            },
        );
        return;
    }

    *restart_count += 1;
    *generation += 1;
    match spawn_helper(command, *generation, output.clone()).await {
        Ok(mut replacement) => {
            if let Err(error) = replacement.write("start").await {
                tracing::warn!(error = %error, "Could not restart speech helper session");
            }
            *helper = Some(replacement);
        }
        Err(error) => {
            let session = active.take().expect("active session checked");
            emit_terminal(
                sink,
                &session,
                VoiceState::Unavailable {
                    reason: VoiceUnavailableReason::SpeechRecognitionUnavailable,
                    message: error,
                },
            );
        }
    }
}

async fn run_actor(
    command: HelperCommand,
    idle_timeout: Duration,
    sink: EventSink,
    mut control: mpsc::Receiver<Control>,
) {
    let (output_tx, mut output_rx) = mpsc::channel(32);
    let mut helper: Option<HelperProcess> = None;
    let mut active: Option<ActiveSession> = None;
    let mut idle_since: Option<Instant> = None;
    let mut restart_count = 0u8;
    let mut generation = 0u64;
    let mut tick = tokio::time::interval(ACTOR_TICK);

    loop {
        tokio::select! {
            message = control.recv() => {
                match message {
                    Some(Control::Start { session, response }) => {
                        if let Some(previous) = active.take() {
                            if let Some(process) = helper.as_mut() {
                                let _ = process.write("cancel").await;
                            }
                            emit_terminal(
                                &sink,
                                &previous,
                                VoiceState::Error {
                                    code: VoiceErrorCode::Cancelled,
                                    message: "Voice session replaced by another surface.".to_string(),
                                },
                            );
                        }
                        idle_since = None;
                        restart_count = 0;
                        active = Some(session);

                        if helper.is_none() {
                            generation += 1;
                            match spawn_helper(&command, generation, output_tx.clone()).await {
                                Ok(process) => helper = Some(process),
                                Err(error) => {
                                    let failed = active.take().expect("new session is active");
                                    emit_terminal(
                                        &sink,
                                        &failed,
                                        VoiceState::Unavailable {
                                            reason: VoiceUnavailableReason::SpeechRecognitionUnavailable,
                                            message: error,
                                        },
                                    );
                                }
                            }
                        }

                        let write_failed = if let Some(process) = helper.as_mut() {
                            process.write("start").await.is_err()
                        } else {
                            false
                        };
                        if write_failed {
                            helper.take();
                            restart_after_crash(
                                &command,
                                &mut helper,
                                &mut active,
                                &mut restart_count,
                                &mut generation,
                                &output_tx,
                                &sink,
                            ).await;
                        }
                        // The public contract returns the opaque session before
                        // asynchronous helper failures are reported as events.
                        let _ = response.send(Ok(()));
                    }
                    Some(Control::Stop { session_id, response }) => {
                        let result = if active.as_ref().is_some_and(|session| session.id == session_id) {
                            if let Some(process) = helper.as_mut() {
                                process.write("stop").await
                            } else {
                                Ok(())
                            }
                        } else {
                            Ok(())
                        };
                        let _ = response.send(result);
                    }
                    Some(Control::Cancel { session_id, response }) => {
                        if active.as_ref().is_some_and(|session| session.id == session_id) {
                            let cancelled = active.take().expect("matching active session");
                            emit_terminal(
                                &sink,
                                &cancelled,
                                VoiceState::Error {
                                    code: VoiceErrorCode::Cancelled,
                                    message: "Voice input cancelled.".to_string(),
                                },
                            );
                            idle_since = Some(Instant::now());
                            if let Some(process) = helper.as_mut() {
                                let _ = process.write("cancel").await;
                            }
                        }
                        let _ = response.send(Ok(()));
                    }
                    Some(Control::Unavailable { session, reason, message, response }) => {
                        emit_terminal(&sink, &session, VoiceState::Unavailable { reason, message });
                        let _ = response.send(Ok(()));
                    }
                    Some(Control::UnavailableActive { reason, message }) => {
                        if let Some(session) = active.take() {
                            emit_terminal(&sink, &session, VoiceState::Unavailable { reason, message });
                            idle_since = Some(Instant::now());
                            if let Some(process) = helper.as_mut() {
                                let _ = process.write("cancel").await;
                            }
                        }
                    }
                    Some(Control::Shutdown) | None => {
                        if let Some(process) = helper.take() {
                            process.quit().await;
                        }
                        break;
                    }
                }
            }
            output = output_rx.recv() => {
                match output {
                    Some(HelperOutput::Line { generation: event_generation, line })
                        if helper.as_ref().is_some_and(|process| process.generation == event_generation) =>
                    {
                        match serde_json::from_str::<HelperEvent>(&line) {
                            Ok(HelperEvent::Ready) => {}
                            Ok(HelperEvent::PreparingModel) => {
                                if let Some(session) = active.as_ref() {
                                    sink(session.event(VoiceState::PreparingModel));
                                }
                            }
                            Ok(HelperEvent::Listening { level }) => {
                                if let Some(session) = active.as_mut() {
                                    session.accepting_results = true;
                                    sink(session.event(VoiceState::Listening {
                                        level: level.unwrap_or(0.0).clamp(0.0, 1.0),
                                    }));
                                }
                            }
                            Ok(HelperEvent::Level { level }) => {
                                if let Some(session) = active.as_ref() {
                                    sink(session.event(VoiceState::Listening {
                                        level: level.clamp(0.0, 1.0),
                                    }));
                                }
                            }
                            Ok(HelperEvent::Partial { text }) => {
                                if let Some(session) = active.as_ref().filter(|session| session.accepting_results) {
                                    sink(session.event(VoiceState::Partial { text }));
                                }
                            }
                            Ok(HelperEvent::Final { text }) => {
                                if active.as_ref().is_some_and(|session| session.accepting_results) {
                                    let session = active.take().expect("accepting session is active");
                                    emit_terminal(&sink, &session, VoiceState::Final { text });
                                    idle_since = Some(Instant::now());
                                }
                            }
                            Ok(HelperEvent::Error { code, message }) => {
                                if let Some(session) = active.take() {
                                    emit_terminal(&sink, &session, helper_error_state(&code, message));
                                    idle_since = Some(Instant::now());
                                }
                            }
                            Err(error) => {
                                tracing::warn!(error = %error, "Ignored invalid speech helper output");
                            }
                        }
                    }
                    Some(HelperOutput::Line { .. }) => {}
                    Some(HelperOutput::Eof { generation: event_generation })
                        if helper.as_ref().is_some_and(|process| process.generation == event_generation) =>
                    {
                        helper.take();
                        restart_after_crash(
                            &command,
                            &mut helper,
                            &mut active,
                            &mut restart_count,
                            &mut generation,
                            &output_tx,
                            &sink,
                        ).await;
                        if active.is_none() {
                            idle_since = None;
                        }
                    }
                    Some(HelperOutput::Eof { .. }) | None => {}
                }
            }
            _ = tick.tick() => {
                if helper.is_some()
                    && active.is_none()
                    && idle_since.is_some_and(|started| started.elapsed() >= idle_timeout)
                {
                    if let Some(process) = helper.take() {
                        process.quit().await;
                    }
                    idle_since = None;
                }
            }
        }
    }
}

#[tauri::command]
pub async fn voice_start(
    surface: VoiceSurface,
    mode: VoiceMode,
    voice: State<'_, VoiceManager>,
    app_state: State<'_, Arc<crate::AppState>>,
) -> Result<VoiceSession, String> {
    if app_state.is_incognito.load(Ordering::SeqCst) {
        return voice.reject_private(surface).await;
    }
    voice.start(surface, mode).await
}

#[tauri::command]
pub async fn voice_stop(session_id: String, voice: State<'_, VoiceManager>) -> Result<(), String> {
    voice.stop(&session_id).await
}

#[tauri::command]
pub async fn voice_cancel(
    session_id: String,
    voice: State<'_, VoiceManager>,
) -> Result<(), String> {
    voice.cancel(&session_id).await
}

pub fn cancel_for_private_mode<R: tauri::Runtime>(app: &AppHandle<R>) {
    if let Some(voice) = app.try_state::<VoiceManager>() {
        voice.unavailable_active(
            VoiceUnavailableReason::PrivateContext,
            "Voice stopped because Private Mode was enabled.",
        );
    }
}

pub fn shutdown<R: tauri::Runtime>(app: &AppHandle<R>) {
    if let Some(voice) = app.try_state::<VoiceManager>() {
        voice.shutdown_now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use parking_lot::Mutex;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::Duration;

    type EventLog = Arc<Mutex<Vec<VoiceStateEvent>>>;

    fn fake_helper(scenario: &str, marker: PathBuf) -> HelperCommand {
        HelperCommand::new("/bin/sh")
            .arg(
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("helpers/fndr-speech/fake-helper.sh"),
            )
            .arg(scenario)
            .arg(marker)
    }

    fn manager(
        scenario: &str,
        idle_timeout: Duration,
    ) -> (VoiceManager, EventLog, tempfile::TempDir) {
        let temp = tempfile::tempdir().expect("temporary fake-helper directory");
        let events = Arc::new(Mutex::new(Vec::new()));
        let sink_events = events.clone();
        let manager = VoiceManager::spawn(
            fake_helper(scenario, temp.path().join("quit-marker")),
            idle_timeout,
            Arc::new(move |event| sink_events.lock().push(event)),
        );
        (manager, events, temp)
    }

    async fn wait_for(events: &EventLog, predicate: impl Fn(&[VoiceStateEvent]) -> bool) {
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if predicate(&events.lock()) {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("voice event timeout");
    }

    #[tokio::test]
    async fn streams_partial_then_final_and_quits_after_idle_timeout() {
        let (manager, events, temp) = manager("normal", Duration::from_millis(40));
        let session = manager
            .start(VoiceSurface::HomeSearch, VoiceMode::Toggle)
            .await
            .expect("start voice");

        wait_for(&events, |events| {
            events.iter().any(|event| {
                event.session_id.as_deref() == Some(&session.session_id)
                    && matches!(&event.state, VoiceState::Partial { text } if text == "Show my")
            })
        })
        .await;
        manager.stop(&session.session_id).await.expect("stop voice");
        wait_for(&events, |events| {
            events.iter().any(|event| {
                matches!(&event.state, VoiceState::Final { text } if text == "Show my meetings")
            }) && events
                .iter()
                .any(|event| matches!(event.state, VoiceState::Idle))
        })
        .await;
        tokio::time::sleep(Duration::from_millis(100)).await;

        assert!(temp.path().join("quit-marker").exists());
    }

    #[tokio::test]
    async fn maps_helper_error_and_returns_to_idle() {
        let (manager, events, _temp) = manager("error", Duration::from_secs(30));
        manager
            .start(VoiceSurface::ScreenGuide, VoiceMode::PushToTalk)
            .await
            .expect("start voice");

        wait_for(&events, |events| {
            events.iter().any(|event| {
                matches!(
                    &event.state,
                    VoiceState::Error {
                        code: VoiceErrorCode::RecognitionFailed,
                        ..
                    }
                )
            }) && events
                .iter()
                .any(|event| matches!(event.state, VoiceState::Idle))
        })
        .await;
    }

    #[tokio::test]
    async fn restarts_one_crashed_helper_then_reports_unavailable() {
        let (manager, events, _temp) = manager("crash", Duration::from_secs(30));
        manager
            .start(VoiceSurface::HomeSearch, VoiceMode::Toggle)
            .await
            .expect("start voice");

        wait_for(&events, |events| {
            events.iter().any(|event| {
                matches!(
                    &event.state,
                    VoiceState::Unavailable {
                        reason: VoiceUnavailableReason::SpeechRecognitionUnavailable,
                        ..
                    }
                )
            })
        })
        .await;
    }

    #[tokio::test]
    async fn cancel_emits_cancelled_then_idle_and_discards_late_final() {
        let (manager, events, _temp) = manager("cancel", Duration::from_secs(30));
        let session = manager
            .start(VoiceSurface::HomeSearch, VoiceMode::Toggle)
            .await
            .expect("start voice");
        wait_for(&events, |events| {
            events
                .iter()
                .any(|event| matches!(event.state, VoiceState::Partial { .. }))
        })
        .await;

        manager
            .cancel(&session.session_id)
            .await
            .expect("cancel voice");
        wait_for(&events, |events| {
            events.iter().any(|event| {
                matches!(
                    &event.state,
                    VoiceState::Error {
                        code: VoiceErrorCode::Cancelled,
                        ..
                    }
                )
            }) && events
                .iter()
                .any(|event| matches!(event.state, VoiceState::Idle))
        })
        .await;
        tokio::time::sleep(Duration::from_millis(40)).await;

        assert!(!events.lock().iter().any(|event| {
            matches!(&event.state, VoiceState::Final { text } if text == "late result")
        }));
    }

    #[tokio::test]
    async fn a_new_surface_cancels_the_old_session_and_stale_stop_is_a_noop() {
        let (manager, events, _temp) = manager("normal", Duration::from_secs(30));
        let first = manager
            .start(VoiceSurface::HomeSearch, VoiceMode::Toggle)
            .await
            .expect("start first voice session");
        wait_for(&events, |events| {
            events.iter().any(|event| {
                event.session_id.as_deref() == Some(&first.session_id)
                    && matches!(event.state, VoiceState::Partial { .. })
            })
        })
        .await;

        let second = manager
            .start(VoiceSurface::ScreenGuide, VoiceMode::PushToTalk)
            .await
            .expect("start replacement voice session");
        manager
            .stop(&first.session_id)
            .await
            .expect("stale stop is accepted as a no-op");
        wait_for(&events, |events| {
            events.iter().any(|event| {
                event.session_id.as_deref() == Some(&second.session_id)
                    && matches!(event.state, VoiceState::Partial { .. })
            })
        })
        .await;

        assert!(events.lock().iter().any(|event| {
            event.session_id.as_deref() == Some(&first.session_id)
                && matches!(
                    event.state,
                    VoiceState::Error {
                        code: VoiceErrorCode::Cancelled,
                        ..
                    }
                )
        }));
        assert!(!events.lock().iter().any(|event| {
            event.session_id.as_deref() == Some(&second.session_id)
                && matches!(event.state, VoiceState::Final { .. })
        }));
    }
}
