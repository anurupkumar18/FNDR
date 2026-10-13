//! Hold-to-dictate: key down captures the focused field and starts a voice
//! session, key up stops it, and the cleaned transcript is typed into the field.
//!
//! The global-shortcut plugin calls its handler on the macOS event thread while
//! holding its registry lock, so the handler only queues a transition. One
//! worker thread owns the rest, which keeps press, release and transcript
//! strictly ordered.

use crate::accessibility::{self, PendingAutofillTarget};
use crate::config::DictationConfig;
use crate::dictation::{
    clean_transcript, is_quick_tap, parse_voice_event, plan_insertion, ClipboardReason,
    InsertFacts, InsertPlan, ScreenVocabulary, SkipReason, VoiceOutcome,
};
use crate::voice::{VoiceManager, VoiceMode, VoiceSurface, VOICE_STATE_EVENT};
use crate::AppState;
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::atomic::Ordering;
use std::sync::mpsc::{channel, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Listener, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

/// A session older than this never got its transcript; the next press replaces it.
const STALE_SESSION: Duration = Duration::from_secs(45);

enum Transition {
    Pressed,
    Released,
    Voice(VoiceOutcome),
}

struct ActiveDictation {
    pressed_at: Instant,
    session_id: Option<String>,
    target: Option<PendingAutofillTarget>,
    vocabulary: JoinHandle<ScreenVocabulary>,
}

#[derive(Default)]
struct Registration {
    shortcut: Option<Shortcut>,
    queue: Option<Sender<Transition>>,
}

static REGISTRATION: Lazy<Mutex<Registration>> = Lazy::new(|| Mutex::new(Registration::default()));

pub fn register_dictation_shortcut(
    app: &AppHandle,
    config: &DictationConfig,
) -> Result<(), String> {
    let mut registration = REGISTRATION.lock();
    let desired = if config.enabled {
        Some(
            config
                .shortcut
                .parse::<Shortcut>()
                .map_err(|err| format!("Invalid Dictation shortcut '{}': {err}", config.shortcut))?,
        )
    } else {
        None
    };
    if desired == registration.shortcut {
        return Ok(());
    }
    if let Some(owned) = registration.shortcut.take() {
        if app.global_shortcut().is_registered(owned) {
            app.global_shortcut()
                .unregister(owned)
                .map_err(|err| format!("Dictation could not release its shortcut: {err}"))?;
        }
    }
    let Some(shortcut) = desired else {
        tracing::info!("dictation: shortcut disabled");
        return Ok(());
    };
    if app.global_shortcut().is_registered(shortcut) {
        return Err(format!(
            "Dictation shortcut '{}' is already used by another FNDR feature.",
            config.shortcut
        ));
    }

    let queue = match registration.queue.clone() {
        Some(queue) => queue,
        None => {
            let queue = start_worker(app.clone())?;
            registration.queue = Some(queue.clone());
            queue
        }
    };
    let press_queue = queue.clone();
    app.global_shortcut()
        .on_shortcut(shortcut, move |_app, _shortcut, event| {
            let transition = match event.state() {
                ShortcutState::Pressed => Transition::Pressed,
                ShortcutState::Released => Transition::Released,
            };
            let _ = press_queue.send(transition);
        })
        .map_err(|err| err.to_string())?;
    registration.shortcut = Some(shortcut);
    tracing::info!(shortcut = %config.shortcut, "dictation: shortcut registered");
    Ok(())
}

fn start_worker(app: AppHandle) -> Result<Sender<Transition>, String> {
    let (queue, inbox) = channel::<Transition>();
    let voice_queue = queue.clone();
    app.listen(VOICE_STATE_EVENT, move |event| {
        if let Some(outcome) = parse_voice_event(event.payload()) {
            let _ = voice_queue.send(Transition::Voice(outcome));
        }
    });
    std::thread::Builder::new()
        .name("fndr-dictation".to_string())
        .spawn(move || {
            let mut active: Option<ActiveDictation> = None;
            while let Ok(transition) = inbox.recv() {
                match transition {
                    Transition::Pressed => on_pressed(&app, &mut active),
                    Transition::Released => on_released(&app, &mut active),
                    Transition::Voice(outcome) => on_voice(&app, &mut active, outcome),
                }
            }
        })
        .map_err(|err| format!("Could not start the dictation worker: {err}"))?;
    Ok(queue)
}

fn on_pressed(app: &AppHandle, active: &mut Option<ActiveDictation>) {
    if let Some(current) = active.as_ref() {
        if current.pressed_at.elapsed() < STALE_SESSION {
            return;
        }
        active.take();
    }
    let Some(state) = app.try_state::<Arc<AppState>>() else {
        return;
    };
    if state.is_incognito.load(Ordering::SeqCst) {
        tracing::info!("dictation: ignored while Private Mode is on");
        return;
    }
    let learned_words = state.config.read().dictation.learned_words.clone();

    let (context, target) = match accessibility::capture_focused_context_snapshot() {
        Ok((context, _image, target)) => (Some(context), Some(target)),
        Err(err) => {
            tracing::info!("dictation: no field captured, the result will be copied: {err}");
            (None, None)
        }
    };
    let field_text = context
        .map(|c| format!("{}\n{}\n{}\n{}", c.label, c.window_title, c.app_name, c.current_value))
        .unwrap_or_default();
    let vocabulary = std::thread::spawn(move || {
        let screen = accessibility::read_screen_text();
        ScreenVocabulary::from_screen_text(&format!("{field_text}\n{screen}"))
            .with_learned_words(&learned_words)
    });

    let voice = app.state::<VoiceManager>();
    match tauri::async_runtime::block_on(voice.start(VoiceSurface::Dictation, VoiceMode::PushToTalk))
    {
        Ok(session) => {
            *active = Some(ActiveDictation {
                pressed_at: Instant::now(),
                session_id: Some(session.session_id),
                target,
                vocabulary,
            });
        }
        Err(err) => tracing::warn!("dictation: voice session did not start: {err}"),
    }
}

fn on_released(app: &AppHandle, active: &mut Option<ActiveDictation>) {
    let Some(current) = active.as_ref() else {
        return;
    };
    let Some(session_id) = current.session_id.clone() else {
        return;
    };
    let voice = app.state::<VoiceManager>();
    if is_quick_tap(current.pressed_at.elapsed()) {
        active.take();
        if let Err(err) = tauri::async_runtime::block_on(voice.cancel(&session_id)) {
            tracing::warn!("dictation: could not cancel a tapped session: {err}");
        }
    } else if let Err(err) = tauri::async_runtime::block_on(voice.stop(&session_id)) {
        tracing::warn!("dictation: could not stop the session: {err}");
        active.take();
    }
}

fn on_voice(app: &AppHandle, active: &mut Option<ActiveDictation>, outcome: VoiceOutcome) {
    let session_matches = |id: &str| active.as_ref().and_then(|a| a.session_id.as_deref()) == Some(id);
    match outcome {
        VoiceOutcome::Ended { session_id } if session_matches(&session_id) => {
            active.take();
        }
        VoiceOutcome::Final { session_id, text } if session_matches(&session_id) => {
            if let Some(finished) = active.take() {
                deliver(app, finished, &text);
            }
        }
        _ => {}
    }
}

fn deliver(app: &AppHandle, finished: ActiveDictation, transcript: &str) {
    let vocabulary = finished.vocabulary.join().unwrap_or_default();
    let cleaned = clean_transcript(transcript, &vocabulary);
    let state = accessibility::dictation_target_state(finished.target.as_ref());
    let facts = InsertFacts {
        secure_input_active: state.secure_field_focused,
        target_captured: state.captured,
        target_still_focused: state.still_focused,
    };
    match plan_insertion(&cleaned, facts) {
        InsertPlan::Type(text) => {
            let typed = finished
                .target
                .as_ref()
                .map(|target| accessibility::inject_text_into_pending_target(target, &text));
            match typed {
                Some(Ok(())) => tracing::info!(chars = text.chars().count(), "dictation: typed"),
                Some(Err(err)) => {
                    tracing::warn!("dictation: typing failed, copying instead: {err}");
                    copy_and_tell(app, &text, ClipboardReason::FocusMoved);
                }
                None => copy_and_tell(app, &text, ClipboardReason::NoTarget),
            }
        }
        InsertPlan::Clipboard(text, reason) => copy_and_tell(app, &text, reason),
        InsertPlan::Skip(SkipReason::SecureField) => {
            tracing::info!("dictation: nothing inserted, a secure field has focus");
        }
        InsertPlan::Skip(SkipReason::Empty) => {
            tracing::info!("dictation: nothing heard");
        }
    }
}

fn copy_and_tell(app: &AppHandle, text: &str, reason: ClipboardReason) {
    if let Err(err) = copy_to_clipboard(text) {
        tracing::warn!("dictation: clipboard copy failed: {err}");
        return;
    }
    let body = match reason {
        ClipboardReason::FocusMoved => "The field lost focus, so the text is on your clipboard.",
        ClipboardReason::NoTarget => "No text field had focus, so the text is on your clipboard.",
    };
    if let Some(state) = app.try_state::<Arc<AppState>>() {
        crate::notify::send(app, &state, "dictation_copied", "Dictation", body);
    }
}

fn copy_to_clipboard(text: &str) -> Result<(), String> {
    let mut child = Command::new("/usr/bin/pbcopy")
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|err| err.to_string())?;
    child
        .stdin
        .take()
        .ok_or_else(|| "pbcopy has no stdin".to_string())?
        .write_all(text.as_bytes())
        .map_err(|err| err.to_string())?;
    let status = child.wait().map_err(|err| err.to_string())?;
    status.success().then_some(()).ok_or_else(|| format!("pbcopy exited with {status}"))
}
