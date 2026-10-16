//! Window layout for Notch Do (ADR 026 addendum): where each window of a work
//! set goes on a display, kept apart from the Accessibility calls that move
//! them so the geometry can be tested without a Mac.
//!
//! Coordinates are the Accessibility ones: the top-left corner of the main
//! display is 0,0 and y grows downwards, across every display.

use serde::{Deserialize, Serialize};

use crate::operator::mcp::{AppRef, Desktop, Limits};

/// The most windows one layout may hold. More is refused, by policy and here.
pub const MAX_WINDOWS: usize = 6;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    pub const fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
}

/// One display: its whole frame and the part windows may use, without the
/// menu bar and the Dock.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Screen {
    pub frame: Rect,
    pub visible: Rect,
}

/// The layouts FNDR knows. Nothing else is accepted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Layout {
    LeftRightSplit,
    TopBottomSplit,
    Thirds,
    Grid2x2,
    Maximize,
    RestorePrevious,
}

impl Layout {
    pub const ALL: [Layout; 6] = [
        Layout::LeftRightSplit,
        Layout::TopBottomSplit,
        Layout::Thirds,
        Layout::Grid2x2,
        Layout::Maximize,
        Layout::RestorePrevious,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Layout::LeftRightSplit => "left_right_split",
            Layout::TopBottomSplit => "top_bottom_split",
            Layout::Thirds => "thirds",
            Layout::Grid2x2 => "grid2x2",
            Layout::Maximize => "maximize",
            Layout::RestorePrevious => "restore_previous",
        }
    }

    pub fn parse(name: &str) -> Option<Layout> {
        let name = name.trim().to_lowercase();
        Layout::ALL.into_iter().find(|layout| layout.name() == name)
    }
}

/// Where `count` windows go on `area`, in the order given. The first window
/// takes the left (or top) cell. Cells meet exactly: no gap, no overlap.
pub fn frames(layout: Layout, area: Rect, count: usize) -> Result<Vec<Rect>, String> {
    if count == 0 {
        return Err("Name at least one window.".to_string());
    }
    if count > MAX_WINDOWS {
        return Err(format!("A layout holds at most {MAX_WINDOWS} windows."));
    }
    let (columns, rows, capacity) = match layout {
        Layout::LeftRightSplit => (count.max(2), 1, count),
        Layout::TopBottomSplit => (1, count.max(2), count),
        Layout::Thirds => (3, 1, 3),
        Layout::Grid2x2 => (2, 2, 4),
        Layout::Maximize => return Ok(vec![area; count]),
        Layout::RestorePrevious => {
            return Err(
                "restore_previous puts windows back where they were; it has no cells.".to_string(),
            )
        }
    };
    if count > capacity {
        return Err(format!(
            "{} holds at most {capacity} windows.",
            layout.name()
        ));
    }
    // Cell edges fall on whole points from the area's own corner, so the
    // cells meet exactly and the last one ends on the area's edge.
    let edge = |length: f64, parts: usize, at: usize| (length * at as f64 / parts as f64).floor();
    let span = |length: f64, parts: usize, at: usize| {
        if at + 1 == parts {
            length - edge(length, parts, at)
        } else {
            edge(length, parts, at + 1) - edge(length, parts, at)
        }
    };
    Ok((0..count)
        .map(|at| {
            let (column, row) = (at % columns, at / columns);
            Rect::new(
                area.x + edge(area.width, columns, column),
                area.y + edge(area.height, rows, row),
                span(area.width, columns, column),
                span(area.height, rows, row),
            )
        })
        .collect())
}

/// `frame` moved and shrunk just enough to sit inside `area`.
pub fn clamp(frame: Rect, area: Rect) -> Rect {
    let width = frame.width.min(area.width);
    let height = frame.height.min(area.height);
    Rect::new(
        frame.x.clamp(area.x, area.x + area.width - width),
        frame.y.clamp(area.y, area.y + area.height - height),
        width,
        height,
    )
}

fn overlap(a: Rect, b: Rect) -> f64 {
    let width = (a.x + a.width).min(b.x + b.width) - a.x.max(b.x);
    let height = (a.y + a.height).min(b.y + b.height) - a.y.max(b.y);
    width.max(0.0) * height.max(0.0)
}

