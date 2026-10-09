//! FNDR's own computer-use tools as a stdio MCP server (ADR 026, item 4).
//!
//! Started as `fndr operator-mcp` and attached to the Codex session in place
//! of a third-party helper. The tool names and argument shapes are the ones
//! `policy.rs` classifies, so the policy does not change. The server only
//! speaks the protocol and keeps the rules; what touches the Mac sits behind
//! `Desktop` (`accessibility/operate.rs` is the real one).
//!
//! Rules kept here:
//! - An element index belongs to the tree it was printed in. Indexes keep
//!   counting up across trees, so an index from an older tree is refused by
//!   number alone, and any action drops the app's tree so the next one needs
//!   a fresh read.
//! - The element is looked at again just before it is pressed or edited; if it
//!   no longer says what FNDR printed, nothing happens.
//! - FNDR itself and the person's blocklist are never read or operated.
//! - No screenshots: text and structure only.
//! - A window id belongs to the window list it was printed in, the same way an
//!   element index belongs to its tree (`layout.rs`).

use std::collections::HashMap;
use std::io::{BufRead, Write};

use serde_json::{json, Value};

use crate::operator::layout::{
    Change, Layout, LayoutOutcome, Placed, Rect, Screen, Target, Window, WindowRef, Windows,
};

/// A running app, as the tools name it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppRef {
    pub pid: i32,
    pub name: String,
    pub bundle: String,
}

/// One printed line of a tree: how deep it sits and what policy will read
/// ("button Play"), with the handle the Mac needs to act on it.
pub struct Entry<H> {
    pub depth: usize,
    pub description: String,
    pub handle: H,
}

pub struct Tree<H> {
    pub window: String,
    pub entries: Vec<Entry<H>>,
    /// The walk stopped at its node or time limit.
    pub truncated: bool,
}

/// What the Mac lets the server do. Every method acts on exactly what it is
/// given; none of them looks anything up by position or by label.
pub trait Desktop {
    /// An element or a window. Equal handles are the same thing on screen.
    type Handle: Clone + PartialEq;
    fn list_apps(&mut self) -> Vec<AppRef>;
    fn read_tree(&mut self, app: &AppRef) -> Result<Tree<Self::Handle>, String>;
    /// What an element says now, in the form `read_tree` printed it.
    fn describe(&mut self, handle: &Self::Handle) -> Option<String>;
    fn press(&mut self, handle: &Self::Handle) -> Result<(), String>;
    fn set_value(&mut self, handle: &Self::Handle, text: &str) -> Result<(), String>;
    /// Types into `target`, or into whatever has focus in `app` when none is named.
    fn type_text(
        &mut self,
        app: &AppRef,
        target: Option<&Self::Handle>,
        text: &str,
    ) -> Result<(), String>;
    fn press_key(&mut self, app: &AppRef, key: &str) -> Result<(), String>;
    fn scroll(&mut self, app: &AppRef, direction: &str, pages: u32) -> Result<(), String>;
    /// The app's windows, front first.
    fn windows(&mut self, app: &AppRef) -> Result<Vec<Window<Self::Handle>>, String>;
    /// Every display, the main one first.
    fn screens(&mut self) -> Vec<Screen>;
    /// Where a window is now, or `None` when it is gone.
    fn frame(&mut self, window: &Self::Handle) -> Option<Rect>;
    fn set_frame(&mut self, window: &Self::Handle, frame: Rect) -> Result<(), String>;
}

/// Apps the server will not read or operate.
#[derive(Debug, Clone, Default)]
pub struct Limits {
    pub blocklist: Vec<String>,
    pub own_pid: i32,
}

impl Limits {
    /// FNDR's own process and the blocklist from the person's settings.
    pub fn from_settings() -> Result<Limits, String> {
        let config = crate::config::Config::load_or_create()
            .map_err(|error| format!("could not read settings: {error}"))?;
        Ok(Limits {
            blocklist: config
                .blocklist
                .iter()
                .map(|entry| entry.trim().to_string())
                .filter(|entry| !entry.is_empty())
                .collect(),
            own_pid: std::process::id() as i32,
        })
    }

    pub(crate) fn refusal(&self, app: &AppRef) -> Option<String> {
        let off_limits = (self.own_pid > 0 && app.pid == self.own_pid)
            || crate::privacy::Blocklist::is_internal_app(&app.name, Some(&app.bundle))
            || crate::privacy::Blocklist::is_blocked(&app.name, &self.blocklist)
            || (!app.bundle.is_empty()
                && crate::privacy::Blocklist::is_blocked(&app.bundle, &self.blocklist));
        off_limits.then(|| format!("{} is off limits to Notch Do.", app.name))
    }
}

/// The last tree printed for an app: its first index and what each line was.
struct View<H> {
    first: usize,
    lines: Vec<(String, H)>,
}

pub struct Server<D: Desktop> {
    desktop: D,
    limits: Limits,
    views: HashMap<i32, View<D::Handle>>,
    next_index: usize,
    windows: Windows<D::Handle>,
}

/// Runs `fndr operator-mcp`: serves the tools on stdin and stdout until
/// Codex closes the pipe or goes away. Refuses to start when the person's
/// settings cannot be read, because the blocklist would then be unknown.
pub fn run() -> i32 {
    let config = match crate::config::Config::load_or_create() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("fndr operator-mcp: could not read settings: {error}");
            return 1;
        }
    };
    if config.actions_kill_switch {
        eprintln!("fndr operator-mcp: actions are turned off in Settings.");
        return 1;
    }
    let limits = match Limits::from_settings() {
        Ok(limits) => limits,
        Err(error) => {
            eprintln!("fndr operator-mcp: {error}");
            return 1;
        }
    };
    // Dies with its parent even if the pipe stays open (an orphan is
    // re-parented to launchd, pid 1).
    let parent = unsafe { libc::getppid() };
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_secs(2));
        if unsafe { libc::getppid() } != parent {
            std::process::exit(0);
        }
    });
    let mut server = Server::new(crate::accessibility::AxDesktop, limits);
    match server.serve(std::io::stdin().lock(), std::io::stdout()) {
        Ok(()) => 0,
        Err(_) => 1,
    }
}

/// Longest tree text sent to the model. The cut falls between lines.
const MAX_TREE_CHARS: usize = 16_000;

impl<D: Desktop> Server<D> {
    pub fn new(desktop: D, limits: Limits) -> Self {
        Self {
            desktop,
            limits,
            views: HashMap::new(),
            next_index: 0,
            windows: Windows::default(),
        }
    }

    /// Reads JSON-RPC lines until the input closes.
    pub fn serve(&mut self, input: impl BufRead, mut output: impl Write) -> std::io::Result<()> {
        for line in input.lines() {
            let line = line?;
            let Ok(message) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            if let Some(reply) = self.handle(&message) {
                writeln!(output, "{reply}")?;
                output.flush()?;
            }
        }
        Ok(())
    }

