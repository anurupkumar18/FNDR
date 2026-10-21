//! The system-wide watch for two taps of Command.
//!
//! A global `NSEvent` monitor sees modifier changes, key presses and clicks
//! in other apps (it needs the Accessibility grant FNDR already holds). The
//! handler reads only the event type, the modifier flags and whether the key
//! was Escape. It never reads or stores which characters were typed.

use super::double_tap::Input;

/// What the monitor tells the rest of Smart Fill.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    DoubleTap,
    Escape,
}

/// Turns raw event facts into recognizer input.
pub(crate) fn classify(event_type: u64, modifier_flags: u64, key_code: u16) -> ClassifiedEvent {
    const KEY_DOWN: u64 = 10;
    const FLAGS_CHANGED: u64 = 12;
    const LEFT_MOUSE_DOWN: u64 = 1;
    const RIGHT_MOUSE_DOWN: u64 = 3;
    const SHIFT: u64 = 1 << 17;
    const CONTROL: u64 = 1 << 18;
    const OPTION: u64 = 1 << 19;
    const COMMAND: u64 = 1 << 20;
    const ESCAPE_KEY_CODE: u16 = 53;

    match event_type {
        FLAGS_CHANGED => ClassifiedEvent::Input(Input::Modifiers {
            command: modifier_flags & COMMAND != 0,
            others: modifier_flags & (SHIFT | CONTROL | OPTION) != 0,
        }),
        KEY_DOWN if key_code == ESCAPE_KEY_CODE => ClassifiedEvent::Escape,
        KEY_DOWN | LEFT_MOUSE_DOWN | RIGHT_MOUSE_DOWN => ClassifiedEvent::Input(Input::Interrupt),
        _ => ClassifiedEvent::Ignored,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClassifiedEvent {
    Input(Input),
    Escape,
    Ignored,
}

#[cfg(target_os = "macos")]
mod platform {
    use std::cell::RefCell;
    use std::sync::Arc;
    use std::time::Instant;

    use block2::RcBlock;
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send, msg_send_id};
    use parking_lot::Mutex;

    use super::{classify, ClassifiedEvent, Signal};
    use crate::smart_fill::double_tap::DoubleTap;

    /// Left and right mouse down, key down, modifier changes.
    const EVENT_MASK: u64 = (1 << 1) | (1 << 3) | (1 << 10) | (1 << 12);

    thread_local! {
        /// Added and removed on the main thread only.
        static MONITOR: RefCell<Option<Retained<AnyObject>>> = const { RefCell::new(None) };
    }

    fn install(on_signal: Arc<dyn Fn(Signal) + Send + Sync>) {
        MONITOR.with(|slot| {
            if slot.borrow().is_some() {
                return;
            }
            let tap = Mutex::new(DoubleTap::new());
            let handler = RcBlock::new(move |event: *mut AnyObject| {
                if event.is_null() {
                    return;
                }
                let (event_type, flags, key_code): (u64, u64, u16) = unsafe {
                    let event_type: u64 = msg_send![event, type];
                    let flags: u64 = msg_send![event, modifierFlags];
                    let key_code: u16 = if event_type == 10 {
                        msg_send![event, keyCode]
                    } else {
                        0
                    };
                    (event_type, flags, key_code)
                };
                match classify(event_type, flags, key_code) {
                    ClassifiedEvent::Input(input) => {
                        if tap.lock().feed(Instant::now(), input) {
                            on_signal(Signal::DoubleTap);
                        }
                    }
                    ClassifiedEvent::Escape => {
                        tap.lock().feed(
                            Instant::now(),
                            crate::smart_fill::double_tap::Input::Interrupt,
                        );
                        on_signal(Signal::Escape);
                    }
                    ClassifiedEvent::Ignored => {}
                }
            });
            let monitor: Option<Retained<AnyObject>> = unsafe {
                msg_send_id![
                    class!(NSEvent),
                    addGlobalMonitorForEventsMatchingMask: EVENT_MASK,
                    handler: &*handler
                ]
            };
            if monitor.is_none() {
                tracing::warn!("Smart Fill could not watch for the Command key");
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

    pub fn set(
        app: &tauri::AppHandle,
        active: bool,
        on_signal: Arc<dyn Fn(Signal) + Send + Sync>,
    ) -> Result<(), String> {
        app.run_on_main_thread(move || {
            if active {
                install(on_signal);
            } else {
                remove();
            }
        })
        .map_err(|err| err.to_string())
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    use std::sync::Arc;

    use super::Signal;

    pub fn set(
        _app: &tauri::AppHandle,
        _active: bool,
        _on_signal: Arc<dyn Fn(Signal) + Send + Sync>,
    ) -> Result<(), String> {
        Ok(())
    }
}

pub use platform::set;

#[cfg(test)]
mod tests {
    use super::*;

    const COMMAND: u64 = 1 << 20;
    const SHIFT: u64 = 1 << 17;

    #[test]
    fn command_alone_is_a_clean_modifier_change() {
        assert_eq!(
            classify(12, COMMAND, 0),
            ClassifiedEvent::Input(Input::Modifiers {
                command: true,
                others: false
            })
        );
    }

    #[test]
    fn command_with_shift_is_not_clean() {
        assert_eq!(
            classify(12, COMMAND | SHIFT, 0),
            ClassifiedEvent::Input(Input::Modifiers {
                command: true,
                others: true
            })
        );
    }

    #[test]
    fn releasing_command_reports_command_up() {
        assert_eq!(
            classify(12, 0, 0),
            ClassifiedEvent::Input(Input::Modifiers {
                command: false,
                others: false
            })
        );
    }

    #[test]
    fn keys_and_clicks_interrupt_and_escape_is_its_own_signal() {
        assert_eq!(
            classify(10, COMMAND, 8),
            ClassifiedEvent::Input(Input::Interrupt)
        );
        assert_eq!(classify(1, 0, 0), ClassifiedEvent::Input(Input::Interrupt));
        assert_eq!(classify(3, 0, 0), ClassifiedEvent::Input(Input::Interrupt));
        assert_eq!(classify(10, 0, 53), ClassifiedEvent::Escape);
        assert_eq!(classify(22, 0, 0), ClassifiedEvent::Ignored);
    }
}
