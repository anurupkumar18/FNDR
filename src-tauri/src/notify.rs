//! Proactive notifications: the in-app toast event, plus a macOS banner when
//! FNDR is in the background.
//!
//! The banner text is fixed per kind and never carries memory text. Notification
//! Center keeps history and can show on a locked screen, and briefing and task
//! text come from what was on the screen.

use crate::AppState;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::{NotificationExt, PermissionState};

/// The webview event `App.tsx` turns into a toast.
const EVENT: &str = "fndr_notification";

/// Banner title and body for a notification kind. `None` for a kind with no
/// reviewed text, which gets no banner.
pub fn banner_text(kind: &str) -> Option<(&'static str, &'static str)> {
    match kind {
        "briefing" => Some(("FNDR briefing ready", "Open FNDR to read it.")),
        "stale_tasks" => Some(("FNDR tasks", "Some of your tasks have gone stale.")),
        "context_switch" => Some((
            "FNDR",
            "You have been switching between many apps. Want to refocus?",
        )),
        _ => None,
    }
}

/// A banner appears only when the person turned them on, is not in incognito,
/// and is not already looking at FNDR (the toast covers that case).
pub fn should_send_banner(enabled: bool, incognito: bool, window_focused: bool) -> bool {
    enabled && !incognito && !window_focused
}

fn main_window_focused(app: &AppHandle) -> bool {
    app.get_webview_window("main")
        .and_then(|window| window.is_focused().ok())
        .unwrap_or(false)
}

fn show_banner(app: &AppHandle, title: &str, body: &str) {
    let notification = app.notification();
    let allowed = match notification.permission_state() {
        Ok(PermissionState::Granted) => true,
        Ok(PermissionState::Denied) => false,
        _ => matches!(
            notification.request_permission(),
            Ok(PermissionState::Granted)
        ),
    };
    if !allowed {
        return;
    }
    if let Err(err) = notification.builder().title(title).body(body).show() {
        tracing::warn!(error = %err, "notify:banner_failed");
    }
}

/// Emits the toast event with the full text and, when allowed, a banner with
/// the reviewed generic text.
pub fn send(app: &AppHandle, state: &AppState, kind: &str, title: &str, body: &str) {
    let _ = app.emit(
        EVENT,
        serde_json::json!({ "title": title, "body": body, "kind": kind }),
    );
    let Some((banner_title, banner_body)) = banner_text(kind) else {
        return;
    };
    let enabled = state.config.read().notifications.banners;
    let incognito = state.is_incognito.load(Ordering::SeqCst);
    if should_send_banner(enabled, incognito, main_window_focused(app)) {
        show_banner(app, banner_title, banner_body);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_banner_needs_the_setting_no_incognito_and_a_window_not_in_front() {
        assert!(should_send_banner(true, false, false));
        assert!(!should_send_banner(false, false, false));
        assert!(!should_send_banner(true, true, false));
        assert!(!should_send_banner(true, false, true));
    }

    #[test]
    fn each_proactive_kind_has_fixed_text_and_an_unknown_kind_has_none() {
        for kind in ["briefing", "stale_tasks", "context_switch"] {
            let (title, body) = banner_text(kind).expect(kind);
            assert!(!title.is_empty() && !body.is_empty());
        }
        assert_eq!(banner_text("proactive_memory"), None);
        assert_eq!(banner_text(""), None);
    }

    #[test]
    fn banner_text_is_fixed_so_it_cannot_carry_memory_text() {
        let (_, body) = banner_text("stale_tasks").unwrap();
        assert!(!body.contains('{') && !body.contains("%s"));
    }
}
