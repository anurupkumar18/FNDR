//! Native runtime for the local-only Screen Guide overlay.
//!
//! Screen pixels, OCR, questions, and conversation history remain transient:
//! this module never calls the memory store or writes an artifact to disk.

use crate::capture::macos::FrontmostAppContext;
use crate::config::{AutofillConfig, ScreenGuideConfig};
use crate::ocr::{OcrEngine, ScreenGuideOcrLine};
use crate::privacy::safety_gate::{self, SafetyDecision};
use crate::privacy::Blocklist;
use crate::speech;
use crate::AppState;
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::io::Write;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

pub const SCREEN_GUIDE_OVERLAY_LABEL: &str = "screen-guide-overlay";
const SCREEN_GUIDE_SHORTCUT_EVENT: &str = "screen-guide://shortcut";
const SCREEN_GUIDE_SUBMIT_EVENT: &str = "screen-guide://submit";
const MAX_SCREEN_GUIDE_QUESTION_CHARS: usize = 800;
const MAX_SCREEN_GUIDE_HISTORY_CHARS: usize = 1_200;
const MAX_SCREEN_GUIDE_OCR_CHARS: usize = 4_000;
const MAX_SCREEN_GUIDE_SPEECH_CHARS: usize = 2_000;
const MAX_SCREEN_GUIDE_ANSWER_CHARS: usize = 360;
const MAX_PENDING_SCREEN_GUIDE_EVENTS: usize = 16;
const OVERLAY_CAPTURE_SETTLE: Duration = Duration::from_millis(80);
const SCREEN_GUIDE_MICROPHONE_STOP_ACK_TIMEOUT: Duration = Duration::from_millis(750);
const SCREEN_GUIDE_MICROPHONE_MAX_DURATION: Duration = Duration::from_secs(65);
const SCREEN_GUIDE_HIDDEN_LEASE: Duration = Duration::from_secs(150);
const SCREEN_GUIDE_INFERENCE_TIMEOUT: Duration = Duration::from_secs(45);

static POINT_TAG_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\[POINT:[^\]\r\n]*\]").expect("valid point tag regex"));
static SCREEN_GUIDE_SAY_PROCESS: Lazy<Mutex<Option<Child>>> = Lazy::new(|| Mutex::new(None));
static SCREEN_GUIDE_REGISTERED_SHORTCUT_ID: Lazy<Mutex<Option<u32>>> =
    Lazy::new(|| Mutex::new(None));
static SCREEN_GUIDE_OVERLAY_SAFE: AtomicBool = AtomicBool::new(false);
static SCREEN_GUIDE_SHUTTING_DOWN: AtomicBool = AtomicBool::new(false);
static SCREEN_GUIDE_OVERLAY_RECOVERY_EPOCH: AtomicU64 = AtomicU64::new(0);
static SCREEN_GUIDE_RESTORE_RETRY_OWNER: AtomicU64 = AtomicU64::new(0);
static SCREEN_GUIDE_RUNTIME: Lazy<Mutex<ScreenGuideRuntime>> =
    Lazy::new(|| Mutex::new(ScreenGuideRuntime::default()));
static SCREEN_GUIDE_OVERLAY_DELIVERY: Lazy<Mutex<ScreenGuideOverlayDelivery>> =
    Lazy::new(|| Mutex::new(ScreenGuideOverlayDelivery::default()));
static SCREEN_GUIDE_MICROPHONE_SAFETY: Lazy<Mutex<ScreenGuideMicrophoneSafety>> =
    Lazy::new(|| Mutex::new(ScreenGuideMicrophoneSafety::default()));
/// Serializes overlay visibility and the short, synchronous capture phase.
/// Global shortcut callbacks cannot await, so this stays a parking_lot lock
/// and no guard is ever held across an async suspension point.
static SCREEN_GUIDE_LIFECYCLE: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));
/// Only one request may own hidden FNDR windows and capture preparation at a
/// time. New input still increments the generation before awaiting this lock,
/// so it cancels the current owner immediately and then takes over cleanly.
static SCREEN_GUIDE_CAPTURE_COORDINATOR: Lazy<tokio::sync::Mutex<()>> =
    Lazy::new(|| tokio::sync::Mutex::new(()));
static SCREEN_GUIDE_DEFERRED_RESTORE: Lazy<Mutex<DeferredFndrRestore>> =
    Lazy::new(|| Mutex::new(DeferredFndrRestore::default()));

fn screen_guide_input_allowed(is_incognito: bool) -> bool {
    !is_incognito
}

fn screen_guide_press_should_begin(active_generation: Option<u64>, is_incognito: bool) -> bool {
    active_generation.is_none() && screen_guide_input_allowed(is_incognito)
}

fn screen_guide_privacy_cleanup_still_owns_turn(
    still_private: bool,
    current_generation: u64,
    privacy_generation: u64,
) -> bool {
    still_private || current_generation == privacy_generation
}

fn screen_guide_privacy_reset_should_retry(
    expected_recovery_epoch: u64,
    current_recovery_epoch: u64,
    still_private: bool,
    current_generation: u64,
    privacy_generation: u64,
) -> bool {
    expected_recovery_epoch == current_recovery_epoch
        && screen_guide_privacy_cleanup_still_owns_turn(
            still_private,
            current_generation,
            privacy_generation,
        )
}

fn screen_guide_input_is_private<R: tauri::Runtime>(app: &AppHandle<R>) -> bool {
    app.try_state::<Arc<AppState>>()
        .is_some_and(|state| !screen_guide_input_allowed(state.is_incognito.load(Ordering::SeqCst)))
}

fn suppress_memory_capture_for_generation<R: tauri::Runtime>(app: &AppHandle<R>, generation: u64) {
    if let Some(state) = app.try_state::<Arc<AppState>>() {
        state
            .screen_guide_capture_generation
            .store(generation, Ordering::SeqCst);
        state
            .screen_guide_capture_epoch
            .fetch_add(1, Ordering::SeqCst);
    }
}

fn release_memory_capture_for_generation<R: tauri::Runtime>(app: &AppHandle<R>, generation: u64) {
    if let Some(state) = app.try_state::<Arc<AppState>>() {
        if state.screen_guide_capture_generation.load(Ordering::SeqCst) != generation {
            return;
        }
        state
            .screen_guide_capture_epoch
            .fetch_add(1, Ordering::SeqCst);
        let _ = state.screen_guide_capture_generation.compare_exchange(
            generation,
            0,
            Ordering::SeqCst,
            Ordering::SeqCst,
        );
    }
}

fn current_memory_capture_generation<R: tauri::Runtime>(app: &AppHandle<R>) -> u64 {
    app.try_state::<Arc<AppState>>()
        .map(|state| state.screen_guide_capture_generation.load(Ordering::SeqCst))
        .unwrap_or(0)
}

#[derive(Debug, Default)]
struct ScreenGuideRuntime {
    enabled: bool,
    generation: u64,
    lease_epoch: u64,
    turn_cancel: Option<Arc<AtomicBool>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScreenGuideMicrophoneWatchdog {
    StopAcknowledgement,
    MaximumDuration,
}

#[derive(Debug, Clone, Copy)]
enum ScreenGuideOverlayResetGuard {
    Microphone {
        watchdog: ScreenGuideMicrophoneWatchdog,
        generation: u64,
        epoch: u64,
    },
    Privacy {
        recovery_epoch: u64,
        privacy_generation: u64,
    },
}

impl ScreenGuideOverlayResetGuard {
    fn is_pending<R: tauri::Runtime>(self, app: &AppHandle<R>) -> bool {
        if SCREEN_GUIDE_SHUTTING_DOWN.load(Ordering::SeqCst) {
            return false;
        }
        match self {
            Self::Microphone {
                watchdog,
                generation,
                epoch,
            } => SCREEN_GUIDE_MICROPHONE_SAFETY
                .lock()
                .watchdog_matches(watchdog, generation, epoch),
            Self::Privacy {
                recovery_epoch,
                privacy_generation,
            } => screen_guide_privacy_reset_should_retry(
                recovery_epoch,
                SCREEN_GUIDE_OVERLAY_RECOVERY_EPOCH.load(Ordering::SeqCst),
                screen_guide_input_is_private(app),
                SCREEN_GUIDE_RUNTIME.lock().generation,
                privacy_generation,
            ),
        }
    }

    fn confirm_window_absent(self) {
        if let Self::Microphone {
            watchdog,
            generation,
            epoch,
        } = self
        {
            let mut safety = SCREEN_GUIDE_MICROPHONE_SAFETY.lock();
            if safety.watchdog_matches(watchdog, generation, epoch) {
                safety.reset();
            }
        }
    }

    fn reason(self) -> &'static str {
        match self {
            Self::Microphone { .. } => "microphone cutoff",
            Self::Privacy { .. } => "privacy transition",
        }
    }
}

#[derive(Debug, Default)]
struct ScreenGuideMicrophoneSafety {
    epoch: u64,
    active_generation: Option<u64>,
    pending_stop_generation: Option<u64>,
    last_stopped_generation: Option<u64>,
}

impl ScreenGuideMicrophoneSafety {
    fn request_stop(&mut self, generation: u64) -> Option<u64> {
        if self
            .last_stopped_generation
            .is_some_and(|stopped| generation <= stopped)
        {
            return None;
        }
        self.epoch = self.epoch.wrapping_add(1);
        self.pending_stop_generation = Some(generation);
        self.last_stopped_generation = Some(
            self.last_stopped_generation
                .map_or(generation, |previous| previous.max(generation)),
        );
        Some(self.epoch)
    }

    fn mark_started(&mut self, generation: u64) -> Option<u64> {
        if self
            .last_stopped_generation
            .is_some_and(|stopped| generation <= stopped)
        {
            return None;
        }
        self.epoch = self.epoch.wrapping_add(1);
        self.active_generation = Some(generation);
        self.pending_stop_generation = None;
        Some(self.epoch)
    }

    fn acknowledge_stop(&mut self, generation: u64) -> bool {
        let acknowledges_requested_stop = self.pending_stop_generation == Some(generation);
        let acknowledges_active_session =
            self.pending_stop_generation.is_none() && self.active_generation == Some(generation);
        if !acknowledges_requested_stop && !acknowledges_active_session {
            return false;
        }
        self.epoch = self.epoch.wrapping_add(1);
        self.pending_stop_generation = None;
        if self
            .active_generation
            .is_some_and(|active| active <= generation)
        {
            self.active_generation = None;
        }
        true
    }

    fn watchdog_matches(
        &self,
        watchdog: ScreenGuideMicrophoneWatchdog,
        generation: u64,
        epoch: u64,
    ) -> bool {
        if self.epoch != epoch {
            return false;
        }
        match watchdog {
            ScreenGuideMicrophoneWatchdog::StopAcknowledgement => {
                self.pending_stop_generation == Some(generation)
            }
            ScreenGuideMicrophoneWatchdog::MaximumDuration => {
                self.active_generation == Some(generation) && self.pending_stop_generation.is_none()
            }
        }
    }

