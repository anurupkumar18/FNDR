//! Smart Fill: press Command twice in any text box and FNDR writes the text.
//!
//! The person is in Messages, a form or a mail draft with the cursor in a
//! text box. Two taps of Command read the screen (text and screenshot), pull
//! the few memories that bear on it, ask ChatGPT on the person's own plan for
//! the text, and write it into the box. A glow follows the caret while it
//! works. Esc, or two more taps, stops it and puts back what was there.
//!
//! Off until the person turns it on in Settings. A password field, an app on
//! the blocklist and FNDR itself are never touched. The screenshot goes to
//! ChatGPT only for this request.

pub mod double_tap;
pub mod monitor;
pub mod overlay;
pub mod prompt;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use tauri::{AppHandle, Manager};

use crate::accessibility::fill::{self, FocusedField};
use crate::privacy::Blocklist;
use crate::AppState;
use monitor::Signal;
use prompt::{FillContext, MemoryNote};

/// How long ChatGPT gets to write the text.
const CODEX_TIMEOUT: Duration = Duration::from_secs(45);
/// How much of the screen's text goes to the model.
const SCREEN_TEXT_CHARS: usize = 6_000;
/// Less on-screen text than this counts as "Accessibility read nothing".
const MIN_SCREEN_TEXT_CHARS: usize = 40;
/// Below this much, Accessibility has most likely read only toolbars and
/// buttons, so the screenshot is read as well.
const ENOUGH_SCREEN_TEXT_CHARS: usize = 400;
/// How long Smart Fill waits for OCR before going on without it.
const OCR_BUDGET: Duration = Duration::from_secs(12);
/// Accessibility walk budget: time and elements. A mail page in a browser is
/// thousands of elements, far more than the default allows.
const AX_WALK_TIME: Duration = Duration::from_millis(400);
const AX_WALK_NODES: usize = 20_000;
/// Shown when FNDR can neither read the window's text nor see the screen.
const SCREEN_RECORDING_HINT: &str = "FNDR can't see the screen. Turn FNDR on under Screen & System Audio Recording in System Settings (remove it and add it again after an update), then reopen FNDR.";
/// How long the word-by-word reveal takes in total, however long the text.
const REVEAL_TOTAL: Duration = Duration::from_millis(1_800);

static RUNNING: AtomicBool = AtomicBool::new(false);
static CANCEL: Mutex<Option<Arc<AtomicBool>>> = Mutex::new(None);

