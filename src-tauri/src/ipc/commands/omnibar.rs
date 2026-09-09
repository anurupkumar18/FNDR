//! Spotlight-style omnibar: global shortcut, frameless always-on-top window.

use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

const OMNIBAR_LABEL: &str = "omnibar";
const OMNIBAR_WIDTH: f64 = 680.0;
const OMNIBAR_HEIGHT: f64 = 480.0;
pub const OMNIBAR_SHORTCUT: &str = "Alt+Space";
static OMNIBAR_REGISTERED_SHORTCUT_ID: once_cell::sync::Lazy<parking_lot::Mutex<Option<u32>>> =
    once_cell::sync::Lazy::new(|| parking_lot::Mutex::new(None));

/// Pre-create the omnibar window at startup (hidden) so it is fully loaded
/// before the first hotkey press. Called once from main.rs setup.
pub fn create_omnibar_window<R: tauri::Runtime>(app: &AppHandle<R>) {
    let url = tauri::WebviewUrl::App("omnibar.html".into());
    match tauri::WebviewWindowBuilder::new(app, OMNIBAR_LABEL, url)
        .title("FNDR")
        .inner_size(OMNIBAR_WIDTH, OMNIBAR_HEIGHT)
        .center()
        .decorations(false)
        .always_on_top(true)
        .resizable(false)
        .skip_taskbar(true)
        .shadow(true)
        .visible(false)
        .build()
    {
        Ok(_) => tracing::info!("omnibar window pre-created (hidden)"),
        Err(err) => tracing::warn!("failed to pre-create omnibar window: {err}"),
    }
}

/// Register the Omnibar-owned shortcut without accepting another feature's
/// registration as success.
pub fn register_omnibar_shortcut<R: tauri::Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    let shortcut: Shortcut = OMNIBAR_SHORTCUT
        .parse()
        .map_err(|err| format!("Invalid omnibar shortcut '{OMNIBAR_SHORTCUT}': {err}"))?;
    if app.global_shortcut().is_registered(shortcut) {
        if *OMNIBAR_REGISTERED_SHORTCUT_ID.lock() == Some(shortcut.id()) {
            return Ok(());
        }
        return Err(format!(
            "Omnibar shortcut '{OMNIBAR_SHORTCUT}' is already used by another FNDR feature."
        ));
    }

    let handle = app.clone();
    app.global_shortcut()
        .on_shortcut(shortcut, move |_app, _shortcut, event| {
            if event.state() != ShortcutState::Pressed {
                return;
            }
            toggle_omnibar(&handle);
        })
        .map_err(|err| err.to_string())?;
    *OMNIBAR_REGISTERED_SHORTCUT_ID.lock() = Some(shortcut.id());
    Ok(())
}

fn toggle_omnibar<R: tauri::Runtime>(app: &AppHandle<R>) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        let Some(window) = handle.get_webview_window(OMNIBAR_LABEL) else {
            tracing::warn!("omnibar: window not found at hotkey time");
            return;
        };
        if window.is_visible().unwrap_or(false) {
            let _ = window.hide();
        } else {
            super::screen_guide::schedule_fndr_window_activation_after_screen_guide_cancel_with_work(
                &handle,
                move |work_handle| {
                    super::autofill::cancel_autofill_for_sibling_activation(&work_handle)
                },
                move |handle, autofill_cancelled| {
                    if let Err(err) = autofill_cancelled {
                        tracing::warn!("Omnibar could not safely stop Autofill: {err}");
                        return;
                    }
                    let Some(window) = handle.get_webview_window(OMNIBAR_LABEL) else {
                        return;
                    };
                    let _ = window.center();
                    let _ = window.show();
                    let _ = window.set_focus();
                    let _ = handle.emit_to(OMNIBAR_LABEL, "omnibar://focus", ());
                },
            );
        }
    });
}

/// Hide the omnibar window (Esc / blur from the frontend).
#[tauri::command]
pub async fn dismiss_omnibar(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(OMNIBAR_LABEL) {
        window.hide().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Open a memory from the omnibar: hide the omnibar, focus the main window,
/// and tell it to open the vault on the given memory.
#[tauri::command]
pub async fn omnibar_open_memory(app: AppHandle, memory_id: String) -> Result<(), String> {
    super::screen_guide::schedule_fndr_window_activation_after_screen_guide_cancel_with_work(
        &app,
        move |work_handle| super::autofill::cancel_autofill_for_sibling_activation(&work_handle),
        move |app, autofill_cancelled| {
            if let Err(err) = autofill_cancelled {
                tracing::warn!("Omnibar could not safely stop Autofill: {err}");
                return;
            }
            if let Some(omnibar) = app.get_webview_window(OMNIBAR_LABEL) {
                let _ = omnibar.hide();
            }
            let Some(main) = app.get_webview_window("main") else {
                return;
            };
            let _ = main.show();
            let _ = main.set_focus();
            let _ = app.emit_to("main", "omnibar://open-memory", memory_id);
        },
    );
    Ok(())
}
