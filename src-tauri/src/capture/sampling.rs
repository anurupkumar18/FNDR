//! Adaptive sampling based on user activity

use crate::config::Config;

/// Adaptive sampler that adjusts FPS based on user activity
pub struct AdaptiveSampler {
    // In a full implementation, this would track keyboard/mouse events
    // For the prototype, we use a simpler time-based approach
}

impl AdaptiveSampler {
    pub fn new() -> Self {
        Self {}
    }

    /// Get the current FPS based on configuration
    /// In a full implementation, this would check for user activity
    pub fn get_current_fps(&self, config: &Config) -> f64 {
        // For prototype: always use base FPS
        // TODO: Implement actual idle detection via IOKit or CGEventTap
        config.fps_base
    }
}

impl Default for AdaptiveSampler {
    fn default() -> Self {
        Self::new()
    }
}
