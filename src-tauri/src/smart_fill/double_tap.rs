//! Recognising two quick taps of the Command key anywhere on the Mac.
//!
//! The recognizer is a plain state machine so the rules are unit-tested
//! without a keyboard. A tap is Command pressed and released alone: no other
//! modifier, no other key and no click while it is down, and not held longer
//! than `MAX_HOLD`. Two taps closer than `MAX_GAP` make a double tap. Any key
//! or click between the taps starts the count again, so Cmd+C, Cmd+V in quick
//! succession never fires.

use std::time::{Duration, Instant};

/// Longest a Command press may last and still be a tap.
pub const MAX_HOLD: Duration = Duration::from_millis(350);
/// Longest pause between releasing the first tap and pressing the second.
pub const MAX_GAP: Duration = Duration::from_millis(400);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
    /// The modifier state changed. `command` is Command down; `others` is any
    /// of Shift, Control, Option or Caps-independent device flags down.
    Modifiers { command: bool, others: bool },
    /// A key went down or the mouse was clicked.
    Interrupt,
}

#[derive(Debug, Default)]
pub struct DoubleTap {
    down_at: Option<Instant>,
    clean: bool,
    last_tap_released_at: Option<Instant>,
}

impl DoubleTap {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feeds one event; true when it completes a double tap.
    pub fn feed(&mut self, now: Instant, input: Input) -> bool {
        match input {
            Input::Interrupt => {
                self.clean = false;
                self.last_tap_released_at = None;
                false
            }
            Input::Modifiers { command, others } => {
                if command {
                    if self.down_at.is_none() {
                        self.down_at = Some(now);
                        self.clean = !others;
                    } else if others {
                        self.clean = false;
                    }
                    return false;
                }
                let Some(down_at) = self.down_at.take() else {
                    return false;
                };
                let was_clean = std::mem::replace(&mut self.clean, false);
                if !was_clean || now.duration_since(down_at) > MAX_HOLD {
                    self.last_tap_released_at = None;
                    return false;
                }
                match self.last_tap_released_at.take() {
                    Some(previous) if down_at.duration_since(previous) <= MAX_GAP => true,
                    _ => {
                        self.last_tap_released_at = Some(now);
                        false
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOWN: Input = Input::Modifiers {
        command: true,
        others: false,
    };
    const UP: Input = Input::Modifiers {
        command: false,
        others: false,
    };

    fn at(base: Instant, ms: u64) -> Instant {
        base + Duration::from_millis(ms)
    }

    #[test]
    fn two_quick_taps_fire_once() {
        let t = Instant::now();
        let mut tap = DoubleTap::new();
        assert!(!tap.feed(at(t, 0), DOWN));
        assert!(!tap.feed(at(t, 80), UP));
        assert!(!tap.feed(at(t, 200), DOWN));
        assert!(tap.feed(at(t, 280), UP));
        // The pair is spent: a third tap starts a new count.
        assert!(!tap.feed(at(t, 360), DOWN));
        assert!(!tap.feed(at(t, 420), UP));
    }

    #[test]
    fn a_slow_second_tap_does_not_fire() {
        let t = Instant::now();
        let mut tap = DoubleTap::new();
        tap.feed(at(t, 0), DOWN);
        tap.feed(at(t, 60), UP);
        tap.feed(at(t, 700), DOWN);
        assert!(!tap.feed(at(t, 760), UP));
    }

    #[test]
    fn a_long_hold_is_not_a_tap() {
        let t = Instant::now();
        let mut tap = DoubleTap::new();
        tap.feed(at(t, 0), DOWN);
        tap.feed(at(t, 900), UP);
        tap.feed(at(t, 950), DOWN);
        assert!(!tap.feed(at(t, 1000), UP));
    }

    #[test]
    fn command_with_a_key_is_not_a_tap() {
        let t = Instant::now();
        let mut tap = DoubleTap::new();
        // Cmd+C then Cmd+V.
        tap.feed(at(t, 0), DOWN);
        tap.feed(at(t, 30), Input::Interrupt);
        tap.feed(at(t, 90), UP);
        tap.feed(at(t, 140), DOWN);
        tap.feed(at(t, 170), Input::Interrupt);
        assert!(!tap.feed(at(t, 230), UP));
    }

    #[test]
    fn a_key_between_two_taps_starts_over() {
        let t = Instant::now();
        let mut tap = DoubleTap::new();
        tap.feed(at(t, 0), DOWN);
        tap.feed(at(t, 50), UP);
        tap.feed(at(t, 100), Input::Interrupt);
        tap.feed(at(t, 150), DOWN);
        assert!(!tap.feed(at(t, 200), UP));
    }

    #[test]
    fn another_modifier_spoils_the_tap() {
        let t = Instant::now();
        let mut tap = DoubleTap::new();
        tap.feed(
            at(t, 0),
            Input::Modifiers {
                command: true,
                others: true,
            },
        );
        tap.feed(at(t, 40), UP);
        tap.feed(at(t, 100), DOWN);
        assert!(!tap.feed(at(t, 150), UP));
    }

    #[test]
    fn a_modifier_added_while_held_spoils_the_tap() {
        let t = Instant::now();
        let mut tap = DoubleTap::new();
        tap.feed(at(t, 0), DOWN);
        tap.feed(
            at(t, 20),
            Input::Modifiers {
                command: true,
                others: true,
            },
        );
        tap.feed(at(t, 60), UP);
        tap.feed(at(t, 120), DOWN);
        assert!(!tap.feed(at(t, 160), UP));
    }
}
