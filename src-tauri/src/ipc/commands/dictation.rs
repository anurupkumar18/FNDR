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
    clean_transcript, held_for, is_quick_tap, parse_voice_event, plan_insertion, ClipboardReason,
    InsertFacts, InsertPlan, ScreenVocabulary, SkipReason, VoiceOutcome,
};
use crate::voice::{VoiceManager, VoiceMode, VoiceSurface, VOICE_STATE_EVENT};
use crate::AppState;
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Listener, Manager, State};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

/// How long delivery waits for the screen read. OCR can take seconds; the text
/// must not wait on it, so a late vocabulary is dropped for this dictation.
const VOCABULARY_WAIT: Duration = Duration::from_millis(1200);

/// A session older than this never got its transcript; the next press replaces it.
const STALE_SESSION: Duration = Duration::from_secs(45);

enum Transition {
    Pressed(Instant),
    Released(Instant),
    Voice(VoiceOutcome),
}

struct ActiveDictation {
    pressed_at: Instant,
    session_id: Option<String>,
    target: Option<PendingAutofillTarget>,
    vocabulary: Receiver<ScreenVocabulary>,
    learned_words: Vec<String>,
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
                ShortcutState::Pressed => Transition::Pressed(Instant::now()),
                ShortcutState::Released => Transition::Released(Instant::now()),
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
                    Transition::Pressed(at) => on_pressed(&app, &mut active, at),
                    Transition::Released(at) => on_released(&app, &mut active, at),
                    Transition::Voice(outcome) => on_voice(&app, &mut active, outcome),
                }
            }
        })
        .map_err(|err| format!("Could not start the dictation worker: {err}"))?;
    Ok(queue)
}

fn on_pressed(app: &AppHandle, active: &mut Option<ActiveDictation>, pressed_at: Instant) {
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
    tracing::info!("dictation: key down");

    let capture_started = Instant::now();
    let (context, target) = match accessibility::capture_focused_field_only() {
        Ok((context, target)) => (Some(context), Some(target)),
        Err(err) => {
            tracing::info!("dictation: no field captured, the result will be copied: {err}");
            (None, None)
        }
    };
    tracing::info!(ms = capture_started.elapsed().as_millis() as u64, "dictation: field captured");
    let field_text = context
        .map(|c| format!("{}\n{}\n{}\n{}", c.label, c.window_title, c.app_name, c.current_value))
        .unwrap_or_default();
    let (vocabulary_tx, vocabulary) = channel();
    let screen_learned_words = learned_words.clone();
    std::thread::spawn(move || {
        let started = Instant::now();
        let screen = accessibility::read_screen_text();
        tracing::info!(
            chars = screen.chars().count(),
            ms = started.elapsed().as_millis() as u64,
            "dictation: screen read finished"
        );
        let _ = vocabulary_tx.send(
            ScreenVocabulary::from_screen_text(&format!("{field_text}\n{screen}"))
                .with_learned_words(&screen_learned_words),
        );
    });

    let voice = app.state::<VoiceManager>();
    match tauri::async_runtime::block_on(voice.start(VoiceSurface::Dictation, VoiceMode::PushToTalk))
    {
        Ok(session) => {
            tracing::info!(captured = target.is_some(), "dictation: listening");
            *active = Some(ActiveDictation {
                pressed_at,
                session_id: Some(session.session_id),
                target,
                vocabulary,
                learned_words,
            });
        }
        Err(err) => tracing::warn!("dictation: voice session did not start: {err}"),
    }
}

