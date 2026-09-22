//! Notch HUD — a hover-summoned FNDR panel parked on the display's notch.
//!
//! The window is created once at a fixed size (the largest footprint the panel
//! ever takes, plus shadow margin) and is never resized: every expansion and
//! collapse is content animation inside it. macOS picks which window receives a
//! click from the window's *frame*, not from what is drawn, so the transparent
//! margin would swallow clicks meant for the app underneath — instead the
//! window stays click-through and a 60 Hz pointer poll flips
//! `ignoresMouseEvents` off only while the cursor is actually over the drawn
//! panel. The same poll is what tells the webview it is hovered, since a
//! click-through window receives no mouse events of its own.
//!
//! Layout numbers here must stay in step with `src/domains/notch/notchMetrics.ts`;
//! `notchMetrics.test.ts` parses this file and fails if they drift.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

pub const NOTCH_HUD_LABEL: &str = "notch-hud";
pub const NOTCH_HUD_SHORTCUT: &str = "Alt+N";
/// Pointer entered/left the drawn panel — payload is a bool.
const NOTCH_HUD_HOVER_EVENT: &str = "notch-hud://hover";
/// Display configuration changed; the webview should re-read the geometry.
const NOTCH_HUD_GEOMETRY_EVENT: &str = "notch-hud://geometry";

// MARK: - Metrics (mirrored in notchMetrics.ts)

/// Collapsed pill on displays without a physical notch.
pub const PILL_CLOSED_WIDTH: f64 = 132.0;
pub const PILL_CLOSED_HEIGHT: f64 = 28.0;
/// The pill is detached from the screen edge; the notch never is.
pub const PILL_TOP_GAP: f64 = 6.0;
/// Largest panel the HUD ever draws — the window envelope is built from this.
pub const PANEL_MAX_WIDTH: f64 = 560.0;
pub const PANEL_MAX_HEIGHT: f64 = 400.0;
/// Margin so the CSS shadow isn't clipped (the window itself draws no shadow).
pub const SHADOW_PADDING: f64 = 28.0;
/// Cosmetic growth on hover, reserved here so it can't clip.
pub const HOVER_BUMP: f64 = 6.0;
/// Slack around the drawn panel before clicks pass through, so a click on the
/// very edge of the silhouette doesn't miss.
pub const HIT_RECT_OUTSET: f64 = 6.0;
/// The auxiliary-area arithmetic can come up implausibly small on odd display
/// configurations; floor it.
const MINIMUM_NOTCH_WIDTH: f64 = 200.0;

const POLL_INTERVAL: Duration = Duration::from_millis(16);
/// Ticks between re-reading the window origin — it only moves when the display
/// configuration does, so this need not run every frame.
const ORIGIN_REFRESH_TICKS: u32 = 30;

static HUD_VISIBLE: AtomicBool = AtomicBool::new(false);
static POINTER_INSIDE: AtomicBool = AtomicBool::new(false);
static POLL_RUNNING: AtomicBool = AtomicBool::new(false);
/// While the panel owns the keyboard it stays interactive regardless of where
/// the cursor wandered, or the first keystroke after a stray mouse move is lost.
static KEYBOARD_ACTIVE: AtomicBool = AtomicBool::new(false);

static HIT_RECT: once_cell::sync::Lazy<parking_lot::Mutex<Option<NotchHitRect>>> =
    once_cell::sync::Lazy::new(|| parking_lot::Mutex::new(None));
static NOTCH_HUD_REGISTERED_SHORTCUT_ID: once_cell::sync::Lazy<parking_lot::Mutex<Option<u32>>> =
    once_cell::sync::Lazy::new(|| parking_lot::Mutex::new(None));

/// The drawn panel's frame in CSS pixels, relative to the window's top-left —
/// reported by the webview every time the silhouette changes size.
#[derive(Debug, Clone, Copy, serde::Deserialize)]
pub struct NotchHitRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// What the webview needs to draw a silhouette that lands exactly on this
/// display's hardware. All values are logical points.
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct NotchHudGeometry {
    pub closed_width: f64,
    pub closed_height: f64,
    /// False on displays without a camera housing — the panel wears the pill
    /// silhouette there instead, detached from the edge.
    pub is_physical_notch: bool,
    pub window_width: f64,
    pub window_height: f64,
    pub screen_width: f64,
    pub screen_height: f64,
}