    fn reset(&mut self) {
        self.epoch = self.epoch.wrapping_add(1);
        self.active_generation = None;
        self.pending_stop_generation = None;
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct DeferredFndrRestore {
    app_was_hidden: bool,
    window_labels: Vec<String>,
    owner_generation: u64,
}

#[derive(Debug)]
struct HideFndrWindowsFailure {
    hidden_window_labels: Vec<String>,
    message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ScreenGuideDisplaySignature {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    scale_factor_bits: u64,
}

impl DeferredFndrRestore {
    fn add_windows(&mut self, labels: Vec<String>) {
        for label in labels {
            if !self.window_labels.contains(&label) {
                self.window_labels.push(label);
            }
        }
    }

    fn is_empty(&self) -> bool {
        !self.app_was_hidden && self.window_labels.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PendingScreenGuideEvent {
    Shortcut {
        action: ScreenGuideShortcutAction,
        generation: u64,
    },
    Submit {
        text: String,
        generation: u64,
    },
}

impl PendingScreenGuideEvent {
    fn generation(&self) -> u64 {
        match self {
            Self::Shortcut { generation, .. } | Self::Submit { generation, .. } => *generation,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum ScreenGuideShortcutAction {
    Press,
    Release,
    Cancel,
}

#[derive(Debug, Clone, Copy)]
enum ScreenGuideShortcutTransition {
    Pressed,
    Released,
}

#[derive(Debug, Default)]
struct ScreenGuideOverlayDelivery {
    ready: bool,
    flushing: bool,
    overflowed: bool,
    pending: VecDeque<PendingScreenGuideEvent>,
}

impl ScreenGuideOverlayDelivery {
    fn discard_submits_for_generation(&mut self, generation: u64) {
        self.pending.retain(|event| {
            !matches!(
                event,
                PendingScreenGuideEvent::Submit {
                    generation: pending_generation,
                    ..
                } if *pending_generation == generation
            )
        });
    }

    fn deliver_or_queue(
        &mut self,
        event: PendingScreenGuideEvent,
    ) -> Option<PendingScreenGuideEvent> {
        if matches!(
            event,
            PendingScreenGuideEvent::Shortcut {
                action: ScreenGuideShortcutAction::Cancel,
                ..
            }
        ) {
            self.discard_submits_for_generation(event.generation());
        }
        if self.ready && !self.flushing {
            return Some(event);
        }
        if self.overflowed {
            return None;
        }
        if self.pending.len() == MAX_PENDING_SCREEN_GUIDE_EVENTS {
            // Never truncate one side of a press/release pair. A single cancel
            // is the only safe replay after startup has seen too many events.
            self.pending.clear();
            let generation = event.generation();
            self.pending.push_back(PendingScreenGuideEvent::Shortcut {
                action: ScreenGuideShortcutAction::Cancel,
                generation,
            });
            self.overflowed = true;
            return None;
        }
        self.pending.push_back(event);
        None
    }

    fn set_ready(&mut self, ready: bool) -> Vec<PendingScreenGuideEvent> {
        self.ready = ready;
        if ready {
            self.flushing = true;
            self.overflowed = false;
            self.pending.drain(..).collect()
        } else {
            // Queued submit events can contain a user's typed question. If the
            // overlay goes away, do not retain or replay that text later.
            self.pending.clear();
            self.flushing = false;
            self.overflowed = false;
            Vec::new()
        }
    }

    fn take_flush_batch(&mut self) -> Option<(Vec<PendingScreenGuideEvent>, bool)> {
        if self.pending.is_empty() {
            self.flushing = false;
            self.overflowed = false;
            return None;
        }
        let overflowed = std::mem::take(&mut self.overflowed);
        Some((self.pending.drain(..).collect(), overflowed))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScreenGuidePointCue {
    /// Horizontal coordinate normalized to the main display (0..=1).
    pub x: f64,
    /// Vertical coordinate normalized to the main display (0..=1).
    pub y: f64,
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScreenGuideAnswer {
    pub answer: String,
    pub point_cue: Option<ScreenGuidePointCue>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScreenGuideCursorPosition {
    /// Overlay-local logical pixels on the main display.
    pub x: f64,
    /// Overlay-local logical pixels on the main display.
    pub y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScreenGuideHistoryEntry {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize)]
struct ScreenGuideShortcutPayload {
    action: ScreenGuideShortcutAction,
    generation: u64,
}

#[derive(Debug, Clone, Serialize)]
struct ScreenGuideSubmitPayload {
    text: String,
    generation: u64,
}

/// Pre-create a full-main-display transparent overlay. It cannot take focus or
/// pointer events, so the application underneath remains interactive.
pub fn create_screen_guide_overlay_window<R: tauri::Runtime>(app: &AppHandle<R>) -> bool {
    if SCREEN_GUIDE_SHUTTING_DOWN.load(Ordering::SeqCst) {
        return false;
    }
    SCREEN_GUIDE_OVERLAY_SAFE.store(false, Ordering::SeqCst);
    let (x, y, width, height) = app
        .primary_monitor()
        .ok()
        .flatten()
        .map(|monitor| {
            let scale = monitor.scale_factor();
            let size = monitor.size();
            let position = monitor.position();
            (
                position.x as f64 / scale,
                position.y as f64 / scale,
                size.width as f64 / scale,
                size.height as f64 / scale,
            )
        })
        .unwrap_or((0.0, 0.0, 1440.0, 900.0));

    let url = tauri::WebviewUrl::App("screen-guide.html".into());
    match tauri::WebviewWindowBuilder::new(app, SCREEN_GUIDE_OVERLAY_LABEL, url)
        .title("FNDR Screen Guide")
        .position(x, y)
        .inner_size(width, height)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .visible_on_all_workspaces(true)
        .content_protected(true)
        .resizable(false)
        .skip_taskbar(true)
        .shadow(false)
        .focused(false)
        .focusable(false)
        .visible(false)
        .build()
    {
        Ok(window) => {
            if let Err(err) = window.set_ignore_cursor_events(true) {
                tracing::warn!(
                    "Screen Guide overlay disabled because it could not become click-through: {err}"
                );
                if let Err(destroy_err) = window.destroy() {
                    tracing::warn!("Screen Guide unsafe overlay cleanup failed: {destroy_err}");
                }
                return false;
            }
            if let Err(err) = configure_screen_guide_overlay_native_window(&window) {
                tracing::warn!(
                    "Screen Guide overlay disabled because native window setup failed: {err}"
                );
                let _ = window.destroy();
                return false;
            }
            SCREEN_GUIDE_OVERLAY_SAFE.store(true, Ordering::SeqCst);
            let teardown_handle = app.clone();
            window.on_window_event(move |event| {
                if !matches!(event, tauri::WindowEvent::Destroyed) {
                    return;
                }
                SCREEN_GUIDE_OVERLAY_SAFE.store(false, Ordering::SeqCst);
                SCREEN_GUIDE_OVERLAY_DELIVERY.lock().set_ready(false);
                SCREEN_GUIDE_MICROPHONE_SAFETY.lock().reset();
                let enabled = SCREEN_GUIDE_RUNTIME.lock().enabled;
                cancel_screen_guide_runtime(enabled);
                stop_say_process();
                // Observe restore state only after any in-flight capture has
                // unwound. The window can be destroyed between hiding FNDR and
                // publishing that restore state, so an eager snapshot can miss
                // work and leave FNDR hidden indefinitely.
                let restore_handle = teardown_handle.clone();
                let recovery_epoch = SCREEN_GUIDE_OVERLAY_RECOVERY_EPOCH
                    .fetch_add(1, Ordering::SeqCst)
                    .wrapping_add(1);
                tauri::async_runtime::spawn(async move {
                    let _capture = SCREEN_GUIDE_CAPTURE_COORDINATOR.lock().await;
                    let capture_owner = current_memory_capture_generation(&restore_handle);
                    let mut retry_delay = Duration::from_millis(250);
                    loop {
                        let cleanup_result = {
                            let _lifecycle = SCREEN_GUIDE_LIFECYCLE.lock();
                            cleanup_screen_guide_surfaces(&restore_handle)
                        };
                        if screen_guide_destroy_cleanup_can_release(true, cleanup_result.is_ok()) {
                            release_memory_capture_for_generation(&restore_handle, capture_owner);
                            break;
                        }
                        tokio::time::sleep(retry_delay).await;
                        retry_delay = (retry_delay * 2).min(Duration::from_secs(5));
                    }
                    drop(_capture);

                    // Recreate the native surface in-process so a WebView
                    // crash does not leave settings and the registered hotkey
                    // claiming that Screen Guide is available when it is not.
                    while !SCREEN_GUIDE_SHUTTING_DOWN.load(Ordering::SeqCst)
                        && SCREEN_GUIDE_OVERLAY_RECOVERY_EPOCH.load(Ordering::SeqCst)
                            == recovery_epoch
                    {
                        let callback_handle = restore_handle.clone();
                        let (done_tx, done_rx) = std::sync::mpsc::sync_channel(1);
                        if restore_handle
                            .run_on_main_thread(move || {
                                let existing_is_safe = SCREEN_GUIDE_OVERLAY_SAFE
                                    .load(Ordering::SeqCst)
                                    && callback_handle
                                        .get_webview_window(SCREEN_GUIDE_OVERLAY_LABEL)
                                        .is_some();
                                let recreated = existing_is_safe
                                    || (callback_handle
                                        .get_webview_window(SCREEN_GUIDE_OVERLAY_LABEL)
                                        .is_none()
                                        && create_screen_guide_overlay_window(&callback_handle));
                                let _ = done_tx.send(recreated);
                            })
                            .is_err()
                        {
                            break;
                        }
                        if done_rx.recv_timeout(Duration::from_secs(5)) == Ok(true) {
                            break;
                        }
                        tokio::time::sleep(retry_delay).await;
                        retry_delay = (retry_delay * 2).min(Duration::from_secs(5));
                    }
                });
            });
            tracing::info!("Screen Guide overlay pre-created");
            true
        }
        Err(err) => {
            tracing::warn!("Screen Guide overlay pre-creation failed: {err}");
            false
        }
    }
}

pub fn register_screen_guide_shortcut<R: tauri::Runtime>(
    app: &AppHandle<R>,
    config: &ScreenGuideConfig,
) -> Result<(), String> {
    if !config.enabled {
        SCREEN_GUIDE_RUNTIME.lock().enabled = false;
        *SCREEN_GUIDE_REGISTERED_SHORTCUT_ID.lock() = None;
        return Ok(());
    }
    config.validate()?;
    if !screen_guide_overlay_can_activate(
        SCREEN_GUIDE_OVERLAY_SAFE.load(Ordering::SeqCst),
        app.get_webview_window(SCREEN_GUIDE_OVERLAY_LABEL).is_some(),
    ) {
        SCREEN_GUIDE_RUNTIME.lock().enabled = false;
        return Err(
            "Screen Guide overlay is unavailable or not safely click-through; restart FNDR and try again."
                .to_string(),
        );
    }

    if let Some(state) = app.try_state::<Arc<AppState>>() {
        validate_screen_guide_shortcut_conflicts(config, &state.config.read().autofill)?;
    }

    let shortcut: Shortcut = config
        .shortcut
        .parse()
        .map_err(|err| format!("Invalid Screen Guide shortcut '{}': {err}", config.shortcut))?;
    if app.global_shortcut().is_registered(shortcut) {
        if *SCREEN_GUIDE_REGISTERED_SHORTCUT_ID.lock() == Some(shortcut.id()) {
            SCREEN_GUIDE_RUNTIME.lock().enabled = true;
            return Ok(());
        }
        return Err(format!(
            "Screen Guide shortcut '{}' is already used by another FNDR feature.",
            config.shortcut
        ));
    }

    // The plugin invokes handlers while holding its shortcut-registry mutex on
    // the macOS event thread. A FIFO worker keeps that callback nonblocking and
    // prevents window getters from participating in a lock inversion.
    let (shortcut_tx, shortcut_rx) = std::sync::mpsc::channel();
    let worker_handle = app.clone();
    std::thread::Builder::new()
        .name("fndr-screen-guide-shortcut".to_string())
        .spawn(move || {
            let mut active_generation = None;
            while let Ok(transition) = shortcut_rx.recv() {
                handle_screen_guide_shortcut_transition(
                    &worker_handle,
                    transition,
                    &mut active_generation,
                );
            }
        })
        .map_err(|err| format!("Could not start Screen Guide shortcut worker: {err}"))?;
    app.global_shortcut()
        .on_shortcut(shortcut, move |_app, _shortcut, event| {
            let transition = match event.state() {
                ShortcutState::Pressed => ScreenGuideShortcutTransition::Pressed,
                ShortcutState::Released => ScreenGuideShortcutTransition::Released,
            };
            let _ = shortcut_tx.send(transition);
        })
        .map_err(|err| err.to_string())?;
    *SCREEN_GUIDE_REGISTERED_SHORTCUT_ID.lock() = Some(shortcut.id());
    SCREEN_GUIDE_RUNTIME.lock().enabled = true;
    Ok(())
}

fn handle_screen_guide_shortcut_transition<R: tauri::Runtime>(
    app: &AppHandle<R>,
    transition: ScreenGuideShortcutTransition,
    active_generation: &mut Option<u64>,
) {
    match transition {
        ScreenGuideShortcutTransition::Pressed => {
            if !screen_guide_press_should_begin(
                *active_generation,
                screen_guide_input_is_private(app),
            ) {
                return;
            }
            let Ok(generation) = begin_screen_guide_input() else {
                return;
            };
            *active_generation = Some(generation);
            schedule_screen_guide_hidden_lease(app, generation);
            let _lifecycle = SCREEN_GUIDE_LIFECYCLE.lock();
            if !screen_guide_runtime_generation_is_current(generation) {
                return;
            }
            suppress_memory_capture_for_generation(app, generation);
            if let Err(err) = super::autofill::cancel_autofill_for_sibling_activation(app) {
                tracing::warn!("Screen Guide could not stop Autofill before input: {err}");
                if cancel_screen_guide_runtime_if_current(generation) {
                    let _ = finish_screen_guide_surface_cleanup(app, generation);
                }
                return;
            }
            stop_say_process();
            adopt_deferred_restore_for_generation(generation);
            if show_screen_guide_overlay(app).is_err() {
                if cancel_screen_guide_runtime_if_current(generation) {
                    let _ = finish_screen_guide_surface_cleanup(app, generation);
                }
                return;
            }
            if dispatch_screen_guide_event(
                app,
                PendingScreenGuideEvent::Shortcut {
                    action: ScreenGuideShortcutAction::Press,
                    generation,
                },
            )
            .is_err()
            {
                if cancel_screen_guide_runtime_if_current(generation) {
                    let _ = finish_screen_guide_surface_cleanup(app, generation);
                }
            } else if !screen_guide_runtime_generation_is_current(generation) {
                let _ = dispatch_screen_guide_event(
                    app,
                    PendingScreenGuideEvent::Shortcut {
                        action: ScreenGuideShortcutAction::Cancel,
                        generation,
                    },
                );
            }
        }
        ScreenGuideShortcutTransition::Released => {
            let Some(generation) = active_generation.take() else {
                return;
            };
            if dispatch_screen_guide_event(
                app,
                PendingScreenGuideEvent::Shortcut {
                    action: ScreenGuideShortcutAction::Release,
                    generation,
                },
            )
            .is_err()
            {
                if cancel_screen_guide_runtime_if_current(generation) {
                    let _lifecycle = SCREEN_GUIDE_LIFECYCLE.lock();
                    let _ = finish_screen_guide_surface_cleanup(app, generation);
                }
            }
        }
    }
}

fn unregister_screen_guide_shortcut<R: tauri::Runtime>(
    app: &AppHandle<R>,
    config: &ScreenGuideConfig,
) -> Result<(), String> {
    if !config.enabled {
        return Ok(());
    }
    let shortcut: Shortcut = config
        .shortcut
        .parse()
        .map_err(|err| format!("Invalid Screen Guide shortcut '{}': {err}", config.shortcut))?;
    if !screen_guide_owns_shortcut(*SCREEN_GUIDE_REGISTERED_SHORTCUT_ID.lock(), shortcut.id()) {
        return Ok(());
    }
    if app.global_shortcut().is_registered(shortcut) {
        app.global_shortcut()
            .unregister(shortcut)
            .map_err(|err| err.to_string())?;
    }
    if *SCREEN_GUIDE_REGISTERED_SHORTCUT_ID.lock() == Some(shortcut.id()) {
        *SCREEN_GUIDE_REGISTERED_SHORTCUT_ID.lock() = None;
    }
    Ok(())
}

fn screen_guide_owns_shortcut(owned_id: Option<u32>, candidate_id: u32) -> bool {
    owned_id == Some(candidate_id)
}

fn screen_guide_registrations_match(left: &ScreenGuideConfig, right: &ScreenGuideConfig) -> bool {
    if left.enabled != right.enabled {
        return false;
    }
    if !left.enabled {
        return true;
    }
    match (
        left.shortcut.parse::<Shortcut>(),
        right.shortcut.parse::<Shortcut>(),
    ) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

fn screen_guide_settings_after_startup_registration(
    mut settings: ScreenGuideConfig,
    registered: bool,
) -> ScreenGuideConfig {
    if !registered {
        settings.enabled = false;
    }
    settings
}

pub fn reconcile_screen_guide_startup_failure(state: &AppState) -> Result<(), String> {
    SCREEN_GUIDE_RUNTIME.lock().enabled = false;
    let mut config = state.config.write();
    config.screen_guide =
        screen_guide_settings_after_startup_registration(config.screen_guide.clone(), false);
    config
        .save()
        .map_err(|err: Box<dyn std::error::Error>| err.to_string())
}

fn replace_screen_guide_shortcut_registration<R: tauri::Runtime>(
    app: &AppHandle<R>,
    previous: &ScreenGuideConfig,
    next: &ScreenGuideConfig,
) -> Result<(), String> {
    unregister_screen_guide_shortcut(app, previous)?;
    if let Err(err) = register_screen_guide_shortcut(app, next) {
        let rollback = register_screen_guide_shortcut(app, previous);
        return match rollback {
            Ok(()) => Err(err),
            Err(rollback_err) => Err(format!(
                "{err} Previous Screen Guide shortcut also could not be restored: {rollback_err}"
            )),
        };
    }
    Ok(())
}

#[tauri::command]
pub async fn get_screen_guide_settings(
    state: State<'_, Arc<AppState>>,
) -> Result<ScreenGuideConfig, String> {
    Ok(state
        .inner()
        .config
        .read()
        .screen_guide
        .clone()
        .normalized())
}

#[tauri::command]
pub async fn set_screen_guide_settings(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    settings: ScreenGuideConfig,
) -> Result<ScreenGuideConfig, String> {
    let mut normalized = settings.normalized();
    normalized.validate()?;
    let canonical: Shortcut = normalized.shortcut.parse().map_err(|err| {
        format!(
            "Invalid Screen Guide shortcut '{}': {err}",
            normalized.shortcut
        )
    })?;
    normalized.shortcut = canonical.into_string();

    let autofill = state.inner().config.read().autofill.clone().normalized();
    validate_screen_guide_shortcut_conflicts(&normalized, &autofill)?;

    let previous = state
        .inner()
        .config
        .read()
        .screen_guide
        .clone()
        .normalized();
    {
        // Invalidate both native and renderer-side work. The distinct cancel
        // event discards chunks instead of turning cancellation into a submit.
        // Cancel in-flight model/transcription work immediately, then take the
        // short window gate and advance once more so any press that raced the
        // gate becomes stale before settings hide the overlay.
        cancel_screen_guide_runtime(normalized.enabled);
        let _lifecycle = SCREEN_GUIDE_LIFECYCLE.lock();
        let cancel_generation = cancel_screen_guide_runtime(normalized.enabled);
        stop_say_process();
        if let Err(err) = dispatch_screen_guide_event(
            &app,
            PendingScreenGuideEvent::Shortcut {
                action: ScreenGuideShortcutAction::Cancel,
                generation: cancel_generation,
            },
        ) {
            cancel_screen_guide_runtime(previous.enabled);
            let _ = finish_current_screen_guide_surface_cleanup(&app);
            return Err(err);
        }
        // A settings change invalidates the displayed answer/cue as well as
        // the request that produced it. The hidden WebView keeps its listeners.
        if let Err(err) = finish_current_screen_guide_surface_cleanup(&app) {
            cancel_screen_guide_runtime(previous.enabled);
            return Err(err);
        }
        if !normalized.enabled {
            SCREEN_GUIDE_OVERLAY_DELIVERY.lock().pending.clear();
        }
    }

    let registration_changed = !screen_guide_registrations_match(&previous, &normalized);
    if registration_changed {
        if let Err(err) = replace_screen_guide_shortcut_registration(&app, &previous, &normalized) {
            let _lifecycle = SCREEN_GUIDE_LIFECYCLE.lock();
            cancel_screen_guide_runtime(previous.enabled);
            return Err(err);
        }
    }

    let save_result = {
        let mut config = state.inner().config.write();
        let persisted_previous = config.screen_guide.clone();
        config.screen_guide = normalized.clone();
        let result = config
            .save()
            .map_err(|err: Box<dyn std::error::Error>| err.to_string());
        if result.is_err() {
            config.screen_guide = persisted_previous;
        }
        result
    };
    if let Err(err) = save_result {
        if registration_changed {
            let _ = replace_screen_guide_shortcut_registration(&app, &normalized, &previous);
        }
        let _lifecycle = SCREEN_GUIDE_LIFECYCLE.lock();
        cancel_screen_guide_runtime(previous.enabled);
        return Err(err);
    }

    Ok(normalized)
}

#[tauri::command]
pub async fn screen_guide_press(app: AppHandle) -> Result<u64, String> {
    if screen_guide_input_is_private(&app) {
        return Err(private_screen_message());
    }
    let generation = begin_screen_guide_input()?;
    schedule_screen_guide_hidden_lease(&app, generation);
    let _lifecycle = SCREEN_GUIDE_LIFECYCLE.lock();
    if !screen_guide_runtime_generation_is_current(generation) {
        return Err(screen_guide_cancelled_message());
    }
    suppress_memory_capture_for_generation(&app, generation);
    if let Err(err) = super::autofill::cancel_autofill_for_sibling_activation(&app) {
        if cancel_screen_guide_runtime_if_current(generation) {
            let _ = finish_screen_guide_surface_cleanup(&app, generation);
        }
        return Err(format!(
            "Screen Guide could not stop Autofill before starting: {err}"
        ));
    }
    stop_say_process();
    adopt_deferred_restore_for_generation(generation);
    if let Err(err) = show_screen_guide_overlay(&app) {
        if cancel_screen_guide_runtime_if_current(generation) {
            let _ = finish_screen_guide_surface_cleanup(&app, generation);
        }
        return Err(err);
    }
    if let Err(err) = dispatch_screen_guide_event(
        &app,
        PendingScreenGuideEvent::Shortcut {
            action: ScreenGuideShortcutAction::Press,
            generation,
        },
    ) {
        if cancel_screen_guide_runtime_if_current(generation) {
            let _ = finish_screen_guide_surface_cleanup(&app, generation);
        }
        return Err(err);
    }
    if !screen_guide_runtime_generation_is_current(generation) {
        let _ = dispatch_screen_guide_event(
            &app,
            PendingScreenGuideEvent::Shortcut {
                action: ScreenGuideShortcutAction::Cancel,
                generation,
            },
        );
        return Err(screen_guide_cancelled_message());
    }
    Ok(generation)
}

#[tauri::command]
pub async fn screen_guide_release(app: AppHandle, generation: u64) -> Result<(), String> {
    if let Err(err) = dispatch_screen_guide_event(
        &app,
        PendingScreenGuideEvent::Shortcut {
            action: ScreenGuideShortcutAction::Release,
            generation,
        },
    ) {
        if cancel_screen_guide_runtime_if_current(generation) {
            let _lifecycle = SCREEN_GUIDE_LIFECYCLE.lock();
            let _ = finish_screen_guide_surface_cleanup(&app, generation);
        }
        return Err(err);
    }
    Ok(())
}

#[tauri::command]
pub async fn submit_screen_guide_text(app: AppHandle, text: String) -> Result<(), String> {
    let text = truncate_chars(text.trim(), MAX_SCREEN_GUIDE_QUESTION_CHARS);
    if text.is_empty() {
        return Err("Screen Guide needs a question.".to_string());
    }
    if screen_guide_input_is_private(&app) {
        return Err(private_screen_message());
    }
    let generation = begin_screen_guide_input()?;
    schedule_screen_guide_hidden_lease(&app, generation);
    let _lifecycle = SCREEN_GUIDE_LIFECYCLE.lock();
    if !screen_guide_runtime_generation_is_current(generation) {
        return Err(screen_guide_cancelled_message());
    }
    suppress_memory_capture_for_generation(&app, generation);
    if let Err(err) = super::autofill::cancel_autofill_for_sibling_activation(&app) {
        if cancel_screen_guide_runtime_if_current(generation) {
            let _ = finish_screen_guide_surface_cleanup(&app, generation);
        }
        return Err(format!(
            "Screen Guide could not stop Autofill before starting: {err}"
        ));
    }
    stop_say_process();
    adopt_deferred_restore_for_generation(generation);
    if let Err(err) = show_screen_guide_overlay(&app) {
        if cancel_screen_guide_runtime_if_current(generation) {
            let _ = finish_screen_guide_surface_cleanup(&app, generation);
        }
        return Err(err);
    }
    if let Err(err) =
        dispatch_screen_guide_event(&app, PendingScreenGuideEvent::Submit { text, generation })
    {
        if cancel_screen_guide_runtime_if_current(generation) {
            let _ = finish_screen_guide_surface_cleanup(&app, generation);
        }
        return Err(err);
    }
    if !screen_guide_runtime_generation_is_current(generation) {
        let _ = dispatch_screen_guide_event(
            &app,
            PendingScreenGuideEvent::Shortcut {
                action: ScreenGuideShortcutAction::Cancel,
                generation,
            },
        );
        return Err(screen_guide_cancelled_message());
    }
    Ok(())
}

/// Explicit give-up hook for the frontend: called when a transcription or
/// ask-the-screen call has run past its client-side timeout. Cancelling a
/// generation that is no longer current is a harmless no-op, so the caller
/// does not need to know whether the turn already finished on its own.
#[tauri::command]
pub async fn cancel_screen_guide_turn(app: AppHandle, generation: u64) -> Result<(), String> {
    cancel_screen_guide_request_and_notify(&app, generation);
    Ok(())
}

#[tauri::command]
pub async fn transcribe_screen_guide_voice_input(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    audio_bytes: Vec<u8>,
    mime_type: Option<String>,
    request_id: u64,
) -> Result<super::stats::VoiceTranscriptionResult, String> {
    let transcribe_started = Instant::now();
    tracing::info!(request_id, audio_bytes = audio_bytes.len(), "screen_guide:transcribe_started");
    if state.inner().is_incognito.load(Ordering::SeqCst) {
        return Err(private_screen_message());
    }
    let cancel = screen_guide_turn_cancel(request_id)?;
    schedule_screen_guide_hidden_lease(&app, request_id);
    let app_data_dir = crate::config::fndr_app_data_dir(app.path()).map_err(|err| err.to_string())?;
    let transcription =
        speech::transcribe_audio_bytes(&app_data_dir, &audio_bytes, mime_type.as_deref());
    tokio::pin!(transcription);
    let cancellation = wait_for_screen_guide_turn_cancellation(Arc::clone(&cancel));
    tokio::pin!(cancellation);

    let text = tokio::select! {
        result = &mut transcription => match result {
            Ok(text) => text,
            Err(err) => {
                tracing::warn!(
                    elapsed_ms = transcribe_started.elapsed().as_millis() as u64,
                    error = %err,
                    "screen_guide:transcribe_failed"
                );
                return Err(err);
            }
        },
        () = &mut cancellation => {
            tracing::info!(
                elapsed_ms = transcribe_started.elapsed().as_millis() as u64,
                "screen_guide:transcribe_cancelled"
            );
            return Err(screen_guide_cancelled_message());
        }
    };
    ensure_screen_guide_request_current(state.inner(), request_id)?;
    tracing::info!(
        elapsed_ms = transcribe_started.elapsed().as_millis() as u64,
        "screen_guide:transcribe_finished"
    );

    Ok(super::stats::VoiceTranscriptionResult {
        text,
        backend: "whisper-small-ggml (enhanced mic mode)".to_string(),
    })
}

/// Called by the hidden overlay after its event listeners are mounted. Events
/// produced during WebView startup are replayed in order instead of being lost.
#[tauri::command]
pub async fn set_screen_guide_overlay_ready(app: AppHandle, ready: bool) -> Result<(), String> {
    if ready
        && !screen_guide_overlay_can_activate(
            SCREEN_GUIDE_OVERLAY_SAFE.load(Ordering::SeqCst),
            app.get_webview_window(SCREEN_GUIDE_OVERLAY_LABEL).is_some(),
        )
    {
        return Err("Screen Guide overlay is not safely click-through.".to_string());
    }

    let _lifecycle = SCREEN_GUIDE_LIFECYCLE.lock();
    let (mut pending, mut overflowed) = {
        let mut delivery = SCREEN_GUIDE_OVERLAY_DELIVERY.lock();
        let overflowed = delivery.overflowed;
        (delivery.set_ready(ready), overflowed)
    };
    if !ready {
        // The renderer calls this only after stopping every MediaStream track.
        // Destruction is covered independently by the native watchdog.
        SCREEN_GUIDE_MICROPHONE_SAFETY.lock().reset();
        let enabled = SCREEN_GUIDE_RUNTIME.lock().enabled;
        cancel_screen_guide_runtime(enabled);
        stop_say_process();
        return finish_current_screen_guide_surface_cleanup(&app);
    }
    loop {
        if overflowed {
            let enabled = SCREEN_GUIDE_RUNTIME.lock().enabled;
            cancel_screen_guide_runtime(enabled);
            stop_say_process();
            let _ = finish_current_screen_guide_surface_cleanup(&app);
        }
        for event in pending {
            if let Err(err) = emit_screen_guide_event(&app, event) {
                SCREEN_GUIDE_OVERLAY_DELIVERY.lock().set_ready(false);
                let enabled = SCREEN_GUIDE_RUNTIME.lock().enabled;
                cancel_screen_guide_runtime(enabled);
                stop_say_process();
                let _ = finish_current_screen_guide_surface_cleanup(&app);
                return Err(err);
            }
        }
        let Some((next_pending, next_overflowed)) =
            SCREEN_GUIDE_OVERLAY_DELIVERY.lock().take_flush_batch()
        else {
            break;
        };
        pending = next_pending;
        overflowed = next_overflowed;
    }
    Ok(())
}

#[tauri::command]
pub async fn screen_guide_microphone_started(
    app: AppHandle,
    generation: u64,
) -> Result<(), String> {
    let epoch = {
        let runtime = SCREEN_GUIDE_RUNTIME.lock();
        if !runtime.enabled
            || runtime.generation != generation
            || screen_guide_input_is_private(&app)
        {
            return Err(screen_guide_cancelled_message());
        }
        SCREEN_GUIDE_MICROPHONE_SAFETY
            .lock()
            .mark_started(generation)
            .ok_or_else(screen_guide_cancelled_message)?
    };
    schedule_screen_guide_microphone_watchdog(
        &app,
        ScreenGuideMicrophoneWatchdog::MaximumDuration,
        generation,
        epoch,
        SCREEN_GUIDE_MICROPHONE_MAX_DURATION,
    );
    Ok(())
}

#[tauri::command]
pub async fn acknowledge_screen_guide_microphone_stopped(generation: u64) -> bool {
    SCREEN_GUIDE_MICROPHONE_SAFETY
        .lock()
        .acknowledge_stop(generation)
}

#[tauri::command]
pub async fn get_screen_guide_cursor_position(
    app: AppHandle,
) -> Result<ScreenGuideCursorPosition, String> {
    let cursor = app
        .cursor_position()
        .map_err(|err| format!("Could not read pointer location: {err}"))?;
    let monitor = app
        .primary_monitor()
        .map_err(|err| err.to_string())?
        .ok_or_else(|| "Main display is unavailable.".to_string())?;
    Ok(screen_guide_cursor_in_monitor(
        cursor.x,
        cursor.y,
        monitor.position().x,
        monitor.position().y,
        monitor.size().width,
        monitor.size().height,
        monitor.scale_factor(),
    ))
}

fn screen_guide_cursor_in_monitor(
    cursor_x: f64,
    cursor_y: f64,
    monitor_x: i32,
    monitor_y: i32,
    monitor_width: u32,
    monitor_height: u32,
    scale_factor: f64,
) -> ScreenGuideCursorPosition {
    let width = monitor_width as f64 / scale_factor;
    let height = monitor_height as f64 / scale_factor;
    ScreenGuideCursorPosition {
        x: ((cursor_x - monitor_x as f64) / scale_factor).clamp(0.0, width),
        y: ((cursor_y - monitor_y as f64) / scale_factor).clamp(0.0, height),
    }
}

/// Hide a completed turn without interrupting its local speech. The renderer
/// passes the client request that scheduled the timer; a timer from an older
/// turn becomes a no-op as soon as new input clears/replaces that identifier.
#[tauri::command]
pub async fn finish_screen_guide_visual(app: AppHandle, request_id: u64) -> Result<bool, String> {
    let _lifecycle = SCREEN_GUIDE_LIFECYCLE.lock();
    let should_finish = {
        let runtime = SCREEN_GUIDE_RUNTIME.lock();
        screen_guide_visual_matches(request_id, runtime.generation, runtime.enabled)
    };
    if !should_finish {
        return Ok(false);
    }

    let result = finish_screen_guide_surface_cleanup(&app, request_id);
    if result.is_ok() {
        let mut runtime = SCREEN_GUIDE_RUNTIME.lock();
        if runtime.generation == request_id {
            let mut delivery = SCREEN_GUIDE_OVERLAY_DELIVERY.lock();
            invalidate_screen_guide_turn(&mut runtime, &mut delivery);
        }
    }
    result.map(|_| true)
}

#[tauri::command]
pub async fn ask_screen_guide(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    question: String,
    history: Vec<ScreenGuideHistoryEntry>,
    request_id: u64,
) -> Result<ScreenGuideAnswer, String> {
    let ask_started = Instant::now();
    tracing::info!(request_id, "screen_guide:ask_started");
    let question = truncate_chars(question.trim(), MAX_SCREEN_GUIDE_QUESTION_CHARS);
    if question.is_empty() {
        return Err("Screen Guide needs a question.".to_string());
    }

    let settings = {
        let config = state.inner().config.read();
        config.screen_guide.clone().normalized()
    };
    if !settings.enabled {
        return Err("Screen Guide is turned off.".to_string());
    }
    if state.inner().is_incognito.load(Ordering::SeqCst) {
        return Err(private_screen_message());
    }

    let (request_generation, turn_cancel) = {
        // The native input generation owns the interaction end-to-end. A
        // delayed renderer request cannot supersede a newer press or submit.
        let _lifecycle = SCREEN_GUIDE_LIFECYCLE.lock();
        let (generation, cancel) = continue_screen_guide_request(request_id)?;
        suppress_memory_capture_for_generation(&app, generation);
        schedule_screen_guide_hidden_lease(&app, generation);
        (generation, cancel)
    };

    let (has_capture_access, permission_detail) =
        crate::capture::permissions::preflight_screen_capture_access();
    if !has_capture_access {
        return Err(format!(
            "Screen Guide needs Screen Recording access. {permission_detail}"
        ));
    }

    let capture_coordinator = SCREEN_GUIDE_CAPTURE_COORDINATOR.lock().await;
    {
        let _lifecycle = SCREEN_GUIDE_LIFECYCLE.lock();
        ensure_screen_guide_request_current(state.inner(), request_generation)?;
        hide_screen_guide_overlay(&app)?;
    }

    // Hide first, then inspect the newly exposed frontmost context. Privacy is
    // decided before CGDisplayCreateImage is ever called. Settling and context
    // probes stay outside the lifecycle lock so new input cancels immediately.
    std::thread::sleep(OVERLAY_CAPTURE_SETTLE);

    // A prior answer may already be keeping FNDR out of the way. Carry that
    // restore state into this turn so a superseding question cannot orphan a
    // hidden panel.
    let mut deferred_restore = SCREEN_GUIDE_DEFERRED_RESTORE.lock().clone();
    let initial_context = crate::capture::macos::get_frontmost_app_info_fresh();
    let fndr_was_frontmost = Blocklist::is_internal_app(
        &initial_context.app_name,
        initial_context.bundle_id.as_deref(),
    );
    // Every visible FNDR WebView is excluded, even when a different app is
    // frontmost: a side-by-side FNDR window must never enter transient OCR.
    // This is a short window-mutation section. A newer turn may advance the
    // generation immediately, then waits only for these native calls to finish.
    let prepare_result = (|| -> Result<(), String> {
        let _lifecycle = SCREEN_GUIDE_LIFECYCLE.lock();
        ensure_screen_guide_request_current(state.inner(), request_generation)?;
        match hide_visible_fndr_windows(&app) {
            Ok(hidden_fndr_windows) => deferred_restore.add_windows(hidden_fndr_windows),
            Err(failure) => {
                // Publish every window already mutated before returning the
                // error. The common restore path can then retry instead of
                // orphaning a partially hidden FNDR surface.
                deferred_restore.add_windows(failure.hidden_window_labels);
                deferred_restore.owner_generation = request_generation;
                *SCREEN_GUIDE_DEFERRED_RESTORE.lock() = deferred_restore.clone();
                return Err(failure.message);
            }
        }
        deferred_restore.owner_generation = request_generation;
        *SCREEN_GUIDE_DEFERRED_RESTORE.lock() = deferred_restore.clone();
        if !screen_guide_runtime_generation_is_current(request_generation) {
            return Err(screen_guide_cancelled_message());
        }
        if fndr_was_frontmost {
            app.hide()
                .map_err(|err| format!("Screen Guide could not hide FNDR before capture: {err}"))?;
            deferred_restore.app_was_hidden = true;
            *SCREEN_GUIDE_DEFERRED_RESTORE.lock() = deferred_restore.clone();
        }
        ensure_screen_guide_request_current(state.inner(), request_generation)
    })();
    if let Err(err) = prepare_result {
        restore_screen_guide_capture_if_owned(
            &app,
            state.inner(),
            request_generation,
            &deferred_restore,
        );
        return Err(err);
    }
    schedule_screen_guide_hidden_lease(&app, request_generation);

    // Re-evaluate after all FNDR surfaces are hidden and, when necessary,
    // macOS has activated the app that was behind FNDR.
    std::thread::sleep(OVERLAY_CAPTURE_SETTLE);
    let context = crate::capture::macos::get_frontmost_app_info_fresh();

    if let Err(err) = ensure_screen_guide_request_current_or_restore(
        &app,
        state.inner(),
        request_generation,
        &deferred_restore,
    ) {
        return Err(err);
    }

    if state.inner().is_incognito.load(Ordering::SeqCst) {
        restore_screen_guide_capture_if_owned(
            &app,
            state.inner(),
            request_generation,
            &deferred_restore,
        );
        return Err(private_screen_message());
    }

    let blocklist = state.inner().config.read().blocklist.clone();
    let url = context.browser_url.clone();
    if let Some(err) = screen_guide_context_verification_error(&context, url.as_deref()) {
        restore_screen_guide_capture_if_owned(
            &app,
            state.inner(),
            request_generation,
            &deferred_restore,
        );
        return Err(err);
    }
    if !screen_guide_context_can_be_captured(&context, url.as_deref(), &blocklist) {
        restore_screen_guide_capture_if_owned(
            &app,
            state.inner(),
            request_generation,
            &deferred_restore,
        );
        return Err(private_screen_message());
    }

    if let Err(err) = ensure_screen_guide_request_current_or_restore(
        &app,
        state.inner(),
        request_generation,
        &deferred_restore,
    ) {
        return Err(err);
    }

    // Omnibar/Autofill cannot reopen and a newer Screen Guide overlay cannot
    // appear inside the synchronous CG capture. All slower context probes stay
    // outside this section.
    let capture_result = (|| -> Result<(ScreenGuideDisplaySignature, Vec<u8>), String> {
        let _lifecycle = SCREEN_GUIDE_LIFECYCLE.lock();
        ensure_screen_guide_request_current(state.inner(), request_generation)?;
        ensure_no_visible_fndr_windows(&app)?;
        let display = screen_guide_main_display_signature(&app)?;
        let image = crate::capture::macos::capture_screen()
            .map_err(|_| "Screen Guide could not capture the main display.".to_string())?;
        Ok((display, image))
    })();
    let (captured_display, image_data) = match capture_result {
        Ok(capture) => capture,
        Err(err) => {
            restore_screen_guide_capture_if_owned(
                &app,
                state.inner(),
                request_generation,
                &deferred_restore,
            );
            return Err(err);
        }
    };
    if let Err(err) = ensure_screen_guide_request_current_or_restore(
        &app,
        state.inner(),
        request_generation,
        &deferred_restore,
    ) {
        return Err(err);
    }
    let post_capture_context = crate::capture::macos::get_frontmost_app_info_fresh();
    let post_capture_url = post_capture_context.browser_url.clone();
    let post_capture_display = screen_guide_main_display_signature(&app).ok();
    let post_capture_blocklist = state.inner().config.read().blocklist.clone();
    if !screen_guide_context_can_be_captured(
        &post_capture_context,
        post_capture_url.as_deref(),
        &post_capture_blocklist,
    ) || post_capture_display
        .as_ref()
        .map(|display| {
            !screen_guide_target_is_unchanged(
                &context,
                url.as_deref(),
                &captured_display,
                &post_capture_context,
                post_capture_url.as_deref(),
                display,
            )
        })
        .unwrap_or(true)
    {
        restore_screen_guide_capture_if_owned(
            &app,
            state.inner(),
            request_generation,
            &deferred_restore,
        );
        return Err(
            "Screen Guide paused because the visible screen changed during capture.".to_string(),
        );
    }
    let show_result = (|| -> Result<(), String> {
        let _lifecycle = SCREEN_GUIDE_LIFECYCLE.lock();
        ensure_screen_guide_request_current(state.inner(), request_generation)?;
        show_screen_guide_overlay(&app)?;
        ensure_screen_guide_request_current(state.inner(), request_generation)
    })();
    if let Err(err) = show_result {
        restore_screen_guide_capture_if_owned(
            &app,
            state.inner(),
            request_generation,
            &deferred_restore,
        );
        return Err(err);
    }
    schedule_screen_guide_hidden_lease(&app, request_generation);
    drop(capture_coordinator);

    ensure_screen_guide_request_current_or_restore(
        &app,
        state.inner(),
        request_generation,
        &deferred_restore,
    )?;

    let ocr = tokio::task::spawn_blocking(move || {
        let engine =
            OcrEngine::new().map_err(|_| "Screen Guide OCR is unavailable.".to_string())?;
        engine
            .recognize_screen_guide(&image_data)
            .map_err(|_| "Screen Guide could not read this screen.".to_string())
    })
    .await
    .map_err(|_| "Screen Guide OCR stopped unexpectedly.".to_string())??;

    ensure_screen_guide_request_current_or_restore(
        &app,
        state.inner(),
        request_generation,
        &deferred_restore,
    )?;
    if state.inner().is_incognito.load(Ordering::SeqCst) {
        restore_screen_guide_capture_if_owned(
            &app,
            state.inner(),
            request_generation,
            &deferred_restore,
        );
        return Err(private_screen_message());
    }

    if ocr.plain_text.trim().is_empty() {
        let result = finish_screen_guide_answer(
            state.inner(),
            &settings,
            request_generation,
            ScreenGuideAnswer {
                answer: "I couldn't find readable text on this screen.".to_string(),
                point_cue: None,
            },
        );
        tracing::info!(
            elapsed_ms = ask_started.elapsed().as_millis() as u64,
            ok = result.is_ok(),
            "screen_guide:ask_finished_no_readable_text"
        );
        return result;
    }

    // A secret detected only after OCR must not proceed into model inference or
    // speech, even though nothing in this path is persisted.
    let latest_blocklist = state.inner().config.read().blocklist.clone();
    if !screen_guide_ocr_is_allowed(&context, url.as_deref(), &ocr.plain_text, &latest_blocklist) {
        restore_screen_guide_capture_if_owned(
            &app,
            state.inner(),
            request_generation,
            &deferred_restore,
        );
        return Err(private_screen_message());
    }

    let positioned_ocr = if ocr.lines.is_empty() {
        ocr.plain_text.clone()
    } else {
        ocr.position_annotated_text()
    };
    let screen_text = truncate_chars(positioned_ocr.trim(), MAX_SCREEN_GUIDE_OCR_CHARS);
    let history_text = transient_history_text(&history);
    let fallback = grounded_fallback(&ocr.plain_text);

    let inference_engine = state.inner().ensure_inference_engine().await;
    ensure_screen_guide_request_current_or_restore(
        &app,
        state.inner(),
        request_generation,
        &deferred_restore,
    )?;
    let inference_started = Instant::now();
    let raw_answer = match inference_engine {
        Ok(Some(engine)) => {
            tracing::info!("screen_guide:inference_started");
            let _pipeline_guard = state.inner().model_pipeline_lock.lock().await;
            ensure_screen_guide_request_current_or_restore(
                &app,
                state.inner(),
                request_generation,
                &deferred_restore,
            )?;
            let answer = engine
                .answer_screen_guide(
                    &question,
                    &screen_text,
                    &history_text,
                    Arc::clone(&turn_cancel),
                    SCREEN_GUIDE_INFERENCE_TIMEOUT,
                )
                .await;
            tracing::info!(
                elapsed_ms = inference_started.elapsed().as_millis() as u64,
                usable = is_usable_model_answer(&answer),
                "screen_guide:inference_finished"
            );
            answer
        }
        Ok(None) | Err(_) => {
            tracing::warn!("screen_guide:inference_engine_unavailable");
            String::new()
        }
    };

    ensure_screen_guide_request_current_or_restore(
        &app,
        state.inner(),
        request_generation,
        &deferred_restore,
    )?;
    let parsed = if is_usable_model_answer(&raw_answer) {
        let mut grounded = ground_screen_guide_point_cue(
            parse_screen_guide_response(&raw_answer),
            &ocr.lines,
            settings.show_cursor,
        );
        if let Some(cue) = grounded.point_cue.as_ref() {
            if !screen_guide_cue_is_fresh(
                &app,
                state.inner(),
                request_generation,
                &context,
                url.as_deref(),
                &captured_display,
                cue,
            )
            .await
            {
                grounded.point_cue = None;
            }
        }
        grounded
    } else {
        fallback
    };
    let result = finish_screen_guide_answer(state.inner(), &settings, request_generation, parsed);
    tracing::info!(
        elapsed_ms = ask_started.elapsed().as_millis() as u64,
        ok = result.is_ok(),
        "screen_guide:ask_finished"
    );
    result
}

fn show_screen_guide_overlay<R: tauri::Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    if screen_guide_input_is_private(app) {
        return Err(private_screen_message());
    }
    if !screen_guide_overlay_can_activate(
        SCREEN_GUIDE_OVERLAY_SAFE.load(Ordering::SeqCst),
        app.get_webview_window(SCREEN_GUIDE_OVERLAY_LABEL).is_some(),
    ) {
        return Err("Screen Guide overlay is not safely click-through.".to_string());
    }
    let window = app
        .get_webview_window(SCREEN_GUIDE_OVERLAY_LABEL)
        .ok_or_else(|| "Screen Guide overlay is unavailable.".to_string())?;
    let monitor = app
        .primary_monitor()
        .map_err(|err| err.to_string())?
        .ok_or_else(|| "Main display is unavailable.".to_string())?;
    window
        .set_position(tauri::PhysicalPosition::new(
            monitor.position().x,
            monitor.position().y,
        ))
        .map_err(|err| format!("Screen Guide could not follow the main display: {err}"))?;
    window
        .set_size(tauri::PhysicalSize::new(
            monitor.size().width,
            monitor.size().height,
        ))
        .map_err(|err| format!("Screen Guide could not fit the main display: {err}"))?;
    order_window_front_without_focus(app, &window)
}

#[cfg(target_os = "macos")]
fn configure_screen_guide_overlay_native_window<R: tauri::Runtime>(
    window: &tauri::WebviewWindow<R>,
) -> Result<(), String> {
    if objc2_foundation::MainThreadMarker::new().is_none() {
        return Err("native overlay setup did not run on the main thread".to_string());
    }
    let pointer = window.ns_window().map_err(|err| err.to_string())?;
    let native_window = unsafe { &*(pointer as *const objc2_app_kit::NSWindow) };
    // The overlay must remain independently showable when the user has hidden
    // FNDR with Cmd-H. Other FNDR windows retain their normal AppKit behavior.
    unsafe { native_window.setCanHide(false) };
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn configure_screen_guide_overlay_native_window<R: tauri::Runtime>(
    _window: &tauri::WebviewWindow<R>,
) -> Result<(), String> {
    Ok(())
}

#[cfg(target_os = "macos")]
fn unhide_application_without_activation<R: tauri::Runtime>(
    app: &AppHandle<R>,
) -> Result<(), String> {
    if let Some(marker) = objc2_foundation::MainThreadMarker::new() {
        unsafe {
            objc2_app_kit::NSApplication::sharedApplication(marker).unhideWithoutActivation();
        }
        return Ok(());
    }

    let (done_tx, done_rx) = std::sync::mpsc::sync_channel(1);
    app.run_on_main_thread(move || {
        let result = objc2_foundation::MainThreadMarker::new()
            .ok_or_else(|| {
                "Screen Guide app visibility callback missed the main thread".to_string()
            })
            .map(|marker| unsafe {
                objc2_app_kit::NSApplication::sharedApplication(marker).unhideWithoutActivation();
            });
        let _ = done_tx.send(result);
    })
    .map_err(|err| err.to_string())?;
    done_rx
        .recv_timeout(Duration::from_secs(2))
        .map_err(|_| "Screen Guide timed out while restoring FNDR visibility.".to_string())?
}

#[cfg(not(target_os = "macos"))]
fn unhide_application_without_activation<R: tauri::Runtime>(
    _app: &AppHandle<R>,
) -> Result<(), String> {
    Ok(())
}

#[cfg(target_os = "macos")]
fn order_window_front_without_focus<R: tauri::Runtime>(
    app: &AppHandle<R>,
    window: &tauri::WebviewWindow<R>,
) -> Result<(), String> {
    if objc2_foundation::MainThreadMarker::new().is_some() {
        let pointer = window.ns_window().map_err(|err| err.to_string())?;
        let native_window = unsafe { &*(pointer as *const objc2_app_kit::NSWindow) };
        unsafe { native_window.orderFrontRegardless() };
        return Ok(());
    }

    let window = window.clone();
    let (done_tx, done_rx) = std::sync::mpsc::sync_channel(1);
    app.run_on_main_thread(move || {
        let result = window
            .ns_window()
            .map_err(|err| err.to_string())
            .map(|pointer| {
                let native_window = unsafe { &*(pointer as *const objc2_app_kit::NSWindow) };
                unsafe { native_window.orderFrontRegardless() };
            });
        let _ = done_tx.send(result);
    })
    .map_err(|err| err.to_string())?;
    done_rx
        .recv_timeout(Duration::from_secs(2))
        .map_err(|_| "Screen Guide timed out while restoring an FNDR window.".to_string())?
}

#[cfg(not(target_os = "macos"))]
fn order_window_front_without_focus<R: tauri::Runtime>(
    _app: &AppHandle<R>,
    window: &tauri::WebviewWindow<R>,
) -> Result<(), String> {
    window.show().map_err(|err| err.to_string())
}

fn screen_guide_overlay_can_activate(is_click_through: bool, window_exists: bool) -> bool {
    is_click_through && window_exists
}

fn screen_guide_main_display_signature<R: tauri::Runtime>(
    app: &AppHandle<R>,
) -> Result<ScreenGuideDisplaySignature, String> {
    let monitor = app
        .primary_monitor()
        .map_err(|err| err.to_string())?
        .ok_or_else(|| "Main display is unavailable.".to_string())?;
    Ok(ScreenGuideDisplaySignature {
        x: monitor.position().x,
        y: monitor.position().y,
        width: monitor.size().width,
        height: monitor.size().height,
        scale_factor_bits: monitor.scale_factor().to_bits(),
    })
}

fn screen_guide_target_is_unchanged(
    captured_context: &FrontmostAppContext,
    captured_url: Option<&str>,
    captured_display: &ScreenGuideDisplaySignature,
    current_context: &FrontmostAppContext,
    current_url: Option<&str>,
    current_display: &ScreenGuideDisplaySignature,
) -> bool {
    let app_matches = match (
        captured_context.bundle_id.as_deref(),
        current_context.bundle_id.as_deref(),
    ) {
        (Some(captured), Some(current)) => captured == current,
        (None, None) => captured_context
            .app_name
            .eq_ignore_ascii_case(&current_context.app_name),
        _ => false,
    };
    let url_matches = captured_url == current_url;
    app_matches
        && captured_context.window_title.trim() == current_context.window_title.trim()
        && url_matches
        && captured_display == current_display
}

async fn screen_guide_cue_is_fresh<R: tauri::Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    request_generation: u64,
    captured_context: &FrontmostAppContext,
    captured_url: Option<&str>,
    captured_display: &ScreenGuideDisplaySignature,
    cue: &ScreenGuidePointCue,
) -> bool {
    let current_context = crate::capture::macos::get_frontmost_app_info_fresh();
    let current_url = current_context.browser_url.clone();
    let blocklist = state.config.read().blocklist.clone();
    if !screen_guide_context_can_be_captured(&current_context, current_url.as_deref(), &blocklist) {
        return false;
    }
    let Ok(current_display) = screen_guide_main_display_signature(app) else {
        return false;
    };
    if !screen_guide_target_is_unchanged(
        captured_context,
        captured_url,
        captured_display,
        &current_context,
        current_url.as_deref(),
        &current_display,
    ) {
        return false;
    }

    // Same-process capture can still see an NSWindowSharingNone window. Hide
    // the overlay synchronously, let AppKit composite the underlying display,
    // and keep the durable memory loop suppressed throughout this interval.
    {
        let _lifecycle = SCREEN_GUIDE_LIFECYCLE.lock();
        if ensure_screen_guide_request_current(state, request_generation).is_err()
            || ensure_no_visible_fndr_windows(app).is_err()
            || hide_screen_guide_overlay(app).is_err()
        {
            return false;
        }
    }
    tokio::time::sleep(OVERLAY_CAPTURE_SETTLE).await;

    let cue_capture = {
        let capture_context = crate::capture::macos::get_frontmost_app_info_fresh();
        let capture_url = capture_context.browser_url.clone();
        let capture_blocklist = state.config.read().blocklist.clone();
        let capture_display = screen_guide_main_display_signature(app).ok();
        let target_is_safe = capture_display
            .as_ref()
            .map(|display| {
                screen_guide_context_can_be_captured(
                    &capture_context,
                    capture_url.as_deref(),
                    &capture_blocklist,
                ) && screen_guide_target_is_unchanged(
                    captured_context,
                    captured_url,
                    captured_display,
                    &capture_context,
                    capture_url.as_deref(),
                    display,
                )
            })
            .unwrap_or(false);
        if !target_is_safe {
            None
        } else {
            let image = {
                let _lifecycle = SCREEN_GUIDE_LIFECYCLE.lock();
                if ensure_screen_guide_request_current(state, request_generation).is_err()
                    || ensure_no_visible_fndr_windows(app).is_err()
                {
                    None
                } else {
                    crate::capture::macos::capture_screen().ok()
                }
            };
            image.map(|image| (capture_context, capture_url, image))
        }
    };

    // Restore the text response even when cue validation fails. A stale turn
    // never re-shows over a newer generation.
    let overlay_restored = {
        let _lifecycle = SCREEN_GUIDE_LIFECYCLE.lock();
        if ensure_screen_guide_request_current(state, request_generation).is_err() {
            false
        } else if show_screen_guide_overlay(app).is_ok() {
            true
        } else {
            if cancel_screen_guide_runtime_if_current(request_generation) {
                let _ = finish_screen_guide_surface_cleanup(app, request_generation);
            }
            false
        }
    };
    if !overlay_restored {
        let local_restore = SCREEN_GUIDE_DEFERRED_RESTORE.lock().clone();
        restore_screen_guide_capture_if_owned(app, state, request_generation, &local_restore);
        return false;
    }
    let Some((cue_capture_context, cue_capture_url, image)) = cue_capture else {
        return false;
    };

    // Apply the same fresh privacy/context checks immediately after capture.
    // The bytes stay in memory and are discarded if anything changed.
    let post_context = crate::capture::macos::get_frontmost_app_info_fresh();
    let post_url = post_context.browser_url.clone();
    let post_blocklist = state.config.read().blocklist.clone();
    let Ok(post_display) = screen_guide_main_display_signature(app) else {
        return false;
    };
    if !screen_guide_context_can_be_captured(&post_context, post_url.as_deref(), &post_blocklist)
        || !screen_guide_target_is_unchanged(
            &cue_capture_context,
            cue_capture_url.as_deref(),
            captured_display,
            &post_context,
            post_url.as_deref(),
            &post_display,
        )
        || ensure_screen_guide_request_current(state, request_generation).is_err()
    {
        return false;
    }

    let current_ocr = match tokio::task::spawn_blocking(move || {
        OcrEngine::new().and_then(|engine| engine.recognize_screen_guide(&image))
    })
    .await
    {
        Ok(Ok(result)) => result,
        _ => return false,
    };
    let latest_blocklist = state.config.read().blocklist.clone();
    ensure_screen_guide_request_current(state, request_generation).is_ok()
        && screen_guide_ocr_is_allowed(
            &post_context,
            post_url.as_deref(),
            &current_ocr.plain_text,
            &latest_blocklist,
        )
        && screen_guide_cue_matches_current_evidence(cue, &current_ocr.lines)
}

fn screen_guide_cue_matches_current_evidence(
    cue: &ScreenGuidePointCue,
    evidence: &[ScreenGuideOcrLine],
) -> bool {
    const POSITION_TOLERANCE: f64 = 0.003;
    let Some(expected_label) = cue.label.as_deref() else {
        return false;
    };
    evidence.iter().any(|line| {
        (line.x - cue.x).abs() <= POSITION_TOLERANCE
            && (line.y - cue.y).abs() <= POSITION_TOLERANCE
            && truncate_chars(line.text.trim(), 80) == expected_label
    })
}

#[cfg(target_os = "macos")]
pub(super) fn hide_webview_window_synchronously_if<R, F>(
    app: &AppHandle<R>,
    label: &str,
    should_hide: F,
) -> Result<bool, String>
where
    R: tauri::Runtime,
    F: FnOnce() -> bool + Send + 'static,
{
    let window = app
        .get_webview_window(label)
        .ok_or_else(|| format!("FNDR window '{label}' is unavailable."))?;
    if objc2_foundation::NSThread::isMainThread_class() {
        if !should_hide() {
            return Ok(false);
        }
        let pointer = window.ns_window().map_err(|err| err.to_string())?;
        let native_window = unsafe { &*(pointer as *const objc2_app_kit::NSWindow) };
        native_window.orderOut(None);
        return Ok(true);
    }

    let callback_window = window.clone();
    let (done_tx, done_rx) = std::sync::mpsc::sync_channel(1);
    app.run_on_main_thread(move || {
        let result = if should_hide() {
            callback_window
                .ns_window()
                .map_err(|err| err.to_string())
                .map(|pointer| {
                    let native_window = unsafe { &*(pointer as *const objc2_app_kit::NSWindow) };
                    native_window.orderOut(None);
                    true
                })
        } else {
            Ok(false)
        };
        let _ = done_tx.send(result);
    })
    .map_err(|err| err.to_string())?;
    done_rx
        .recv_timeout(Duration::from_secs(2))
        .map_err(|_| format!("FNDR window '{label}' did not hide in time."))?
}

#[cfg(not(target_os = "macos"))]
pub(super) fn hide_webview_window_synchronously_if<R, F>(
    app: &AppHandle<R>,
    label: &str,
    should_hide: F,
) -> Result<bool, String>
where
    R: tauri::Runtime,
    F: FnOnce() -> bool + Send + 'static,
{
    if !should_hide() {
        return Ok(false);
    }
    let window = app
        .get_webview_window(label)
        .ok_or_else(|| format!("FNDR window '{label}' is unavailable."))?;
    window.hide().map_err(|err| err.to_string())?;
    Ok(true)
}

pub(super) fn hide_webview_window_synchronously<R: tauri::Runtime>(
    app: &AppHandle<R>,
    label: &str,
) -> Result<(), String> {
    hide_webview_window_synchronously_if(app, label, || true).map(|_| ())
}

fn hide_screen_guide_overlay<R: tauri::Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    hide_webview_window_synchronously(app, SCREEN_GUIDE_OVERLAY_LABEL)
}

fn hide_visible_fndr_windows<R: tauri::Runtime>(
    app: &AppHandle<R>,
) -> Result<Vec<String>, HideFndrWindowsFailure> {
    let mut hidden: Vec<String> = Vec::new();
    for (label, window) in app.webview_windows() {
        if label == SCREEN_GUIDE_OVERLAY_LABEL {
            continue;
        }
        let visible = match window.is_visible() {
            Ok(visible) => visible,
            Err(err) => {
                return Err(HideFndrWindowsFailure {
                    hidden_window_labels: hidden,
                    message: format!(
                        "Screen Guide could not verify FNDR window '{label}' before capture: {err}"
                    ),
                });
            }
        };
        if !visible {
            continue;
        }
        if let Err(err) = window.hide() {
            // Treat a failed hide as an uncertain mutation and include the
            // current label in the owned restore attempt as well.
            hidden.push(label.to_string());
            return Err(HideFndrWindowsFailure {
                hidden_window_labels: hidden,
                message: format!(
                    "Screen Guide could not exclude FNDR window '{label}' from capture: {err}"
                ),
            });
        }
        hidden.push(label.to_string());
    }
    Ok(hidden)
}

fn ensure_no_visible_fndr_windows<R: tauri::Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    for (label, window) in app.webview_windows() {
        if label == SCREEN_GUIDE_OVERLAY_LABEL {
            continue;
        }
        match window.is_visible() {
            Ok(false) => {}
            Ok(true) => {
                return Err(format!(
                    "Screen Guide paused because FNDR window '{label}' reopened during capture. Try again."
                ));
            }
            Err(err) => {
                return Err(format!(
                    "Screen Guide could not verify FNDR window '{label}' before capture: {err}"
                ));
            }
        }
    }
    Ok(())
}

fn restore_screen_guide_after_capture<R: tauri::Runtime>(
    app: &AppHandle<R>,
    restore: &DeferredFndrRestore,
    show_overlay: bool,
) -> Result<(), String> {
    let mut first_error = None;
    if restore.app_was_hidden {
        if let Err(err) = unhide_application_without_activation(app) {
            first_error = Some(err);
        }
    }
    for label in &restore.window_labels {
        if let Some(window) = app.get_webview_window(label) {
            if let Err(err) = order_window_front_without_focus(app, &window) {
                first_error.get_or_insert(err);
            }
        }
    }
    if show_overlay {
        if let Err(err) = show_screen_guide_overlay(app) {
            first_error.get_or_insert(err);
        }
    }
    match first_error {
        Some(err) => {
            if !restore.is_empty() {
                let mut deferred = SCREEN_GUIDE_DEFERRED_RESTORE.lock();
                if deferred.is_empty() {
                    *deferred = restore.clone();
                } else if deferred.owner_generation == restore.owner_generation {
                    deferred.app_was_hidden |= restore.app_was_hidden;
                    deferred.add_windows(restore.window_labels.clone());
                }
            }
            Err(err)
        }
        None => {
            let mut deferred = SCREEN_GUIDE_DEFERRED_RESTORE.lock();
            if *deferred == *restore {
                *deferred = DeferredFndrRestore::default();
            }
            Ok(())
        }
    }
}

fn restore_deferred_fndr_windows<R: tauri::Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    let restore = SCREEN_GUIDE_DEFERRED_RESTORE.lock().clone();
    if restore.is_empty() {
        return Ok(());
    }
    restore_screen_guide_after_capture(app, &restore, false)
}

fn combine_screen_guide_surface_cleanup(
    hide_result: Result<(), String>,
    restore_result: Result<(), String>,
) -> Result<(), String> {
    match (hide_result, restore_result) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(hide), Ok(())) => Err(hide),
        (Ok(()), Err(restore)) => Err(restore),
        (Err(hide), Err(restore)) => Err(format!("{hide} Restore also failed: {restore}")),
    }
}

fn screen_guide_destroy_cleanup_can_release(
    capture_coordinator_unwound: bool,
    surfaces_clean: bool,
) -> bool {
    capture_coordinator_unwound && surfaces_clean
}

/// A terminal Screen Guide cleanup is complete only when its own overlay is
/// confirmed absent and every FNDR surface excluded for the turn is restored.
fn cleanup_screen_guide_surfaces<R: tauri::Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    let hide_result = if app.get_webview_window(SCREEN_GUIDE_OVERLAY_LABEL).is_some() {
        hide_screen_guide_overlay(app)
    } else {
        // A destroyed window is already absent from future captures.
        Ok(())
    };
    let restore_result = restore_deferred_fndr_windows(app);
    combine_screen_guide_surface_cleanup(hide_result, restore_result)
}

fn finish_screen_guide_surface_cleanup<R: tauri::Runtime>(
    app: &AppHandle<R>,
    owner_generation: u64,
) -> Result<(), String> {
    let result = cleanup_screen_guide_surfaces(app);
    if result.is_ok() {
        release_memory_capture_for_generation(app, owner_generation);
    } else if owner_generation != 0 {
        cancel_screen_guide_request_and_notify(app, owner_generation);
        schedule_deferred_fndr_restore_retry(app, owner_generation);
    }
    result
}

fn cancel_screen_guide_request_and_notify<R: tauri::Runtime>(
    app: &AppHandle<R>,
    request_generation: u64,
) -> bool {
    if !cancel_screen_guide_runtime_if_current(request_generation) {
        return false;
    }
    stop_say_process();
    let _ = dispatch_screen_guide_event(
        app,
        PendingScreenGuideEvent::Shortcut {
            action: ScreenGuideShortcutAction::Cancel,
            generation: request_generation,
        },
    );
    true
}

fn finish_current_screen_guide_surface_cleanup<R: tauri::Runtime>(
    app: &AppHandle<R>,
) -> Result<(), String> {
    let owner_generation = current_memory_capture_generation(app);
    finish_screen_guide_surface_cleanup(app, owner_generation)
}

fn schedule_deferred_fndr_restore_retry<R: tauri::Runtime>(
    app: &AppHandle<R>,
    owner_generation: u64,
) {
    if owner_generation == 0
        || SCREEN_GUIDE_RESTORE_RETRY_OWNER.swap(owner_generation, Ordering::SeqCst)
            == owner_generation
    {
        return;
    }
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut delay = Duration::from_millis(250);
        loop {
            tokio::time::sleep(delay).await;
            if SCREEN_GUIDE_RESTORE_RETRY_OWNER.load(Ordering::SeqCst) != owner_generation {
                return;
            }
            if current_memory_capture_generation(&handle) != owner_generation {
                return;
            }
            let cleanup_result = {
                let _lifecycle = SCREEN_GUIDE_LIFECYCLE.lock();
                cleanup_screen_guide_surfaces(&handle)
            };
            if cleanup_result.is_ok() {
                release_memory_capture_for_generation(&handle, owner_generation);
                break;
            }
            delay = (delay * 2).min(Duration::from_secs(5));
        }
        let _ = SCREEN_GUIDE_RESTORE_RETRY_OWNER.compare_exchange(
            owner_generation,
            0,
            Ordering::SeqCst,
            Ordering::SeqCst,
        );
    });
}

/// A renderer crash or hung local model cannot leave FNDR windows hidden
/// forever. Normal answer/error timers consume the client token first, making
/// this bounded lease a no-op on healthy turns.
fn schedule_screen_guide_hidden_lease<R: tauri::Runtime>(
    app: &AppHandle<R>,
    request_generation: u64,
) {
    let lease_epoch = {
        let mut runtime = SCREEN_GUIDE_RUNTIME.lock();
        if !runtime.enabled || runtime.generation != request_generation {
            return;
        }
        runtime.lease_epoch = runtime.lease_epoch.wrapping_add(1);
        runtime.lease_epoch
    };
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(SCREEN_GUIDE_HIDDEN_LEASE).await;
        let _lifecycle = SCREEN_GUIDE_LIFECYCLE.lock();
        let timed_out = {
            let mut runtime = SCREEN_GUIDE_RUNTIME.lock();
            if !screen_guide_lease_matches(
                request_generation,
                lease_epoch,
                runtime.generation,
                runtime.lease_epoch,
                runtime.enabled,
            ) {
                false
            } else {
                let mut delivery = SCREEN_GUIDE_OVERLAY_DELIVERY.lock();
                invalidate_screen_guide_turn(&mut runtime, &mut delivery);
                true
            }
        };
        if !timed_out {
            return;
        }
        stop_say_process();
        let _ = dispatch_screen_guide_event(
            &handle,
            PendingScreenGuideEvent::Shortcut {
                action: ScreenGuideShortcutAction::Cancel,
                generation: request_generation,
            },
        );
        let _ = finish_screen_guide_surface_cleanup(&handle, request_generation);
    });
}

fn screen_guide_lease_matches(
    generation: u64,
    epoch: u64,
    current_generation: u64,
    current_epoch: u64,
    enabled: bool,
) -> bool {
    enabled && generation == current_generation && epoch == current_epoch
}

fn finish_screen_guide_answer(
    state: &AppState,
    settings: &ScreenGuideConfig,
    request_generation: u64,
    mut answer: ScreenGuideAnswer,
) -> Result<ScreenGuideAnswer, String> {
    // Keep this lock held through process launch so disable/new-input
    // cancellation cannot race between the generation check and local speech.
    let runtime = SCREEN_GUIDE_RUNTIME.lock();
    let enabled = state.config.read().screen_guide.enabled
        && !state.is_incognito.load(Ordering::SeqCst)
        && SCREEN_GUIDE_OVERLAY_DELIVERY.lock().ready;
    if !screen_guide_request_is_current(
        request_generation,
        runtime.generation,
        runtime.enabled && enabled,
    ) {
        return Err(screen_guide_cancelled_message());
    }
    answer.answer = truncate_chars(answer.answer.trim(), MAX_SCREEN_GUIDE_ANSWER_CHARS);
    if settings.speak_responses {
        // Speech failure should not discard an otherwise useful local answer.
        let _ = start_say_process(&answer.answer);
    }
    Ok(answer)
}

fn screen_guide_cancelled_message() -> String {
    "Screen Guide request was cancelled.".to_string()
}

fn begin_screen_guide_input() -> Result<u64, String> {
    let mut runtime = SCREEN_GUIDE_RUNTIME.lock();
    if !runtime.enabled {
        return Err("Screen Guide is turned off.".to_string());
    }
    let mut delivery = SCREEN_GUIDE_OVERLAY_DELIVERY.lock();
    invalidate_screen_guide_turn(&mut runtime, &mut delivery);
    runtime.turn_cancel = Some(Arc::new(AtomicBool::new(false)));
    Ok(runtime.generation)
}

/// Transfer any windows hidden by the prior turn only after the new turn has
/// obtained the lifecycle lock. Generation invalidation happens earlier, so a
/// new press cancels immediately without racing this ownership hand-off.
fn adopt_deferred_restore_for_generation(generation: u64) {
    if !screen_guide_runtime_generation_is_current(generation) {
        return;
    }
    let mut deferred = SCREEN_GUIDE_DEFERRED_RESTORE.lock();
    if !deferred.is_empty() {
        deferred.owner_generation = generation;
    }
}

fn continue_screen_guide_request(request_id: u64) -> Result<(u64, Arc<AtomicBool>), String> {
    let runtime = SCREEN_GUIDE_RUNTIME.lock();
    if !runtime.enabled || runtime.generation != request_id {
        return Err(screen_guide_cancelled_message());
    }
    let cancel = runtime
        .turn_cancel
        .as_ref()
        .cloned()
        .ok_or_else(screen_guide_cancelled_message)?;
    Ok((runtime.generation, cancel))
}

fn screen_guide_turn_cancel(request_id: u64) -> Result<Arc<AtomicBool>, String> {
    let runtime = SCREEN_GUIDE_RUNTIME.lock();
    if !runtime.enabled || runtime.generation != request_id {
        return Err(screen_guide_cancelled_message());
    }
    runtime
        .turn_cancel
        .as_ref()
        .cloned()
        .ok_or_else(screen_guide_cancelled_message)
}

async fn wait_for_screen_guide_turn_cancellation(cancel: Arc<AtomicBool>) {
    while !cancel.load(Ordering::SeqCst) {
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

fn cancel_screen_guide_runtime(enabled: bool) -> u64 {
    let mut runtime = SCREEN_GUIDE_RUNTIME.lock();
    let mut delivery = SCREEN_GUIDE_OVERLAY_DELIVERY.lock();
    invalidate_screen_guide_turn(&mut runtime, &mut delivery);
    runtime.enabled = enabled;
    runtime.generation
}

fn screen_guide_runtime_generation_is_current(generation: u64) -> bool {
    let runtime = SCREEN_GUIDE_RUNTIME.lock();
    runtime.enabled && runtime.generation == generation
}

fn cancel_screen_guide_runtime_if_current(generation: u64) -> bool {
    let mut runtime = SCREEN_GUIDE_RUNTIME.lock();
    if runtime.generation != generation {
        return false;
    }
    let mut delivery = SCREEN_GUIDE_OVERLAY_DELIVERY.lock();
    invalidate_screen_guide_turn(&mut runtime, &mut delivery);
    true
}

/// Supersede Screen Guide and serialize a sibling FNDR window activation with
/// its final capture. The lifecycle lock is acquired on a worker, never from a
/// main-thread callback, because Screen Guide's native window calls can need
/// the main thread while holding that lock.
pub(super) fn schedule_fndr_window_activation_after_screen_guide_cancel_with_work<
    R: tauri::Runtime,
    T: Send + 'static,
    W: FnOnce(AppHandle<R>) -> T + Send + 'static,
    F: FnOnce(AppHandle<R>, T) + Send + 'static,
>(
    app: &AppHandle<R>,
    work_before_activation: W,
    activate: F,
) {
    let enabled = SCREEN_GUIDE_RUNTIME.lock().enabled;
    let cancellation_generation = cancel_screen_guide_runtime(enabled);
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _lifecycle = SCREEN_GUIDE_LIFECYCLE.lock();
        if SCREEN_GUIDE_RUNTIME.lock().generation != cancellation_generation {
            return;
        }
        stop_say_process();
        let _ = dispatch_screen_guide_event(
            &handle,
            PendingScreenGuideEvent::Shortcut {
                action: ScreenGuideShortcutAction::Cancel,
                generation: cancellation_generation,
            },
        );
        if handle
            .get_webview_window(SCREEN_GUIDE_OVERLAY_LABEL)
            .is_some()
        {
            if hide_screen_guide_overlay(&handle).is_err() {
                let _ = finish_current_screen_guide_surface_cleanup(&handle);
                return;
            }
            std::thread::sleep(OVERLAY_CAPTURE_SETTLE);
        }

        // Work that can inspect the target screen runs only after the guide is
        // synchronously hidden and while every deferred FNDR surface remains
        // excluded. Restore those surfaces only after inspection completes.
        let prepared = work_before_activation(handle.clone());
        if finish_current_screen_guide_surface_cleanup(&handle).is_err() {
            return;
        }

        let callback_handle = handle.clone();
        let (done_tx, done_rx) = std::sync::mpsc::sync_channel(1);
        if handle
            .run_on_main_thread(move || {
                if SCREEN_GUIDE_RUNTIME.lock().generation == cancellation_generation {
                    activate(callback_handle, prepared);
                }
                let _ = done_tx.send(());
            })
            .is_ok()
        {
            let _ = done_rx.recv_timeout(Duration::from_secs(5));
        }
    });
}

fn cancel_screen_guide_turn_work(runtime: &mut ScreenGuideRuntime) {
    if let Some(cancel) = runtime.turn_cancel.take() {
        cancel.store(true, Ordering::SeqCst);
    }
}

fn invalidate_screen_guide_turn(
    runtime: &mut ScreenGuideRuntime,
    delivery: &mut ScreenGuideOverlayDelivery,
) -> u64 {
    let cancelled_generation = runtime.generation;
    cancel_screen_guide_turn_work(runtime);
    runtime.generation = runtime.generation.wrapping_add(1);
    delivery.discard_submits_for_generation(cancelled_generation);
    runtime.generation
}

fn screen_guide_request_is_current(request: u64, current: u64, enabled: bool) -> bool {
    enabled && request == current
}

fn screen_guide_visual_matches(request_id: u64, current_generation: u64, enabled: bool) -> bool {
    enabled && current_generation == request_id
}

fn ensure_screen_guide_request_current(
    state: &AppState,
    request_generation: u64,
) -> Result<(), String> {
    let runtime = SCREEN_GUIDE_RUNTIME.lock();
    let enabled =
        state.config.read().screen_guide.enabled && !state.is_incognito.load(Ordering::SeqCst);
    screen_guide_request_is_current(
        request_generation,
        runtime.generation,
        runtime.enabled && enabled,
    )
    .then_some(())
    .ok_or_else(screen_guide_cancelled_message)
}

fn screen_guide_overlay_should_be_visible(state: &AppState, request_generation: u64) -> bool {
    ensure_screen_guide_request_current(state, request_generation).is_ok()
}

fn ensure_screen_guide_request_current_or_restore<R: tauri::Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    request_generation: u64,
    local_restore: &DeferredFndrRestore,
) -> Result<(), String> {
    ensure_screen_guide_request_current(state, request_generation).map_err(|err| {
        restore_screen_guide_capture_if_owned(app, state, request_generation, local_restore);
        err
    })
}

fn screen_guide_restore_plan(
    deferred: &DeferredFndrRestore,
    local_restore: &DeferredFndrRestore,
    request_generation: u64,
    show_overlay: bool,
) -> Option<(DeferredFndrRestore, bool)> {
    let restore = if deferred.owner_generation == request_generation {
        deferred.clone()
    } else if deferred.owner_generation == 0 && local_restore.owner_generation == request_generation
    {
        local_restore.clone()
    } else {
        return None;
    };
    Some((restore, show_overlay))
}

fn restore_screen_guide_capture_if_owned<R: tauri::Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    request_generation: u64,
    local_restore: &DeferredFndrRestore,
) {
    let _lifecycle = SCREEN_GUIDE_LIFECYCLE.lock();
    let deferred = SCREEN_GUIDE_DEFERRED_RESTORE.lock().clone();
    let show_overlay = screen_guide_overlay_should_be_visible(state, request_generation);
    let Some((restore, show_overlay)) =
        screen_guide_restore_plan(&deferred, local_restore, request_generation, show_overlay)
    else {
        return;
    };
    // Restore ownership is independent from whether privacy still allows the
    // overlay to be shown. In particular, entering incognito after FNDR was
    // hidden must restore those windows immediately, not wait for a lease.
    if !show_overlay {
        cancel_screen_guide_request_and_notify(app, request_generation);
        let cleanup_result = combine_screen_guide_surface_cleanup(
            if app.get_webview_window(SCREEN_GUIDE_OVERLAY_LABEL).is_some() {
                hide_screen_guide_overlay(app)
            } else {
                Ok(())
            },
            restore_screen_guide_after_capture(app, &restore, false),
        );
        if cleanup_result.is_ok() {
            release_memory_capture_for_generation(app, request_generation);
        } else {
            schedule_deferred_fndr_restore_retry(app, request_generation);
        }
        return;
    }

    if restore_screen_guide_after_capture(app, &restore, true).is_err() {
        // A partial restore or failed re-show ends the turn. Re-hide any
        // surface that may have appeared and retain capture suppression until
        // the terminal cleanup is confirmed or retried successfully.
        cancel_screen_guide_request_and_notify(app, request_generation);
        let cleanup_result = combine_screen_guide_surface_cleanup(
            if app.get_webview_window(SCREEN_GUIDE_OVERLAY_LABEL).is_some() {
                hide_screen_guide_overlay(app)
            } else {
                Ok(())
            },
            restore_screen_guide_after_capture(app, &restore, false),
        );
        if cleanup_result.is_ok() {
            release_memory_capture_for_generation(app, request_generation);
        } else {
            schedule_deferred_fndr_restore_retry(app, request_generation);
        }
    }
}

