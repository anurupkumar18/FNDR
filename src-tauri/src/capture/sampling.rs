//! Adaptive sampling based on user activity

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
        
        if idle_secs >= deep_idle_seconds {
            0.0 // Pause completely
        } else if idle_secs >= config.idle_pause_seconds as f64 {
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
