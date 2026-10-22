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
pub mod files;
pub mod form;
pub mod monitor;
pub mod overlay;
pub mod prompt;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use tauri::{AppHandle, Manager};

use crate::accessibility::fill::{self, FocusedField};
use crate::accessibility::form as accessibility_form;
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
/// Accessibility walk budget: time and elements. A mail page in a browser is
/// thousands of elements, far more than the default allows.
const AX_WALK_TIME: Duration = Duration::from_millis(400);
const AX_WALK_NODES: usize = 20_000;
/// Shown when FNDR can neither read the window's text nor see the screen.
const SCREEN_RECORDING_HINT: &str = "FNDR can't see the screen. Turn FNDR on under Screen & System Audio Recording in System Settings (remove it and add it again after an update), then reopen FNDR.";
/// How long Smart Fill spends finding a form's fields, and how many elements
/// it looks at: a long page is thousands.
const FORM_WALK_TIME: Duration = Duration::from_millis(900);
const FORM_WALK_NODES: usize = 40_000;
/// ChatGPT's turn for a whole form is longer than for one box.
const FORM_CODEX_TIMEOUT: Duration = Duration::from_secs(60);
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

/// How many personal details Smart Fill has saved, for Settings.
#[tauri::command]
pub async fn saved_details_count(state: tauri::State<'_, Arc<AppState>>) -> Result<usize, String> {
    Ok(crate::profile_facts::usable_count(
        state.app_data_dir.as_path(),
    ))
}