fn dispatch_screen_guide_event<R: tauri::Runtime>(
    app: &AppHandle<R>,
    event: PendingScreenGuideEvent,
) -> Result<(), String> {
    let (microphone_start_generation, microphone_stop_generation) = match &event {
        PendingScreenGuideEvent::Shortcut {
            action: ScreenGuideShortcutAction::Release | ScreenGuideShortcutAction::Cancel,
            generation,
        }
        | PendingScreenGuideEvent::Submit { generation, .. } => (None, Some(*generation)),
        PendingScreenGuideEvent::Shortcut {
            action: ScreenGuideShortcutAction::Press,
            generation,
        } => (Some(*generation), None),
    };
    if let Some(generation) = microphone_start_generation {
        arm_screen_guide_microphone_session(app, generation);
    }
    if let Some(generation) = microphone_stop_generation {
        request_screen_guide_microphone_stop(app, generation);
    }
    let event = SCREEN_GUIDE_OVERLAY_DELIVERY.lock().deliver_or_queue(event);
    if let Some(event) = event {
        emit_screen_guide_event(app, event)?;
    }
    Ok(())
}

fn arm_screen_guide_microphone_session<R: tauri::Runtime>(app: &AppHandle<R>, generation: u64) {
    let Some(epoch) = SCREEN_GUIDE_MICROPHONE_SAFETY
        .lock()
        .mark_started(generation)
    else {
        return;
    };
    schedule_screen_guide_microphone_watchdog(
        app,
        ScreenGuideMicrophoneWatchdog::MaximumDuration,
        generation,
        epoch,
        SCREEN_GUIDE_MICROPHONE_MAX_DURATION,
    );
}

