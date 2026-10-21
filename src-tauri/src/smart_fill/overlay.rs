//! The glow that follows the caret while Smart Fill works.
//!
//! One small transparent window, click-through, never focused and hidden from
//! screenshots, moved to the caret of the field being written. The page it
//! shows (`smart-fill.html`) draws the glow from the phase events sent here.

use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, LogicalPosition, Manager};

use crate::accessibility::fill::ScreenRect;

pub const OVERLAY_LABEL: &str = "smart-fill-overlay";
pub const STATE_EVENT: &str = "smart-fill://state";

const WIDTH: f64 = 360.0;
const HEIGHT: f64 = 96.0;
/// Where the caret sits inside the window.
const CARET_X: f64 = 28.0;
const CARET_Y: f64 = HEIGHT / 2.0;

#[derive(Serialize, Clone)]
struct StateEvent<'a> {
    phase: &'a str,
    message: Option<String>,
}

/// Creates the hidden overlay at startup.
pub fn create(app: &AppHandle) {
    let url = tauri::WebviewUrl::App("smart-fill.html".into());
    let built = tauri::WebviewWindowBuilder::new(app, OVERLAY_LABEL, url)
        .title("FNDR Smart Fill")
        .inner_size(WIDTH, HEIGHT)
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
        .build();
    match built {
        Ok(window) => {
            if let Err(err) = window.set_ignore_cursor_events(true) {
                tracing::warn!(
                    "Smart Fill glow disabled: it could not become click-through: {err}"
                );
                let _ = window.destroy();
                return;
            }
            if let Err(err) = configure_native_window(&window) {
                tracing::warn!("Smart Fill glow disabled: native window setup failed: {err}");
                let _ = window.destroy();
            }
        }
        Err(err) => tracing::warn!("Smart Fill glow could not be created: {err}"),
    }
}

/// The glow must show over another app's full-screen Space and must not hide
/// with FNDR (Cmd-H), the same way Screen Guide's overlay does.
#[cfg(target_os = "macos")]
fn configure_native_window(window: &tauri::WebviewWindow) -> Result<(), String> {
    use objc2_app_kit::NSWindowCollectionBehavior;

    if objc2_foundation::MainThreadMarker::new().is_none() {
        return Err("native setup did not run on the main thread".to_string());
    }
    let pointer = window.ns_window().map_err(|err| err.to_string())?;
    let native_window = unsafe { &*(pointer as *const objc2_app_kit::NSWindow) };
    unsafe {
        native_window.setCollectionBehavior(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::Stationary
                | NSWindowCollectionBehavior::FullScreenAuxiliary
                | NSWindowCollectionBehavior::IgnoresCycle,
        );
        native_window.setCanHide(false);
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn configure_native_window(_window: &tauri::WebviewWindow) -> Result<(), String> {
    Ok(())
}

/// Near the mouse, for a notice that has no caret to sit on.
fn near_mouse(app: &AppHandle) -> (f64, f64) {
    let scale = app
        .primary_monitor()
        .ok()
        .flatten()
        .map_or(1.0, |monitor| monitor.scale_factor());
    app.cursor_position()
        .map(|position| (position.x / scale + 16.0, position.y / scale + 16.0))
        .unwrap_or((80.0, 80.0))
}

fn place(app: &AppHandle, caret: Option<ScreenRect>) {
    let Some(window) = app.get_webview_window(OVERLAY_LABEL) else {
        return;
    };
    let (x, y) = match caret {
        Some(rect) => (rect.x - CARET_X, rect.y + rect.height / 2.0 - CARET_Y),
        None => near_mouse(app),
    };
    let _ = window.set_position(LogicalPosition::new(x, y));
}

fn emit(app: &AppHandle, phase: &str, message: Option<String>) {
    let _ = app.emit_to(OVERLAY_LABEL, STATE_EVENT, StateEvent { phase, message });
}

/// The request started: show the glow at the caret, thinking.
pub fn begin(app: &AppHandle, caret: Option<ScreenRect>) {
    place(app, caret);
    if let Some(window) = app.get_webview_window(OVERLAY_LABEL) {
        let _ = window.show();
    }
    emit(app, "thinking", None);
}

/// Text is going in: keep the glow on the caret.
pub fn follow(app: &AppHandle, caret: Option<ScreenRect>) {
    if caret.is_some() {
        place(app, caret);
    }
    emit(app, "writing", None);
}

/// The request ended. `phase` is `done`, `stopped`, `notice` or `error`; the
/// last two carry a sentence for the person. The window hides itself after.
pub fn finish(app: &AppHandle, phase: &'static str, message: Option<String>) {
    if message.is_some() {
        if let Some(window) = app.get_webview_window(OVERLAY_LABEL) {
            let already_shown = window.is_visible().unwrap_or(false);
            if !already_shown {
                place(app, None);
            }
            let _ = window.show();
        }
    }
    emit(app, phase, message.clone());
    let linger = if message.is_some() {
        Duration::from_millis(2_600)
    } else {
        Duration::from_millis(900)
    };
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(linger).await;
        if super::RUNNING.load(std::sync::atomic::Ordering::SeqCst) {
            return;
        }
        if let Some(window) = handle.get_webview_window(OVERLAY_LABEL) {
            let _ = window.hide();
        }
    });
}
