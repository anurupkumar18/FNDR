//! Adaptive sampling based on user activity.
//!
//! Uses `CGEventSourceSecondsSinceLastEventType` to detect keyboard/mouse idle
//! time without spawning a background thread or requiring a run-loop.
//!
//! `TriggerGate` decides, on each poll, whether the screen is worth looking at:
//! an app switch, a window title change, or typing and scrolling that has gone
//! quiet. A timer only backs it up. The capture loop keeps its perceptual dedupe
//! behind the gate, so a trigger that finds an unchanged screen costs one
//! capture and stores nothing.

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

    /// Key down, scroll wheel and mouse button presses. Mouse movement is left
    /// out on purpose: a resting hand nudging the mouse changes nothing on screen.
    const CONTENT_INPUT_EVENT_TYPES: [u32; 4] = [
        10, // kCGEventKeyDown
        22, // kCGEventScrollWheel
        1,  // kCGEventLeftMouseDown
        3,  // kCGEventRightMouseDown
    ];

    pub fn get_content_input_idle_seconds() -> f64 {
        CONTENT_INPUT_EVENT_TYPES
            .iter()
            .map(|event_type| unsafe { CGEventSourceSecondsSinceLastEventType(1, *event_type) })
            .fold(f64::MAX, f64::min)
    }
}

#[cfg(not(target_os = "macos"))]
mod macos_idle {
    pub fn get_idle_seconds() -> f64 {
        0.0 // Fallback for non-macOS
    }

    pub fn get_content_input_idle_seconds() -> f64 {
        0.0
    }
}

/// Seconds since the last key press, scroll or click.
pub fn content_input_idle_seconds() -> f64 {
    macos_idle::get_content_input_idle_seconds()
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
        let active_fps = config.fps_base.max(0.05);
        let idle_fps = config.idle_fps.clamp(0.05, active_fps);

        if idle_secs >= config.capture_pipeline.deep_idle_seconds {
            0.0 // Pause completely
        } else if idle_secs >= config.idle_pause_seconds as f64 {
            // Smooth transition into idle mode to avoid abrupt capture rate jumps.
            let idle_excess = (idle_secs - config.idle_pause_seconds as f64).max(0.0);
            let blend = (idle_excess / config.capture_pipeline.idle_blend_seconds).clamp(0.0, 1.0);
            active_fps - (active_fps - idle_fps) * blend
        } else {
            active_fps
        }
    }
}

/// Why the gate let a poll through to the pixel capture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureTrigger {
    Startup,
    AppSwitch,
    WindowChange,
    InputSettled,
    Fallback,
}

impl CaptureTrigger {
    pub fn as_str(self) -> &'static str {
        match self {
            CaptureTrigger::Startup => "startup",
            CaptureTrigger::AppSwitch => "app_switch",
            CaptureTrigger::WindowChange => "window_change",
            CaptureTrigger::InputSettled => "input_settled",
            CaptureTrigger::Fallback => "fallback",
        }
    }
}

/// What one poll saw. Cheap to build: no pixels are involved.
#[derive(Debug, Clone, Copy)]
pub struct ScreenObservation<'a> {
    pub bundle_id: Option<&'a str>,
    pub window_title: &'a str,
    pub url: Option<&'a str>,
    /// Seconds since the last key press, scroll or click.
    pub input_idle_secs: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TriggerPolicy {
    pub settle_secs: f64,
    pub fallback_secs: f64,
    pub title_cooldown_secs: f64,
    /// Input quiet for this long counts as idle, which stretches the fallback.
    pub idle_after_secs: f64,
}

impl TriggerPolicy {
    pub fn from_config(config: &Config) -> Self {
        let pipeline = &config.capture_pipeline;
        Self {
            settle_secs: pipeline.input_settle_ms as f64 / 1000.0,
            fallback_secs: pipeline.trigger_fallback_secs as f64,
            title_cooldown_secs: pipeline.title_cooldown_secs as f64,
            idle_after_secs: config.idle_pause_seconds as f64,
        }
    }
}

/// A title with its leading and trailing symbols removed, so a spinner glyph or
/// an unsaved-changes dot does not read as a new window.
fn comparable_title(title: &str) -> &str {
    title.trim_matches(|c: char| !c.is_alphanumeric())
}

#[derive(Debug, Default)]
pub struct TriggerGate {
    last_fired_at: Option<f64>,
    bundle_id: Option<String>,
    window_title: String,
    url: Option<String>,
}

impl TriggerGate {
    pub fn new() -> Self {
        Self::default()
    }