pub fn notch_hud_window_size() -> (f64, f64) {
    (
        PANEL_MAX_WIDTH + SHADOW_PADDING * 2.0 + HOVER_BUMP,
        PANEL_MAX_HEIGHT + SHADOW_PADDING + HOVER_BUMP + PILL_TOP_GAP,
    )
}

fn pill_geometry(screen_width: f64, screen_height: f64) -> NotchHudGeometry {
    let (window_width, window_height) = notch_hud_window_size();
    NotchHudGeometry {
        closed_width: PILL_CLOSED_WIDTH,
        closed_height: PILL_CLOSED_HEIGHT,
        is_physical_notch: false,
        window_width,
        window_height,
        screen_width,
        screen_height,
    }
}

/// Width of the cutout, derived from the menu-bar areas flanking it.
fn physical_notch_width(screen_width: f64, left_aux: f64, right_aux: f64) -> f64 {
    (screen_width - left_aux - right_aux).max(MINIMUM_NOTCH_WIDTH)
}

/// Centered horizontally, flush with the top edge. Physical pixels, matching
/// Tauri's monitor coordinate space.
fn notch_hud_origin(
    monitor_x: f64,
    monitor_y: f64,
    monitor_width: f64,
    window_width: f64,
    scale_factor: f64,
) -> (f64, f64) {
    (
        monitor_x + (monitor_width - window_width * scale_factor) / 2.0,
        monitor_y,
    )
}

fn pointer_is_over_panel(rect: NotchHitRect, local_x: f64, local_y: f64) -> bool {
    local_x >= rect.x - HIT_RECT_OUTSET
        && local_x <= rect.x + rect.width + HIT_RECT_OUTSET
        && local_y >= rect.y - HIT_RECT_OUTSET
        && local_y <= rect.y + rect.height + HIT_RECT_OUTSET
}

// MARK: - Geometry probing

#[cfg(target_os = "macos")]
fn read_geometry_on_main_thread() -> Option<NotchHudGeometry> {
    use objc2_app_kit::NSScreen;

    let mtm = objc2_foundation::MainThreadMarker::new()?;
    // Deliberately `mainScreen` and not `screens()[0]`: on macOS 27 the screen
    // list comes back as a Swift-bridged array whose `count` is encoded 'Q'
    // where objc2 0.2 expects 'q', and its encoding check panics the main
    // thread in debug builds. `mainScreen` is the menu-bar screen at launch,
    // which is the display `primary_monitor()` parks the window on anyway.
    let screen = NSScreen::mainScreen(mtm)?;
    let frame = screen.frame();
    let screen_width = frame.size.width;
    let screen_height = frame.size.height;

    let inset_top = unsafe { screen.safeAreaInsets() }.top;
    if inset_top <= 0.0 {
        return Some(pill_geometry(screen_width, screen_height));
    }

    let left_aux = unsafe { screen.auxiliaryTopLeftArea() }.size.width;
    let right_aux = unsafe { screen.auxiliaryTopRightArea() }.size.width;
    let (window_width, window_height) = notch_hud_window_size();
    Some(NotchHudGeometry {
        closed_width: physical_notch_width(screen_width, left_aux, right_aux),
        closed_height: inset_top,
        is_physical_notch: true,
        window_width,
        window_height,
        screen_width,
        screen_height,
    })
}

#[cfg(not(target_os = "macos"))]
fn read_geometry_on_main_thread() -> Option<NotchHudGeometry> {
    None
}

/// Falls back to the pill on any display the notch metrics can't be read from —
/// the HUD is still usable there, just detached from the edge.
fn notch_hud_geometry<R: tauri::Runtime>(app: &AppHandle<R>) -> NotchHudGeometry {
    let fallback = || {
        let (width, height) = app
            .primary_monitor()
            .ok()
            .flatten()
            .map(|monitor| {
                let scale = monitor.scale_factor();
                (
                    monitor.size().width as f64 / scale,
                    monitor.size().height as f64 / scale,
                )
            })
            .unwrap_or((1440.0, 900.0));
        pill_geometry(width, height)
    };

    if objc2_foundation::MainThreadMarker::new().is_some() {
        return read_geometry_on_main_thread().unwrap_or_else(fallback);
    }

    let (done_tx, done_rx) = std::sync::mpsc::sync_channel(1);
    if app
        .run_on_main_thread(move || {
            let _ = done_tx.send(read_geometry_on_main_thread());
        })
        .is_err()
    {
        return fallback();
    }
    done_rx
        .recv_timeout(Duration::from_secs(2))
        .ok()
        .flatten()
        .unwrap_or_else(fallback)
}

