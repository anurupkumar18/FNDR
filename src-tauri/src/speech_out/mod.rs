//! FNDR's natural voice, spoken from the person's ChatGPT plan (ADR 028).
//!
//! One `codex app-server` holds one ephemeral, tool-less thread and its
//! realtime voice session: protocol v3 over WebRTC, where the webview owns the
//! peer connection and this broker only relays the SDP. FNDR sends reply text
//! it already decided to say, through `thread/realtime/appendSpeech`, and
//! nothing else: no startup context, no memory, no screen text, no microphone.
//! Realtime v3 does not speak without an incoming audio track, so the webview
//! sends a silent one (`src/shared/voice/realtimeOut.ts`).
//!
//! Protocol shapes come from `codex app-server generate-json-schema
//! --experimental` of Codex 0.162.0-alpha.2 and a live run on 2026-10-09; the
//! realtime methods are experimental there and may change.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::{json, Value};
use tokio::sync::{mpsc, oneshot, Mutex};

use crate::ipc::commands::codex_account::{
    self, parse_account, parse_window, AppServer, CodexUsageWindow,
};

/// Above this share of either plan usage window, FNDR stops using the plan's
/// voice and falls back to an on-device one, so speech never spends the
/// last of the limits a person needs for their own Codex work.
pub const FALLBACK_ABOVE_USED_PERCENT: f64 = 95.0;
/// Narration and short answers fit; anything longer is not a spoken reply.
pub const MAX_SPOKEN_CHARS: usize = 600;
/// Where the realtime call is placed (`/backend-api/.../realtime/calls`).
pub const SPEECH_HOST: &str = "chatgpt.com";

/// The Codex feature that carries realtime voice. Off by default in 0.153,
/// on in 0.162.
const REALTIME_FEATURE: &str = "realtime_conversation";
/// v1 and v2 are refused for a ChatGPT sign-in ("AVAS realtime calls require
/// realtime v1 or v3"; v1 then asks for an alpha header). Only v3 connected.
const REALTIME_VERSION: &str = "v3";
const SDP_NOTIFICATION: &str = "thread/realtime/sdp";
const START_TIMEOUT: Duration = Duration::from_secs(25);
const CALL_TIMEOUT: Duration = Duration::from_secs(10);
const STOP_TIMEOUT: Duration = Duration::from_secs(3);
const STATUS_TTL: Duration = Duration::from_secs(30);
const MAX_SDP_BYTES: usize = 64 * 1024;
/// Ids for calls made by the session task, far above the ones `AppServer`
/// used while the session was set up.
const FIRST_CALL_ID: u64 = 1 << 32;

/// The only instructions the voice session gets. Fixed text, no context.
pub(crate) const READ_ALOUD_INSTRUCTIONS: &str = "Read aloud exactly the text you are given, word for word. Do not answer it, add to it, or ask anything. Stay silent otherwise.";

pub(crate) const PRIVATE_MODE_REFUSAL: &str =
    "Private Mode is on, so FNDR is not sending anything to ChatGPT to be spoken.";
