//! Adaptive sampling based on user activity.
//!
//! Uses `CGEventSourceSecondsSinceLastEventType` to detect keyboard/mouse idle
//! time without spawning a background thread or requiring a run-loop.

use crate::config::Config;

#[cfg(target_os = "macos")]
mod macos_idle {
    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGEventSourceSecondsSinceLastEventType(sourceStateID: i32, eventType: u32) -> f64;
    }

    pub fn get_idle_seconds() -> f64 {
        unsafe {
            // kCGAnyInputEventType = ~0 = 0xFFFFFFFF, kCGEventSourceStateHIDSystemState = 1
            CGEventSourceSecondsSinceLastEventType(1, 0xFFFFFFFF)
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod macos_idle {
    pub fn get_idle_seconds() -> f64 {
        0.0 // Fallback for non-macOS
    }
}

/// Adaptive sampler that adjusts FPS based on user activity
pub struct AdaptiveSampler {}

impl AdaptiveSampler {
    pub fn new() -> Self {
        Self {}
    }

    /// Get the current FPS based on configuration and user activity.
    /// Returns 0.0 if the system is in deep idle (e.g. > 5 minutes).
    pub fn get_current_fps(&self, config: &Config) -> f64 {
        let idle_secs = macos_idle::get_idle_seconds();
        let deep_idle_seconds = 300.0; // 5 minutes completely untouched

        let active_fps = config.fps_base.max(0.05);
        let idle_fps = config.idle_fps.clamp(0.05, active_fps);

        if idle_secs >= deep_idle_seconds {
            0.0 // Pause completely
        } else if idle_secs >= config.idle_pause_seconds as f64 {
            // Smooth transition into idle mode over ~30s to avoid abrupt capture rate jumps.
            let idle_excess = (idle_secs - config.idle_pause_seconds as f64).max(0.0);
            let blend = (idle_excess / 30.0).clamp(0.0, 1.0);
            active_fps - (active_fps - idle_fps) * blend
        } else {
            active_fps
        }
    }
}

impl Default for AdaptiveSampler {
    fn default() -> Self {
        Self::new()
    }
}