/// Whether Smart Fill is on.
#[tauri::command]
pub async fn get_smart_fill_enabled(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<bool, String> {
    Ok(state.config.read().autofill.smart_fill)
}

/// Turns Smart Fill on or off, saves it, and starts or stops the Command
/// watch. It touches only this setting, not Auto-fill's shortcut.
#[tauri::command]
pub async fn set_smart_fill_enabled(
    app: AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
    enabled: bool,
) -> Result<bool, String> {
    {
        let mut config = state.config.write();
        let previous = config.autofill.smart_fill;
        config.autofill.smart_fill = enabled;
        if let Err(err) = config.save() {
            config.autofill.smart_fill = previous;
            return Err(err.to_string());
        }
    }
    apply_setting(&app, enabled);
    Ok(enabled)
}

/// Starts or stops the Command watch to match the setting.
pub fn apply_setting(app: &AppHandle, enabled: bool) {
    let handle = app.clone();
    let on_signal: Arc<dyn Fn(Signal) + Send + Sync> =
        Arc::new(move |signal| on_signal(&handle, signal));
    if enabled && !crate::accessibility::has_accessibility_permission() {
        tracing::warn!("Smart Fill is on but FNDR has no Accessibility permission");
    }
    if let Err(err) = monitor::set(app, enabled, on_signal) {
        tracing::warn!("Smart Fill could not change its Command watch: {err}");
    }
}

fn on_signal(app: &AppHandle, signal: Signal) {
    match signal {
        Signal::Escape => cancel_running(),
        Signal::DoubleTap => {
            if RUNNING.load(Ordering::SeqCst) {
                cancel_running();
                return;
            }
            let handle = app.clone();
            tauri::async_runtime::spawn(async move { run(handle).await });
        }
    }
}

fn cancel_running() {
    if let Some(flag) = CANCEL.lock().as_ref() {
        flag.store(true, Ordering::SeqCst);
    }
}

/// What a finished request tells the glow.
enum Outcome {
    Written,
    /// Nothing to say: a stray double tap, or the setting is off.
    Silent,
    Stopped,
    Nothing(&'static str),
    Failed(String),
}

async fn run(app: AppHandle) {
    if RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }
    let cancel = Arc::new(AtomicBool::new(false));
    *CANCEL.lock() = Some(cancel.clone());

    let outcome = fill_the_field(&app, cancel).await;

    *CANCEL.lock() = None;
    RUNNING.store(false, Ordering::SeqCst);
    match outcome {
        Outcome::Written => overlay::finish(&app, "done", None),
        Outcome::Silent => {}
        Outcome::Stopped => overlay::finish(&app, "stopped", None),
        Outcome::Nothing(message) => overlay::finish(&app, "notice", Some(message.to_string())),
        Outcome::Failed(message) => {
            tracing::info!("Smart Fill stopped: {message}");
            overlay::finish(&app, "error", Some(message));
        }
    }
}

async fn fill_the_field(app: &AppHandle, cancel: Arc<AtomicBool>) -> Outcome {
    let Some(state) = app.try_state::<Arc<AppState>>() else {
        return Outcome::Failed("FNDR is still starting.".to_string());
    };
    let state = state.inner().clone();
    let (enabled, blocklist) = {
        let config = state.config.read();
        (config.autofill.smart_fill, config.blocklist.clone())
    };
    if !enabled {
        return Outcome::Silent;
    }
    if state.is_incognito.load(Ordering::SeqCst) {
        return Outcome::Nothing("Smart Fill is paused while Private Mode is on.");
    }

    let field = match tokio::task::spawn_blocking(fill::focused_field).await {
        Ok(Ok(field)) => field,
        Ok(Err(refusal)) if refusal.show => return Outcome::Nothing(refusal.message),
        Ok(Err(_)) => return Outcome::Silent,
        Err(_) => return Outcome::Failed("The text box could not be read.".to_string()),
    };
    if field.pid == std::process::id() as i32 {
        return Outcome::Silent;
    }

    // Who owns the box is read first; nothing else is read or sent from an
    // app on the blocklist.
    let whose = tokio::task::spawn_blocking(whose_box).await;
    let Ok(whose) = whose else {
        return Outcome::Failed("The window could not be read.".to_string());
    };
    if Blocklist::is_blocked(&whose.app_name, &blocklist)
        || Blocklist::is_context_blocked(None, Some(&whose.window_title), &blocklist)
    {
        return Outcome::Nothing("This app is on FNDR's blocklist.");
    }

    let gathered = {
        let pid = field.pid;
        tokio::task::spawn_blocking(move || gather(pid, whose)).await
    };
    let Ok(mut gathered) = gathered else {
        return Outcome::Failed("The screen could not be read.".to_string());
    };
    let screen_visible = crate::capture::permissions::preflight_screen_capture_access().0;
    if !screen_visible {
        // Without the permission the capture shows only the wallpaper.
        gathered.screenshot = None;
    }
    let mut ocr_chars = 0;
    if gathered.screen_text.trim().chars().count() < ENOUGH_SCREEN_TEXT_CHARS {
        if let Some(image) = gathered.screenshot.clone() {
            let read = tokio::time::timeout(
                OCR_BUDGET,
                tokio::task::spawn_blocking(move || crate::accessibility::ocr_screen_text(&image)),
            )
            .await;
            if let Ok(Ok(read)) = read {
                ocr_chars = read.trim().chars().count();
                if !read.trim().is_empty() {
                    gathered.screen_text = format!("{}\n{}", gathered.screen_text.trim(), read);
                }
            }
        }
    }
    let text_chars = gathered.screen_text.trim().chars().count();
    tracing::info!(
        text_chars,
        ocr_chars,
        screenshot = gathered.screenshot.is_some(),
        screen_visible,
        web_field = field.web,
        app = %gathered.app_name,
        role = %field.role,
        label_chars = gathered.field_label.chars().count(),
        value_chars = field.value.chars().count(),
        "smart_fill:read the screen"
    );
    if text_chars < MIN_SCREEN_TEXT_CHARS && gathered.screenshot.is_none() {
        return Outcome::Failed(SCREEN_RECORDING_HINT.to_string());
    }
    if cancel.load(Ordering::SeqCst) {
        return Outcome::Stopped;
    }

    let caret = fill::caret_rect(field.pid);
    overlay::begin(app, caret);

    let notes = memory_notes(&state, &gathered).await;
    let user_text = prompt::user_text(&FillContext {
        app_name: &gathered.app_name,
        window_title: &gathered.window_title,
        field_label: &gathered.field_label,
        current_value: &field.value,
        screen_text: &gathered.screen_text,
        notes: &notes,
    });
    let answer = crate::ipc::commands::codex_account::answer_smart_fill_with_codex(
        user_text,
        gathered.screenshot,
        cancel.clone(),
        CODEX_TIMEOUT,
    )
    .await;
    if cancel.load(Ordering::SeqCst) {
        return Outcome::Stopped;
    }
    let answer = match answer {
        Ok(answer) => answer,
        Err(message) => return Outcome::Failed(message),
    };
    let Some(reply) = prompt::clean_reply(&answer) else {
        tracing::info!(
            answer_chars = answer.chars().count(),
            "smart_fill:no text to write"
        );
        return Outcome::Nothing("ChatGPT could not tell what to write in this box.");
    };

    let handle = app.clone();
    let revealed =
        tokio::task::spawn_blocking(move || reveal(&handle, &field, &reply, &cancel)).await;
    match revealed {
        Ok(outcome) => outcome,
        Err(_) => Outcome::Failed("Writing stopped unexpectedly.".to_string()),
    }
}

/// The app, window and field label of the box, and the screenshot the
/// snapshot already took when the label was missing.
struct Whose {
    app_name: String,
    window_title: String,
    field_label: String,
    screenshot: Option<Vec<u8>>,
}

fn whose_box() -> Whose {
    match crate::accessibility::capture_focused_context_snapshot() {
        Ok((context, screenshot, _)) => Whose {
            app_name: context.app_name,
            window_title: context.window_title,
            field_label: context.label,
            screenshot,
        },
        Err(_) => Whose {
            app_name: String::new(),
            window_title: String::new(),
            field_label: String::new(),
            screenshot: None,
        },
    }
}

struct Gathered {
    app_name: String,
    window_title: String,
    field_label: String,
    screen_text: String,
    screenshot: Option<Vec<u8>>,
}

/// The on-screen text and a screenshot, read before the glow appears.
fn gather(pid: i32, whose: Whose) -> Gathered {
    let screen_text = crate::accessibility::focused_text_budgeted(
        pid,
        SCREEN_TEXT_CHARS,
        AX_WALK_TIME,
        AX_WALK_NODES,
    )
    .map(|read| read.text)
    .unwrap_or_default();
    let screenshot = whose
        .screenshot
        .or_else(|| crate::capture::macos::capture_screen().ok());
    Gathered {
        app_name: whose.app_name,
        window_title: whose.window_title,
        field_label: whose.field_label,
        screen_text,
        screenshot,
    }
}

/// The few memories that bear on what is on screen. Goes through the one
/// retrieval function, so the blocklist and quality rules apply.
async fn memory_notes(state: &AppState, gathered: &Gathered) -> Vec<MemoryNote> {
    let tail: String = gathered
        .screen_text
        .chars()
        .rev()
        .take(300)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let query = format!(
        "{} {} {}",
        gathered.window_title, gathered.field_label, tail
    );
    if query.trim().is_empty() {
        return Vec::new();
    }
    match crate::ipc::commands::search::search_ranked_results(
        state,
        query.trim(),
        Some("90d"),
        None,
        4,
    )
    .await
    {
        Ok(results) => results
            .into_iter()
            .map(|result| MemoryNote {
                app_name: result.app_name,
                window_title: result.window_title,
                text: if result.snippet.trim().is_empty() {
                    result.clean_text
                } else {
                    result.snippet
                },
            })
            .collect(),
        Err(err) => {
            tracing::debug!("Smart Fill went without memories: {err}");
            Vec::new()
        }
    }
}

/// Cumulative prefixes of `text`, a couple of words further each step, the
/// last one being the whole text.
fn reveal_steps(text: &str) -> Vec<String> {
    const WORDS_PER_STEP: usize = 2;
    let mut steps = Vec::new();
    let mut words_seen = 0;
    let mut end = 0;
    for piece in text.split_inclusive(char::is_whitespace) {
        end += piece.len();
        if !piece.trim().is_empty() {
            words_seen += 1;
        }
        if words_seen % WORDS_PER_STEP == 0 && piece.ends_with(char::is_whitespace) {
            steps.push(text[..end].trim_end().to_string());
        }
    }
    if steps.last().map(String::as_str) != Some(text) {
        steps.push(text.to_string());
    }
    steps.dedup();
    steps
}

fn step_delay(steps: usize) -> Duration {
    let per_step = REVEAL_TOTAL / steps.max(1) as u32;
    per_step.clamp(Duration::from_millis(15), Duration::from_millis(70))
}

/// Writes the reply into the field, word by word when the app keeps what is
/// set through Accessibility and as one paste when it does not. Esc puts back
/// the text the field held.
fn reveal(app: &AppHandle, field: &FocusedField, reply: &str, cancel: &AtomicBool) -> Outcome {
    if field.web {
        return paste_whole(app, field, reply, cancel);
    }
    let steps = reveal_steps(reply);
    let delay = step_delay(steps.len());
    let mut wrote_any = false;
    for (index, step) in steps.iter().enumerate() {
        if cancel.load(Ordering::SeqCst) {
            return restore(field, wrote_any);
        }
        match fill::set_value(field.pid, &field.role, step) {
            Ok(()) => wrote_any = true,
            Err(message) if index == 0 => {
                tracing::debug!("Smart Fill pastes instead: {message}");
                return paste_whole(app, field, reply, cancel);
            }
            Err(message) => return Outcome::Failed(message),
        }
        overlay::follow(app, fill::caret_rect(field.pid));
        std::thread::sleep(delay);
    }
    Outcome::Written
}

fn paste_whole(app: &AppHandle, field: &FocusedField, reply: &str, cancel: &AtomicBool) -> Outcome {
    if cancel.load(Ordering::SeqCst) {
        return Outcome::Stopped;
    }
    match fill::paste_replacing(field.pid, reply) {
        Ok(()) => {
            std::thread::sleep(Duration::from_millis(120));
            overlay::follow(app, fill::caret_rect(field.pid));
            Outcome::Written
        }
        Err(message) => Outcome::Failed(message),
    }
}

fn restore(field: &FocusedField, wrote_any: bool) -> Outcome {
    if wrote_any {
        if let Err(message) = fill::set_value(field.pid, &field.role, &field.value) {
            tracing::warn!("Smart Fill could not put back the old text: {message}");
        }
    }
    Outcome::Stopped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_reveal_grows_two_words_at_a_time_and_ends_on_the_whole_text() {
        let steps = reveal_steps("Sounds good, see you at seven tonight");
        assert_eq!(
            steps,
            vec![
                "Sounds good,".to_string(),
                "Sounds good, see you".to_string(),
                "Sounds good, see you at seven".to_string(),
                "Sounds good, see you at seven tonight".to_string(),
            ]
        );
    }

    #[test]
    fn one_word_is_one_step() {
        assert_eq!(reveal_steps("Yes"), vec!["Yes".to_string()]);
    }

    #[test]
    fn line_breaks_survive_the_reveal() {
        let steps = reveal_steps("Hi Sam,\n\nSee you then");
        assert_eq!(steps.last().unwrap(), "Hi Sam,\n\nSee you then");
        assert!(steps
            .iter()
            .all(|s| "Hi Sam,\n\nSee you then".starts_with(s.as_str())));
    }

    #[test]
    fn long_replies_are_not_slower_than_the_total() {
        let steps = 300;
        assert!(step_delay(steps) * steps as u32 <= Duration::from_millis(300 * 70));
        assert_eq!(step_delay(1), Duration::from_millis(70));
        assert_eq!(step_delay(1_000), Duration::from_millis(15));
    }

    #[test]
    #[ignore = "Needs the OCR engine and synthetic screenshots in $FNDR_SMART_FILL_FIXTURES; run: cargo test --lib ocr_reads_a_conversation -- --ignored --nocapture"]
    fn ocr_reads_a_conversation_and_an_email_from_a_screenshot() {
        let dir = std::env::var("FNDR_SMART_FILL_FIXTURES").unwrap_or_else(|_| {
            format!(
                "{}/smartfill-fixtures",
                std::env::var("HOME").unwrap_or_default()
            )
        });
        for (name, needles) in [
            ("imessage.png", ["saturday", "speaker", "thai"]),
            ("email.png", ["budget", "thursday", "finance"]),
        ] {
            let image = std::fs::read(format!("{dir}/{name}")).expect("fixture");
            let started = std::time::Instant::now();
            let text = crate::accessibility::ocr_screen_text(&image).to_lowercase();
            println!(
                "{name}: {} chars in {:?}",
                text.chars().count(),
                started.elapsed()
            );
            for needle in needles {
                assert!(text.contains(needle), "{name}: OCR missed {needle:?}");
            }
        }
    }
}
