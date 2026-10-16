//! The separate menu bar capture indicator and pause shortcut.

use crate::AppState;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

const TRAY_ID: &str = "capture-recording-status";
pub const PAUSE_SHORTCUT: &str = "Control+Alt+P";
static REGISTERED_SHORTCUT_ID: once_cell::sync::Lazy<parking_lot::Mutex<Option<u32>>> =
    once_cell::sync::Lazy::new(|| parking_lot::Mutex::new(None));

fn indicator_copy(
    user_paused: bool,
    private_mode: bool,
    capturing: bool,
) -> (&'static str, &'static str) {
    if private_mode {
        (
            "PRIVATE",
            "FNDR Private Mode is on. Control+Option+P toggles capture pause.",
        )
    } else if user_paused {
        (
            "PAUSED",
            "FNDR screen context capture is paused. Control+Option+P to resume.",
        )
    } else if capturing {
        (
            "● REC",
            "FNDR screen context capture is on. Control+Option+P to pause.",
        )
    } else {
        (
            "IDLE",
            "FNDR screen context capture is temporarily idle. Control+Option+P to pause.",
        )
    }
}

pub fn refresh_indicator<R: tauri::Runtime>(app: &AppHandle<R>, state: &AppState) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    let (title, tooltip) = indicator_copy(
        state.is_user_capture_paused(),
        state.is_incognito.load(Ordering::SeqCst),
        state.is_capturing(),
    );
    if let Err(error) = tray.set_title(Some(title)) {
        tracing::warn!(%error, "capture indicator title update failed");
    }
    if let Err(error) = tray.set_tooltip(Some(tooltip)) {
        tracing::warn!(%error, "capture indicator tooltip update failed");
    }
}

pub fn create_indicator<R: tauri::Runtime>(app: &AppHandle<R>, state: &AppState) {
    if app.tray_by_id(TRAY_ID).is_none() {
        if let Err(error) = TrayIconBuilder::with_id(TRAY_ID)
            .title("IDLE")
            .tooltip("FNDR screen context capture status")
            .show_menu_on_left_click(false)
            .build(app)
        {
            tracing::warn!(%error, "capture indicator could not start");
            return;
        }
    }
    refresh_indicator(app, state);
}

pub fn register_pause_shortcut<R: tauri::Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    let shortcut: Shortcut = PAUSE_SHORTCUT
        .parse()
        .map_err(|error| format!("Invalid capture pause shortcut: {error}"))?;
    if app.global_shortcut().is_registered(shortcut) {
        if *REGISTERED_SHORTCUT_ID.lock() == Some(shortcut.id()) {
            return Ok(());
        }
        return Err("Capture pause shortcut is already used by another FNDR feature".to_string());
    }
    let handle = app.clone();
    app.global_shortcut()
        .on_shortcut(shortcut, move |_app, _shortcut, event| {
            if event.state() != ShortcutState::Pressed {
                return;
            }
            let state = handle.state::<Arc<AppState>>();
            if let Err(error) = crate::ipc::commands::stats::toggle_capture_for_user(state.inner())
            {
                tracing::warn!(%error, "capture pause shortcut failed");
            }
            crate::ipc::commands::emit_capture_status(state.inner());
        })
        .map_err(|error| error.to_string())?;
    *REGISTERED_SHORTCUT_ID.lock() = Some(shortcut.id());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indicator_distinguishes_recording_pause_private_and_internal_idle() {
        assert_eq!(indicator_copy(false, false, true).0, "● REC");
        assert_eq!(indicator_copy(true, false, false).0, "PAUSED");
        assert_eq!(indicator_copy(false, true, false).0, "PRIVATE");
        assert_eq!(indicator_copy(false, false, false).0, "IDLE");
        assert!(indicator_copy(false, false, true)
            .1
            .contains("Control+Option+P"));
    }
}