// MARK: - Window lifecycle

/// Pre-create the HUD window (hidden) so it is loaded before the first summon.
/// Called once from main.rs setup, on the main thread.
pub fn create_notch_hud_window<R: tauri::Runtime>(app: &AppHandle<R>) {
    if app.get_webview_window(NOTCH_HUD_LABEL).is_some() {
        return;
    }
    let (window_width, window_height) = notch_hud_window_size();
    let url = tauri::WebviewUrl::App("notch.html".into());
    let window = match tauri::WebviewWindowBuilder::new(app, NOTCH_HUD_LABEL, url)
        .title("FNDR Notch")
        .inner_size(window_width, window_height)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .visible_on_all_workspaces(true)
        .resizable(false)
        .skip_taskbar(true)
        .shadow(false)
        .focused(false)
        .focusable(false)
        .visible(false)
        .build()
    {
        Ok(window) => window,
        Err(err) => {
            tracing::warn!("failed to pre-create notch HUD window: {err}");
            return;
        }
    };

    // Decorative until the pointer poll says the cursor is over the panel.
    if let Err(err) = window.set_ignore_cursor_events(true) {
        tracing::warn!("notch HUD disabled because it could not become click-through: {err}");
        let _ = window.destroy();
        return;
    }
    if let Err(err) = configure_notch_hud_native_window(&window) {
        tracing::warn!("notch HUD native window setup failed: {err}");
    }
    position_notch_hud(app, &window);
    tracing::info!("notch HUD window pre-created (hidden)");
}

/// Above the menu bar (the panel has to cover the notch itself), present on
/// every Space, and not hidden by Cmd-H along with the rest of FNDR.
#[cfg(target_os = "macos")]
fn configure_notch_hud_native_window<R: tauri::Runtime>(
    window: &tauri::WebviewWindow<R>,
) -> Result<(), String> {
    use objc2_app_kit::{NSWindow, NSWindowCollectionBehavior};

    if objc2_foundation::MainThreadMarker::new().is_none() {
        return Err("native notch HUD setup did not run on the main thread".to_string());
    }
    let pointer = window.ns_window().map_err(|err| err.to_string())?;
    let native_window = unsafe { &*(pointer as *const NSWindow) };
    unsafe {
        native_window.setLevel(objc2_app_kit::NSMainMenuWindowLevel + 3);
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
fn configure_notch_hud_native_window<R: tauri::Runtime>(
    _window: &tauri::WebviewWindow<R>,
) -> Result<(), String> {
    Ok(())
}

fn position_notch_hud<R: tauri::Runtime>(app: &AppHandle<R>, window: &tauri::WebviewWindow<R>) {
    let Ok(Some(monitor)) = app.primary_monitor() else {
        return;
    };
    let scale = monitor.scale_factor();
    let (window_width, _) = notch_hud_window_size();
    let (x, y) = notch_hud_origin(
        monitor.position().x as f64,
        monitor.position().y as f64,
        monitor.size().width as f64,
        window_width,
        scale,
    );
    if let Err(err) = window.set_position(tauri::PhysicalPosition::new(x, y)) {
        tracing::warn!("notch HUD could not park at the top of the display: {err}");
    }
}

pub fn show_notch_hud<R: tauri::Runtime>(app: &AppHandle<R>) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        let Some(window) = handle.get_webview_window(NOTCH_HUD_LABEL) else {
            tracing::warn!("notch HUD: window not found at summon time");
            return;
        };
        position_notch_hud(&handle, &window);
        let _ = window.show();
        HUD_VISIBLE.store(true, Ordering::SeqCst);
        let _ = handle.emit_to(
            NOTCH_HUD_LABEL,
            NOTCH_HUD_GEOMETRY_EVENT,
            notch_hud_geometry(&handle),
        );
        start_pointer_poll(&handle);
    });
}

pub fn hide_notch_hud<R: tauri::Runtime>(app: &AppHandle<R>) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        HUD_VISIBLE.store(false, Ordering::SeqCst);
        POINTER_INSIDE.store(false, Ordering::SeqCst);
        KEYBOARD_ACTIVE.store(false, Ordering::SeqCst);
        let Some(window) = handle.get_webview_window(NOTCH_HUD_LABEL) else {
            return;
        };
        let _ = window.set_focusable(false);
        let _ = window.set_ignore_cursor_events(true);
        let _ = window.hide();
    });
}