fn request_screen_guide_microphone_stop<R: tauri::Runtime>(app: &AppHandle<R>, generation: u64) {
    let Some(epoch) = SCREEN_GUIDE_MICROPHONE_SAFETY
        .lock()
        .request_stop(generation)
    else {
        return;
    };
    schedule_screen_guide_microphone_watchdog(
        app,
        ScreenGuideMicrophoneWatchdog::StopAcknowledgement,
        generation,
        epoch,
        SCREEN_GUIDE_MICROPHONE_STOP_ACK_TIMEOUT,
    );
}

fn schedule_screen_guide_microphone_watchdog<R: tauri::Runtime>(
    app: &AppHandle<R>,
    watchdog: ScreenGuideMicrophoneWatchdog,
    generation: u64,
    epoch: u64,
    timeout: Duration,
) {
    schedule_screen_guide_overlay_reset(
        app,
        ScreenGuideOverlayResetGuard::Microphone {
            watchdog,
            generation,
            epoch,
        },
        timeout,
    );
}

fn schedule_screen_guide_overlay_reset<R: tauri::Runtime>(
    app: &AppHandle<R>,
    guard: ScreenGuideOverlayResetGuard,
    initial_delay: Duration,
) {
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(initial_delay).await;
        let mut retry_delay = Duration::from_millis(100);
        while guard.is_pending(&handle) {
            let (done_tx, done_rx) = tokio::sync::oneshot::channel();
            let callback_handle = handle.clone();
            let schedule_result = handle.run_on_main_thread(move || {
                if !guard.is_pending(&callback_handle) {
                    let _ = done_tx.send(true);
                    return;
                }
                let Some(window) = callback_handle.get_webview_window(SCREEN_GUIDE_OVERLAY_LABEL)
                else {
                    guard.confirm_window_absent();
                    let _ = done_tx.send(true);
                    return;
                };
                if let Err(err) = window.destroy() {
                    tracing::warn!(
                        "Screen Guide {} reset failed and will retry: {err}",
                        guard.reason()
                    );
                }
                let absent = callback_handle
                    .get_webview_window(SCREEN_GUIDE_OVERLAY_LABEL)
                    .is_none();
                if absent {
                    guard.confirm_window_absent();
                }
                let _ = done_tx.send(absent || !guard.is_pending(&callback_handle));
            });
            let completed = match schedule_result {
                Ok(()) => tokio::time::timeout(Duration::from_secs(2), done_rx)
                    .await
                    .ok()
                    .and_then(Result::ok)
                    .unwrap_or(false),
                Err(err) => {
                    tracing::warn!(
                        "Screen Guide {} reset could not reach the main thread and will retry: {err}",
                        guard.reason()
                    );
                    false
                }
            };
            if completed || !guard.is_pending(&handle) {
                return;
            }
            tokio::time::sleep(retry_delay).await;
            retry_delay = (retry_delay * 2).min(Duration::from_secs(2));
        }
    });
}

