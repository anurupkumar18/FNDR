//! Arranging a work set's windows once it has opened (ADR 027 addendum).
//! Best effort and honest: the opens already happened, so a refusal or a
//! window that never appeared is an `Arrangement` with `arranged: false` and
//! a plain reason, never an error.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::operator::layout::{Layout, LayoutOutcome, WindowRef, MAX_WINDOWS};
use crate::AppState;

/// How long FNDR waits for opened windows to appear.
pub const WINDOW_WAIT: Duration = Duration::from_secs(3);
const WINDOW_POLL: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Arrangement {
    pub arranged: bool,
    pub layout: Layout,
    /// What happened, in plain words: "Arranged side by side", or why not.
    pub detail: String,
}

impl Arrangement {
    fn not(layout: Layout, reason: impl Into<String>) -> Self {
        Self {
            arranged: false,
            layout,
            detail: reason.into(),
        }
    }

    /// The sentence a finished run ends with.
    pub fn sentence(&self) -> String {
        let detail = self.detail.trim_end_matches('.');
        if self.arranged {
            format!("{detail}.")
        } else {
            format!("Not arranged: {detail}.")
        }
    }
}

/// One item that opened: the app it opens in and what its window shows.
#[derive(Debug, Clone, PartialEq)]
pub struct Opened {
    pub app: String,
    pub label: String,
}

/// The Mac's windows, or a fake of them in tests.
pub trait WindowSource {
    /// Why no window may be moved now: actions off, FNDR private, or no
    /// Accessibility access.
    fn blocked(&self) -> Option<String>;
    /// Why this app's windows are off limits.
    fn refusal(&self, app: &str) -> Option<String>;
    fn list(&mut self) -> Result<Vec<WindowRef>, String>;
    fn arrange(&mut self, windows: &[WindowRef], layout: Layout) -> LayoutOutcome;
}

/// Side by side with three windows is thirds.
pub fn fit(layout: Layout, count: usize) -> Layout {
    match (layout, count) {
        (Layout::LeftRightSplit, 3) => Layout::Thirds,
        _ => layout,
    }
}

fn done(layout: Layout) -> &'static str {
    match layout {
        Layout::LeftRightSplit => "Arranged side by side",
        Layout::TopBottomSplit => "Arranged one above the other",
        Layout::Thirds => "Arranged in thirds",
        Layout::Grid2x2 => "Arranged in a grid",
        Layout::Maximize => "Filled the screen",
        Layout::RestorePrevious => "Put back where they were",
    }
}

/// The part of a label a window title is likely to show: "notes.pdf" of
/// "notes.pdf, page 4".
fn core(label: &str) -> String {
    label
        .split(", page ")
        .next()
        .unwrap_or(label)
        .trim()
        .to_lowercase()
}

/// How well a window fits an opened item: 2 its title shows the item, 1 it
/// belongs to the item's app, 0 not at all.
fn fits(window: &WindowRef, opened: &Opened) -> u8 {
    let title = window.title.trim().to_lowercase();
    let label = core(&opened.label);
    let named = title.chars().count() >= 3
        && label.chars().count() >= 3
        && (title.contains(&label) || label.contains(&title));
    if named {
        2
    } else if window.app.eq_ignore_ascii_case(opened.app.trim())
        || window.bundle.eq_ignore_ascii_case(opened.app.trim())
    {
        1
    } else {
        0
    }
}

/// One window per opened item, in the items' order: the best fit not taken
/// yet, never a minimized one. An item whose window was not found is skipped.
pub fn pick(windows: &[WindowRef], opened: &[Opened]) -> Vec<Option<WindowRef>> {
    let mut taken: Vec<u64> = Vec::new();
    opened
        .iter()
        .map(|item| {
            let best = windows
                .iter()
                .filter(|window| !window.minimized && !taken.contains(&window.id))
                .map(|window| (fits(window, item), window))
                .filter(|(score, _)| *score > 0)
                .fold(
                    None::<(u8, &WindowRef)>,
                    |best, (score, window)| match best {
                        Some((kept, _)) if kept >= score => best,
                        _ => Some((score, window)),
                    },
                )
                .map(|(_, window)| window.clone());
            if let Some(window) = &best {
                taken.push(window.id);
            }
            best
        })
        .collect()
}