    pub fn handle(&mut self, message: &Value) -> Option<Value> {
        let id = message.get("id")?.clone();
        let method = message.get("method").and_then(Value::as_str).unwrap_or("");
        let params = message.get("params").cloned().unwrap_or(Value::Null);
        let result = match method {
            "initialize" => json!({
                "protocolVersion": params
                    .get("protocolVersion")
                    .and_then(Value::as_str)
                    .unwrap_or("2025-06-18"),
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "fndr-operator", "version": env!("CARGO_PKG_VERSION") },
            }),
            "ping" => json!({}),
            "tools/list" => json!({ "tools": tool_definitions() }),
            "tools/call" => {
                let name = params.get("name").and_then(Value::as_str).unwrap_or("");
                let args = params.get("arguments").cloned().unwrap_or(json!({}));
                match self.call(name, &args) {
                    Ok(text) => json!({ "content": [{ "type": "text", "text": text }] }),
                    Err(text) => {
                        json!({ "content": [{ "type": "text", "text": text }], "isError": true })
                    }
                }
            }
            _ => {
                return Some(json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": { "code": -32601, "message": format!("Unknown method {method}") },
                }))
            }
        };
        Some(json!({ "jsonrpc": "2.0", "id": id, "result": result }))
    }

    fn call(&mut self, tool: &str, args: &Value) -> Result<String, String> {
        let text = |key: &str| args.get(key).and_then(Value::as_str);
        match tool {
            "list_apps" => return Ok(self.visible_apps()),
            "list_windows" => return self.list_windows(text("app")),
            "arrange_windows" => return self.arrange_windows(args),
            _ => {}
        }
        let app = self.resolve(text("app").unwrap_or_default())?;
        match tool {
            "move_window" | "resize_window" => self.place_window(tool, &app, args),
            "get_app_state" => self.get_app_state(&app),
            "click" => {
                if args.get("element_index").is_none() {
                    return Err(
                        "click needs an element_index from get_app_state; coordinates are not supported."
                            .to_string(),
                    );
                }
                let at = self.locate(&app, args)?;
                let (printed, handle) = &self.views[&app.pid].lines[at];
                self.desktop.press(handle)?;
                let done = format!("Pressed {printed}.");
                self.after_action(&app, done)
            }
            "set_value" => {
                let value = text("value")
                    .or_else(|| text("text"))
                    .ok_or("set_value needs a value.")?;
                let at = self.locate(&app, args)?;
                let (printed, handle) = &self.views[&app.pid].lines[at];
                refuse_secure(printed)?;
                self.desktop.set_value(handle, value)?;
                let done = format!("Set {printed}.");
                self.after_action(&app, done)
            }
            "type_text" => {
                let value = text("text")
                    .or_else(|| text("value"))
                    .ok_or("type_text needs text.")?;
                if args.get("element_index").is_some() {
                    let at = self.locate(&app, args)?;
                    let (printed, handle) = &self.views[&app.pid].lines[at];
                    refuse_secure(printed)?;
                    self.desktop.type_text(&app, Some(handle), value)?;
                } else {
                    self.desktop.type_text(&app, None, value)?;
                }
                self.after_action(&app, "Typed the text.".to_string())
            }
            "press_key" => {
                let key = text("key").ok_or("press_key needs a key.")?;
                self.desktop.press_key(&app, key)?;
                self.after_action(&app, format!("Pressed {key}."))
            }
            "scroll" => {
                let direction = text("direction").unwrap_or("down").to_lowercase();
                if !["up", "down", "left", "right"].contains(&direction.as_str()) {
                    return Err("direction must be up, down, left or right.".to_string());
                }
                let pages = args
                    .get("pages")
                    .and_then(Value::as_u64)
                    .unwrap_or(1)
                    .clamp(1, 10) as u32;
                self.desktop.scroll(&app, &direction, pages)?;
                self.after_action(&app, format!("Scrolled {direction}."))
            }
            other => Err(format!("FNDR does not provide a tool called {other}.")),
        }
    }

    fn visible_apps(&mut self) -> String {
        let mut lines: Vec<String> = self
            .desktop
            .list_apps()
            .into_iter()
            .filter(|app| self.limits.refusal(app).is_none())
            .map(|app| format!("{} - {}", app.name, app.bundle))
            .collect();
        lines.sort();
        lines.dedup();
        lines.join("\n")
    }

    /// The running app the name means, if FNDR may operate it.
    fn resolve(&mut self, name: &str) -> Result<AppRef, String> {
        let apps = self.desktop.list_apps();
        resolve_app(&apps, name, &self.limits)
    }

    fn get_app_state(&mut self, app: &AppRef) -> Result<String, String> {
        let tree = self.desktop.read_tree(app)?;
        let first = self.next_index;
        let mut out = format!(
            "App={} (pid {})\nWindow: \"{}\", App: {}.\n",
            app.bundle,
            app.pid,
            tree.window.replace('"', "'"),
            app.name
        );
        let mut lines = Vec::with_capacity(tree.entries.len());
        let mut truncated = tree.truncated;
        let mut used = out.chars().count();
        for entry in tree.entries {
            let line = format!(
                "{}{} {}\n",
                "\t".repeat(entry.depth),
                first + lines.len(),
                entry.description
            );
            // Only whole lines are printed, and only printed lines can be used.
            used += line.chars().count();
            if used > MAX_TREE_CHARS {
                truncated = true;
                break;
            }
            out.push_str(&line);
            lines.push((entry.description, entry.handle));
        }
        if truncated {
            out.push_str("(The tree was cut short.)\n");
        }
        self.next_index = first + lines.len();
        self.views.insert(app.pid, View { first, lines });
        Ok(out)
    }

    /// Where `element_index` sits in the app's current tree, refused unless
    /// it is from that tree and the element still says what was printed.
    fn locate(&mut self, app: &AppRef, args: &Value) -> Result<usize, String> {
        let raw = args.get("element_index").ok_or("Name an element_index.")?;
        let index = match raw {
            Value::Number(number) => number.as_u64(),
            Value::String(text) => text.trim().parse().ok(),
            _ => None,
        }
        .ok_or("element_index must be a number from get_app_state.")? as usize;
        let view = self.views.get(&app.pid).ok_or_else(|| {
            format!(
                "Call get_app_state for {} before using an element.",
                app.name
            )
        })?;
        if index < view.first || index >= view.first + view.lines.len() {
            return Err(format!(
                "Element {index} is not in the current view of {}. Call get_app_state again.",
                app.name
            ));
        }
        let at = index - view.first;
        let (printed, handle) = &view.lines[at];
        if self.desktop.describe(handle).as_deref() != Some(printed.as_str()) {
            let message =
                format!("Element {index} no longer reads \"{printed}\". Call get_app_state again.");
            self.views.remove(&app.pid);
            return Err(message);
        }
        Ok(at)
    }

    fn list_windows(&mut self, app: Option<&str>) -> Result<String, String> {
        let (windows, screens) = self.windows.list(&mut self.desktop, &self.limits, app)?;
        let mut out = String::from("Displays:\n");
        for (at, screen) in screens.iter().enumerate() {
            out.push_str(&format!(
                "display {}: {}\n",
                at + 1,
                rect_text(screen.visible)
            ));
        }
        if windows.is_empty() {
            out.push_str("No windows FNDR may arrange are open.\n");
            return Ok(out);
        }
        out.push_str("Windows (ids belong to this list only):\n");
        for window in &windows {
            out.push_str(&window_line(window));
            out.push('\n');
        }
        Ok(out)
    }

    fn arrange_windows(&mut self, args: &Value) -> Result<String, String> {
        let names: Vec<&str> = Layout::ALL.iter().map(|layout| layout.name()).collect();
        let layout = args
            .get("layout")
            .and_then(Value::as_str)
            .and_then(Layout::parse)
            .ok_or_else(|| format!("layout must be one of {}.", names.join(", ")))?;
        let entries = args
            .get("windows")
            .and_then(Value::as_array)
            .ok_or("windows must be a list of {id, app} from list_windows.")?;
        let mut targets = Vec::with_capacity(entries.len());
        for entry in entries {
            let id = window_id(entry.get("id"))?;
            let app = entry
                .get("app")
                .and_then(Value::as_str)
                .filter(|app| !app.trim().is_empty())
                .ok_or("Each window needs the app it belongs to.")?;
            targets.push(Target { id, app });
        }
        let display = match args.get("display") {
            None | Some(Value::Null) => None,
            Some(value) => Some(
                value
                    .as_u64()
                    .and_then(|number| (number as usize).checked_sub(1))
                    .ok_or("display must be a display number from list_windows.")?,
            ),
        };
        let outcome = self
            .windows
            .arrange(&mut self.desktop, &targets, layout, display);
        outcome_text(outcome)
    }

    fn place_window(&mut self, tool: &str, app: &AppRef, args: &Value) -> Result<String, String> {
        if let Some(reason) = crate::operator::layout::off_limits(app, &self.limits) {
            return Err(reason);
        }
        let id = window_id(args.get("window"))?;
        let number = |key: &str| {
            args.get(key)
                .and_then(Value::as_f64)
                .ok_or_else(|| format!("{tool} needs a number for {key}."))
        };
        let change = if tool == "move_window" {
            Change::Move {
                x: number("x")?,
                y: number("y")?,
            }
        } else {
            Change::Resize {
                width: number("width")?,
                height: number("height")?,
            }
        };
        let target = Target { id, app: &app.name };
        let outcome = self.windows.place(&mut self.desktop, target, change);
        outcome_text(outcome)
    }

    fn after_action(&mut self, app: &AppRef, done: String) -> Result<String, String> {
        self.views.remove(&app.pid);
        Ok(format!("{done} Call get_app_state to see the result."))
    }
}