fn on_released(app: &AppHandle, active: &mut Option<ActiveDictation>, released_at: Instant) {
    let Some(current) = active.as_ref() else {
        return;
    };
    let Some(session_id) = current.session_id.clone() else {
        return;
    };
    let voice = app.state::<VoiceManager>();
    let held = held_for(current.pressed_at, released_at);
    tracing::info!(held_ms = held.as_millis() as u64, "dictation: key up");
    if is_quick_tap(held) {
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
    // The transcript is what the person said; keep it out of the logs.
    let kind = match &outcome {
        VoiceOutcome::Final { .. } => "final",
        VoiceOutcome::Ended { .. } => "ended",
    };
    tracing::info!(kind, "dictation: voice event");
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
    let vocabulary = finished
        .vocabulary
        .recv_timeout(VOCABULARY_WAIT)
        .unwrap_or_else(|_| {
            tracing::info!("dictation: the screen read was not ready, using learned words only");
            ScreenVocabulary::default().with_learned_words(&finished.learned_words)
        });
    let cleaned = clean_transcript(transcript, &vocabulary);
    let state = accessibility::dictation_target_state(finished.target.as_ref());
    let facts = InsertFacts {
        secure_input_active: state.secure_field_focused,
        target_captured: state.captured,
        target_still_focused: state.still_focused,
    };
    tracing::info!(?facts, chars = cleaned.chars().count(), "dictation: delivering");
    match plan_insertion(&cleaned, facts) {
        InsertPlan::Type(text) => {
            let typed = finished
                .target
                .as_ref()
                .map(|target| accessibility::insert_text_at_caret(target, &text));
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
    if let Err(err) = accessibility::write_clipboard_text(text) {
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

const MAX_LEARNED_WORDS: usize = 200;

fn normalized(mut config: DictationConfig) -> DictationConfig {
    config.shortcut = config.shortcut.trim().to_string();
    let mut seen = std::collections::HashSet::new();
    config.learned_words = config
        .learned_words
        .iter()
        .map(|word| word.trim().to_string())
        .filter(|word| !word.is_empty() && seen.insert(word.to_lowercase()))
        .take(MAX_LEARNED_WORDS)
        .collect();
    config
}

/// Dictation holds its key down, so a shortcut another feature owns, or one
/// that is not registered in this build but is promised elsewhere, would fight it.
fn validate_shortcut_conflicts(
    dictation: &DictationConfig,
    config: &crate::config::Config,
) -> Result<(), String> {
    if !dictation.enabled {
        return Ok(());
    }
    let wanted: Shortcut = dictation
        .shortcut
        .parse()
        .map_err(|err| format!("Invalid Dictation shortcut '{}': {err}", dictation.shortcut))?;
    let others = [
        ("Omnibar", Some(super::omnibar::OMNIBAR_SHORTCUT.to_string())),
        ("the notch", Some(super::notch::NOTCH_HUD_SHORTCUT.to_string())),
        ("Auto-fill", config.autofill.enabled.then(|| config.autofill.shortcut.clone())),
        ("Screen Guide", config.screen_guide.enabled.then(|| config.screen_guide.shortcut.clone())),
    ];
    for (name, shortcut) in others {
        let Some(shortcut) = shortcut else { continue };
        if shortcut.parse::<Shortcut>().is_ok_and(|other| other == wanted) {
            return Err(format!(
                "Dictation shortcut '{}' conflicts with {name}.",
                dictation.shortcut
            ));
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn get_dictation_settings(
    state: State<'_, Arc<AppState>>,
) -> Result<DictationConfig, String> {
    Ok(normalized(state.inner().config.read().dictation.clone()))
}

#[tauri::command]
pub async fn set_dictation_settings(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    settings: DictationConfig,
) -> Result<DictationConfig, String> {
    let mut next = normalized(settings);
    let shortcut: Shortcut = next
        .shortcut
        .parse()
        .map_err(|err| format!("Invalid Dictation shortcut '{}': {err}", next.shortcut))?;
    next.shortcut = shortcut.into_string();
    validate_shortcut_conflicts(&next, &state.inner().config.read())?;

    let previous = state.inner().config.read().dictation.clone();
    register_dictation_shortcut(&app, &next)?;

    let saved = {
        let mut config = state.inner().config.write();
        config.dictation = next.clone();
        let result = config
            .save()
            .map_err(|err: Box<dyn std::error::Error>| err.to_string());
        if result.is_err() {
            config.dictation = previous.clone();
        }
        result
    };
    if let Err(err) = saved {
        return match register_dictation_shortcut(&app, &previous) {
            Ok(()) => Err(err),
            Err(rollback) => Err(format!(
                "{err} The previous Dictation shortcut could not be restored: {rollback}"
            )),
        };
    }
    Ok(next)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn enabled(shortcut: &str) -> DictationConfig {
        DictationConfig {
            enabled: true,
            shortcut: shortcut.to_string(),
            learned_words: Vec::new(),
        }
    }

    #[test]
    fn learned_words_are_trimmed_deduped_and_capped() {
        let config = normalized(DictationConfig {
            enabled: true,
            shortcut: " Control+Alt+D ".into(),
            learned_words: vec![" Kunj ".into(), "kunj".into(), "".into(), "LanceDB".into()],
        });
        assert_eq!(config.shortcut, "Control+Alt+D");
        assert_eq!(config.learned_words, vec!["Kunj", "LanceDB"]);

        let many = DictationConfig {
            learned_words: (0..500).map(|i| format!("word{i}")).collect(),
            ..DictationConfig::default()
        };
        assert_eq!(normalized(many).learned_words.len(), MAX_LEARNED_WORDS);
    }

    #[test]
    fn the_default_shortcut_conflicts_with_nothing_fndr_ships() {
        let mut config = Config::default();
        config.autofill.enabled = true;
        config.screen_guide.enabled = true;
        assert!(validate_shortcut_conflicts(&enabled("Control+Alt+D"), &config).is_ok());
    }

    #[test]
    fn a_shortcut_another_surface_owns_is_refused() {
        let mut config = Config::default();
        config.autofill.enabled = true;
        config.screen_guide.enabled = true;
        for taken in ["Alt+Space", "Alt+N", "Alt+F", "Control+Alt+Space"] {
            let err = validate_shortcut_conflicts(&enabled(taken), &config).unwrap_err();
            assert!(err.contains("conflicts with"), "{taken}: {err}");
        }
    }

    #[test]
    fn a_disabled_dictation_is_not_validated() {
        let off = DictationConfig {
            shortcut: "Alt+Space".into(),
            ..DictationConfig::default()
        };
        assert!(validate_shortcut_conflicts(&off, &Config::default()).is_ok());
    }
}