    /// `now_secs` is any monotonic clock in seconds. Returns the reason to capture,
    /// or `None` to skip this poll. A trigger is consumed when it fires.
    pub fn decide(
        &mut self,
        now_secs: f64,
        observed: &ScreenObservation<'_>,
        policy: &TriggerPolicy,
    ) -> Option<CaptureTrigger> {
        let Some(last_fired_at) = self.last_fired_at else {
            return Some(self.fire(now_secs, observed, CaptureTrigger::Startup));
        };
        let since_fired = now_secs - last_fired_at;

        if self.bundle_id.as_deref() != observed.bundle_id {
            return Some(self.fire(now_secs, observed, CaptureTrigger::AppSwitch));
        }
        let window_changed = comparable_title(&self.window_title)
            != comparable_title(observed.window_title)
            || self.url.as_deref() != observed.url;
        if window_changed && since_fired >= policy.title_cooldown_secs {
            return Some(self.fire(now_secs, observed, CaptureTrigger::WindowChange));
        }
        let input_at = now_secs - observed.input_idle_secs;
        if input_at > last_fired_at && observed.input_idle_secs >= policy.settle_secs {
            return Some(self.fire(now_secs, observed, CaptureTrigger::InputSettled));
        }
        let fallback_secs = if observed.input_idle_secs >= policy.idle_after_secs {
            policy.fallback_secs * 2.0
        } else {
            policy.fallback_secs
        };
        if since_fired >= fallback_secs {
            return Some(self.fire(now_secs, observed, CaptureTrigger::Fallback));
        }
        None
    }

    fn fire(
        &mut self,
        now_secs: f64,
        observed: &ScreenObservation<'_>,
        trigger: CaptureTrigger,
    ) -> CaptureTrigger {
        self.last_fired_at = Some(now_secs);
        self.bundle_id = observed.bundle_id.map(str::to_string);
        self.window_title = observed.window_title.to_string();
        self.url = observed.url.map(str::to_string);
        trigger
    }
}