/// The display holding most of `frame`, or `None` when it is on none. A tie
/// goes to the display listed first.
pub fn screen_holding(frame: Rect, screens: &[Screen]) -> Option<usize> {
    let mut best: Option<(usize, f64)> = None;
    for (at, screen) in screens.iter().enumerate() {
        let shared = overlap(frame, screen.frame);
        if shared > 0.0 && best.map_or(true, |(_, most)| shared > most) {
            best = Some((at, shared));
        }
    }
    best.map(|(at, _)| at)
}

/// The display holding most of `frame`, or the one whose centre is nearest.
pub fn nearest_screen(frame: Rect, screens: &[Screen]) -> Option<usize> {
    if let Some(at) = screen_holding(frame, screens) {
        return Some(at);
    }
    let centre = |rect: Rect| (rect.x + rect.width / 2.0, rect.y + rect.height / 2.0);
    let (x, y) = centre(frame);
    screens
        .iter()
        .enumerate()
        .map(|(at, screen)| {
            let (sx, sy) = centre(screen.frame);
            (at, (sx - x).powi(2) + (sy - y).powi(2))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(at, _)| at)
}

/// One window of an app, as the Mac reports it.
pub struct Window<H> {
    pub title: String,
    pub frame: Rect,
    pub minimized: bool,
    pub handle: H,
}

/// A window as FNDR listed it. `id` is valid only in the list it came from:
/// ids keep counting up across lists, so an id from an older list is refused
/// by number alone.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WindowRef {
    pub id: u64,
    pub app: String,
    pub bundle: String,
    pub title: String,
    pub frame: Rect,
    /// Index into the displays listed with it, `None` when off every display.
    pub display: Option<usize>,
    pub minimized: bool,
}

/// What became of one window.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Placed {
    pub id: u64,
    pub app: String,
    pub title: String,
    /// Where FNDR asked the window to go.
    pub asked: Rect,
    /// Where the window says it is afterwards. An app may keep a minimum
    /// size, so this can differ from `asked`.
    pub now: Option<Rect>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LayoutOutcome {
    /// Every window named was tried; each says how it went.
    Arranged(Vec<Placed>),
    /// Nothing was moved.
    Refused(String),
}

/// A window a call names: its id from the latest list and the app it is
/// said to belong to, which must be the app FNDR listed it under.
#[derive(Debug, Clone, Copy)]
pub struct Target<'a> {
    pub id: u64,
    pub app: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Change {
    Move { x: f64, y: f64 },
    Resize { width: f64, height: f64 },
}

struct Listing<H> {
    first: u64,
    windows: Vec<(WindowRef, H)>,
}

/// The latest window list and the frames windows had before FNDR first moved
/// them, for `restore_previous`.
pub struct Windows<H> {
    listing: Option<Listing<H>>,
    next_id: u64,
    previous: Vec<(H, Rect)>,
}

impl<H> Default for Windows<H> {
    fn default() -> Self {
        Self {
            listing: None,
            next_id: 1,
            previous: Vec::new(),
        }
    }
}

/// Why FNDR will not arrange this app's windows: FNDR itself, the blocklist,
/// or an app on the policy's sensitive list.
pub(crate) fn off_limits(app: &AppRef, limits: &Limits) -> Option<String> {
    limits.refusal(app).or_else(|| {
        (crate::operator::policy::is_sensitive_app(&app.name)
            || crate::operator::policy::is_sensitive_app(&app.bundle))
        .then(|| format!("{} is off limits to Notch Do.", app.name))
    })
}

impl<H: Clone + PartialEq> Windows<H> {
    /// Lists the windows of `app`, or of every app FNDR may operate, and
    /// makes this the list later calls name windows from.
    pub fn list<D: Desktop<Handle = H>>(
        &mut self,
        desktop: &mut D,
        limits: &Limits,
        app: Option<&str>,
    ) -> Result<(Vec<WindowRef>, Vec<Screen>), String> {
        let running = desktop.list_apps();
        let apps = match app.map(str::trim).filter(|name| !name.is_empty()) {
            Some(name) => {
                let found = crate::operator::mcp::resolve_app(&running, name, limits)?;
                if let Some(reason) = off_limits(&found, limits) {
                    return Err(reason);
                }
                vec![found]
            }
            None => {
                let mut allowed: Vec<AppRef> = running
                    .into_iter()
                    .filter(|app| off_limits(app, limits).is_none())
                    .collect();
                allowed.dedup_by_key(|app| app.pid);
                allowed
            }
        };
        let screens = desktop.screens();
        let first = self.next_id;
        let mut windows = Vec::new();
        for app in &apps {
            let found = match desktop.windows(app) {
                Ok(found) => found,
                Err(error) if apps.len() == 1 => return Err(error),
                Err(_) => continue,
            };
            for window in found {
                let id = first + windows.len() as u64;
                windows.push((
                    WindowRef {
                        id,
                        app: app.name.clone(),
                        bundle: app.bundle.clone(),
                        title: window.title,
                        frame: window.frame,
                        display: screen_holding(window.frame, &screens),
                        minimized: window.minimized,
                    },
                    window.handle,
                ));
            }
        }
        self.next_id = first + windows.len() as u64;
        let refs = windows.iter().map(|(window, _)| window.clone()).collect();
        self.listing = Some(Listing { first, windows });
        Ok((refs, screens))
    }