/// Forgets every saved personal detail.
#[tauri::command]
pub async fn forget_saved_details(state: tauri::State<'_, Arc<AppState>>) -> Result<usize, String> {
    crate::profile_facts::forget_all(state.app_data_dir.as_path())
        .map_err(|err| err.to_string())?;
    Ok(0)
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
    /// A form was filled; the sentence says how much.
    Filled(String),
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
        Outcome::Written => {
            tracing::info!("smart_fill:wrote the reply");
            overlay::finish(&app, "done", None)
        }
        Outcome::Filled(message) => overlay::finish(&app, "done", Some(message)),
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

    let Some(front_pid) = tokio::task::spawn_blocking(fill::frontmost)
        .await
        .ok()
        .flatten()
        .filter(|pid| *pid != std::process::id() as i32)
    else {
        return Outcome::Silent;
    };

    // Who owns the window is read first; nothing else is read or sent from an
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

    // A form in the window: fill all of it from memory.
    let fields = tokio::task::spawn_blocking(move || {
        accessibility_form::discover(front_pid, FORM_WALK_TIME, FORM_WALK_NODES)
    })
    .await
    .unwrap_or_default();
    let blank = form::blank_form_fields(&fields);
    let focused_is_text_area = tokio::task::spawn_blocking(fill::focused_field)
        .await
        .ok()
        .and_then(Result::ok)
        .is_some_and(|field| field.role == "AXTextArea");
    if form::looks_like_form(blank, focused_is_text_area) {
        return fill_the_form(app, &state, cancel, fields).await;
    }

    let field = match tokio::task::spawn_blocking(fill::focused_field).await {
        Ok(Ok(field)) => field,
        Ok(Err(refusal)) if refusal.show => return Outcome::Nothing(refusal.message),
        Ok(Err(_)) => return Outcome::Silent,
        Err(_) => return Outcome::Failed("The text box could not be read.".to_string()),
    };

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
    let text_chars = gathered.screen_text.trim().chars().count();
    tracing::info!(
        text_chars,
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

    tracing::info!(
        reply_chars = reply.chars().count(),
        web_field = field.web,
        "smart_fill:writing the reply"
    );
    let handle = app.clone();
    let revealed =
        tokio::task::spawn_blocking(move || reveal(&handle, &field, &reply, &cancel)).await;
    match revealed {
        Ok(outcome) => outcome,
        Err(_) => Outcome::Failed("Writing stopped unexpectedly.".to_string()),
    }
}

/// Fills a whole form from what memory states: the fields and the notes go to
/// ChatGPT (no screenshot), the answer is checked, and each value is written
/// into its field in order.
async fn fill_the_form(
    app: &AppHandle,
    state: &Arc<AppState>,
    cancel: Arc<AtomicBool>,
    fields: Vec<accessibility_form::FormField>,
) -> Outcome {
    let infos = form::model_fields(&fields);
    let uploads = form::upload_fields(&fields);
    overlay::begin(app, None);
    overlay::progress(app, None, "Looking through your memory");

    // For a form with file boxes, the names of the person's recent files go to
    // the model too (names only), so it can say which one a box is asking for.
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    let files = match (&home, uploads.is_empty()) {
        (Some(home), false) => {
            let home = home.clone();
            tokio::task::spawn_blocking(move || files::candidates(&home, files::MAX_CANDIDATES))
                .await
                .unwrap_or_default()
        }
        _ => Vec::new(),
    };
    let file_labels: Vec<String> = match &home {
        Some(home) => files.iter().map(|file| files::label(home, file)).collect(),
        None => Vec::new(),
    };

    let notes = form_notes(state, &infos).await;
    tracing::info!(
        fields = infos.len(),
        uploads = uploads.len(),
        files = file_labels.len(),
        notes = notes.len(),
        note_apps = %notes
            .iter()
            .map(|note| format!("{}:{}", note.app_name, note.text.chars().count()))
            .collect::<Vec<_>>()
            .join(","),
        "smart_fill:form read the fields and memory"
    );
    let answer = crate::ipc::commands::codex_account::answer_smart_fill_form_with_codex(
        form::user_text(&infos, &notes, &uploads, &file_labels),
        cancel.clone(),
        FORM_CODEX_TIMEOUT,
    )
    .await;
    if cancel.load(Ordering::SeqCst) {
        return Outcome::Stopped;
    }
    let answer = match answer {
        Ok(answer) => answer,
        Err(message) => return Outcome::Failed(message),
    };
    let values = form::parse_values(&answer, &infos);
    let suggestions: Vec<String> = form::parse_files(&answer, &uploads, file_labels.len())
        .into_iter()
        .filter_map(|(field, file)| {
            let label = uploads.iter().find(|(index, _)| *index == field)?.1.clone();
            let name = file_labels.get(file)?;
            Some(format!("{}: {}", label.trim_end_matches(['*', ' ']), name))
        })
        .take(2)
        .collect();
    tracing::info!(
        values = values.len(),
        fields = infos.len(),
        suggested_files = suggestions.len(),
        "smart_fill:form values from memory"
    );
    if values.is_empty() {
        return if suggestions.is_empty() {
            Outcome::Nothing("FNDR's memory doesn't state what this form asks for.")
        } else {
            Outcome::Filled(format!("Attach: {}", suggestions.join(" | ")))
        };
    }

    let handle = app.clone();
    let total = values.len();
    let written = tokio::task::spawn_blocking(move || {
        fill::with_clipboard_restored(|| write_form(&handle, &fields, &values, &cancel))
    })
    .await;
    match written {
        Ok((count, stopped)) if stopped => {
            tracing::info!(count, "smart_fill:form stopped");
            Outcome::Stopped
        }
        Ok((count, _)) => {
            let mut message = format!("Filled {count} of {} fields", infos.len());
            if !suggestions.is_empty() {
                message.push_str(&format!(". Attach: {}", suggestions.join(" | ")));
            }
            Outcome::Filled(message)
        }
        Err(_) => Outcome::Failed(format!(
            "Filling stopped unexpectedly after {total} values."
        )),
    }
}

/// Writes each value into its field, announcing progress. Returns how many
/// went in and whether Esc stopped it. A field that refuses is skipped.
fn write_form(
    app: &AppHandle,
    fields: &[accessibility_form::FormField],
    values: &[(usize, String)],
    cancel: &AtomicBool,
) -> (usize, bool) {
    let total = values.len();
    let mut done = 0;
    for (position, (index, text)) in values.iter().enumerate() {
        if cancel.load(Ordering::SeqCst) {
            return (done, true);
        }
        let Some(field) = fields.iter().find(|field| field.index == *index) else {
            continue;
        };
        let text = match field.kind {
            accessibility_form::FieldKind::Date => match form::date_keystrokes(text) {
                Some(digits) => digits,
                None => continue,
            },
            _ => text.clone(),
        };
        overlay::progress(app, None, &format!("Filling {} of {total}", position + 1));
        match accessibility_form::write(field, &text) {
            Ok(()) => done += 1,
            Err(message) => tracing::debug!("Smart Fill skipped a field: {message}"),
        }
        std::thread::sleep(Duration::from_millis(40));
    }
    (done, false)
}

/// The notes that may state what a form asks for: a few searches built from
/// the field labels and the usual personal details, through the one retrieval
/// function so the blocklist and quality rules apply.
async fn form_notes(state: &AppState, fields: &[form::FieldInfo]) -> Vec<MemoryNote> {
    let mut queries: Vec<String> = vec![
        "my full name email phone number".to_string(),
        "my date of birth".to_string(),
        "my home address city state zip country".to_string(),
        "my university degree major graduation".to_string(),
        "my passport number nationality".to_string(),
    ];
    let labels: Vec<&str> = fields.iter().map(|field| field.label.as_str()).collect();
    for chunk in labels.chunks(6).take(4) {
        queries.push(format!("my {}", chunk.join(" ")));
    }
    let mut seen = std::collections::HashSet::new();
    let mut notes = Vec::new();
    for query in queries {
        let Ok(results) = crate::ipc::commands::search::search_ranked_results(
            state,
            &query,
            Some("365d"),
            None,
            5,
        )
        .await
        else {
            continue;
        };
        for result in results {
            if !seen.insert(result.id.clone()) {
                continue;
            }
            notes.push(note_for(state, result).await);
        }
    }
    // The person's own saved details come first: they are what a form asks for.
    if let Some(text) = crate::profile_facts::saved_details_text(state.app_data_dir.as_path()) {
        notes.insert(
            0,
            MemoryNote {
                app_name: "Saved details".to_string(),
                window_title: "the person's own".to_string(),
                text,
            },
        );
    }
    notes
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
        Ok(results) => {
            let mut notes = Vec::new();
            for result in results {
                notes.push(note_for(state, result).await);
            }
            notes
        }
        Err(err) => {
            tracing::debug!("Smart Fill went without memories: {err}");
            Vec::new()
        }
    }
}

/// What a memory said, as the model should read it. Search results carry only
/// a short summary of a capture ("reviewed some details" states none of them);
/// the page's own text is kept as chunks beside the memory. The chunks win when
/// they hold more than the summary.
async fn note_for(state: &AppState, result: crate::storage::SearchResult) -> MemoryNote {
    let chunks = state
        .store
        .list_chunks_for_memory(&result.id)
        .await
        .unwrap_or_default();
    let from_chunks = chunks
        .iter()
        .map(|chunk| chunk.text.trim())
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    let summary = [&result.clean_text, &result.text, &result.snippet]
        .into_iter()
        .max_by_key(|text| text.trim().chars().count())
        .cloned()
        .unwrap_or_default();
    tracing::info!(
        app = %result.app_name,
        chunks = chunks.len(),
        chunk_chars = from_chunks.chars().count(),
        summary_chars = summary.chars().count(),
        "smart_fill:note text"
    );
    let text = if from_chunks.chars().count() > summary.chars().count() {
        from_chunks
    } else {
        summary
    };
    MemoryNote {
        app_name: result.app_name,
        window_title: result.window_title,
        text,
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
}
