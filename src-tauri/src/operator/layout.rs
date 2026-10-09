//! Window layout for Notch Do (ADR 026 addendum): where each window of a work
//! set goes on a display, kept apart from the Accessibility calls that move
//! them so the geometry can be tested without a Mac.
//!
//! Coordinates are the Accessibility ones: the top-left corner of the main
//! display is 0,0 and y grows downwards, across every display.

use serde::{Deserialize, Serialize};

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