    /// Where each target sits in the current list, refused unless every one
    /// is from that list, belongs to the app named and can be moved.
    fn locate(&self, targets: &[Target]) -> Result<Vec<usize>, String> {
        if targets.is_empty() {
            return Err("Name at least one window.".to_string());
        }
        if targets.len() > MAX_WINDOWS {
            return Err(format!("A layout holds at most {MAX_WINDOWS} windows."));
        }
        let listing = self
            .listing
            .as_ref()
            .ok_or("Call list_windows before moving a window.")?;
        let mut found = Vec::with_capacity(targets.len());
        for target in targets {
            let at = target
                .id
                .checked_sub(listing.first)
                .map(|at| at as usize)
                .filter(|at| *at < listing.windows.len())
                .ok_or_else(|| {
                    format!(
                        "Window {} is not in the current list of windows. Call list_windows again.",
                        target.id
                    )
                })?;
            let window = &listing.windows[at].0;
            let app = AppRef {
                pid: 0,
                name: window.app.clone(),
                bundle: window.bundle.clone(),
            };
            if !crate::operator::mcp::app_matches(&app, target.app) {
                return Err(format!(
                    "Window {} belongs to {}, not {}.",
                    target.id,
                    window.app,
                    target.app.trim()
                ));
            }
            if window.minimized {
                return Err(format!("Window {} is minimized.", target.id));
            }
            if found.contains(&at) {
                return Err(format!("Window {} is named twice.", target.id));
            }
            found.push(at);
        }
        Ok(found)
    }

    /// Puts `targets` into `layout`, in order, on `display` (an index into
    /// the displays) or on the display holding the first of them.
    pub fn arrange<D: Desktop<Handle = H>>(
        &mut self,
        desktop: &mut D,
        targets: &[Target],
        layout: Layout,
        display: Option<usize>,
    ) -> LayoutOutcome {
        let found = match self.locate(targets) {
            Ok(found) => found,
            Err(reason) => return LayoutOutcome::Refused(reason),
        };
        let current = match self.current_frames(desktop, &found) {
            Ok(current) => current,
            Err(reason) => return LayoutOutcome::Refused(reason),
        };
        let asked: Vec<Option<Rect>> = if layout == Layout::RestorePrevious {
            let listing = self.listing.as_ref().expect("located");
            found
                .iter()
                .map(|at| {
                    let handle = &listing.windows[*at].1;
                    self.previous
                        .iter()
                        .find(|(seen, _)| seen == handle)
                        .map(|(_, frame)| *frame)
                })
                .collect()
        } else {
            let screens = desktop.screens();
            let screen = match display {
                Some(at) => match screens.get(at) {
                    Some(screen) => *screen,
                    None => {
                        return LayoutOutcome::Refused(format!("There is no display {}.", at + 1))
                    }
                },
                None => match nearest_screen(current[0], &screens) {
                    Some(at) => screens[at],
                    None => return LayoutOutcome::Refused("No display was found.".to_string()),
                },
            };
            match frames(layout, screen.visible, found.len()) {
                Ok(cells) => cells.into_iter().map(Some).collect(),
                Err(reason) => return LayoutOutcome::Refused(reason),
            }
        };
        let placed = found
            .iter()
            .zip(current)
            .zip(asked)
            .map(|((at, before), asked)| match asked {
                Some(asked) => self.set(
                    desktop,
                    *at,
                    before,
                    asked,
                    layout == Layout::RestorePrevious,
                ),
                None => {
                    let window = &self.listing.as_ref().expect("located").windows[*at].0;
                    Placed {
                        id: window.id,
                        app: window.app.clone(),
                        title: window.title.clone(),
                        asked: before,
                        now: Some(before),
                        error: Some(
                            "FNDR has not moved this window, so there is nothing to put back."
                                .to_string(),
                        ),
                    }
                }
            })
            .collect();
        self.listing = None;
        LayoutOutcome::Arranged(placed)
    }