fn toggle_notch_hud_window<R: tauri::Runtime>(app: &AppHandle<R>) {
    let visible = app
        .get_webview_window(NOTCH_HUD_LABEL)
        .and_then(|window| window.is_visible().ok())
        .unwrap_or(false);
    if visible {
        hide_notch_hud(app);
    } else {
        show_notch_hud(app);
    }
}

/// Register the HUD-owned shortcut without accepting another feature's
/// registration as success.
pub fn register_notch_hud_shortcut<R: tauri::Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    let shortcut: Shortcut = NOTCH_HUD_SHORTCUT
        .parse()
        .map_err(|err| format!("Invalid notch HUD shortcut '{NOTCH_HUD_SHORTCUT}': {err}"))?;
    if app.global_shortcut().is_registered(shortcut) {
        if *NOTCH_HUD_REGISTERED_SHORTCUT_ID.lock() == Some(shortcut.id()) {
            return Ok(());
        }
        return Err(format!(
            "Notch HUD shortcut '{NOTCH_HUD_SHORTCUT}' is already used by another FNDR feature."
        ));
    }

    let handle = app.clone();
    app.global_shortcut()
        .on_shortcut(shortcut, move |_app, _shortcut, event| {
            if event.state() != ShortcutState::Pressed {
                return;
            }
            toggle_notch_hud_window(&handle);
        })
        .map_err(|err| err.to_string())?;
    *NOTCH_HUD_REGISTERED_SHORTCUT_ID.lock() = Some(shortcut.id());
    Ok(())
}

// MARK: - Pointer poll
//
// Polls rather than listening for mouse-moved events: a window ignoring mouse
// events receives none of its own, and the webview can't see the cursor while
// it is click-through.

fn start_pointer_poll<R: tauri::Runtime>(app: &AppHandle<R>) {
    if POLL_RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        let mut origin: Option<(f64, f64, f64)> = None;
        let mut ticks_since_origin = ORIGIN_REFRESH_TICKS;
        loop {
            std::thread::sleep(POLL_INTERVAL);
            let Some(window) = app.get_webview_window(NOTCH_HUD_LABEL) else {
                break;
            };
            if !HUD_VISIBLE.load(Ordering::SeqCst) {
                origin = None;
                ticks_since_origin = ORIGIN_REFRESH_TICKS;
                continue;
            }
            if ticks_since_origin >= ORIGIN_REFRESH_TICKS {
                origin = window
                    .outer_position()
                    .ok()
                    .zip(window.scale_factor().ok())
                    .map(|(position, scale)| (position.x as f64, position.y as f64, scale));
                ticks_since_origin = 0;
            }
            ticks_since_origin += 1;

            let Some((origin_x, origin_y, scale)) = origin else {
                continue;
            };
            let Some(rect) = *HIT_RECT.lock() else {
                continue;
            };
            let Ok(cursor) = app.cursor_position() else {
                continue;
            };
            let inside = pointer_is_over_panel(
                rect,
                (cursor.x - origin_x) / scale,
                (cursor.y - origin_y) / scale,
            );
            if POINTER_INSIDE.swap(inside, Ordering::SeqCst) == inside {
                continue;
            }
            let interactive = inside || KEYBOARD_ACTIVE.load(Ordering::SeqCst);
            let _ = window.set_ignore_cursor_events(!interactive);
            let _ = app.emit_to(NOTCH_HUD_LABEL, NOTCH_HUD_HOVER_EVENT, inside);
        }
        POLL_RUNNING.store(false, Ordering::SeqCst);
    });
}

// MARK: - Commands

#[tauri::command]
pub async fn get_notch_hud_geometry(app: AppHandle) -> Result<NotchHudGeometry, String> {
    Ok(notch_hud_geometry(&app))
}

/// Reported by the webview whenever the drawn silhouette changes size, so the
/// window's click capture tracks the shape instead of its full envelope.
#[tauri::command]
pub async fn set_notch_hud_hit_rect(rect: NotchHitRect) -> Result<(), String> {
    *HIT_RECT.lock() = Some(rect);
    Ok(())
}