const NOT_CONNECTED: &str = "The ChatGPT voice is not connected.";
const SESSION_ENDED: &str = "The ChatGPT voice session ended.";

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VoiceOutState {
    Ready,
    NotInstalled,
    Broken,
    SignedOut,
    /// This Codex has no realtime voice.
    Unsupported,
    /// Above [`FALLBACK_ABOVE_USED_PERCENT`] of a usage window.
    NearLimit,
    OverLimit,
    PrivateMode,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VoiceOutStatus {
    pub state: VoiceOutState,
    pub detail: Option<String>,
    /// The higher used share of the plan's two usage windows, 0 to 100.
    pub used_percent: Option<f64>,
    pub fallback_above_percent: f64,
    pub connected: bool,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VoiceOutVoices {
    pub voices: Vec<String>,
}

fn status_of(state: VoiceOutState, detail: Option<String>, used: Option<f64>) -> VoiceOutStatus {
    VoiceOutStatus {
        state,
        detail,
        used_percent: used,
        fallback_above_percent: FALLBACK_ABOVE_USED_PERCENT,
        connected: false,
    }
}

/// What the plan's voice can do right now, from what Codex reported.
pub(crate) fn classify(
    account_kind: Option<&str>,
    windows: &[Option<CodexUsageWindow>],
    limit_reached: bool,
    realtime_supported: bool,
) -> VoiceOutStatus {
    let used = windows
        .iter()
        .flatten()
        .map(|window| window.used_percent)
        .fold(None, |high: Option<f64>, used| {
            Some(high.map_or(used, |h| h.max(used)))
        });
    let (state, detail) = if account_kind != Some("chatgpt") {
        (
            VoiceOutState::SignedOut,
            "Sign in with ChatGPT to use its voice.".to_string(),
        )
    } else if !realtime_supported {
        (
            VoiceOutState::Unsupported,
            "This Codex has no realtime voice. Update Codex to use it.".to_string(),
        )
    } else if limit_reached || used.is_some_and(|u| u >= 100.0) {
        (
            VoiceOutState::OverLimit,
            "The ChatGPT plan's usage limit is reached.".to_string(),
        )
    } else if used.is_some_and(|u| u > FALLBACK_ABOVE_USED_PERCENT) {
        (
            VoiceOutState::NearLimit,
            format!(
                "The ChatGPT plan is above {FALLBACK_ABOVE_USED_PERCENT:.0} percent of its usage limit, so FNDR speaks on the Mac instead."
            ),
        )
    } else {
        (VoiceOutState::Ready, String::new())
    };
    let detail = (!detail.is_empty()).then_some(detail);
    status_of(state, detail, used)
}

/// The `thread/realtime/start` request: audio out over WebRTC, v3, without
/// Codex's startup context and without automatic Codex handoffs.
pub(crate) fn start_params(thread_id: &str, sdp_offer: &str, voice: Option<&str>) -> Value {
    let mut params = json!({
        "threadId": thread_id,
        "outputModality": "audio",
        "version": REALTIME_VERSION,
        "includeStartupContext": false,
        "clientManagedHandoffs": true,
        "prompt": READ_ALOUD_INSTRUCTIONS,
        "transport": { "type": "webrtc", "sdp": sdp_offer },
    });
    if let Some(voice) = voice.filter(|v| codex_account::is_plain_config_key(v)) {
        params["voice"] = json!(voice);
    }
    params
}

/// Which Codex to start, and with what arguments.
#[derive(Clone)]
pub(crate) enum Launcher {
    /// The installed Codex (`None`: found the usual way), tool-less: acting
    /// features off, every MCP server disabled, realtime voice on.
    Codex(Option<PathBuf>),
    /// A fixed program and arguments, for tests.
    #[cfg(test)]
    Fixed {
        executable: PathBuf,
        args: Vec<String>,
    },
}

impl Launcher {
    async fn resolve(&self) -> Result<(PathBuf, Vec<String>), (VoiceOutState, String)> {
        match self {
            #[cfg(test)]
            Self::Fixed { executable, args } => Ok((executable.clone(), args.clone())),
            Self::Codex(path) => {
                let executable = match path {
                    Some(path) => path.clone(),
                    None => codex_account::ready_executable()
                        .map_err(|e| (VoiceOutState::NotInstalled, e))?,
                };
                codex_account::refresh_codex_features(&executable)
                    .await
                    .map_err(|e| (VoiceOutState::Broken, e))?;
                if codex_account::codex_knows_feature(REALTIME_FEATURE) != Some(true) {
                    return Err((
                        VoiceOutState::Unsupported,
                        "This Codex has no realtime voice. Update Codex to use it.".to_string(),
                    ));
                }
                let names = codex_account::configured_mcp_server_names(&executable)
                    .await
                    .map_err(|e| (VoiceOutState::Broken, e))?;
                let mut args = codex_account::read_only_session_args(&names)
                    .map_err(|e| (VoiceOutState::Broken, e))?;
                args.push("--enable".to_string());
                args.push(REALTIME_FEATURE.to_string());
                Ok((executable, args))
            }
        }
    }
}

/// Records one send in Privacy Activity: bytes of text FNDR supplied.
pub(crate) type Ledger = Arc<dyn Fn(usize) + Send + Sync>;

struct Call {
    method: &'static str,
    params: Value,
    /// A notification that carries the real answer (the SDP answer arrives
    /// as `thread/realtime/sdp`, not in the response).
    awaits: Option<&'static str>,
    reply: oneshot::Sender<Result<Value, String>>,
}

struct Waiting {
    method: &'static str,
    awaits: Option<&'static str>,
    reply: oneshot::Sender<Result<Value, String>>,
}

/// Applies one message from Codex to the calls waiting on it.
fn settle(waiting: &mut HashMap<u64, Waiting>, realtime_live: &AtomicBool, message: &Value) {
    let method = message.get("method").and_then(Value::as_str);
    if let (None, Some(id)) = (method, message.get("id").and_then(Value::as_u64)) {
        let Some(entry) = waiting.get(&id) else {
            return;
        };
        if let Some(error) = message.get("error") {
            let detail = error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("unknown error");
            let entry = waiting.remove(&id).expect("present");
            let _ = entry
                .reply
                .send(Err(format!("Codex {} failed: {detail}", entry.method)));
        } else if entry.awaits.is_none() {
            let entry = waiting.remove(&id).expect("present");
            let _ = entry
                .reply
                .send(Ok(message.get("result").cloned().unwrap_or(Value::Null)));
        }
        return;
    }
    let params = message.get("params").cloned().unwrap_or(Value::Null);
    match method {
        Some(SDP_NOTIFICATION) => {
            let id = waiting
                .iter()
                .find(|(_, entry)| entry.awaits == Some(SDP_NOTIFICATION))
                .map(|(id, _)| *id);
            if let Some(entry) = id.and_then(|id| waiting.remove(&id)) {
                let sdp = params.get("sdp").cloned().unwrap_or(Value::Null);
                let _ = entry.reply.send(Ok(sdp));
            }
        }
        Some("thread/realtime/error") | Some("thread/realtime/closed") => {
            if method == Some("thread/realtime/closed") {
                realtime_live.store(false, Ordering::SeqCst);
            }
            let reason = params
                .get("message")
                .or_else(|| params.get("reason"))
                .and_then(Value::as_str)
                .unwrap_or(SESSION_ENDED)
                .chars()
                .take(300)
                .collect::<String>();
            let ids: Vec<u64> = waiting
                .iter()
                .filter(|(_, entry)| entry.awaits.is_some())
                .map(|(id, _)| *id)
                .collect();
            for id in ids {
                if let Some(entry) = waiting.remove(&id) {
                    let _ = entry
                        .reply
                        .send(Err(format!("The ChatGPT voice failed: {reason}")));
                }
            }
        }
        _ => {}
    }
}

/// Owns the app-server for the session's life: writes calls, reads
/// everything Codex says, and notices at once when Codex goes away.
async fn run_session(
    mut server: AppServer,
    mut calls: mpsc::UnboundedReceiver<Call>,
    alive: Arc<AtomicBool>,
    realtime_live: Arc<AtomicBool>,
) {
    let mut waiting: HashMap<u64, Waiting> = HashMap::new();
    let mut next_id = FIRST_CALL_ID;
    let ended = loop {
        tokio::select! {
            call = calls.recv() => {
                let Some(call) = call else { break SESSION_ENDED.to_string() };
                let id = next_id;
                next_id += 1;
                let request = json!({ "method": call.method, "id": id, "params": call.params });
                if let Err(error) = server.write(request).await {
                    let _ = call.reply.send(Err(error.clone()));
                    break error;
                }
                waiting.insert(id, Waiting { method: call.method, awaits: call.awaits, reply: call.reply });
            }
            message = server.read_message() => match message {
                Ok(message) => settle(&mut waiting, &realtime_live, &message),
                Err(error) => break error,
            },
        }
    };
    alive.store(false, Ordering::SeqCst);
    realtime_live.store(false, Ordering::SeqCst);
    for (_, entry) in waiting.drain() {
        let _ = entry.reply.send(Err(ended.clone()));
    }
    server.shutdown().await;
}

struct Session {
    calls: mpsc::UnboundedSender<Call>,
    thread_id: String,
    alive: Arc<AtomicBool>,
    realtime_live: Arc<AtomicBool>,
    task: tokio::task::JoinHandle<()>,
}

impl Session {
    fn alive(&self) -> bool {
        self.alive.load(Ordering::SeqCst) && !self.task.is_finished()
    }

    fn speaking_ready(&self) -> bool {
        self.alive() && self.realtime_live.load(Ordering::SeqCst)
    }

    async fn call(
        &self,
        method: &'static str,
        params: Value,
        awaits: Option<&'static str>,
        limit: Duration,
    ) -> Result<Value, String> {
        let (reply, answer) = oneshot::channel();
        self.calls
            .send(Call {
                method,
                params,
                awaits,
                reply,
            })
            .map_err(|_| SESSION_ENDED.to_string())?;
        match tokio::time::timeout(limit, answer).await {
            Err(_) => Err(format!("Codex {method} timed out")),
            Ok(Err(_)) => Err(SESSION_ENDED.to_string()),
            Ok(Ok(result)) => result,
        }
    }
}

impl Drop for Session {
    /// Ending the task drops the `AppServer`, whose child is killed on drop.
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[derive(Default)]
struct BrokerState {
    session: Option<Session>,
    probed: Option<(Instant, VoiceOutStatus, VoiceOutVoices)>,
}

/// One voice session at a time, for the whole app.
pub struct Broker {
    launcher: Launcher,
    ledger: Ledger,
    start_timeout: Duration,
    state: Mutex<BrokerState>,
}

impl Broker {
    pub(crate) fn new(launcher: Launcher, ledger: Ledger) -> Self {
        Self {
            launcher,
            ledger,
            start_timeout: START_TIMEOUT,
            state: Mutex::new(BrokerState::default()),
        }
    }

    /// Sign-in, usage and voices, read from a short-lived app-server.
    async fn probe(&self) -> (VoiceOutStatus, VoiceOutVoices) {
        let none = VoiceOutVoices::default();
        let (executable, args) = match self.launcher.resolve().await {
            Ok(found) => found,
            Err((state, detail)) => return (status_of(state, Some(detail), None), none),
        };
        let mut server = match AppServer::spawn_experimental(&executable, &args).await {
            Ok(server) => server,
            Err(error) => return (status_of(VoiceOutState::Broken, Some(error), None), none),
        };
        let kind = server
            .request("account/read", json!({ "refreshToken": false }))
            .await
            .ok()
            .and_then(|result| parse_account(&result))
            .map(|account| account.kind);
        let limits = server
            .request("account/rateLimits/read", Value::Null)
            .await
            .ok()
            .and_then(|result| result.get("rateLimits").cloned())
            .unwrap_or(Value::Null);
        let listed = server
            .request("thread/realtime/listVoices", json!({}))
            .await;
        server.shutdown().await;

        let windows = [
            parse_window(limits.get("primary")),
            parse_window(limits.get("secondary")),
        ];
        let limit_reached = limits
            .get("rateLimitReachedType")
            .is_some_and(|reached| !reached.is_null());
        let status = classify(kind.as_deref(), &windows, limit_reached, listed.is_ok());
        let mut voices: Vec<String> = Vec::new();
        if let Ok(listed) = &listed {
            for list in ["v1", "v2"] {
                let names = listed
                    .pointer(&format!("/voices/{list}"))
                    .and_then(Value::as_array);
                for name in names.into_iter().flatten().filter_map(Value::as_str) {
                    if !voices.iter().any(|known| known == name) {
                        voices.push(name.to_string());
                    }
                }
            }
        }
        (status, VoiceOutVoices { voices })
    }

    async fn probed(&self, state: &mut BrokerState) -> (VoiceOutStatus, VoiceOutVoices) {
        if let Some((at, status, voices)) = &state.probed {
            if at.elapsed() < STATUS_TTL {
                return (status.clone(), voices.clone());
            }
        }
        let (status, voices) = self.probe().await;
        state.probed = Some((Instant::now(), status.clone(), voices.clone()));
        (status, voices)
    }

    pub async fn status(&self, private_mode: bool) -> VoiceOutStatus {
        if private_mode {
            return status_of(
                VoiceOutState::PrivateMode,
                Some(PRIVATE_MODE_REFUSAL.to_string()),
                None,
            );
        }
        let mut state = self.state.lock().await;
        let (mut status, _) = self.probed(&mut state).await;
        status.connected = state.session.as_ref().is_some_and(Session::speaking_ready);
        status
    }

    pub async fn voices(&self) -> VoiceOutVoices {
        let mut state = self.state.lock().await;
        self.probed(&mut state).await.1
    }

    async fn open_session(&self) -> Result<Session, String> {
        let (executable, args) = self.launcher.resolve().await.map_err(|(_, e)| e)?;
        let mut server = AppServer::spawn_experimental(&executable, &args).await?;
        let account = server
            .request("account/read", json!({ "refreshToken": false }))
            .await;
        if account.ok().and_then(|a| parse_account(&a)).map(|a| a.kind) != Some("chatgpt".into()) {
            server.shutdown().await;
            return Err("Sign in with ChatGPT to use its voice.".to_string());
        }
        let cwd = std::env::temp_dir().join("fndr-voice-out");
        std::fs::create_dir_all(&cwd).map_err(|e| format!("Could not prepare the voice: {e}"))?;
        let thread = server
            .request(
                "thread/start",
                json!({
                    "ephemeral": true,
                    "cwd": cwd,
                    "sandbox": "read-only",
                    "approvalPolicy": "never",
                    "serviceName": "fndr_voice_out",
                }),
            )
            .await;
        let thread_id = match thread.ok().and_then(|t| {
            t.pointer("/thread/id")
                .and_then(Value::as_str)
                .map(str::to_string)
        }) {
            Some(id) => id,
            None => {
                server.shutdown().await;
                return Err("Codex did not start a voice thread.".to_string());
            }
        };
        let (calls, receiver) = mpsc::unbounded_channel();
        let alive = Arc::new(AtomicBool::new(true));
        let realtime_live = Arc::new(AtomicBool::new(false));
        let task = tokio::spawn(run_session(
            server,
            receiver,
            alive.clone(),
            realtime_live.clone(),
        ));
        Ok(Session {
            calls,
            thread_id,
            alive,
            realtime_live,
            task,
        })
    }

    /// Connects the webview's peer connection: sends its SDP offer and
    /// returns Codex's answer. Replaces any realtime session already open.
    pub async fn start(
        &self,
        sdp_offer: &str,
        voice: Option<&str>,
        private_mode: bool,
    ) -> Result<String, String> {
        if private_mode {
            return Err(PRIVATE_MODE_REFUSAL.to_string());
        }
        if sdp_offer.trim().is_empty() || sdp_offer.len() > MAX_SDP_BYTES {
            return Err("The voice connection offer is not usable.".to_string());
        }
        let mut state = self.state.lock().await;
        let (status, _) = self.probed(&mut state).await;
        if status.state != VoiceOutState::Ready {
            return Err(status.detail.unwrap_or_else(|| NOT_CONNECTED.to_string()));
        }
        // A Codex that exited a moment ago can still look alive; one retry
        // on a fresh session covers that.
        let reused = state.session.as_ref().is_some_and(Session::alive);
        match self.start_once(&mut state, sdp_offer, voice).await {
            Err(_) if reused => self.start_once(&mut state, sdp_offer, voice).await,
            answer => answer,
        }
    }

    async fn start_once(
        &self,
        state: &mut BrokerState,
        sdp_offer: &str,
        voice: Option<&str>,
    ) -> Result<String, String> {
        if !state.session.as_ref().is_some_and(Session::alive) {
            state.session = None;
            state.session = Some(self.open_session().await?);
        }
        let session = state.session.as_ref().expect("opened above");
        if session.realtime_live.swap(false, Ordering::SeqCst) {
            let stop = json!({ "threadId": session.thread_id });
            let _ = session
                .call("thread/realtime/stop", stop, None, STOP_TIMEOUT)
                .await;
        }
        (self.ledger)(READ_ALOUD_INSTRUCTIONS.len());
        let params = start_params(&session.thread_id, sdp_offer, voice);
        let answer = session
            .call(
                "thread/realtime/start",
                params,
                Some(SDP_NOTIFICATION),
                self.start_timeout,
            )
            .await
            .and_then(|sdp| {
                sdp.as_str()
                    .map(str::to_string)
                    .ok_or_else(|| "Codex sent no voice connection answer.".to_string())
            });
        match answer {
            Ok(sdp) => {
                session.realtime_live.store(true, Ordering::SeqCst);
                Ok(sdp)
            }
            Err(error) => {
                // A start that failed or hung leaves nothing half-open.
                state.session = None;
                Err(error)
            }
        }
    }

    /// Hands reply text to the connected voice to be spoken. Only text: the
    /// caller passes FNDR-authored lines, never memory or screen text.
    pub async fn speak(&self, text: &str, private_mode: bool) -> Result<(), String> {
        if private_mode {
            return Err(PRIVATE_MODE_REFUSAL.to_string());
        }
        let text = text.trim();
        if text.is_empty() {
            return Err("There is nothing to say.".to_string());
        }
        if text.chars().count() > MAX_SPOKEN_CHARS {
            return Err(format!(
                "A spoken reply is at most {MAX_SPOKEN_CHARS} characters."
            ));
        }
        let mut state = self.state.lock().await;
        let Some(session) = state.session.as_ref().filter(|s| s.speaking_ready()) else {
            return Err(NOT_CONNECTED.to_string());
        };
        (self.ledger)(text.len());
        let params = json!({ "threadId": session.thread_id, "text": text });
        let sent = session
            .call("thread/realtime/appendSpeech", params, None, CALL_TIMEOUT)
            .await;
        if sent.is_err() && !session.alive() {
            state.session = None;
        }
        sent.map(|_| ())
    }

    /// Ends the realtime call, and with it any speech still being made, but
    /// keeps Codex running so the next connection is quicker.
    pub async fn cancel(&self) {
        let state = self.state.lock().await;
        if let Some(session) = state.session.as_ref() {
            if session.realtime_live.swap(false, Ordering::SeqCst) {
                let stop = json!({ "threadId": session.thread_id });
                let _ = session
                    .call("thread/realtime/stop", stop, None, STOP_TIMEOUT)
                    .await;
            }
        }
    }

    /// Ends the session and the Codex process behind it.
    pub async fn stop(&self) {
        self.cancel().await;
        self.state.lock().await.session = None;
    }
}

/// The app's broker: the installed Codex, every send in Privacy Activity.
pub fn shared() -> &'static Broker {
    static SHARED: OnceLock<Broker> = OnceLock::new();
    SHARED.get_or_init(|| {
        Broker::new(
            Launcher::Codex(None),
            Arc::new(|bytes| {
                crate::privacy_proof::record_model_request(
                    crate::privacy_proof::Feature::VoiceOut,
                    SPEECH_HOST,
                    bytes,
                )
            }),
        )
    })
}

#[cfg(test)]
mod tests;