    /// Moves or resizes one window, kept inside the visible part of the
    /// display it lands on.
    pub fn place<D: Desktop<Handle = H>>(
        &mut self,
        desktop: &mut D,
        target: Target,
        change: Change,
    ) -> LayoutOutcome {
        let found = match self.locate(&[target]) {
            Ok(found) => found,
            Err(reason) => return LayoutOutcome::Refused(reason),
        };
        let before = match self.current_frames(desktop, &found) {
            Ok(current) => current[0],
            Err(reason) => return LayoutOutcome::Refused(reason),
        };
        let wanted = match change {
            Change::Move { x, y } => Rect::new(x, y, before.width, before.height),
            Change::Resize { width, height } => Rect::new(before.x, before.y, width, height),
        };
        let numbers = [wanted.x, wanted.y, wanted.width, wanted.height];
        if numbers.iter().any(|n| !n.is_finite()) || wanted.width < 1.0 || wanted.height < 1.0 {
            return LayoutOutcome::Refused(
                "Give whole, positive sizes and a position.".to_string(),
            );
        }
        let screens = desktop.screens();
        let asked = match nearest_screen(wanted, &screens) {
            Some(at) => clamp(wanted, screens[at].visible),
            None => return LayoutOutcome::Refused("No display was found.".to_string()),
        };
        let placed = self.set(desktop, found[0], before, asked, false);
        self.listing = None;
        LayoutOutcome::Arranged(vec![placed])
    }

    /// Each window's frame now; refuses, and drops the list, when one is gone.
    fn current_frames<D: Desktop<Handle = H>>(
        &mut self,
        desktop: &mut D,
        found: &[usize],
    ) -> Result<Vec<Rect>, String> {
        let listing = self.listing.as_ref().expect("located");
        let mut frames = Vec::with_capacity(found.len());
        for at in found {
            let (window, handle) = &listing.windows[*at];
            match desktop.frame(handle) {
                Some(frame) => frames.push(frame),
                None => {
                    let reason = format!("Window {} is gone. Call list_windows again.", window.id);
                    self.listing = None;
                    return Err(reason);
                }
            }
        }
        Ok(frames)
    }

    fn set<D: Desktop<Handle = H>>(
        &mut self,
        desktop: &mut D,
        at: usize,
        before: Rect,
        asked: Rect,
        restoring: bool,
    ) -> Placed {
        let (window, handle) = self.listing.as_ref().expect("located").windows[at].clone();
        let remembered = self.previous.iter().position(|(seen, _)| *seen == handle);
        let result = desktop.set_frame(&handle, asked);
        let now = desktop.frame(&handle);
        let placed = Placed {
            id: window.id,
            app: window.app,
            title: window.title,
            asked,
            now,
            error: result.err(),
        };
        match (restoring, remembered) {
            (true, Some(position)) if placed.error.is_none() => {
                self.previous.remove(position);
            }
            (false, None) => {
                self.previous.push((handle, before));
            }
            _ => {}
        }
        placed
    }
}

static SHARED: once_cell::sync::Lazy<parking_lot::Mutex<Windows<crate::accessibility::AxElement>>> =
    once_cell::sync::Lazy::new(|| parking_lot::Mutex::new(Windows::default()));

/// Lists windows for code inside FNDR, such as a work set that has just
/// opened its documents. `app` narrows the list to one app. The ids are
/// valid for the next `arrange` call only.
///
/// The caller keeps the run's guards: nothing is listed or moved while
/// Private Mode is on or actions are switched off, the same as every Notch Do
/// action (`limits` comes from `Limits::from_settings`).
pub fn list_windows(app: Option<&str>, limits: &Limits) -> Result<Vec<WindowRef>, String> {
    SHARED
        .lock()
        .list(&mut crate::accessibility::AxDesktop, limits, app)
        .map(|(windows, _)| windows)
}