impl Default for AdaptiveSampler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const POLICY: TriggerPolicy = TriggerPolicy {
        settle_secs: 1.5,
        fallback_secs: 10.0,
        title_cooldown_secs: 5.0,
        idle_after_secs: 5.0,
    };

    fn seen<'a>(bundle: &'a str, title: &'a str, input_idle_secs: f64) -> ScreenObservation<'a> {
        ScreenObservation {
            bundle_id: Some(bundle),
            window_title: title,
            url: None,
            input_idle_secs,
        }
    }

    #[test]
    fn the_first_poll_always_captures() {
        let mut gate = TriggerGate::new();
        let first = gate.decide(0.0, &seen("com.apple.Terminal", "zsh", 1.0), &POLICY);
        assert_eq!(first, Some(CaptureTrigger::Startup));
    }

    #[test]
    fn an_unchanged_screen_is_skipped_until_the_fallback_timer() {
        let mut gate = TriggerGate::new();
        let active_policy = TriggerPolicy {
            idle_after_secs: 30.0,
            ..POLICY
        };
        let screen = seen("com.apple.Safari", "Docs", 2.0);
        gate.decide(0.0, &screen, &active_policy);
        for second in 1..10 {
            let unchanged = seen("com.apple.Safari", "Docs", 2.0 + second as f64);
            assert_eq!(gate.decide(second as f64, &unchanged, &active_policy), None);
        }
        assert_eq!(
            gate.decide(
                10.0,
                &seen("com.apple.Safari", "Docs", 12.0),
                &active_policy
            ),
            Some(CaptureTrigger::Fallback)
        );
        assert_eq!(
            gate.decide(
                11.0,
                &seen("com.apple.Safari", "Docs", 13.0),
                &active_policy
            ),
            None
        );
    }

    #[test]
    fn the_fallback_doubles_once_input_has_stopped() {
        let mut gate = TriggerGate::new();
        gate.decide(0.0, &seen("com.apple.Safari", "Docs", 30.0), &POLICY);
        assert_eq!(
            gate.decide(10.0, &seen("com.apple.Safari", "Docs", 40.0), &POLICY),
            None
        );
        assert_eq!(
            gate.decide(20.0, &seen("com.apple.Safari", "Docs", 50.0), &POLICY),
            Some(CaptureTrigger::Fallback)
        );
    }

    #[test]
    fn an_app_switch_captures_at_once() {
        let mut gate = TriggerGate::new();
        gate.decide(0.0, &seen("com.apple.Safari", "Docs", 2.0), &POLICY);
        assert_eq!(
            gate.decide(1.0, &seen("com.apple.Terminal", "zsh", 0.5), &POLICY),
            Some(CaptureTrigger::AppSwitch)
        );
    }

    #[test]
    fn a_title_change_waits_out_the_cooldown_and_is_not_lost() {
        let mut gate = TriggerGate::new();
        gate.decide(0.0, &seen("com.apple.Safari", "Inbox", 2.0), &POLICY);
        assert_eq!(
            gate.decide(2.0, &seen("com.apple.Safari", "Invoice 114", 4.0), &POLICY),
            None
        );
        assert_eq!(
            gate.decide(4.0, &seen("com.apple.Safari", "Invoice 114", 6.0), &POLICY),
            None
        );
        assert_eq!(
            gate.decide(5.0, &seen("com.apple.Safari", "Invoice 114", 7.0), &POLICY),
            Some(CaptureTrigger::WindowChange)
        );
        assert_eq!(
            gate.decide(6.0, &seen("com.apple.Safari", "Invoice 114", 8.0), &POLICY),
            None
        );
    }

    #[test]
    fn a_url_change_with_the_same_title_is_a_window_change() {
        let mut gate = TriggerGate::new();
        let mut page = seen("com.google.Chrome", "Dashboard", 2.0);
        page.url = Some("https://example.com/a");
        gate.decide(0.0, &page, &POLICY);
        page.url = Some("https://example.com/b");
        assert_eq!(
            gate.decide(6.0, &page, &POLICY),
            Some(CaptureTrigger::WindowChange)
        );
    }

    #[test]
    fn an_animated_title_does_not_drive_the_capture_rate() {
        let mut gate = TriggerGate::new();
        gate.decide(
            0.0,
            &seen("com.apple.Terminal", "\u{280B} build", 2.0),
            &POLICY,
        );
        let spinner = [
            "\u{2819} build",
            "\u{2839} build",
            "\u{2838} build",
            "\u{283C} build",
        ];
        for (i, title) in spinner.iter().enumerate() {
            let at = 2.0 + i as f64 * 2.0;
            assert_eq!(
                gate.decide(at, &seen("com.apple.Terminal", title, 2.0 + at), &POLICY),
                None,
                "spinner frame {title} at {at}"
            );
        }
    }

    #[test]
    fn typing_is_captured_once_when_it_goes_quiet() {
        let mut gate = TriggerGate::new();
        let editor = "com.microsoft.VSCode";
        gate.decide(0.0, &seen(editor, "main.rs", 3.0), &POLICY);
        assert_eq!(
            gate.decide(2.0, &seen(editor, "main.rs", 0.2), &POLICY),
            None
        );
        assert_eq!(
            gate.decide(4.0, &seen(editor, "main.rs", 0.4), &POLICY),
            None
        );
        assert_eq!(
            gate.decide(6.0, &seen(editor, "main.rs", 2.0), &POLICY),
            Some(CaptureTrigger::InputSettled)
        );
        assert_eq!(
            gate.decide(8.0, &seen(editor, "main.rs", 4.0), &POLICY),
            None
        );
    }

    #[test]
    fn input_from_before_the_last_capture_does_not_retrigger() {
        let mut gate = TriggerGate::new();
        let reader = "com.apple.Preview";
        gate.decide(0.0, &seen(reader, "paper.pdf", 1.0), &POLICY);
        assert_eq!(
            gate.decide(2.0, &seen(reader, "paper.pdf", 3.0), &POLICY),
            None
        );
    }

    #[test]
    fn continuous_typing_is_still_captured_by_the_fallback() {
        let mut gate = TriggerGate::new();
        let editor = "com.microsoft.VSCode";
        gate.decide(0.0, &seen(editor, "main.rs", 0.1), &POLICY);
        let mut fired = Vec::new();
        for second in 1..=30 {
            let typing = seen(editor, "main.rs", 0.1);
            if let Some(trigger) = gate.decide(second as f64, &typing, &POLICY) {
                fired.push((second, trigger));
            }
        }
        assert_eq!(
            fired,
            vec![
                (10, CaptureTrigger::Fallback),
                (20, CaptureTrigger::Fallback),
                (30, CaptureTrigger::Fallback)
            ]
        );
    }

    #[test]
    fn policy_reads_the_config_keys() {
        let mut config = Config::default();
        config.capture_pipeline.input_settle_ms = 2_000;
        config.capture_pipeline.trigger_fallback_secs = 12;
        config.idle_pause_seconds = 7;
        let policy = TriggerPolicy::from_config(&config);
        assert_eq!(policy.settle_secs, 2.0);
        assert_eq!(policy.fallback_secs, 12.0);
        assert_eq!(policy.idle_after_secs, 7.0);
    }
}
