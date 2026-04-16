//! Adaptive sampling based on user activity.
//!
//! Uses `CGEventSourceSecondsSinceLastEventType` to detect keyboard/mouse idle
//! time without spawning a background thread or requiring a run-loop.

use crate::config::Config;

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    /// Returns the elapsed time since the last event of the given types was
    /// received from HID state (kCGEventSourceStateHIDSystemState = 1).
    fn CGEventSourceSecondsSinceLastEventType(
        state_id: i32,
        event_type: u32,
    ) -> f64;
}

/// Bitmask covering mouse moves, clicks, drags, scroll, and key events.
/// 0xFFFFFFFF is the wildcard mask accepted by the API (kCGAnyInputEventType).
const CG_ANY_INPUT_EVENT_TYPE: u32 = 0xFFFF_FFFF;

/// CGEventSourceStateID = HIDSystemState (1) — reflects physical device input.
const CG_EVENT_SOURCE_HID: i32 = 1;

/// Returns the number of seconds since the user last generated a keyboard or
/// mouse event, queried directly from the HID state.
fn seconds_since_last_input() -> f64 {
    // SAFETY: CGEventSourceSecondsSinceLastEventType is a pure read-only C
    // function from the CoreGraphics framework with no side-effects.
    unsafe { CGEventSourceSecondsSinceLastEventType(CG_EVENT_SOURCE_HID, CG_ANY_INPUT_EVENT_TYPE) }
}

/// Adaptive sampler that adjusts FPS based on user activity.
pub struct AdaptiveSampler;

impl AdaptiveSampler {
    pub fn new() -> Self {
        Self
    }

    /// Returns the capture FPS appropriate for the current idle state.
    ///
    /// If the user has been idle longer than `config.idle_pause_seconds`, returns
    /// `config.idle_fps`; otherwise returns `config.fps_base`.
    pub fn get_current_fps(&self, config: &Config) -> f64 {
        let idle_secs = seconds_since_last_input();
        if idle_secs >= config.idle_pause_seconds as f64 {
            config.idle_fps
        } else {
            config.fps_base
        }
    }
}

impl Default for AdaptiveSampler {
    fn default() -> Self {
        Self::new()
    }
}