fn force_screen_guide_overlay_reset_for_privacy<R: tauri::Runtime>(
    app: &AppHandle<R>,
    generation: u64,
) {
    schedule_screen_guide_overlay_reset(
        app,
        ScreenGuideOverlayResetGuard::Privacy {
            recovery_epoch: SCREEN_GUIDE_OVERLAY_RECOVERY_EPOCH.load(Ordering::SeqCst),
            privacy_generation: generation,
        },
        Duration::ZERO,
    );
}

fn emit_screen_guide_event<R: tauri::Runtime>(
    app: &AppHandle<R>,
    event: PendingScreenGuideEvent,
) -> Result<(), String> {
    match event {
        PendingScreenGuideEvent::Shortcut { action, generation } => app
            .emit_to(
                SCREEN_GUIDE_OVERLAY_LABEL,
                SCREEN_GUIDE_SHORTCUT_EVENT,
                ScreenGuideShortcutPayload { action, generation },
            )
            .map_err(|err| err.to_string()),
        PendingScreenGuideEvent::Submit { text, generation } => app
            .emit_to(
                SCREEN_GUIDE_OVERLAY_LABEL,
                SCREEN_GUIDE_SUBMIT_EVENT,
                ScreenGuideSubmitPayload { text, generation },
            )
            .map_err(|err| err.to_string()),
    }
}