/// Waits up to `wait` for a window of every opened item, then puts the ones
/// found into `layout`.
pub fn arrange_opened<S: WindowSource>(
    source: &mut S,
    opened: &[Opened],
    layout: Layout,
    wait: Duration,
    every: Duration,
) -> Arrangement {
    if let Some(reason) = source.blocked() {
        return Arrangement::not(layout, reason);
    }
    if opened.is_empty() {
        return Arrangement::not(layout, "nothing opened");
    }
    if opened.len() > MAX_WINDOWS {
        return Arrangement::not(
            layout,
            format!("a layout holds at most {MAX_WINDOWS} windows"),
        );
    }
    if let Some(reason) = opened.iter().find_map(|item| source.refusal(&item.app)) {
        return Arrangement::not(layout, reason);
    }
    let deadline = Instant::now() + wait;
    let picked = loop {
        let windows = match source.list() {
            Ok(windows) => windows,
            Err(error) => {
                return Arrangement::not(layout, format!("FNDR could not list windows: {error}"))
            }
        };
        let picked = pick(&windows, opened);
        if picked.iter().all(Option::is_some) || Instant::now() >= deadline {
            break picked;
        }
        std::thread::sleep(every);
    };
    let windows: Vec<WindowRef> = picked.into_iter().flatten().collect();
    match windows.len() {
        0 => return Arrangement::not(layout, "FNDR did not find the windows it opened"),
        1 if layout != Layout::Maximize => {
            return Arrangement::not(
                layout,
                if opened.len() == 1 {
                    "only one place opened, so there was nothing to arrange"
                } else {
                    "FNDR found only one of the windows it opened"
                },
            )
        }
        _ => {}
    }
    let layout = fit(layout, windows.len());
    match source.arrange(&windows, layout) {
        LayoutOutcome::Refused(reason) => Arrangement::not(layout, reason),
        LayoutOutcome::Arranged(placed) => {
            let failed: Vec<&str> = placed
                .iter()
                .filter(|window| window.error.is_some())
                .map(|window| window.app.as_str())
                .collect();
            if failed.len() == placed.len() {
                let reason = placed
                    .iter()
                    .find_map(|window| window.error.clone())
                    .unwrap_or_else(|| "no window moved".to_string());
                Arrangement::not(layout, reason)
            } else if failed.is_empty() {
                Arrangement {
                    arranged: true,
                    layout,
                    detail: done(layout).to_string(),
                }
            } else {
                Arrangement {
                    arranged: true,
                    layout,
                    detail: format!("{}, except {}", done(layout), failed.join(", ")),
                }
            }
        }
    }
}

const NO_ACCESSIBILITY: &str = "FNDR needs Accessibility access to move windows (System Settings, Privacy & Security, Accessibility)";

/// The Mac's windows through the layout module, under the run's guards.
struct MacWindows {
    halt: Option<String>,
    limits: crate::operator::mcp::Limits,
}

impl WindowSource for MacWindows {
    fn blocked(&self) -> Option<String> {
        self.halt.clone().or_else(|| {
            (!crate::accessibility::has_accessibility_permission())
                .then(|| NO_ACCESSIBILITY.to_string())
        })
    }

    fn refusal(&self, app: &str) -> Option<String> {
        let app = app.trim();
        let off = !app.is_empty()
            && (crate::privacy::Blocklist::is_internal_app(app, Some(app))
                || crate::privacy::Blocklist::is_blocked(app, &self.limits.blocklist)
                || crate::operator::policy::is_sensitive_app(app));
        off.then(|| format!("{app} is off limits to Notch Do"))
    }

    fn list(&mut self) -> Result<Vec<WindowRef>, String> {
        crate::operator::layout::list_windows(None, &self.limits)
    }

    fn arrange(&mut self, windows: &[WindowRef], layout: Layout) -> LayoutOutcome {
        crate::operator::layout::arrange(windows, layout)
    }
}

/// Why nothing may be moved: the actions kill switch or Private Mode.
fn halted(state: &AppState) -> Option<String> {
    if state.config.read().actions_kill_switch {
        Some("actions are turned off in Settings".to_string())
    } else if state.is_incognito.load(std::sync::atomic::Ordering::SeqCst) {
        Some("FNDR is private right now".to_string())
    } else {
        None
    }
}