/// Called when the panel opens or closes its input: a borderless HUD is not
/// focusable by default, and only takes the keyboard for as long as it needs it.
#[tauri::command]
pub async fn set_notch_hud_keyboard(app: AppHandle, active: bool) -> Result<(), String> {
    KEYBOARD_ACTIVE.store(active, Ordering::SeqCst);
    let (done_tx, done_rx) = std::sync::mpsc::sync_channel(1);
    let handle = app.clone();
    app.run_on_main_thread(move || {
        let result = (|| {
            let window = handle
                .get_webview_window(NOTCH_HUD_LABEL)
                .ok_or_else(|| "Notch HUD is unavailable.".to_string())?;
            window.set_focusable(active).map_err(|e| e.to_string())?;
            if active {
                window
                    .set_ignore_cursor_events(false)
                    .map_err(|e| e.to_string())?;
                window.set_focus().map_err(|e| e.to_string())?;
            } else if !POINTER_INSIDE.load(Ordering::SeqCst) {
                window
                    .set_ignore_cursor_events(true)
                    .map_err(|e| e.to_string())?;
            }
            Ok::<(), String>(())
        })();
        let _ = done_tx.send(result);
    })
    .map_err(|err| err.to_string())?;
    done_rx
        .recv_timeout(Duration::from_secs(2))
        .map_err(|_| "Notch HUD timed out while changing keyboard focus.".to_string())?
}

#[tauri::command]
pub async fn dismiss_notch_hud(app: AppHandle) -> Result<(), String> {
    hide_notch_hud(&app);
    Ok(())
}

#[tauri::command]
pub async fn toggle_notch_hud(app: AppHandle) -> Result<(), String> {
    toggle_notch_hud_window(&app);
    Ok(())
}

/// Open a memory from the HUD: hand focus back, then tell the main window to
/// open the vault on it — same activation handshake the omnibar uses.
#[tauri::command]
pub async fn notch_hud_open_memory(app: AppHandle, memory_id: String) -> Result<(), String> {
    KEYBOARD_ACTIVE.store(false, Ordering::SeqCst);
    super::screen_guide::schedule_fndr_window_activation_after_screen_guide_cancel_with_work(
        &app,
        move |work_handle| super::autofill::cancel_autofill_for_sibling_activation(&work_handle),
        move |app, autofill_cancelled| {
            if let Err(err) = autofill_cancelled {
                tracing::warn!("Notch HUD could not safely stop Autofill: {err}");
                return;
            }
            if let Some(hud) = app.get_webview_window(NOTCH_HUD_LABEL) {
                let _ = hud.set_focusable(false);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn rect() -> NotchHitRect {
        NotchHitRect {
            x: 100.0,
            y: 0.0,
            width: 200.0,
            height: 40.0,
        }
    }

    #[test]
    fn pointer_over_the_drawn_panel_makes_the_window_interactive() {
        assert!(pointer_is_over_panel(rect(), 200.0, 20.0));
        assert!(pointer_is_over_panel(rect(), 100.0, 0.0));
    }

    #[test]
    fn transparent_window_margin_stays_click_through() {
        // Well outside the silhouette but still inside the window envelope.
        assert!(!pointer_is_over_panel(rect(), 40.0, 20.0));
        assert!(!pointer_is_over_panel(rect(), 200.0, 120.0));
    }

    #[test]
    fn hit_rect_edges_keep_their_slack() {
        let outset_edge = 100.0 - HIT_RECT_OUTSET;
        assert!(pointer_is_over_panel(rect(), outset_edge, 20.0));
        assert!(!pointer_is_over_panel(rect(), outset_edge - 1.0, 20.0));
    }

    #[test]
    fn notch_width_falls_back_when_auxiliary_areas_read_small() {
        assert_eq!(physical_notch_width(1512.0, 700.0, 712.0), 200.0);
        assert_eq!(physical_notch_width(1512.0, 656.0, 656.0), 200.0);
        assert_eq!(physical_notch_width(1512.0, 600.0, 600.0), 312.0);
    }

    #[test]
    fn window_parks_centered_on_the_top_edge() {
        let (window_width, _) = notch_hud_window_size();
        let (x, y) = notch_hud_origin(0.0, 0.0, 3024.0, window_width, 2.0);
        assert_eq!(y, 0.0);
        assert_eq!(x, (3024.0 - window_width * 2.0) / 2.0);
    }

    #[test]
    fn window_envelope_clears_the_largest_panel_plus_its_shadow() {
        let (width, height) = notch_hud_window_size();
        assert!(width >= PANEL_MAX_WIDTH + SHADOW_PADDING * 2.0);
        assert!(height >= PANEL_MAX_HEIGHT + SHADOW_PADDING);
    }
}