/// How strongly `wanted` names `app`: 3 for its name or bundle id, 2 for the
/// first word of its name, 1 for any word of it, 0 for not at all.
fn match_rank(app: &AppRef, wanted: &str) -> u8 {
    let wanted = wanted.trim().to_lowercase();
    if wanted.is_empty() {
        return 0;
    }
    let rank = |candidate: String| {
        if candidate == wanted {
            3
        } else if candidate.starts_with(&format!("{wanted} ")) {
            2
        } else if candidate.split_whitespace().any(|word| word == wanted) {
            1
        } else {
            0
        }
    };
    rank(app.name.to_lowercase()).max(rank(app.bundle.to_lowercase()))
}

/// Whether `wanted` names `app` the way a tool call may name it.
pub(crate) fn app_matches(app: &AppRef, wanted: &str) -> bool {
    match_rank(app, wanted) > 0
}

/// The running app `name` means, if FNDR may operate it. An exact name or
/// bundle id wins over a first word, which wins over any word.
pub(crate) fn resolve_app(apps: &[AppRef], name: &str, limits: &Limits) -> Result<AppRef, String> {
    if name.trim().is_empty() {
        return Err("Name the app.".to_string());
    }
    let found = (1..=3)
        .rev()
        .find_map(|wanted| apps.iter().find(|app| match_rank(app, name) == wanted))
        .cloned()
        .ok_or_else(|| format!("{name} is not running. Open it first."))?;
    match limits.refusal(&found) {
        Some(reason) => Err(reason),
        None => Ok(found),
    }
}

fn refuse_secure(description: &str) -> Result<(), String> {
    let lower = description.to_lowercase();
    let secure = lower.contains("secure text field")
        || lower
            .split(|c: char| !c.is_alphanumeric())
            .any(|word| word == "password" || word == "passcode");
    if secure {
        Err("Credentials are never entered.".to_string())
    } else {
        Ok(())
    }
}

fn rect_text(rect: Rect) -> String {
    // Adding zero turns -0 into 0.
    let point = |n: f64| format!("{:.0}", n.round() + 0.0);
    format!(
        "{},{} {}x{}",
        point(rect.x),
        point(rect.y),
        point(rect.width),
        point(rect.height)
    )
}

/// A title on one short line, with nothing that could close its quotes.
fn quoted(title: &str) -> String {
    let flat: String = title
        .chars()
        .map(|c| match c {
            '"' => '\'',
            c if c.is_control() => ' ',
            c => c,
        })
        .collect();
    let words = flat.split_whitespace().collect::<Vec<_>>().join(" ");
    let cut: String = words.chars().take(80).collect();
    format!("\"{cut}\"")
}

fn window_line(window: &WindowRef) -> String {
    let place = match window.display {
        Some(at) => format!("display {}", at + 1),
        None => "off screen".to_string(),
    };
    let minimized = if window.minimized { " minimized" } else { "" };
    format!(
        "window {} {} {} {} {place}{minimized}",
        window.id,
        window.app,
        quoted(&window.title),
        rect_text(window.frame)
    )
}

fn window_id(value: Option<&Value>) -> Result<u64, String> {
    match value {
        Some(Value::Number(number)) => number.as_u64(),
        Some(Value::String(text)) => text.trim().parse().ok(),
        _ => None,
    }
    .ok_or_else(|| "Name a window by its id from list_windows.".to_string())
}