/// Arranges the windows of what just opened. Runs the Accessibility calls
/// off the async runtime.
pub async fn after_open(state: &AppState, opened: Vec<Opened>, layout: Layout) -> Arrangement {
    let halt = halted(state);
    let limits = match crate::operator::mcp::Limits::from_settings() {
        Ok(limits) => limits,
        Err(error) => return Arrangement::not(layout, error),
    };
    tokio::task::spawn_blocking(move || {
        arrange_opened(
            &mut MacWindows { halt, limits },
            &opened,
            layout,
            WINDOW_WAIT,
            WINDOW_POLL,
        )
    })
    .await
    .unwrap_or_else(|error| Arrangement::not(layout, format!("FNDR could not arrange: {error}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operator::layout::{Placed, Rect};

    fn window(id: u64, app: &str, title: &str) -> WindowRef {
        WindowRef {
            id,
            app: app.into(),
            bundle: format!("com.example.{}", app.to_lowercase()),
            title: title.into(),
            frame: Rect::new(0.0, 0.0, 800.0, 600.0),
            display: Some(0),
            minimized: false,
        }
    }

    fn opened(app: &str, label: &str) -> Opened {
        Opened {
            app: app.into(),
            label: label.into(),
        }
    }

    /// A fake Mac: windows appear after `appear_after` lists; `arrange`
    /// records what it was asked and answers with `answer`.
    struct Fake {
        blocked: Option<String>,
        windows: Vec<WindowRef>,
        appear_after: usize,
        lists: usize,
        asked: Option<(Vec<u64>, Layout)>,
        answer: Option<LayoutOutcome>,
    }

    impl Fake {
        fn with(windows: Vec<WindowRef>) -> Self {
            Fake {
                blocked: None,
                windows,
                appear_after: 0,
                lists: 0,
                asked: None,
                answer: None,
            }
        }
    }

    impl WindowSource for Fake {
        fn blocked(&self) -> Option<String> {
            self.blocked.clone()
        }

        fn refusal(&self, app: &str) -> Option<String> {
            (app == "1Password").then(|| format!("{app} is off limits to Notch Do"))
        }

        fn list(&mut self) -> Result<Vec<WindowRef>, String> {
            self.lists += 1;
            Ok(if self.lists > self.appear_after {
                self.windows.clone()
            } else {
                Vec::new()
            })
        }

        fn arrange(&mut self, windows: &[WindowRef], layout: Layout) -> LayoutOutcome {
            self.asked = Some((windows.iter().map(|w| w.id).collect(), layout));
            self.answer.clone().unwrap_or_else(|| {
                LayoutOutcome::Arranged(
                    windows
                        .iter()
                        .map(|w| Placed {
                            id: w.id,
                            app: w.app.clone(),
                            title: w.title.clone(),
                            asked: w.frame,
                            now: Some(w.frame),
                            error: None,
                        })
                        .collect(),
                )
            })
        }
    }

    const QUICK: Duration = Duration::from_millis(40);
    const TICK: Duration = Duration::from_millis(1);

    #[test]
    fn opened_windows_are_found_by_title_then_app_and_arranged_in_order() {
        let mut mac = Fake::with(vec![
            window(1, "Slack", "Team chat"),
            window(2, "Preview", "notes.pdf"),
            window(3, "Safari", "Assignment 3 - Canvas"),
        ]);
        mac.appear_after = 2;
        let arrangement = arrange_opened(
            &mut mac,
            &[
                opened("Google Chrome", "Assignment 3"),
                opened("Preview", "notes.pdf, page 4"),
            ],
            Layout::LeftRightSplit,
            Duration::from_secs(2),
            TICK,
        );
        assert_eq!(
            arrangement,
            Arrangement {
                arranged: true,
                layout: Layout::LeftRightSplit,
                detail: "Arranged side by side".into(),
            }
        );
        assert_eq!(arrangement.sentence(), "Arranged side by side.");
        assert_eq!(mac.lists, 3, "it waited for the windows to appear");
        assert_eq!(
            mac.asked,
            Some((vec![3, 2], Layout::LeftRightSplit)),
            "the page opened in Safari is found by its title"
        );
    }

    #[test]
    fn three_windows_side_by_side_are_thirds() {
        let mut mac = Fake::with(vec![
            window(1, "Safari", "A"),
            window(2, "Preview", "b.pdf"),
            window(3, "Pages", "C report"),
        ]);
        let arrangement = arrange_opened(
            &mut mac,
            &[
                opened("Safari", "A"),
                opened("Preview", "b.pdf"),
                opened("Pages", "C report"),
            ],
            Layout::LeftRightSplit,
            QUICK,
            TICK,
        );
        assert_eq!(arrangement.layout, Layout::Thirds);
        assert_eq!(arrangement.detail, "Arranged in thirds");
    }

    #[test]
    fn refusals_and_missing_windows_say_why_and_move_nothing() {
        let both = [opened("Safari", "Canvas"), opened("Preview", "notes.pdf")];

        let mut mac = Fake::with(vec![window(1, "Safari", "Canvas")]);
        mac.blocked = Some(NO_ACCESSIBILITY.to_string());
        let no_access = arrange_opened(&mut mac, &both, Layout::LeftRightSplit, QUICK, TICK);
        assert!(!no_access.arranged);
        assert_eq!(no_access.detail, NO_ACCESSIBILITY);
        assert_eq!(mac.lists, 0, "nothing is listed without access");

        let mut mac = Fake::with(vec![window(1, "Safari", "Canvas")]);
        mac.blocked = Some("FNDR is private right now".into());
        let private = arrange_opened(&mut mac, &both, Layout::LeftRightSplit, QUICK, TICK);
        assert_eq!(
            private.sentence(),
            "Not arranged: FNDR is private right now."
        );
        assert!(mac.asked.is_none());

        let mut mac = Fake::with(vec![window(1, "Safari", "Canvas")]);
        let off = arrange_opened(
            &mut mac,
            &[opened("Safari", "Canvas"), opened("1Password", "Vault")],
            Layout::LeftRightSplit,
            QUICK,
            TICK,
        );
        assert_eq!(off.detail, "1Password is off limits to Notch Do");
        assert!(mac.asked.is_none());

        let seven: Vec<Opened> = (0..7).map(|n| opened("Safari", &format!("p{n}"))).collect();
        let many = arrange_opened(&mut mac, &seven, Layout::Grid2x2, QUICK, TICK);
        assert_eq!(many.detail, "a layout holds at most 6 windows");

        let mut mac = Fake::with(vec![window(1, "Safari", "Canvas")]);
        let one = arrange_opened(&mut mac, &both, Layout::LeftRightSplit, QUICK, TICK);
        assert!(!one.arranged);
        assert_eq!(one.detail, "FNDR found only one of the windows it opened");
        assert!(mac.lists >= 2, "it kept looking until the wait ran out");
        assert!(mac.asked.is_none());

        let mut mac = Fake::with(Vec::new());
        let none = arrange_opened(&mut mac, &both, Layout::LeftRightSplit, QUICK, TICK);
        assert_eq!(none.detail, "FNDR did not find the windows it opened");

        let mut mac = Fake::with(vec![
            window(1, "Safari", "Canvas"),
            window(2, "Preview", "notes.pdf"),
        ]);
        mac.answer = Some(LayoutOutcome::Refused("No display was found.".into()));
        let refused = arrange_opened(&mut mac, &both, Layout::LeftRightSplit, QUICK, TICK);
        assert_eq!(refused.sentence(), "Not arranged: No display was found.");
    }

    #[test]
    fn a_minimized_window_is_never_picked() {
        let mut hidden = window(1, "Preview", "notes.pdf");
        hidden.minimized = true;
        let picked = pick(
            &[hidden, window(2, "Preview", "other.pdf")],
            &[opened("Preview", "notes.pdf")],
        );
        assert_eq!(picked[0].as_ref().map(|w| w.id), Some(2));
    }

    #[test]
    fn an_arrangement_serializes_in_the_shape_the_app_reads() {
        let value = serde_json::to_value(Arrangement::not(Layout::Grid2x2, "x")).unwrap();
        assert_eq!(
            value,
            serde_json::json!({ "arranged": false, "layout": "grid2x2", "detail": "x" })
        );
    }
}