fn private_screen_message() -> String {
    "Screen Guide will not inspect this private screen.".to_string()
}

fn screen_guide_context_verification_error(
    context: &FrontmostAppContext,
    browser_url: Option<&str>,
) -> Option<String> {
    if context.app_name.trim().is_empty() || context.app_name.eq_ignore_ascii_case("unknown") {
        return Some(
            "Screen Guide couldn't identify the frontmost app. Bring the screen you want help with forward, then try again."
                .to_string(),
        );
    }
    if crate::capture::macos::is_browser_app(&context.app_name) && browser_url.is_none() {
        return Some(format!(
            "Screen Guide couldn't verify the active page in {}. Allow FNDR to control that browser in System Settings → Privacy & Security → Automation, or use Safari, Chrome, Arc, Brave, or Edge.",
            context.app_name
        ));
    }
    if !context.window_title_verified {
        return Some(
            "Screen Guide couldn't verify the active window. Bring a window forward and allow FNDR in System Settings → Privacy & Security → Accessibility, then try again."
                .to_string(),
        );
    }
    None
}

fn screen_guide_context_is_verifiable(
    context: &FrontmostAppContext,
    browser_url: Option<&str>,
) -> bool {
    screen_guide_context_verification_error(context, browser_url).is_none()
}

fn screen_guide_context_can_be_captured(
    context: &FrontmostAppContext,
    browser_url: Option<&str>,
    blocklist: &[String],
) -> bool {
    screen_guide_context_is_verifiable(context, browser_url)
        && crate::capture::capture_context_skip_reason(
            &context.app_name,
            context.bundle_id.as_deref(),
            &context.window_title,
            browser_url,
            blocklist,
        )
        .is_none()
        && safety_gate::evaluate(
            Some(&context.app_name),
            context.bundle_id.as_deref(),
            browser_url,
            Some(&context.window_title),
            None,
            blocklist,
        ) == SafetyDecision::Allow
}