fn placed_line(placed: &Placed) -> String {
    let head = format!(
        "window {} {} {}",
        placed.id,
        placed.app,
        quoted(&placed.title)
    );
    if let Some(error) = &placed.error {
        return format!("{head} not moved: {error}");
    }
    let Some(now) = placed.now else {
        return format!("{head} moved, then could not be read back");
    };
    let off = [
        now.x - placed.asked.x,
        now.y - placed.asked.y,
        now.width - placed.asked.width,
        now.height - placed.asked.height,
    ]
    .iter()
    .any(|delta| delta.abs() >= 1.0);
    if off {
        format!(
            "{head} now {} (asked for {})",
            rect_text(now),
            rect_text(placed.asked)
        )
    } else {
        format!("{head} now {}", rect_text(now))
    }
}

fn outcome_text(outcome: LayoutOutcome) -> Result<String, String> {
    let placed = match outcome {
        LayoutOutcome::Refused(reason) => return Err(reason),
        LayoutOutcome::Arranged(placed) => placed,
    };
    let lines: Vec<String> = placed.iter().map(placed_line).collect();
    let text = format!(
        "{}\nCall list_windows again before the next window change.",
        lines.join("\n")
    );
    if placed.iter().all(|placed| placed.error.is_some()) {
        Err(text)
    } else {
        Ok(text)
    }
}

