//! Notch Do's voice settings and its Esc outside the notch (ADR 020
//! amendment, 2026-10-09).
//!
//! While a run is working or waiting on an approval, the operated app is
//! frontmost and the notch has no key focus, so its own Esc handler never
//! fires. A global key monitor watches for Esc in other apps for that time
//! only and tells the notch, which stops the run. FNDR's executor presses keys
//! through System Events (`accessibility/operate.rs`, `osascript`); those
//! presses are recognized by their source process and ignored, so a planned
//! Escape never stops the run that sent it.

use std::sync::Arc;

use tauri::State;

use crate::AppState;

/// Sent to the webview when Esc is pressed in another app during a run.
pub const NOTCH_DO_ESCAPE_EVENT: &str = "notch-do://escape";

/// The mute, or `None` while the person has never set it here.
#[tauri::command]
pub async fn get_notch_do_muted(state: State<'_, Arc<AppState>>) -> Result<Option<bool>, String> {
    Ok(state.inner().config.read().notch_do_muted)
}

#[tauri::command]
pub async fn set_notch_do_muted(
    state: State<'_, Arc<AppState>>,
    muted: bool,
) -> Result<bool, String> {
    let mut config = state.inner().config.write();
    let previous = config.notch_do_muted.replace(muted);
    if let Err(err) = config.save() {
        config.notch_do_muted = previous;
        return Err(err.to_string());
    }
    Ok(muted)
}

/// Turns the Esc monitor on while a run works or waits on an approval, and off
/// when it ends. Needs the Accessibility grant Notch Do already holds.
#[tauri::command]
pub async fn set_notch_escape_monitor(app: tauri::AppHandle, active: bool) -> Result<(), String> {
    monitor::set(app, active)
}

/// Whether an Esc came from FNDR itself: its own process, or the System Events
/// path its executor presses keys through.
fn is_own_key(source_pid: i64, own_pid: i64, source_path: Option<&str>) -> bool {
    if source_pid == own_pid {
        return true;
    }
    source_path
        .is_some_and(|path| path.ends_with("/osascript") || path.contains("/System Events.app/"))
}

#[cfg(target_os = "macos")]
mod monitor {
    use std::cell::RefCell;
    use std::ffi::c_void;

    use block2::RcBlock;
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send, msg_send_id};
    use tauri::Emitter;

    use super::{is_own_key, NOTCH_DO_ESCAPE_EVENT};

    const KEY_DOWN_MASK: u64 = 1 << 10;
    const ESCAPE_KEY_CODE: u16 = 53;
    /// `kCGEventSourceUnixProcessID`.
    const SOURCE_PID_FIELD: u32 = 41;

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGEventGetIntegerValueField(event: *const c_void, field: u32) -> i64;
    }

    thread_local! {
        /// Added and removed on the main thread only.
        static MONITOR: RefCell<Option<Retained<AnyObject>>> = const { RefCell::new(None) };
    }

    fn process_path(pid: i64) -> Option<String> {
        let pid = i32::try_from(pid).ok().filter(|pid| *pid > 0)?;
        let mut buffer = vec![0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
        let written =
            unsafe { libc::proc_pidpath(pid, buffer.as_mut_ptr().cast(), buffer.len() as u32) };
        if written <= 0 {
            return None;
        }
        buffer.truncate(written as usize);
        String::from_utf8(buffer).ok()
    }

    unsafe fn escape_from_the_person(event: *mut AnyObject) -> bool {
        if event.is_null() {
            return false;
        }
        let key_code: u16 = msg_send![event, keyCode];
        if key_code != ESCAPE_KEY_CODE {
            return false;
        }
        let cg_event: *const c_void = msg_send![event, CGEvent];
        let source_pid = if cg_event.is_null() {
            0
        } else {
            CGEventGetIntegerValueField(cg_event, SOURCE_PID_FIELD)
        };
        !is_own_key(
            source_pid,
            i64::from(std::process::id()),
            process_path(source_pid).as_deref(),
        )
    }

    fn install(app: tauri::AppHandle) {
        MONITOR.with(|slot| {
            if slot.borrow().is_some() {
                return;
            }
            let handler = RcBlock::new(move |event: *mut AnyObject| {
                if unsafe { escape_from_the_person(event) } {
                    let _ = app.emit(NOTCH_DO_ESCAPE_EVENT, ());
                }
            });
            let monitor: Option<Retained<AnyObject>> = unsafe {
                msg_send_id![
                    class!(NSEvent),
                    addGlobalMonitorForEventsMatchingMask: KEY_DOWN_MASK,
                    handler: &*handler
                ]
            };
            if monitor.is_none() {
                tracing::warn!("Notch Do could not watch for Esc outside the notch");
            }
            *slot.borrow_mut() = monitor;
        });
    }

    fn remove() {
        MONITOR.with(|slot| {
            if let Some(monitor) = slot.borrow_mut().take() {
                let _: () = unsafe { msg_send![class!(NSEvent), removeMonitor: &*monitor] };
            }
        });
    }

    pub fn set(app: tauri::AppHandle, active: bool) -> Result<(), String> {
        let handle = app.clone();
        app.run_on_main_thread(move || {
            if active {
                install(handle);
            } else {
                remove();
            }
        })
        .map_err(|err| err.to_string())
    }
}

#[cfg(not(target_os = "macos"))]
mod monitor {
    pub fn set(_app: tauri::AppHandle, _active: bool) -> Result<(), String> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::is_own_key;

    #[test]
    fn an_escape_from_fndr_or_its_executor_is_not_the_person_s() {
        assert!(is_own_key(4242, 4242, None));
        assert!(is_own_key(
            77,
            4242,
            Some("/System/Library/CoreServices/System Events.app/Contents/MacOS/System Events")
        ));
        assert!(is_own_key(78, 4242, Some("/usr/bin/osascript")));
    }

    #[test]
    fn an_escape_from_the_keyboard_or_another_app_is_the_person_s() {
        assert!(!is_own_key(0, 4242, None));
        assert!(!is_own_key(
            90,
            4242,
            Some("/Applications/Safari.app/Contents/MacOS/Safari")
        ));
        assert!(!is_own_key(91, 4242, Some("/usr/bin/osascript-helper")));
    }
}