fn screen_guide_ocr_is_allowed(
    context: &FrontmostAppContext,
    browser_url: Option<&str>,
    ocr_text: &str,
    blocklist: &[String],
) -> bool {
    safety_gate::evaluate(
        Some(&context.app_name),
        context.bundle_id.as_deref(),
        browser_url,
        Some(&context.window_title),
        Some(ocr_text),
        blocklist,
    ) == SafetyDecision::Allow
}

fn transient_history_text(history: &[ScreenGuideHistoryEntry]) -> String {
    let mut remaining = MAX_SCREEN_GUIDE_HISTORY_CHARS;
    let mut lines = Vec::new();
    for entry in history.iter().rev().take(6).rev() {
        let role = match entry.role.trim().to_ascii_lowercase().as_str() {
            "user" => "User",
            "assistant" => "Screen Guide",
            _ => continue,
        };
        if remaining == 0 {
            break;
        }
        let content = truncate_chars(entry.content.trim(), remaining);
        if content.is_empty() {
            continue;
        }
        remaining = remaining.saturating_sub(content.chars().count());
        lines.push(format!("{role}: {content}"));
    }
    lines.join("\n")
}

fn grounded_fallback(screen_text: &str) -> ScreenGuideAnswer {
    let excerpt = truncate_chars(&collapse_whitespace(screen_text), 280);
    let answer = if excerpt.is_empty() {
        "I couldn't find readable text on this screen.".to_string()
    } else {
        format!("I couldn't use the local model, but the visible screen includes: {excerpt}")
    };
    ScreenGuideAnswer {
        answer,
        point_cue: None,
    }
}

fn is_usable_model_answer(raw: &str) -> bool {
    let raw = raw.trim();
    !raw.to_ascii_lowercase().starts_with("ai error:")
        && screen_guide_spoken_text(raw).chars().count() >= 3
}

/// A local model may only select one of the rounded coordinates supplied by
/// Apple Vision. Snap back to the exact observation and use its OCR text as the
/// label so neither coordinates nor UI copy can be invented at this boundary.
fn ground_screen_guide_point_cue(
    mut answer: ScreenGuideAnswer,
    evidence: &[ScreenGuideOcrLine],
    show_cursor: bool,
) -> ScreenGuideAnswer {
    const LOC_ROUNDING_TOLERANCE: f64 = 0.000_051;

    let matched = answer
        .point_cue
        .as_ref()
        .filter(|_| show_cursor)
        .and_then(|cue| {
            evidence.iter().find(|line| {
                (line.x - cue.x).abs() <= LOC_ROUNDING_TOLERANCE
                    && (line.y - cue.y).abs() <= LOC_ROUNDING_TOLERANCE
            })
        });
    answer.point_cue = matched.map(|line| ScreenGuidePointCue {
        x: line.x,
        y: line.y,
        label: Some(truncate_chars(&line.text, 80)),
    });
    answer
}

pub(super) fn validate_screen_guide_shortcut_conflicts(
    screen_guide: &ScreenGuideConfig,
    autofill: &AutofillConfig,
) -> Result<(), String> {
    if !screen_guide.enabled {
        return Ok(());
    }

    let screen_shortcut: Shortcut = screen_guide.shortcut.parse().map_err(|err| {
        format!(
            "Invalid Screen Guide shortcut '{}': {err}",
            screen_guide.shortcut
        )
    })?;
    let omnibar_shortcut: Shortcut = super::omnibar::OMNIBAR_SHORTCUT
        .parse()
        .map_err(|err| format!("Invalid Omnibar shortcut: {err}"))?;
    if screen_shortcut == omnibar_shortcut {
        return Err(format!(
            "Screen Guide shortcut '{}' conflicts with Omnibar.",
            screen_guide.shortcut
        ));
    }

    if autofill.enabled {
        let autofill_shortcut: Shortcut = autofill
            .shortcut
            .parse()
            .map_err(|err| format!("Invalid auto-fill shortcut '{}': {err}", autofill.shortcut))?;
        if screen_shortcut == autofill_shortcut {
            return Err(format!(
                "Screen Guide shortcut '{}' conflicts with Auto-fill.",
                screen_guide.shortcut
            ));
        }
    }
    Ok(())
}

pub(crate) fn parse_screen_guide_response(raw: &str) -> ScreenGuideAnswer {
    let trimmed = raw.trim();
    let Some(tag_start) = trimmed.rfind("[POINT:") else {
        return ScreenGuideAnswer {
            answer: screen_guide_spoken_text(trimmed),
            point_cue: None,
        };
    };
    if !trimmed.ends_with(']') {
        return ScreenGuideAnswer {
            answer: screen_guide_spoken_text(trimmed),
            point_cue: None,
        };
    }

    let tag = &trimmed[tag_start + "[POINT:".len()..trimmed.len() - 1];
    let point_cue = parse_point_cue(tag);
    ScreenGuideAnswer {
        answer: screen_guide_spoken_text(&trimmed[..tag_start]),
        point_cue,
    }
}

fn parse_point_cue(tag: &str) -> Option<ScreenGuidePointCue> {
    let tag = tag.trim();
    if tag.eq_ignore_ascii_case("none") {
        return None;
    }
    let (coordinates, label) = tag.split_once(':').unwrap_or((tag, ""));
    let mut values = coordinates.split(',').map(str::trim);
    let x = values.next()?.parse::<f64>().ok()?;
    let y = values.next()?.parse::<f64>().ok()?;
    if values.next().is_some()
        || !x.is_finite()
        || !y.is_finite()
        || !(0.0..=1.0).contains(&x)
        || !(0.0..=1.0).contains(&y)
    {
        return None;
    }
    let label = truncate_chars(label.trim(), 80);
    Some(ScreenGuidePointCue {
        x,
        y,
        label: (!label.is_empty()).then_some(label),
    })
}

pub(crate) fn screen_guide_spoken_text(raw: &str) -> String {
    collapse_whitespace(POINT_TAG_RE.replace_all(raw, " ").as_ref())
}

fn collapse_whitespace(raw: &str) -> String {
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn truncate_chars(raw: &str, max_chars: usize) -> String {
    raw.chars().take(max_chars).collect()
}

fn start_say_process(raw: &str) -> Result<(), String> {
    let text = truncate_chars(
        &screen_guide_spoken_text(raw),
        MAX_SCREEN_GUIDE_SPEECH_CHARS,
    );
    if text.is_empty() {
        return Ok(());
    }
    stop_say_process();
    // Feed speech over stdin so private answer text is not exposed in the
    // process argument list. No shell or temporary speech file is involved.
    let mut child = Command::new("/usr/bin/say")
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|err| format!("Screen Guide speech is unavailable: {err}"))?;
    let Some(mut stdin) = child.stdin.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return Err("Screen Guide speech input is unavailable.".to_string());
    };
    let write_result = stdin.write_all(text.as_bytes());
    drop(stdin);
    if let Err(err) = write_result {
        let _ = child.kill();
        let _ = child.wait();
        return Err(format!("Screen Guide speech input failed: {err}"));
    }
    *SCREEN_GUIDE_SAY_PROCESS.lock() = Some(child);
    Ok(())
}

fn stop_say_process() {
    if let Some(mut child) = SCREEN_GUIDE_SAY_PROCESS.lock().take() {
        let _ = child.kill();
        let _ = child.wait();
    }
}

pub fn shutdown_screen_guide<R: tauri::Runtime>(app: &AppHandle<R>) {
    SCREEN_GUIDE_SHUTTING_DOWN.store(true, Ordering::SeqCst);
    SCREEN_GUIDE_OVERLAY_RECOVERY_EPOCH.fetch_add(1, Ordering::SeqCst);
    let enabled = SCREEN_GUIDE_RUNTIME.lock().enabled;
    cancel_screen_guide_runtime(enabled);
    stop_say_process();
    // Process exit needs no capture hand-back. Remaining fail-closed avoids a
    // final ordinary capture while native windows and workers are unwinding.
    let _ = app;
}