fn tool_definitions() -> Value {
    let app = json!({ "type": "string", "description": "App name or bundle id." });
    let element =
        json!({ "type": "string", "description": "element_index from the latest get_app_state." });
    let window =
        json!({ "type": "integer", "description": "Window id from the latest list_windows." });
    let layouts: Vec<&str> = Layout::ALL.iter().map(|layout| layout.name()).collect();
    json!([
        {
            "name": "list_apps",
            "description": "Lists running apps.",
            "inputSchema": { "type": "object", "properties": {} },
        },
        {
            "name": "get_app_state",
            "description": "Reads an app's front window as indexed text lines. Indexes belong to this reading only.",
            "inputSchema": { "type": "object", "properties": { "app": app }, "required": ["app"] },
        },
        {
            "name": "click",
            "description": "Presses the element with this index.",
            "inputSchema": { "type": "object", "properties": { "app": app, "element_index": element }, "required": ["app", "element_index"] },
        },
        {
            "name": "set_value",
            "description": "Sets the text of the text field with this index.",
            "inputSchema": { "type": "object", "properties": { "app": app, "element_index": element, "value": { "type": "string" } }, "required": ["app", "element_index", "value"] },
        },
        {
            "name": "type_text",
            "description": "Types text into the focused field of the app, or into element_index when given.",
            "inputSchema": { "type": "object", "properties": { "app": app, "text": { "type": "string" }, "element_index": element }, "required": ["app", "text"] },
        },
        {
            "name": "press_key",
            "description": "Sends a key or shortcut such as Return, Tab, space or cmd+l.",
            "inputSchema": { "type": "object", "properties": { "app": app, "key": { "type": "string" } }, "required": ["app", "key"] },
        },
        {
            "name": "scroll",
            "description": "Scrolls the front window by pages.",
            "inputSchema": { "type": "object", "properties": { "app": app, "direction": { "type": "string", "enum": ["up", "down", "left", "right"] }, "pages": { "type": "integer" } }, "required": ["app", "direction"] },
        },
        {
            "name": "list_windows",
            "description": "Lists open windows with ids, frames and displays, for one app or every app. Ids belong to this list only.",
            "inputSchema": { "type": "object", "properties": { "app": app } },
        },
        {
            "name": "arrange_windows",
            "description": "Arranges up to six windows, in order, on one display. left_right_split puts the first on the left. restore_previous puts windows back where they were before FNDR moved them.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "layout": { "type": "string", "enum": layouts },
                    "windows": {
                        "type": "array",
                        "maxItems": crate::operator::layout::MAX_WINDOWS,
                        "items": { "type": "object", "properties": { "id": window, "app": app }, "required": ["id", "app"] },
                    },
                    "display": { "type": "integer", "description": "Display number from list_windows. Defaults to the first window's display." },
                },
                "required": ["layout", "windows"],
            },
        },
        {
            "name": "move_window",
            "description": "Moves a window's top-left corner to x, y in screen points. It is kept on the visible part of a display.",
            "inputSchema": { "type": "object", "properties": { "app": app, "window": window, "x": { "type": "number" }, "y": { "type": "number" } }, "required": ["app", "window", "x", "y"] },
        },
        {
            "name": "resize_window",
            "description": "Resizes a window in screen points. It is kept on the visible part of its display.",
            "inputSchema": { "type": "object", "properties": { "app": app, "window": window, "width": { "type": "number" }, "height": { "type": "number" } }, "required": ["app", "window", "width", "height"] },
        },
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operator::policy::{classify, Observed, Risk};
    use std::cell::RefCell;
    use std::rc::Rc;

    /// A scripted Mac: one app whose lines can be swapped between reads, and a
    /// log of everything done to it.
    #[derive(Clone)]
    struct Fake {
        apps: Vec<AppRef>,
        lines: Rc<RefCell<Vec<&'static str>>>,
        log: Rc<RefCell<Vec<String>>>,
        windows: Rc<RefCell<Vec<FakeWindow>>>,
    }

    #[derive(Clone)]
    struct FakeWindow {
        pid: i32,
        title: &'static str,
        frame: Rect,
        minimized: bool,
        open: bool,
    }

    /// A laptop display and, to its right and 200 points higher, an external one.
    const LAPTOP: Screen = Screen {
        frame: Rect::new(0.0, 0.0, 1512.0, 982.0),
        visible: Rect::new(0.0, 33.0, 1512.0, 879.0),
    };
    const EXTERNAL: Screen = Screen {
        frame: Rect::new(1512.0, -200.0, 1920.0, 1080.0),
        visible: Rect::new(1512.0, -175.0, 1920.0, 1055.0),
    };

    impl Fake {
        fn new(lines: Vec<&'static str>) -> Self {
            let app = |pid, name: &str, bundle: &str| AppRef {
                pid,
                name: name.into(),
                bundle: bundle.into(),
            };
            Self {
                apps: vec![
                    app(10, "Safari", "com.apple.Safari"),
                    app(11, "FNDR", "com.fndr.app"),
                    app(12, "Vault", "com.example.vault"),
                    app(13, "Spotify", "com.spotify.client"),
                    app(14, "TextEdit", "com.apple.TextEdit"),
                    app(15, "Preview", "com.apple.Preview"),
                    app(16, "1Password 7", "com.agilebits.onepassword7"),
                ],
                lines: Rc::new(RefCell::new(lines)),
                log: Rc::new(RefCell::new(Vec::new())),
                windows: Rc::new(RefCell::new(
                    [
                        (
                            14,
                            "Essay.txt",
                            Rect::new(100.0, 100.0, 600.0, 500.0),
                            false,
                        ),
                        (
                            14,
                            "Notes \"draft\"\n2",
                            Rect::new(200.0, 150.0, 500.0, 400.0),
                            false,
                        ),
                        (
                            15,
                            "Reading.pdf",
                            Rect::new(2000.0, 0.0, 800.0, 900.0),
                            false,
                        ),
                        (15, "Old.pdf", Rect::new(0.0, 0.0, 400.0, 400.0), true),
                        (11, "FNDR", Rect::new(0.0, 0.0, 400.0, 400.0), false),
                        (12, "Vault", Rect::new(0.0, 0.0, 400.0, 400.0), false),
                        (16, "1Password", Rect::new(0.0, 0.0, 400.0, 400.0), false),
                    ]
                    .into_iter()
                    .map(|(pid, title, frame, minimized)| FakeWindow {
                        pid,
                        title,
                        frame,
                        minimized,
                        open: true,
                    })
                    .collect(),
                )),
            }
        }
        fn done(&self) -> Vec<String> {
            self.log.borrow().clone()
        }
    }

    impl Desktop for Fake {
        type Handle = usize;
        fn list_apps(&mut self) -> Vec<AppRef> {
            self.apps.clone()
        }
        fn read_tree(&mut self, _: &AppRef) -> Result<Tree<usize>, String> {
            Ok(Tree {
                window: "Fixture".into(),
                entries: self
                    .lines
                    .borrow()
                    .iter()
                    .enumerate()
                    .map(|(position, line)| Entry {
                        depth: usize::from(position > 0),
                        description: (*line).to_string(),
                        handle: position,
                    })
                    .collect(),
                truncated: false,
            })
        }
        fn describe(&mut self, handle: &usize) -> Option<String> {
            self.lines
                .borrow()
                .get(*handle)
                .map(|line| line.to_string())
        }
        fn press(&mut self, handle: &usize) -> Result<(), String> {
            self.log.borrow_mut().push(format!("press {handle}"));
            Ok(())
        }
        fn set_value(&mut self, handle: &usize, text: &str) -> Result<(), String> {
            self.log.borrow_mut().push(format!("set {handle} {text}"));
            Ok(())
        }
        fn type_text(
            &mut self,
            _: &AppRef,
            target: Option<&usize>,
            text: &str,
        ) -> Result<(), String> {
            self.log
                .borrow_mut()
                .push(format!("type {target:?} {text}"));
            Ok(())
        }
        fn press_key(&mut self, _: &AppRef, key: &str) -> Result<(), String> {
            self.log.borrow_mut().push(format!("key {key}"));
            Ok(())
        }
        fn scroll(&mut self, _: &AppRef, direction: &str, pages: u32) -> Result<(), String> {
            self.log
                .borrow_mut()
                .push(format!("scroll {direction} {pages}"));
            Ok(())
        }
        fn windows(&mut self, app: &AppRef) -> Result<Vec<Window<usize>>, String> {
            Ok(self
                .windows
                .borrow()
                .iter()
                .enumerate()
                .filter(|(_, window)| window.pid == app.pid && window.open)
                .map(|(handle, window)| Window {
                    title: window.title.to_string(),
                    frame: window.frame,
                    minimized: window.minimized,
                    handle,
                })
                .collect())
        }
        fn screens(&mut self) -> Vec<Screen> {
            vec![LAPTOP, EXTERNAL]
        }
        fn frame(&mut self, window: &usize) -> Option<Rect> {
            let windows = self.windows.borrow();
            windows
                .get(*window)
                .filter(|window| window.open)
                .map(|window| window.frame)
        }
        fn set_frame(&mut self, window: &usize, frame: Rect) -> Result<(), String> {
            self.log
                .borrow_mut()
                .push(format!("frame {window} {}", rect_text(frame)));
            self.windows.borrow_mut()[*window].frame = frame;
            Ok(())
        }
    }

    const SHOP: [&str; 6] = [
        "standard window Shop",
        "search field Search products",
        "link Next page",
        "button Add to cart",
        "button Buy now",
        "secure text field Password",
    ];

    fn server(lines: Vec<&'static str>) -> (Server<Fake>, Fake) {
        let fake = Fake::new(lines);
        let limits = Limits {
            blocklist: vec!["vault".into()],
            own_pid: 11,
        };
        (Server::new(fake.clone(), limits), fake)
    }

    fn call(server: &mut Server<Fake>, tool: &str, args: Value) -> Result<String, String> {
        server.call(tool, &args)
    }

    #[test]
    fn answers_the_protocol_calls_codex_makes() {
        let (mut server, _) = server(SHOP.to_vec());
        let init = server
            .handle(&json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}))
            .unwrap();
        assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
        assert!(init["result"]["capabilities"]["tools"].is_object());
        assert!(server
            .handle(&json!({"jsonrpc":"2.0","method":"notifications/initialized"}))
            .is_none());
        let listed = server
            .handle(&json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}))
            .unwrap();
        let names: Vec<&str> = listed["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|tool| tool["name"].as_str())
            .collect();
        for expected in [
            "get_app_state",
            "click",
            "set_value",
            "type_text",
            "press_key",
            "scroll",
            "list_apps",
        ] {
            assert!(names.contains(&expected), "missing {expected}");
        }
        let unknown = server
            .handle(&json!({"jsonrpc":"2.0","id":3,"method":"nope"}))
            .unwrap();
        assert_eq!(unknown["error"]["code"], -32601);
    }

    #[test]
    fn serves_over_a_pipe_and_stops_when_input_closes() {
        let (mut server, _) = server(SHOP.to_vec());
        let input = b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\nnot json\n";
        let mut out = Vec::new();
        server.serve(&input[..], &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert_eq!(text.lines().count(), 1);
        assert!(text.contains("\"id\":1"));
    }

    #[test]
    fn prints_a_tree_policy_can_read_and_classify() {
        let (mut server, _) = server(SHOP.to_vec());
        let tree = call(&mut server, "get_app_state", json!({"app": "Safari"})).unwrap();
        assert!(
            tree.starts_with("App=com.apple.Safari (pid 10)\nWindow: \"Fixture\", App: Safari.\n")
        );
        assert!(tree.contains("\n0 standard window Shop\n\t1 search field Search products\n"));
        let mut observed = Observed::default();
        observed.observe_tree("Safari", &tree);
        assert_eq!(
            observed.describe("Safari", "5").as_deref(),
            Some("secure text field password")
        );
        let verdict = |tool: &str, args: Value| classify(tool, &args, &observed).risk;
        assert_eq!(
            verdict("click", json!({"app":"Safari","element_index":"2"})),
            Risk::Runs
        );
        assert_eq!(
            verdict("click", json!({"app":"Safari","element_index":"3"})),
            Risk::Confirm
        );
        assert_eq!(
            verdict("click", json!({"app":"Safari","element_index":"4"})),
            Risk::Never
        );
        assert_eq!(
            verdict(
                "set_value",
                json!({"app":"Safari","element_index":"5","value":"x"})
            ),
            Risk::Never
        );
    }

    #[test]
    fn clicking_presses_exactly_the_element_that_was_printed() {
        let (mut server, fake) = server(SHOP.to_vec());
        call(&mut server, "get_app_state", json!({"app": "safari"})).unwrap();
        let done = call(
            &mut server,
            "click",
            json!({"app": "Safari", "element_index": "2"}),
        )
        .unwrap();
        assert!(done.starts_with("Pressed link Next page."));
        assert_eq!(fake.done(), vec!["press 2"]);
    }

    #[test]
    fn an_index_from_an_older_tree_is_refused() {
        let (mut server, fake) = server(SHOP.to_vec());
        call(&mut server, "get_app_state", json!({"app": "Safari"})).unwrap();
        let second = call(&mut server, "get_app_state", json!({"app": "Safari"})).unwrap();
        assert!(
            second.contains("\n6 standard window Shop\n"),
            "indexes keep counting"
        );
        let refused = call(
            &mut server,
            "click",
            json!({"app": "Safari", "element_index": "2"}),
        )
        .unwrap_err();
        assert!(refused.contains("not in the current view"), "{refused}");
        assert!(fake.done().is_empty());
    }

    #[test]
    fn any_action_drops_the_tree_so_the_next_one_needs_a_new_read() {
        let (mut server, fake) = server(SHOP.to_vec());
        call(&mut server, "get_app_state", json!({"app": "Safari"})).unwrap();
        call(
            &mut server,
            "click",
            json!({"app": "Safari", "element_index": 2}),
        )
        .unwrap();
        let refused = call(
            &mut server,
            "click",
            json!({"app": "Safari", "element_index": 3}),
        )
        .unwrap_err();
        assert!(refused.contains("before using an element"), "{refused}");
        assert_eq!(fake.done(), vec!["press 2"]);
    }

    #[test]
    fn a_button_that_moved_since_it_was_printed_is_not_pressed() {
        let (mut server, fake) = server(vec![
            "standard window",
            "button Play",
            "button Delete account",
        ]);
        call(&mut server, "get_app_state", json!({"app": "Safari"})).unwrap();
        fake.lines.borrow_mut().swap(1, 2);
        let refused = call(
            &mut server,
            "click",
            json!({"app": "Safari", "element_index": "1"}),
        )
        .unwrap_err();
        assert!(
            refused.contains("no longer reads \"button Play\""),
            "{refused}"
        );
        assert!(fake.done().is_empty());
    }

    #[test]
    fn coordinates_and_junk_indexes_are_refused() {
        let (mut server, fake) = server(SHOP.to_vec());
        call(&mut server, "get_app_state", json!({"app": "Safari"})).unwrap();
        let coords = call(
            &mut server,
            "click",
            json!({"app": "Safari", "x": 5, "y": 5}),
        )
        .unwrap_err();
        assert!(coords.contains("coordinates are not supported"));
        assert!(call(
            &mut server,
            "click",
            json!({"app": "Safari", "element_index": "two"})
        )
        .is_err());
        assert!(call(
            &mut server,
            "click",
            json!({"app": "Safari", "element_index": "99"})
        )
        .is_err());
        assert!(fake.done().is_empty());
    }

    #[test]
    fn fndr_and_blocklisted_apps_are_neither_read_nor_operated() {
        let (mut server, fake) = server(SHOP.to_vec());
        for app in ["FNDR", "com.fndr.app", "Vault"] {
            for tool in ["get_app_state", "press_key", "type_text"] {
                let err = call(
                    &mut server,
                    tool,
                    json!({"app": app, "key": "a", "text": "a"}),
                )
                .unwrap_err();
                assert!(err.contains("off limits"), "{app} {tool}: {err}");
            }
        }
        let listed = call(&mut server, "list_apps", json!({})).unwrap();
        assert!(listed.contains("Safari") && !listed.contains("FNDR") && !listed.contains("Vault"));
        assert!(fake.done().is_empty());
    }

    #[test]
    fn credentials_are_never_entered_even_if_policy_were_bypassed() {
        let (mut server, fake) = server(SHOP.to_vec());
        call(&mut server, "get_app_state", json!({"app": "Safari"})).unwrap();
        let err = call(
            &mut server,
            "set_value",
            json!({"app": "Safari", "element_index": 5, "value": "hunter2"}),
        )
        .unwrap_err();
        assert!(err.contains("Credentials"));
        let err = call(
            &mut server,
            "type_text",
            json!({"app": "Safari", "element_index": 5, "text": "hunter2"}),
        )
        .unwrap_err();
        assert!(err.contains("Credentials"));
        assert!(fake.done().is_empty());
    }

    #[test]
    fn a_long_tree_is_cut_between_lines_and_unprinted_lines_cannot_be_used() {
        let long: &'static str = Box::leak(format!("button {}", "x".repeat(90)).into_boxed_str());
        let (mut server, fake) = server(vec![long; 400]);
        let tree = call(&mut server, "get_app_state", json!({"app": "Safari"})).unwrap();
        assert!(tree.chars().count() <= MAX_TREE_CHARS + 40);
        assert!(tree.ends_with("(The tree was cut short.)\n"), "{tree}");
        let printed = tree
            .lines()
            .filter(|line| line.trim_start().starts_with(|c: char| c.is_ascii_digit()))
            .count();
        assert!(printed < 400);
        for line in tree.lines().skip(2) {
            assert!(
                line.ends_with(&"x".repeat(90)) || line.starts_with('('),
                "cut line: {line}"
            );
        }
        let refused = call(
            &mut server,
            "click",
            json!({"app": "Safari", "element_index": printed}),
        )
        .unwrap_err();
        assert!(refused.contains("not in the current view"), "{refused}");
        assert!(fake.done().is_empty());
    }

    #[test]
    fn typing_keys_and_scrolling_reach_the_named_app() {
        let (mut server, fake) = server(SHOP.to_vec());
        call(&mut server, "get_app_state", json!({"app": "Safari"})).unwrap();
        call(
            &mut server,
            "set_value",
            json!({"app": "Safari", "element_index": 1, "value": "shoes"}),
        )
        .unwrap();
        call(
            &mut server,
            "type_text",
            json!({"app": "Safari", "text": "more"}),
        )
        .unwrap();
        call(
            &mut server,
            "press_key",
            json!({"app": "Safari", "key": "Return"}),
        )
        .unwrap();
        call(
            &mut server,
            "scroll",
            json!({"app": "Safari", "direction": "down", "pages": 50}),
        )
        .unwrap();
        assert_eq!(
            fake.done(),
            vec![
                "set 1 shoes",
                "type None more",
                "key Return",
                "scroll down 10"
            ]
        );
        assert!(call(
            &mut server,
            "scroll",
            json!({"app": "Safari", "direction": "sideways"})
        )
        .is_err());
        assert!(call(&mut server, "drag", json!({"app": "Safari"})).is_err());
        assert!(
            call(&mut server, "get_app_state", json!({"app": "Photoshop"}))
                .unwrap_err()
                .contains("not running")
        );
    }

    fn frame_of(fake: &Fake, handle: usize) -> String {
        rect_text(fake.windows.borrow()[handle].frame)
    }

    /// The id printed for the window titled `title` in a list_windows text.
    fn id_in(list: &str, title: &str) -> u64 {
        list.lines()
            .find(|line| line.contains(&format!("\"{title}\"")))
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|id| id.parse().ok())
            .unwrap_or_else(|| panic!("{title} not in {list}"))
    }

    #[test]
    fn lists_windows_of_apps_fndr_may_operate_with_ids_frames_and_displays() {
        let (mut server, _) = server(SHOP.to_vec());
        let list = call(&mut server, "list_windows", json!({})).unwrap();
        assert!(
            list.starts_with(
                "Displays:\ndisplay 1: 0,33 1512x879\ndisplay 2: 1512,-175 1920x1055\n"
            ),
            "{list}"
        );
        assert!(
            list.contains("window 1 TextEdit \"Essay.txt\" 100,100 600x500 display 1\n"),
            "{list}"
        );
        assert!(
            list.contains("TextEdit \"Notes 'draft' 2\" 200,150 500x400 display 1"),
            "{list}"
        );
        assert!(
            list.contains("Preview \"Reading.pdf\" 2000,0 800x900 display 2\n"),
            "{list}"
        );
        assert!(
            list.contains("\"Old.pdf\" 0,0 400x400 display 1 minimized"),
            "{list}"
        );
        for hidden in ["FNDR", "Vault", "1Password"] {
            assert!(
                !list.contains(&format!("\"{hidden}\"")),
                "{hidden} listed: {list}"
            );
        }
        let one = call(&mut server, "list_windows", json!({"app": "preview"})).unwrap();
        assert!(
            one.contains("Reading.pdf") && !one.contains("Essay.txt"),
            "{one}"
        );
        for app in ["FNDR", "Vault", "1Password"] {
            let refused = call(&mut server, "list_windows", json!({"app": app})).unwrap_err();
            assert!(refused.contains("off limits"), "{app}: {refused}");
        }
    }

    #[test]
    fn arranges_a_doc_left_and_a_pdf_right_then_puts_them_back() {
        let (mut server, fake) = server(SHOP.to_vec());
        let list = call(&mut server, "list_windows", json!({})).unwrap();
        let (essay, pdf) = (id_in(&list, "Essay.txt"), id_in(&list, "Reading.pdf"));
        let done = call(
            &mut server,
            "arrange_windows",
            json!({"layout": "left_right_split", "windows": [{"id": essay, "app": "TextEdit"}, {"id": pdf, "app": "Preview"}]}),
        )
        .unwrap();
        assert!(done.contains("\"Essay.txt\" now 0,33 756x879"), "{done}");
        assert_eq!(rect_text(Rect::new(-0.0, -0.4, 10.0, 10.0)), "0,0 10x10");
        assert!(
            done.contains("\"Reading.pdf\" now 756,33 756x879"),
            "laid out on the first window's display: {done}"
        );
        assert_eq!(
            (frame_of(&fake, 0), frame_of(&fake, 2)),
            ("0,33 756x879".into(), "756,33 756x879".into())
        );

        // A second layout still restores to where FNDR found them.
        let list = call(&mut server, "list_windows", json!({})).unwrap();
        let (essay, pdf) = (id_in(&list, "Essay.txt"), id_in(&list, "Reading.pdf"));
        call(
            &mut server,
            "arrange_windows",
            json!({"layout": "top_bottom_split", "windows": [{"id": pdf, "app": "Preview"}, {"id": essay, "app": "TextEdit"}], "display": 2}),
        )
        .unwrap();
        assert_eq!(frame_of(&fake, 2), "1512,-175 1920x527");
        assert_eq!(frame_of(&fake, 0), "1512,352 1920x528");

        let list = call(&mut server, "list_windows", json!({})).unwrap();
        let (essay, pdf) = (id_in(&list, "Essay.txt"), id_in(&list, "Reading.pdf"));
        let back = json!({"layout": "restore_previous", "windows": [{"id": essay, "app": "TextEdit"}, {"id": pdf, "app": "Preview"}]});
        call(&mut server, "arrange_windows", back.clone()).unwrap();
        assert_eq!(
            (frame_of(&fake, 0), frame_of(&fake, 2)),
            ("100,100 600x500".into(), "2000,0 800x900".into())
        );

        let list = call(&mut server, "list_windows", json!({})).unwrap();
        let essay = id_in(&list, "Essay.txt");
        let again = call(
            &mut server,
            "arrange_windows",
            json!({"layout": "restore_previous", "windows": [{"id": essay, "app": "TextEdit"}]}),
        )
        .unwrap_err();
        assert!(again.contains("nothing to put back"), "{again}");
    }

    #[test]
    fn a_window_id_from_an_older_list_or_another_app_is_refused() {
        let (mut server, fake) = server(SHOP.to_vec());
        let arrange = |server: &mut Server<Fake>, windows: Value| {
            call(
                server,
                "arrange_windows",
                json!({"layout": "maximize", "windows": windows}),
            )
        };
        let none = arrange(&mut server, json!([{"id": 1, "app": "TextEdit"}])).unwrap_err();
        assert!(none.contains("list_windows"), "{none}");

        let old = call(&mut server, "list_windows", json!({})).unwrap();
        let new = call(&mut server, "list_windows", json!({})).unwrap();
        let stale = id_in(&old, "Essay.txt");
        assert_ne!(stale, id_in(&new, "Essay.txt"), "ids keep counting");
        let refused = arrange(&mut server, json!([{"id": stale, "app": "TextEdit"}])).unwrap_err();
        assert!(refused.contains("not in the current list"), "{refused}");
        assert!(arrange(&mut server, json!([{"id": 999, "app": "TextEdit"}])).is_err());

        let essay = id_in(&new, "Essay.txt");
        let other = arrange(&mut server, json!([{"id": essay, "app": "Preview"}])).unwrap_err();
        assert!(other.contains("belongs to TextEdit"), "{other}");
        let twice = arrange(
            &mut server,
            json!([{"id": essay, "app": "TextEdit"}, {"id": essay, "app": "TextEdit"}]),
        )
        .unwrap_err();
        assert!(twice.contains("twice"), "{twice}");
        let minimized = arrange(
            &mut server,
            json!([{"id": id_in(&new, "Old.pdf"), "app": "Preview"}]),
        )
        .unwrap_err();
        assert!(minimized.contains("minimized"), "{minimized}");
        let seven: Vec<Value> = (0..7)
            .map(|_| json!({"id": essay, "app": "TextEdit"}))
            .collect();
        assert!(arrange(&mut server, Value::Array(seven))
            .unwrap_err()
            .contains("at most 6"));
        let no_app = arrange(&mut server, json!([{"id": essay}])).unwrap_err();
        assert!(no_app.contains("app it belongs to"), "{no_app}");
        let unknown = call(
            &mut server,
            "arrange_windows",
            json!({"layout": "cascade", "windows": [{"id": essay, "app": "TextEdit"}]}),
        )
        .unwrap_err();
        assert!(unknown.contains("left_right_split"), "{unknown}");
        let thirds = call(&mut server, "arrange_windows", json!({"layout": "grid2x2", "windows": [{"id": essay, "app": "TextEdit"}], "display": 3})).unwrap_err();
        assert!(thirds.contains("no display 3"), "{thirds}");

        // A window closed since the list is refused, not guessed.
        fake.windows.borrow_mut()[0].open = false;
        let gone = arrange(&mut server, json!([{"id": essay, "app": "TextEdit"}])).unwrap_err();
        assert!(gone.contains("is gone"), "{gone}");
        assert!(fake.done().is_empty(), "nothing moved: {:?}", fake.done());

        // Any change drops the list, like an element tree.
        fake.windows.borrow_mut()[0].open = true;
        let list = call(&mut server, "list_windows", json!({})).unwrap();
        let essay = id_in(&list, "Essay.txt");
        arrange(&mut server, json!([{"id": essay, "app": "TextEdit"}])).unwrap();
        let after = arrange(&mut server, json!([{"id": essay, "app": "TextEdit"}])).unwrap_err();
        assert!(after.contains("list_windows"), "{after}");
    }

    #[test]
    fn off_limits_windows_cannot_be_named_even_by_guessing_an_id() {
        let (mut server, fake) = server(SHOP.to_vec());
        let list = call(&mut server, "list_windows", json!({})).unwrap();
        let ids: Vec<u64> = list
            .lines()
            .filter(|line| line.starts_with("window "))
            .filter_map(|line| line.split_whitespace().nth(1)?.parse().ok())
            .collect();
        let next = ids.iter().max().unwrap() + 1;
        for app in ["FNDR", "Vault", "1Password 7"] {
            let refused = call(
                &mut server,
                "arrange_windows",
                json!({"layout": "maximize", "windows": [{"id": next, "app": app}]}),
            )
            .unwrap_err();
            assert!(
                refused.contains("not in the current list"),
                "{app}: {refused}"
            );
            let moved = call(
                &mut server,
                "move_window",
                json!({"app": app, "window": next, "x": 0, "y": 0}),
            )
            .unwrap_err();
            assert!(moved.contains("off limits"), "{app}: {moved}");
        }
        assert!(fake.done().is_empty());
    }

    #[test]
    fn moving_and_resizing_keep_a_window_on_the_visible_part_of_a_display() {
        let (mut server, fake) = server(SHOP.to_vec());
        let mut step = |tool: &str, args: Value| {
            let list = call(&mut server, "list_windows", json!({"app": "TextEdit"})).unwrap();
            let mut args = args;
            args["window"] = json!(id_in(&list, "Essay.txt"));
            args["app"] = json!("TextEdit");
            call(&mut server, tool, args)
        };
        step("move_window", json!({"x": 300, "y": 200})).unwrap();
        assert_eq!(frame_of(&fake, 0), "300,200 600x500");
        step("move_window", json!({"x": -400, "y": -50})).unwrap();
        assert_eq!(
            frame_of(&fake, 0),
            "0,33 600x500",
            "under the menu bar and off the left edge"
        );
        step("move_window", json!({"x": 1400, "y": 800})).unwrap();
        assert_eq!(
            frame_of(&fake, 0),
            "1512,380 600x500",
            "most of it lands on the second display"
        );
        step("resize_window", json!({"width": 5000, "height": 5000})).unwrap();
        assert_eq!(frame_of(&fake, 0), "1512,-175 1920x1055");
        step("resize_window", json!({"width": 640, "height": 480})).unwrap();
        assert_eq!(frame_of(&fake, 0), "1512,-175 640x480");
        assert!(step("resize_window", json!({"width": 0, "height": 480})).is_err());
        assert!(step("move_window", json!({"x": "left", "y": 0})).is_err());
        assert!(step("move_window", json!({"y": 0})).is_err());
    }

    #[test]
    fn window_tools_are_served_with_shapes_policy_reads() {
        let (mut server, _) = server(SHOP.to_vec());
        let listed = server
            .handle(&json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}))
            .unwrap();
        let tools = listed["result"]["tools"].as_array().unwrap();
        let tool = |name: &str| {
            tools
                .iter()
                .find(|tool| tool["name"] == name)
                .unwrap_or_else(|| panic!("{name} missing"))
        };
        let arrange = &tool("arrange_windows")["inputSchema"];
        let layouts: Vec<&str> = arrange["properties"]["layout"]["enum"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .collect();
        assert_eq!(
            layouts,
            [
                "left_right_split",
                "top_bottom_split",
                "thirds",
                "grid2x2",
                "maximize",
                "restore_previous"
            ]
        );
        assert_eq!(arrange["properties"]["windows"]["maxItems"], 6);
        assert_eq!(
            arrange["properties"]["windows"]["items"]["required"],
            json!(["id", "app"])
        );
        assert_eq!(
            tool("move_window")["inputSchema"]["required"],
            json!(["app", "window", "x", "y"])
        );
        assert_eq!(
            tool("resize_window")["inputSchema"]["required"],
            json!(["app", "window", "width", "height"])
        );
        assert!(
            tool("list_windows")["inputSchema"]["required"].is_null(),
            "app is optional"
        );

        let observed = Observed::default();
        let verdict = |tool: &str, args: Value| classify(tool, &args, &observed).risk;
        assert_eq!(verdict("list_windows", json!({})), Risk::Runs);
        assert_eq!(
            verdict(
                "arrange_windows",
                json!({"layout": "left_right_split", "windows": [{"id": 1, "app": "TextEdit"}, {"id": 2, "app": "Preview"}]})
            ),
            Risk::Runs
        );
        assert_eq!(
            verdict(
                "move_window",
                json!({"app": "TextEdit", "window": 1, "x": 0, "y": 0})
            ),
            Risk::Runs
        );
    }
}