/// Puts the windows from the latest `list_windows` into `layout`, in order,
/// on the display holding the first of them. `RestorePrevious` puts them back
/// where they were before FNDR first moved them.
pub fn arrange(windows: &[WindowRef], layout: Layout) -> LayoutOutcome {
    let targets: Vec<Target> = windows
        .iter()
        .map(|window| Target {
            id: window.id,
            app: &window.app,
        })
        .collect();
    SHARED
        .lock()
        .arrange(&mut crate::accessibility::AxDesktop, &targets, layout, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 1512x982 laptop display with a 33 point menu bar and the Dock at the
    /// bottom, and a 1920x1080 display to its right, its top 200 points higher.
    const LAPTOP: Screen = Screen {
        frame: Rect::new(0.0, 0.0, 1512.0, 982.0),
        visible: Rect::new(0.0, 33.0, 1512.0, 879.0),
    };
    const EXTERNAL: Screen = Screen {
        frame: Rect::new(1512.0, -200.0, 1920.0, 1080.0),
        visible: Rect::new(1512.0, -175.0, 1920.0, 1055.0),
    };

    fn area() -> Rect {
        LAPTOP.visible
    }

    /// Every cell lies inside the area and no two cells overlap.
    fn tiles(cells: &[Rect], area: Rect) {
        for (at, cell) in cells.iter().enumerate() {
            assert!(cell.width > 0.0 && cell.height > 0.0, "{cell:?}");
            assert_eq!(clamp(*cell, area), *cell, "cell {at} leaves the area");
            for other in &cells[at + 1..] {
                assert_eq!(overlap(*cell, *other), 0.0, "{cell:?} overlaps {other:?}");
            }
        }
    }

    #[test]
    fn left_right_split_gives_each_window_a_column_and_the_first_the_left() {
        let two = frames(Layout::LeftRightSplit, area(), 2).unwrap();
        assert_eq!(
            two,
            vec![
                Rect::new(0.0, 33.0, 756.0, 879.0),
                Rect::new(756.0, 33.0, 756.0, 879.0)
            ]
        );
        let one = frames(Layout::LeftRightSplit, area(), 1).unwrap();
        assert_eq!(
            one,
            vec![Rect::new(0.0, 33.0, 756.0, 879.0)],
            "one window takes the left half"
        );
        for count in 3..=MAX_WINDOWS {
            let cells = frames(Layout::LeftRightSplit, area(), count).unwrap();
            assert_eq!(cells.len(), count);
            tiles(&cells, area());
            let width: f64 = cells.iter().map(|cell| cell.width).sum();
            assert_eq!(width, 1512.0, "{count} columns fill the width");
            assert!(cells.windows(2).all(|pair| pair[0].x < pair[1].x));
        }
    }

    #[test]
    fn top_bottom_split_gives_each_window_a_row() {
        let two = frames(Layout::TopBottomSplit, area(), 2).unwrap();
        assert_eq!(
            two,
            vec![
                Rect::new(0.0, 33.0, 1512.0, 439.0),
                Rect::new(0.0, 472.0, 1512.0, 440.0)
            ]
        );
        assert_eq!(
            frames(Layout::TopBottomSplit, area(), 1).unwrap(),
            vec![two[0]]
        );
        for count in 3..=MAX_WINDOWS {
            let cells = frames(Layout::TopBottomSplit, area(), count).unwrap();
            tiles(&cells, area());
            let height: f64 = cells.iter().map(|cell| cell.height).sum();
            assert_eq!(height, 879.0);
        }
    }

    #[test]
    fn thirds_are_three_columns_and_hold_at_most_three_windows() {
        let three = frames(Layout::Thirds, area(), 3).unwrap();
        assert_eq!(
            three,
            vec![
                Rect::new(0.0, 33.0, 504.0, 879.0),
                Rect::new(504.0, 33.0, 504.0, 879.0),
                Rect::new(1008.0, 33.0, 504.0, 879.0)
            ]
        );
        assert_eq!(
            frames(Layout::Thirds, area(), 1).unwrap(),
            three[..1].to_vec()
        );
        assert_eq!(
            frames(Layout::Thirds, area(), 2).unwrap(),
            three[..2].to_vec()
        );
        for count in 4..=MAX_WINDOWS {
            assert!(frames(Layout::Thirds, area(), count).is_err(), "{count}");
        }
    }

    #[test]
    fn a_grid_fills_left_to_right_then_top_to_bottom_and_holds_four() {
        let four = frames(Layout::Grid2x2, area(), 4).unwrap();
        assert_eq!(
            four,
            vec![
                Rect::new(0.0, 33.0, 756.0, 439.0),
                Rect::new(756.0, 33.0, 756.0, 439.0),
                Rect::new(0.0, 472.0, 756.0, 440.0),
                Rect::new(756.0, 472.0, 756.0, 440.0)
            ]
        );
        for count in 1..=3 {
            assert_eq!(
                frames(Layout::Grid2x2, area(), count).unwrap(),
                four[..count].to_vec()
            );
        }
        assert!(frames(Layout::Grid2x2, area(), 5).is_err());
    }

    #[test]
    fn maximize_gives_every_window_the_whole_visible_area() {
        for count in 1..=MAX_WINDOWS {
            let cells = frames(Layout::Maximize, area(), count).unwrap();
            assert_eq!(cells, vec![area(); count]);
        }
    }

    #[test]
    fn no_windows_too_many_or_restore_have_no_geometry() {
        for layout in Layout::ALL {
            assert!(frames(layout, area(), 0).is_err(), "{layout:?}");
            assert!(
                frames(layout, area(), MAX_WINDOWS + 1).is_err(),
                "{layout:?}"
            );
        }
        assert!(frames(Layout::RestorePrevious, area(), 2).is_err());
    }

    #[test]
    fn odd_sizes_split_into_whole_points_that_still_fill_the_area() {
        let odd = Rect::new(10.5, 25.0, 1001.0, 701.0);
        let cells = frames(Layout::LeftRightSplit, odd, 3).unwrap();
        tiles(&cells, odd);
        for cell in &cells {
            assert_eq!(cell.width, cell.width.round());
        }
        assert_eq!(cells.iter().map(|cell| cell.width).sum::<f64>(), 1001.0);
        assert_eq!(cells[2].x + cells[2].width, odd.x + odd.width);
    }

    #[test]
    fn layouts_land_on_a_second_display_in_its_own_coordinates() {
        let cells = frames(Layout::LeftRightSplit, EXTERNAL.visible, 2).unwrap();
        assert_eq!(
            cells,
            vec![
                Rect::new(1512.0, -175.0, 960.0, 1055.0),
                Rect::new(2472.0, -175.0, 960.0, 1055.0)
            ]
        );
    }

    #[test]
    fn clamping_keeps_a_window_inside_the_visible_area() {
        let a = area();
        assert_eq!(
            clamp(Rect::new(100.0, 100.0, 400.0, 300.0), a),
            Rect::new(100.0, 100.0, 400.0, 300.0)
        );
        assert_eq!(
            clamp(Rect::new(-50.0, 0.0, 400.0, 300.0), a),
            Rect::new(0.0, 33.0, 400.0, 300.0),
            "under the menu bar"
        );
        assert_eq!(
            clamp(Rect::new(1400.0, 800.0, 400.0, 300.0), a),
            Rect::new(1112.0, 612.0, 400.0, 300.0),
            "over the Dock"
        );
        assert_eq!(
            clamp(Rect::new(10.0, 40.0, 4000.0, 3000.0), a),
            a,
            "too big"
        );
    }

    #[test]
    fn a_window_belongs_to_the_display_holding_most_of_it() {
        let screens = [LAPTOP, EXTERNAL];
        assert_eq!(
            screen_holding(Rect::new(100.0, 100.0, 400.0, 300.0), &screens),
            Some(0)
        );
        assert_eq!(
            screen_holding(Rect::new(1400.0, 100.0, 400.0, 300.0), &screens),
            Some(1)
        );
        assert_eq!(
            screen_holding(Rect::new(1312.0, 100.0, 400.0, 300.0), &screens),
            Some(0),
            "a tie stays on the first"
        );
        let lost = Rect::new(9000.0, 9000.0, 100.0, 100.0);
        assert_eq!(screen_holding(lost, &screens), None);
        assert_eq!(nearest_screen(lost, &screens), Some(1));
        assert_eq!(
            nearest_screen(Rect::new(-900.0, 400.0, 100.0, 100.0), &screens),
            Some(0)
        );
        assert_eq!(nearest_screen(lost, &[]), None);
    }

    #[test]
    fn layout_names_are_a_closed_set() {
        for layout in Layout::ALL {
            assert_eq!(Layout::parse(layout.name()), Some(layout));
        }
        assert_eq!(Layout::parse(" Grid2x2 "), Some(Layout::Grid2x2));
        assert_eq!(Layout::parse("cascade"), None);
        let parsed: Layout = serde_json::from_str("\"left_right_split\"").unwrap();
        assert_eq!(parsed, Layout::LeftRightSplit);
    }
}