/// Hard privacy transition used by capture controls. Cancellation is visible
/// to the renderer immediately so an active microphone stops; native window
/// cleanup then runs behind the lifecycle gate without blocking the caller.
pub fn cancel_screen_guide_for_privacy<R: tauri::Runtime>(app: &AppHandle<R>) {
    let enabled = SCREEN_GUIDE_RUNTIME.lock().enabled;
    let immediate_generation = cancel_screen_guide_runtime(enabled);
    stop_say_process();
    let _ = dispatch_screen_guide_event(
        app,
        PendingScreenGuideEvent::Shortcut {
            action: ScreenGuideShortcutAction::Cancel,
            generation: immediate_generation,
        },
    );
    // Incognito/privacy transitions do not trust the renderer event loop: a
    // destroyed WKWebView releases its MediaStream at the native boundary.
    force_screen_guide_overlay_reset_for_privacy(app, immediate_generation);

    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _lifecycle = SCREEN_GUIDE_LIFECYCLE.lock();
        let still_private = screen_guide_input_is_private(&handle);
        let current_generation = SCREEN_GUIDE_RUNTIME.lock().generation;
        if !screen_guide_privacy_cleanup_still_owns_turn(
            still_private,
            current_generation,
            immediate_generation,
        ) {
            // Incognito was already lifted and a newer user-owned turn won
            // the lifecycle race. Never let delayed privacy cleanup hide it.
            return;
        }
        let generation = if still_private {
            let runtime_enabled = SCREEN_GUIDE_RUNTIME.lock().enabled;
            cancel_screen_guide_runtime(runtime_enabled)
        } else {
            immediate_generation
        };
        let _ = dispatch_screen_guide_event(
            &handle,
            PendingScreenGuideEvent::Shortcut {
                action: ScreenGuideShortcutAction::Cancel,
                generation,
            },
        );
        let _ = finish_current_screen_guide_surface_cleanup(&handle);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_trailing_normalized_point_cue_and_strips_it_from_answer() {
        let parsed =
            parse_screen_guide_response("Open the Settings menu. [POINT:0.250,0.750:Settings]  ");

        assert_eq!(parsed.answer, "Open the Settings menu.");
        assert_eq!(
            parsed.point_cue,
            Some(ScreenGuidePointCue {
                x: 0.25,
                y: 0.75,
                label: Some("Settings".to_string()),
            })
        );
    }

    #[test]
    fn parses_explicit_none_point_cue() {
        let parsed = parse_screen_guide_response("That option is not visible. [POINT:none]");

        assert_eq!(parsed.answer, "That option is not visible.");
        assert_eq!(parsed.point_cue, None);
    }

    #[test]
    fn rejects_out_of_range_or_non_finite_point_coordinates() {
        for raw in [
            "No safe target. [POINT:1.01,0.4:Outside]",
            "No safe target. [POINT:-0.1,0.4:Outside]",
            "No safe target. [POINT:NaN,0.4:Outside]",
        ] {
            let parsed = parse_screen_guide_response(raw);
            assert_eq!(parsed.answer, "No safe target.");
            assert_eq!(parsed.point_cue, None);
        }
    }

    #[test]
    fn strips_point_markup_from_spoken_text() {
        assert_eq!(
            screen_guide_spoken_text("Click here [POINT:0.4,0.2:Button] then continue."),
            "Click here then continue."
        );
    }

    #[test]
    fn runtime_accepts_only_an_exact_ocr_location_and_uses_its_label() {
        let evidence = vec![ScreenGuideOcrLine {
            text: "Save changes".to_string(),
            x: 0.80004,
            y: 0.10004,
        }];
        let parsed = parse_screen_guide_response("Use Save. [POINT:0.8000,0.1000:Save]");
        let grounded = ground_screen_guide_point_cue(parsed, &evidence, true);

        assert_eq!(
            grounded.point_cue,
            Some(ScreenGuidePointCue {
                x: 0.80004,
                y: 0.10004,
                label: Some("Save changes".to_string()),
            })
        );
    }

    #[test]
    fn runtime_rejects_unobserved_or_disabled_point_cues() {
        let evidence = vec![ScreenGuideOcrLine {
            text: "Save changes".to_string(),
            x: 0.8,
            y: 0.1,
        }];

        let unobserved = parse_screen_guide_response("Use Save. [POINT:0.7,0.1:Save]");
        assert_eq!(
            ground_screen_guide_point_cue(unobserved, &evidence, true).point_cue,
            None
        );

        let disabled = parse_screen_guide_response("Use Save. [POINT:0.8,0.1:Save]");
        assert_eq!(
            ground_screen_guide_point_cue(disabled, &evidence, false).point_cue,
            None
        );
    }

    #[test]
    fn point_cue_requires_the_same_app_window_url_and_display() {
        let captured_context = FrontmostAppContext {
            app_name: "Safari".to_string(),
            bundle_id: Some("com.apple.Safari".to_string()),
            window_title: "Account settings".to_string(),
            window_title_verified: true,
            browser_url: None,
        };
        let display = ScreenGuideDisplaySignature {
            x: 0,
            y: 0,
            width: 3024,
            height: 1964,
            scale_factor_bits: 2.0_f64.to_bits(),
        };

        assert!(screen_guide_target_is_unchanged(
            &captured_context,
            Some("https://example.com/settings"),
            &display,
            &captured_context,
            Some("https://example.com/settings"),
            &display,
        ));

        let changed_window = FrontmostAppContext {
            window_title: "Billing".to_string(),
            ..captured_context.clone()
        };
        assert!(!screen_guide_target_is_unchanged(
            &captured_context,
            Some("https://example.com/settings"),
            &display,
            &changed_window,
            Some("https://example.com/billing"),
            &display,
        ));

        let changed_display = ScreenGuideDisplaySignature {
            width: 3456,
            ..display.clone()
        };
        assert!(!screen_guide_target_is_unchanged(
            &captured_context,
            None,
            &display,
            &captured_context,
            None,
            &changed_display,
        ));
    }

    #[test]
    fn retina_cursor_coordinates_map_from_physical_to_overlay_points() {
        let position = screen_guide_cursor_in_monitor(1512.0, 982.0, 0, 0, 3024, 1964, 2.0);
        assert_eq!(position, ScreenGuideCursorPosition { x: 756.0, y: 491.0 });

        let offset = screen_guide_cursor_in_monitor(2500.0, 500.0, 2000, -200, 2000, 1200, 2.0);
        assert_eq!(offset, ScreenGuideCursorPosition { x: 250.0, y: 350.0 });
    }

    #[test]
    fn unsafe_or_missing_overlay_cannot_activate() {
        assert!(screen_guide_overlay_can_activate(true, true));
        assert!(!screen_guide_overlay_can_activate(false, true));
        assert!(!screen_guide_overlay_can_activate(true, false));
    }

    #[test]
    fn surface_cleanup_requires_both_overlay_hide_and_window_restore() {
        assert!(combine_screen_guide_surface_cleanup(Ok(()), Ok(())).is_ok());
        assert!(combine_screen_guide_surface_cleanup(
            Err("overlay stayed visible".to_string()),
            Ok(())
        )
        .is_err());
        assert!(combine_screen_guide_surface_cleanup(
            Ok(()),
            Err("window stayed hidden".to_string())
        )
        .is_err());
    }

    #[test]
    fn destroyed_overlay_keeps_capture_suppressed_until_capture_and_restore_unwind() {
        assert!(!screen_guide_destroy_cleanup_can_release(false, false));
        assert!(!screen_guide_destroy_cleanup_can_release(false, true));
        assert!(!screen_guide_destroy_cleanup_can_release(true, false));
        assert!(screen_guide_destroy_cleanup_can_release(true, true));
    }

    #[test]
    fn point_markup_without_an_answer_is_not_usable() {
        assert!(!is_usable_model_answer("[POINT:none]"));
    }

    #[test]
    fn unchanged_metadata_cannot_authorize_a_changed_visual_target() {
        let cue = ScreenGuidePointCue {
            x: 0.5,
            y: 0.5,
            label: Some("Save changes".to_string()),
        };
        let unchanged = vec![ScreenGuideOcrLine {
            text: "Save changes".to_string(),
            x: 0.501,
            y: 0.499,
        }];
        let moved = vec![ScreenGuideOcrLine {
            text: "Save changes".to_string(),
            x: 0.5,
            y: 0.65,
        }];
        let replaced = vec![ScreenGuideOcrLine {
            text: "Delete account".to_string(),
            x: 0.5,
            y: 0.5,
        }];

        assert!(screen_guide_cue_matches_current_evidence(&cue, &unchanged));
        assert!(!screen_guide_cue_matches_current_evidence(&cue, &moved));
        assert!(!screen_guide_cue_matches_current_evidence(&cue, &replaced));
    }

    #[test]
    fn sensitive_second_pass_ocr_suppresses_the_point_cue() {
        let context = FrontmostAppContext {
            app_name: "Terminal".to_string(),
            bundle_id: Some("com.apple.Terminal".to_string()),
            window_title: "shell".to_string(),
            window_title_verified: true,
            browser_url: None,
        };

        assert!(screen_guide_ocr_is_allowed(
            &context,
            None,
            "Build completed successfully",
            &[],
        ));
        assert!(!screen_guide_ocr_is_allowed(
            &context,
            None,
            "export api_key=private-value",
            &[],
        ));
    }

    #[test]
    fn unverifiable_window_metadata_has_an_actionable_permission_error() {
        let context = FrontmostAppContext {
            app_name: "Notes".to_string(),
            bundle_id: Some("com.apple.Notes".to_string()),
            window_title: "com.apple.Notes".to_string(),
            window_title_verified: false,
            browser_url: None,
        };

        let error = screen_guide_context_verification_error(&context, None)
            .expect("unverified window should fail closed");
        assert!(error.contains("Accessibility"));
        assert!(!error.contains("private screen"));
        assert!(!screen_guide_context_can_be_captured(&context, None, &[]));
    }

    #[test]
    fn missing_browser_url_has_an_actionable_automation_or_support_error() {
        let context = FrontmostAppContext {
            app_name: "Firefox".to_string(),
            bundle_id: Some("org.mozilla.firefox".to_string()),
            window_title: "Example".to_string(),
            window_title_verified: true,
            browser_url: None,
        };

        let error = screen_guide_context_verification_error(&context, None)
            .expect("browser without a verified URL should fail closed");
        assert!(error.contains("Automation"));
        assert!(error.contains("Safari, Chrome, Arc, Brave, or Edge"));
        assert!(!screen_guide_context_can_be_captured(&context, None, &[]));
    }

    #[test]
    fn rejects_shortcuts_reserved_by_omnibar_or_enabled_autofill() {
        let mut screen_guide = ScreenGuideConfig {
            enabled: true,
            ..ScreenGuideConfig::default()
        };
        let mut autofill = crate::config::AutofillConfig::default();

        screen_guide.shortcut = super::super::omnibar::OMNIBAR_SHORTCUT.to_string();
        assert!(validate_screen_guide_shortcut_conflicts(&screen_guide, &autofill).is_err());

        screen_guide.shortcut = "Control+Shift+G".to_string();
        autofill.enabled = true;
        autofill.shortcut = "Control+Shift+G".to_string();
        assert!(validate_screen_guide_shortcut_conflicts(&screen_guide, &autofill).is_err());

        autofill.enabled = false;
        assert!(validate_screen_guide_shortcut_conflicts(&screen_guide, &autofill).is_ok());
    }

    #[test]
    fn hidden_overlay_events_are_queued_until_ready() {
        let mut delivery = ScreenGuideOverlayDelivery::default();
        assert!(delivery
            .deliver_or_queue(PendingScreenGuideEvent::Shortcut {
                action: ScreenGuideShortcutAction::Press,
                generation: 1,
            })
            .is_none());
        assert!(delivery
            .deliver_or_queue(PendingScreenGuideEvent::Shortcut {
                action: ScreenGuideShortcutAction::Release,
                generation: 1,
            })
            .is_none());

        let pending = delivery.set_ready(true);
        assert_eq!(
            pending,
            vec![
                PendingScreenGuideEvent::Shortcut {
                    action: ScreenGuideShortcutAction::Press,
                    generation: 1,
                },
                PendingScreenGuideEvent::Shortcut {
                    action: ScreenGuideShortcutAction::Release,
                    generation: 1,
                },
            ]
        );
        assert!(delivery.take_flush_batch().is_none());
    }

    #[test]
    fn overlay_shutdown_discards_queued_questions() {
        let mut delivery = ScreenGuideOverlayDelivery::default();
        delivery.deliver_or_queue(PendingScreenGuideEvent::Submit {
            text: "private pending question".to_string(),
            generation: 4,
        });

        assert!(delivery.set_ready(false).is_empty());
        assert!(delivery.set_ready(true).is_empty());
    }

    #[test]
    fn terminal_cancel_discards_only_its_queued_question() {
        let mut delivery = ScreenGuideOverlayDelivery::default();
        delivery.deliver_or_queue(PendingScreenGuideEvent::Submit {
            text: "cancelled private question".to_string(),
            generation: 4,
        });
        delivery.deliver_or_queue(PendingScreenGuideEvent::Submit {
            text: "newer question".to_string(),
            generation: 5,
        });
        delivery.deliver_or_queue(PendingScreenGuideEvent::Shortcut {
            action: ScreenGuideShortcutAction::Cancel,
            generation: 4,
        });

        assert_eq!(
            delivery.set_ready(true),
            vec![
                PendingScreenGuideEvent::Submit {
                    text: "newer question".to_string(),
                    generation: 5,
                },
                PendingScreenGuideEvent::Shortcut {
                    action: ScreenGuideShortcutAction::Cancel,
                    generation: 4,
                },
            ]
        );
    }

    #[test]
    fn generation_invalidation_discards_ended_turn_question_only() {
        let mut runtime = ScreenGuideRuntime {
            enabled: true,
            generation: 8,
            lease_epoch: 0,
            turn_cancel: Some(Arc::new(AtomicBool::new(false))),
        };
        let mut delivery = ScreenGuideOverlayDelivery::default();
        delivery.deliver_or_queue(PendingScreenGuideEvent::Submit {
            text: "ended question".to_string(),
            generation: 8,
        });
        delivery.deliver_or_queue(PendingScreenGuideEvent::Submit {
            text: "newer question".to_string(),
            generation: 9,
        });

        assert_eq!(invalidate_screen_guide_turn(&mut runtime, &mut delivery), 9);
        assert_eq!(
            delivery.set_ready(true),
            vec![PendingScreenGuideEvent::Submit {
                text: "newer question".to_string(),
                generation: 9,
            }]
        );
    }

    #[test]
    fn microphone_watchdog_requires_the_matching_stop_acknowledgement() {
        let mut safety = ScreenGuideMicrophoneSafety::default();
        let active_epoch = safety
            .mark_started(3)
            .expect("a current generation may start recording");
        assert!(safety.watchdog_matches(
            ScreenGuideMicrophoneWatchdog::MaximumDuration,
            3,
            active_epoch,
        ));

        let stop_epoch = safety
            .request_stop(4)
            .expect("a newer generation requests a stop");
        assert!(!safety.watchdog_matches(
            ScreenGuideMicrophoneWatchdog::MaximumDuration,
            3,
            active_epoch,
        ));
        assert!(safety.watchdog_matches(
            ScreenGuideMicrophoneWatchdog::StopAcknowledgement,
            4,
            stop_epoch,
        ));
        assert!(!safety.acknowledge_stop(3));
        assert!(safety.acknowledge_stop(4));
        assert!(!safety.watchdog_matches(
            ScreenGuideMicrophoneWatchdog::StopAcknowledgement,
            4,
            stop_epoch,
        ));
        assert!(safety.mark_started(4).is_none());
        assert!(safety.mark_started(5).is_some());
        assert!(safety.request_stop(4).is_none());
    }

    #[test]
    fn startup_event_overflow_replays_only_a_balanced_cancel() {
        let mut delivery = ScreenGuideOverlayDelivery::default();
        for generation in 1..=MAX_PENDING_SCREEN_GUIDE_EVENTS as u64 {
            delivery.deliver_or_queue(PendingScreenGuideEvent::Shortcut {
                action: ScreenGuideShortcutAction::Press,
                generation,
            });
        }
        delivery.deliver_or_queue(PendingScreenGuideEvent::Shortcut {
            action: ScreenGuideShortcutAction::Release,
            generation: 17,
        });
        delivery.deliver_or_queue(PendingScreenGuideEvent::Shortcut {
            action: ScreenGuideShortcutAction::Press,
            generation: 18,
        });

        assert_eq!(
            delivery.set_ready(true),
            vec![PendingScreenGuideEvent::Shortcut {
                action: ScreenGuideShortcutAction::Cancel,
                generation: 17,
            }]
        );
    }

    #[test]
    fn generation_guard_rejects_stale_or_disabled_results() {
        assert!(screen_guide_request_is_current(7, 7, true));
        assert!(!screen_guide_request_is_current(6, 7, true));
        assert!(!screen_guide_request_is_current(7, 7, false));
    }

    #[test]
    fn visual_timeout_only_matches_its_completed_client_turn() {
        assert!(screen_guide_visual_matches(7, 7, true));
        assert!(!screen_guide_visual_matches(7, 8, true));
        assert!(!screen_guide_visual_matches(7, 7, false));
    }

    #[test]
    fn renewed_hidden_lease_supersedes_older_timers() {
        assert!(screen_guide_lease_matches(7, 3, 7, 3, true));
        assert!(!screen_guide_lease_matches(7, 2, 7, 3, true));
        assert!(!screen_guide_lease_matches(7, 3, 8, 3, true));
    }

    #[test]
    fn incognito_rejects_input_before_microphone_or_overlay_work() {
        assert!(screen_guide_input_allowed(false));
        assert!(!screen_guide_input_allowed(true));
        assert!(screen_guide_press_should_begin(None, false));
        assert!(!screen_guide_press_should_begin(Some(7), false));
        assert!(!screen_guide_press_should_begin(None, true));
    }

    #[test]
    fn delayed_privacy_cleanup_never_cancels_a_post_resume_turn() {
        assert!(screen_guide_privacy_cleanup_still_owns_turn(true, 12, 11));
        assert!(screen_guide_privacy_cleanup_still_owns_turn(false, 11, 11));
        assert!(!screen_guide_privacy_cleanup_still_owns_turn(false, 12, 11));
    }

    #[test]
    fn stale_privacy_reset_retry_cannot_destroy_a_newer_turn_after_privacy_lifts() {
        let overlay_epoch = 7;
        let privacy_generation = 11;
        let newer_turn_generation = 12;

        assert!(!screen_guide_privacy_reset_should_retry(
            overlay_epoch,
            overlay_epoch,
            false,
            newer_turn_generation,
            privacy_generation,
        ));
        assert!(screen_guide_privacy_reset_should_retry(
            overlay_epoch,
            overlay_epoch,
            true,
            newer_turn_generation,
            privacy_generation,
        ));
        assert!(!screen_guide_privacy_reset_should_retry(
            overlay_epoch,
            overlay_epoch + 1,
            true,
            privacy_generation,
            privacy_generation,
        ));
    }

    #[test]
    fn startup_registration_failure_reconciles_enabled_settings() {
        let mut settings = ScreenGuideConfig::default();
        settings.enabled = true;

        assert!(screen_guide_settings_after_startup_registration(settings.clone(), true).enabled);
        assert!(!screen_guide_settings_after_startup_registration(settings, false).enabled);
    }

    #[test]
    fn owned_windows_restore_even_when_privacy_forbids_the_overlay() {
        let deferred = DeferredFndrRestore {
            app_was_hidden: true,
            window_labels: vec!["main".to_string()],
            owner_generation: 9,
        };
        let plan = screen_guide_restore_plan(&deferred, &DeferredFndrRestore::default(), 9, false);

        assert_eq!(plan, Some((deferred, false)));
    }

    #[test]
    fn newer_restore_owner_cannot_be_replaced_by_a_stale_local_copy() {
        let deferred = DeferredFndrRestore {
            owner_generation: 10,
            ..DeferredFndrRestore::default()
        };
        let local = DeferredFndrRestore {
            app_was_hidden: true,
            owner_generation: 9,
            ..DeferredFndrRestore::default()
        };

        assert!(screen_guide_restore_plan(&deferred, &local, 9, false).is_none());
    }

    #[test]
    fn shortcut_cleanup_requires_screen_guide_ownership() {
        assert!(screen_guide_owns_shortcut(Some(11), 11));
        assert!(!screen_guide_owns_shortcut(None, 11));
        assert!(!screen_guide_owns_shortcut(Some(12), 11));
    }

    #[test]
    fn shortcut_actions_serialize_to_the_frontend_union() {
        assert_eq!(
            serde_json::to_string(&ScreenGuideShortcutAction::Press).unwrap(),
            "\"press\""
        );
        assert_eq!(
            serde_json::to_string(&ScreenGuideShortcutAction::Release).unwrap(),
            "\"release\""
        );
        assert_eq!(
            serde_json::to_string(&ScreenGuideShortcutAction::Cancel).unwrap(),
            "\"cancel\""
        );
    }
}
