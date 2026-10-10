//! Risk levels for every action Notch Do can take (ADR-022 amendment).
//!
//! The level comes from the tool name, its arguments and what FNDR itself has
//! seen of the target element. Model prose is never an input, so nothing the
//! model says can lower a level.

use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Risk {
    Runs,
    Confirm,
    Never,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Decision {
    pub risk: Risk,
    pub reason: String,
}

/// What FNDR has observed of each app's UI during a run: element labels from
/// `get_app_state` trees, and the element the last click or edit targeted.
#[derive(Debug, Default)]
pub struct Observed {
    /// Element index to its tree line (role and label), per app key.
    elements: HashMap<String, HashMap<String, String>>,
    /// Every name an app was seen under (argument, bundle id, display name).
    aliases: HashMap<String, String>,
    /// The element last clicked or edited, per app key: its index and the
    /// description it had then.
    focus: HashMap<String, (String, String)>,
}

impl Observed {
    /// Records a `get_app_state` result: `<tabs><index> <role> <label>` lines,
    /// with `App=<bundle id>` and `App: <name>.` headers naming the app.
    pub fn observe_tree(&mut self, app: &str, tree: &str) {
        let key = app_key(app);
        let mut names = vec![key.clone()];
        let mut elements = HashMap::new();
        for line in tree.lines() {
            if let Some(rest) = line.strip_prefix("App=") {
                names.extend(rest.split_whitespace().next().map(app_key));
            }
            if let Some(start) = line.find("App: ") {
                let name = line[start + 5..].trim_end_matches('.').trim();
                names.push(app_key(name));
            }
            let trimmed = line.trim_start_matches('\t');
            if let Some((index, description)) = trimmed.split_once(' ') {
                if index.chars().all(|c| c.is_ascii_digit()) && !index.is_empty() {
                    elements.insert(index.to_string(), description.to_lowercase());
                }
            }
        }
        for name in names {
            self.aliases.insert(name, key.clone());
        }
        self.elements.insert(key, elements);
        // A fresh reading corrects what FNDR believes has focus: if that
        // element is gone or is something else now, FNDR no longer knows.
        let canonical = self.resolve(app);
        let still_there = self
            .focus
            .get(&canonical)
            .is_some_and(|(index, description)| {
                self.elements
                    .get(&canonical)
                    .and_then(|elements| elements.get(index))
                    == Some(description)
            });
        if !still_there {
            self.focus.remove(&canonical);
        }
    }

    /// Focus moved by something other than a click FNDR saw (a key press, a
    /// page's own script), so the next typing is no longer aimed at a known field.
    pub fn forget_focus(&mut self, app: &str) {
        let key = self.resolve(app);
        self.focus.remove(&key);
    }

    /// The element a click or edit just acted on, which now has focus.
    pub fn note_target(&mut self, app: &str, element_index: &str) {
        let key = self.resolve(app);
        if let Some(description) = self.element(app, element_index) {
            self.focus
                .insert(key, (element_index.trim().to_string(), description));
        } else {
            self.focus.remove(&key);
        }
    }

    fn resolve(&self, app: &str) -> String {
        let key = app_key(app);
        self.aliases.get(&key).cloned().unwrap_or(key)
    }

    fn element(&self, app: &str, element_index: &str) -> Option<String> {
        self.elements
            .get(&self.resolve(app))?
            .get(element_index.trim())
            .cloned()
    }

    /// The label FNDR saw for an element, for the notch's action rows.
    pub fn describe(&self, app: &str, element_index: &str) -> Option<String> {
        self.element(app, element_index)
    }

    fn focused(&self, app: &str) -> Option<&String> {
        self.focus
            .get(&self.resolve(app))
            .map(|(_, description)| description)
    }

    /// The display name an app argument refers to, when a tree named it.
    fn names_for(&self, app: &str) -> Vec<String> {
        let key = self.resolve(app);
        let mut names: Vec<String> = self
            .aliases
            .iter()
            .filter(|(_, canonical)| **canonical == key)
            .map(|(alias, _)| alias.clone())
            .collect();
        names.push(app_key(app));
        names
    }
}

fn app_key(app: &str) -> String {
    app.trim().to_lowercase()
}

/// Password managers and settings that change security or privacy.
const SENSITIVE_APPS: &[&str] = &[
    "1password",
    "bitwarden",
    "keychain access",
    "com.apple.keychainaccess",
    "passwords",
    "com.apple.passwords",
    "lastpass",
    "dashlane",
    "keepassxc",
    "enpass",
    "nordpass",
    "proton pass",
    "authy",
    "system settings",
    "system preferences",
    "com.apple.systempreferences",
    // Anything typed here runs as a command or a script.
    "terminal",
    "com.apple.terminal",
    "iterm",
    "iterm2",
    "com.googlecode.iterm2",
    "warp",
    "kitty",
    "alacritty",
    "ghostty",
    "hyper",
    "script editor",
    "com.apple.scripteditor2",
    "shortcuts",
    "com.apple.shortcuts",
    "automator",
    "com.apple.automator",
    // Money, disks and force quit.
    "wallet",
    "disk utility",
    "com.apple.diskutility",
    "activity monitor",
    "com.apple.activitymonitor",
];

/// Apps where Return or a Send button delivers a message.
const MESSAGING_APPS: &[&str] = &[
    "messages",
    "com.apple.mobilesms",
    "mail",
    "com.apple.mail",
    "slack",
    "whatsapp",
    "telegram",
    "discord",
    "signal",
    "microsoft teams",
    "outlook",
    "microsoft outlook",
    "messenger",
    "spark",
    "superhuman",
    "wechat",
];

const MEDIA_APPS: &[&str] = &[
    "spotify",
    "com.spotify.client",
    "music",
    "com.apple.music",
    "podcasts",
    "tv",
    "vlc",
    "iina",
    "quicktime player",
];

const BROWSERS: &[&str] = &[
    "safari",
    "com.apple.safari",
    "google chrome",
    "chrome",
    "com.google.chrome",
    "arc",
    "company.thebrowser.browser",
    "dia",
    "company.thebrowser.dia",
    "firefox",
    "org.mozilla.firefox",
    "brave browser",
    "com.brave.browser",
    "microsoft edge",
    "opera",
    "vivaldi",
    "orion",
    "zen",
];

/// Labels that send, delete or spend money. Matched as whole words.
const NEVER_LABELS: &[&str] = &[
    "send",
    "delete",
    "remove",
    "trash",
    "erase",
    "buy",
    "purchase",
    "pay",
    "checkout",
    "check out",
    "place order",
    "order now",
    "subscribe",
    "upgrade",
    "premium",
    "move to bin",
    "empty bin",
    "unsubscribe",
    "transfer",
];

/// Labels that submit or commit something on the person's behalf.
const CONFIRM_LABELS: &[&str] = &[
    "submit",
    "sign in",
    "log in",
    "login",
    "sign up",
    "sign out",
    "log out",
    "logout",
    "post",
    "reply",
    "publish",
    "share",
    "save",
    "confirm",
    "continue",
    "accept",
    "agree",
    "install",
    "allow",
    "upload",
    "download",
    "apply",
    "add to cart",
    "add to bag",
    "book",
    "reserve",
    "donate",
    "vote",
    "follow",
    "unfollow",
    "block",
    "report",
    "archive",
    "discard",
    "clear",
    "reset",
    "cancel",
    "deactivate",
    "close account",
];

/// Words in a tree line that name what an element is, not what it says.
const ROLE_WORDS: &[&str] = &[
    "button", "link", "text", "field", "area", "search", "secure", "row", "cell", "tab",
    "checkbox", "radio", "menu", "item", "pop", "up", "image", "group", "standard", "window",
    "combo", "box", "static", "heading", "list", "toolbar",
];

/// Playback controls a browser tab may show.
const PLAYBACK_LABELS: &[&str] = &["play", "pause"];

/// Keys that move around a browser without submitting, closing or deleting.
const BROWSER_RUN_KEYS: &[&str] = &[
    "up",
    "down",
    "left",
    "right",
    "pageup",
    "pagedown",
    "page+up",
    "page+down",
    "home",
    "end",
    "space",
    "escape",
    "esc",
    "tab",
    "shift+tab",
    "cmd+l",
    "cmd+t",
    "cmd+r",
    "cmd+f",
    "cmd+[",
    "cmd+]",
    "cmd+left",
    "cmd+right",
    "ctrl+tab",
    "ctrl+shift+tab",
];

/// Whether FNDR can read what an element says: it has words beyond its
/// role, in a script the label lists above can be matched against.
fn is_readable(description: &str) -> bool {
    let says_something = description
        .split(|c: char| !c.is_alphanumeric())
        .any(|word| !word.is_empty() && !ROLE_WORDS.contains(&word));
    says_something
        && description
            .chars()
            .all(|c| !c.is_alphabetic() || c.is_ascii())
}

fn is_link_or_tab(description: &str) -> bool {
    let role = description.split_whitespace().next().unwrap_or_default();
    role == "link" || role == "tab"
}

const SEARCH_WORDS: &[&str] = &["search", "address", "url", "location", "find"];

fn in_list(names: &[String], list: &[&str]) -> bool {
    names.iter().any(|name| {
        list.iter().any(|entry| {
            name == entry
                || name.starts_with(&format!("{entry} "))
                || (name.contains('.') && name.split('.').skip(1).any(|segment| segment == *entry))
        })
    })
}

/// Whether an app is one whose playback Notch Do may drive without asking.
pub fn is_media_app(app: &str) -> bool {
    in_list(&[app_key(app)], MEDIA_APPS)
}

/// Whether an app is one Notch Do never reads or operates in any way.
pub fn is_sensitive_app(app: &str) -> bool {
    in_list(&[app_key(app)], SENSITIVE_APPS)
}

fn has_word(text: &str, words: &[&str]) -> bool {
    let padded = format!(" {} ", text.replace(|c: char| !c.is_alphanumeric(), " "));
    words
        .iter()
        .any(|word| padded.contains(&format!(" {word} ")))
}

fn is_secure(description: &str) -> bool {
    description.contains("secure text field") || has_word(description, &["password", "passcode"])
}

/// A field's role comes first on its line. Looking for the words anywhere
/// would let text on a page ("text search field for your password") or a
/// button's label pose as a search box and be clicked or typed into unasked.
const FIELD_ROLES: &[&str] = &[
    "search text field",
    "search field",
    "text field",
    "combo box",
    "text box",
];

fn is_search_field(description: &str) -> bool {
    let description = description.trim_start();
    let field = FIELD_ROLES.iter().any(|role| {
        description
            .strip_prefix(role)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with(' '))
    });
    field && has_word(description, SEARCH_WORDS)
}

/// A browser's address bar: a search field that also goes wherever it is told.
fn is_address_field(description: &str) -> bool {
    is_search_field(description) && has_word(description, &["address", "url", "location"])
}

/// Whether typed text is an address and not words to search for. A link the
/// person did not ask for waits for a tap (ADR 024), and typing one into the
/// address bar is the same thing by another route.
fn looks_like_address(text: &str) -> bool {
    let text = text.trim().to_lowercase();
    if text.is_empty() || text.contains(char::is_whitespace) {
        return false;
    }
    let scheme = text
        .split_once(':')
        .is_some_and(|(scheme, _)| !scheme.is_empty() && scheme.chars().all(|c| c.is_ascii_alphabetic()));
    let host = text
        .split(['/', '?', '#'])
        .next()
        .is_some_and(|host| host.contains('.') && !host.starts_with('.') && !host.ends_with('.'));
    scheme || host || text.starts_with("localhost")
}

fn decision(risk: Risk, reason: impl Into<String>) -> Decision {
    Decision {
        risk,
        reason: reason.into(),
    }
}

fn normalize_key(key: &str) -> String {
    key.to_lowercase()
        .replace("command", "cmd")
        .replace("control", "ctrl")
        .replace("option", "alt")
        .replace(['-', ' '], "+")
        .replace("enter", "return")
}

/// A layout names its windows with their apps, so each app is judged here
/// without FNDR having to remember which window belongs to which app.
fn arrange_decision(args: &Value, observed: &Observed) -> Decision {
    let layout = args
        .get("layout")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if crate::operator::layout::Layout::parse(layout).is_none() {
        return decision(Risk::Never, "a layout FNDR does not know");
    }
    let windows = match args.get("windows").and_then(Value::as_array) {
        Some(windows) if !windows.is_empty() => windows,
        _ => return decision(Risk::Never, "names no windows"),
    };
    if windows.len() > crate::operator::layout::MAX_WINDOWS {
        return decision(Risk::Never, "arranges more than six windows");
    }
    let apps: Vec<&str> = windows
        .iter()
        .filter_map(|window| window.get("app").and_then(Value::as_str))
        .filter(|app| !app.trim().is_empty())
        .collect();
    if apps
        .iter()
        .any(|app| in_list(&observed.names_for(app), SENSITIVE_APPS))
    {
        return decision(
            Risk::Never,
            "password managers and security settings are off limits",
        );
    }
    if apps.len() < windows.len() {
        return decision(Risk::Confirm, "a window's app is not named");
    }
    decision(Risk::Runs, "arranges windows")
}

/// The risk of one action. `tool` and `args` come from the structured call;
/// `observed` is FNDR's own record of the target app's UI.
pub fn classify(tool: &str, args: &Value, observed: &Observed) -> Decision {
    let text = |key: &str| args.get(key).and_then(Value::as_str).unwrap_or_default();
    let app = text("app");
    let names = if app.is_empty() {
        Vec::new()
    } else {
        observed.names_for(app)
    };
    // Playing and searching in a media app run (ADR-022 amendment): its search
    // box and play controls rarely carry a "search" label, and Codex often
    // clicks them by position.
    let media = in_list(&names, MEDIA_APPS);

    match tool {
        "open_app" => {
            let name = vec![app_key(text("name"))];
            if in_list(&name, SENSITIVE_APPS) {
                decision(
                    Risk::Never,
                    "password managers and security settings are off limits",
                )
            } else {
                decision(Risk::Runs, "opens an app")
            }
        }
        "open_url" => {
            let url = text("url").trim().to_lowercase();
            if url.starts_with("https://") || url.starts_with("http://") {
                decision(Risk::Runs, "opens a web page")
            } else {
                decision(Risk::Never, "only http and https links can be opened")
            }
        }
        "wait_until_frontmost" | "list_apps" => decision(Risk::Runs, "reads which app is open"),
        _ if in_list(&names, SENSITIVE_APPS) => decision(
            Risk::Never,
            "password managers and security settings are off limits",
        ),
        "get_app_state" | "select_text" => decision(Risk::Runs, "reads the screen"),
        "scroll" => decision(Risk::Runs, "scrolls"),
        "list_windows" => decision(Risk::Runs, "reads which windows are open"),
        "move_window" | "resize_window" if app.is_empty() => {
            decision(Risk::Confirm, "the window's app is not named")
        }
        // A window FNDR moved can be put back with restore_previous, and
        // moving one changes nothing inside it.
        "move_window" | "resize_window" => decision(Risk::Runs, "moves a window"),
        "arrange_windows" => arrange_decision(args, observed),
        "click" | "perform_secondary_action" => {
            let target = observed.element(app, text("element_index"));
            match target.as_deref() {
                Some(label) if has_word(label, NEVER_LABELS) => {
                    decision(Risk::Never, "sending, deleting and buying are not allowed")
                }
                _ if tool == "perform_secondary_action" => {
                    decision(Risk::Confirm, "uses a menu action")
                }
                Some(label) if has_word(label, CONFIRM_LABELS) => {
                    decision(Risk::Confirm, "submits something")
                }
                Some(_) if media => decision(Risk::Runs, "clicks in a media app"),
                Some(label) if !is_readable(label) => {
                    decision(Risk::Confirm, "the click target has no label FNDR can read")
                }
                Some(label)
                    if in_list(&names, BROWSERS)
                        && (is_link_or_tab(label)
                            || is_search_field(label)
                            || has_word(label, PLAYBACK_LABELS)) =>
                {
                    decision(
                        Risk::Runs,
                        "follows a link, a tab or a search box in a browser",
                    )
                }
                Some(_) if in_list(&names, BROWSERS) => {
                    decision(Risk::Confirm, "clicks a control on a web page")
                }
                Some(_) => decision(Risk::Confirm, "clicks outside media apps and browsers"),
                None if media && args.get("element_index").is_none() => {
                    decision(Risk::Runs, "clicks by position in a media app")
                }
                None => decision(Risk::Confirm, "the click target is not known"),
            }
        }
        "type_text" | "set_value" => {
            let target = if tool == "set_value" {
                observed.element(app, text("element_index"))
            } else {
                observed.focused(app).cloned()
            };
            match target.as_deref() {
                Some(field) if is_secure(field) => {
                    decision(Risk::Never, "credentials are never entered")
                }
                Some(field)
                    if is_address_field(field)
                        && looks_like_address(text(if tool == "set_value" { "value" } else { "text" })) =>
                {
                    decision(Risk::Confirm, "goes to an address typed into the address bar")
                }
                Some(field) if is_search_field(field) => {
                    decision(Risk::Runs, "types into a search field")
                }
                _ if media => decision(Risk::Runs, "searches in a media app"),
                _ => decision(Risk::Confirm, "types outside a search field"),
            }
        }
        "press_key" => {
            let key = normalize_key(text("key"));
            let focus = observed.focused(app).map(String::as_str);
            let in_search = focus.is_some_and(is_search_field);
            let deletes = [
                "cmd+backspace",
                "cmd+delete",
                "cmd+shift+backspace",
                "cmd+shift+delete",
                "cmd+alt+backspace",
            ];
            let ends_session = ["cmd+shift+q", "ctrl+cmd+q", "cmd+alt+esc", "cmd+alt+escape"];
            if deletes.contains(&key.as_str()) {
                decision(Risk::Never, "deleting is not allowed")
            } else if ends_session.contains(&key.as_str()) {
                decision(Risk::Never, "ends the session or force quits")
            } else if key.ends_with("return") {
                if in_search || media {
                    decision(Risk::Runs, "runs a search")
                } else if in_list(&names, MESSAGING_APPS) {
                    decision(Risk::Never, "Return sends in a messaging app")
                } else {
                    decision(Risk::Confirm, "Return may submit a form")
                }
            } else if media {
                decision(Risk::Runs, "a playback key")
            } else if in_list(&names, BROWSERS) && BROWSER_RUN_KEYS.contains(&key.as_str()) {
                decision(Risk::Runs, "a navigation key")
            } else if in_list(&names, BROWSERS) {
                decision(Risk::Confirm, "a key that can close, save or change a page")
            } else {
                decision(Risk::Confirm, "a key press outside media apps and browsers")
            }
        }
        "drag" => decision(Risk::Confirm, "drags"),
        _ => decision(Risk::Never, "unknown tool"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const SPOTIFY_TREE: &str = "App=com.spotify.client (pid 42)\n\
Window: \"Spotify Premium\", App: Spotify.\n\
0 standard window Spotify Premium\n\
\t1 search text field What do you want to play?\n\
\t2 button Play\n\
\t3 button Buy Premium\n\
\t4 row Never Gonna Give You Up\n";

    const MAIL_TREE: &str = "App=com.apple.mail (pid 7)\n\
0 standard window Inbox\n\
\t1 text area Message body\n\
\t2 button Send\n\
\t3 search text field Search\n\
\t4 secure text field Password\n";

    fn observed() -> Observed {
        let mut observed = Observed::default();
        observed.observe_tree("Spotify", SPOTIFY_TREE);
        observed.observe_tree("Mail", MAIL_TREE);
        observed
    }

    fn risk(tool: &str, args: Value, observed: &Observed) -> Risk {
        classify(tool, &args, observed).risk
    }

    const SHOP_TREE: &str = "App=com.google.Chrome (pid 9)\n\
Window: \"Shop\", App: Google Chrome.\n\
0 standard window Shop\n\
\t1 text field Address and search bar\n\
\t2 button Add to cart\n\
\t3 button Sign out\n\
\t4 button\n\
\t5 button Enviar\n\
\t6 link Next page\n\
\t7 link Log out\n\
\t8 secure text field Password\n\
\t9 button Play\n\
\t10 button Details\n\
\t11 link Delete account\n";

    fn browsing() -> Observed {
        let mut observed = Observed::default();
        observed.observe_tree("Google Chrome", SHOP_TREE);
        observed
    }

    fn chrome_click(index: &str, observed: &Observed) -> Risk {
        risk(
            "click",
            json!({"app": "Google Chrome", "element_index": index}),
            observed,
        )
    }

    #[test]
    fn a_browser_asks_before_clicking_anything_but_links_tabs_search_and_playback() {
        let o = browsing();
        for (index, what) in [("1", "search box"), ("6", "link"), ("9", "play")] {
            assert_eq!(chrome_click(index, &o), Risk::Runs, "{what}");
        }
        for (index, what) in [
            ("2", "add to cart"),
            ("3", "sign out"),
            ("7", "a link that logs out"),
            ("8", "password field"),
            ("10", "a button with an unlisted label"),
        ] {
            assert_eq!(chrome_click(index, &o), Risk::Confirm, "{what}");
        }
        assert_eq!(chrome_click("11", &o), Risk::Never);
    }

    #[test]
    fn a_label_fndr_cannot_read_is_treated_as_unknown() {
        let o = browsing();
        assert_eq!(chrome_click("4", &o), Risk::Confirm, "no label");
        assert_eq!(chrome_click("5", &o), Risk::Confirm, "another language");
        assert!(is_readable("button add to cart"));
        assert!(!is_readable("button"));
        assert!(!is_readable("link 送信"));
    }

    #[test]
    fn a_browser_runs_navigation_keys_and_asks_for_the_rest() {
        let o = browsing();
        let key = |key: &str| risk("press_key", json!({"app": "Google Chrome", "key": key}), &o);
        for name in [
            "down",
            "Page Down",
            "space",
            "tab",
            "cmd+l",
            "cmd+r",
            "escape",
        ] {
            assert_eq!(key(name), Risk::Runs, "{name}");
        }
        for name in [
            "cmd+w",
            "cmd+q",
            "cmd+s",
            "cmd+p",
            "backspace",
            "delete",
            "return",
            "a",
        ] {
            assert_eq!(key(name), Risk::Confirm, "{name}");
        }
    }

    #[test]
    fn words_on_a_page_cannot_pose_as_a_search_box() {
        // FNDR's own executor numbers page text and labelled groups too, and
        // any button can be labelled anything.
        let mut o = Observed::default();
        o.observe_tree(
            "Google Chrome",
            "App=com.google.Chrome (pid 9)\n0 standard window Blog\n\
             \t1 text search field for coupons\n\
             \t2 button Find text field\n\
             \t3 group search text field\n\
             \t4 search field Search this site\n\
             \t5 text field Address and search bar\n",
        );
        let click = |index: &str| {
            risk("click", json!({"app": "Google Chrome", "element_index": index}), &o)
        };
        for posing in ["1", "2", "3"] {
            assert_eq!(click(posing), Risk::Confirm, "element {posing}");
        }
        assert_eq!(click("4"), Risk::Runs);
        assert_eq!(click("5"), Risk::Runs);

        // Typing after pressing one of them is not typing into a search box.
        o.note_target("Google Chrome", "1");
        let typing = risk("type_text", json!({"app": "Google Chrome", "text": "x"}), &o);
        assert_eq!(typing, Risk::Confirm);
    }

    #[test]
    fn an_address_typed_into_the_address_bar_waits_for_a_tap() {
        // Live run, 2026-10-08: with no page to read, the model used Chrome's
        // address bar. Words there are a search; an address is a link.
        let mut o = Observed::default();
        o.observe_tree(
            "Google Chrome",
            "App=com.google.Chrome (pid 9)\n0 standard window New Tab\n\t1 text field (settable, string) Address and search bar\n\t2 search text field Search this site\n",
        );
        o.note_target("Google Chrome", "1");
        let typing = |o: &Observed, text: &str| {
            risk("type_text", json!({"app": "Google Chrome", "text": text}), o)
        };
        assert_eq!(typing(&o, "running shoes"), Risk::Runs);
        for address in ["evil.example/steal?d=1", "https://evil.example", "javascript:alert(1)", "localhost:3000", "192.168.1.1"] {
            assert_eq!(typing(&o, address), Risk::Confirm, "{address}");
        }
        let setting = |o: &Observed, value: &str| {
            risk("set_value", json!({"app": "Google Chrome", "element_index": "1", "value": value}), o)
        };
        assert_eq!(setting(&o, "evil.example"), Risk::Confirm);
        assert_eq!(setting(&o, "running shoes"), Risk::Runs);
        // A page's own search box takes whatever is typed: it cannot leave the site.
        o.note_target("Google Chrome", "2");
        assert_eq!(typing(&o, "evil.example"), Risk::Runs);
    }

    #[test]
    fn typing_runs_only_while_fndr_knows_the_search_box_has_focus() {
        let mut o = browsing();
        let typing =
            |o: &Observed| risk("type_text", json!({"app": "Google Chrome", "text": "x"}), o);
        assert_eq!(typing(&o), Risk::Confirm, "nothing clicked yet");

        o.note_target("Google Chrome", "1");
        assert_eq!(typing(&o), Risk::Runs, "the search box was clicked");

        // Tab moved focus somewhere FNDR did not see.
        o.forget_focus("Google Chrome");
        assert_eq!(typing(&o), Risk::Confirm);

        // The page changed under the same index: it is a password field now.
        o.note_target("Google Chrome", "1");
        o.observe_tree(
            "Google Chrome",
            "App=com.google.Chrome (pid 9)\n0 standard window Bank\n\t1 secure text field Password\n",
        );
        assert_eq!(typing(&o), Risk::Confirm);

        // An unchanged reading keeps what FNDR knows.
        o.observe_tree("Google Chrome", SHOP_TREE);
        o.note_target("Google Chrome", "1");
        o.observe_tree("Google Chrome", SHOP_TREE);
        assert_eq!(typing(&o), Risk::Runs);
    }

    #[test]
    fn apps_that_run_commands_or_move_money_are_never_opened() {
        let o = observed();
        for name in [
            "Terminal",
            "iTerm",
            "Script Editor",
            "Shortcuts",
            "Automator",
            "Wallet",
            "Disk Utility",
        ] {
            assert_eq!(
                risk("open_app", json!({"name": name}), &o),
                Risk::Never,
                "{name}"
            );
        }
        assert_eq!(risk("open_app", json!({"name": "Notes"}), &o), Risk::Runs);
    }

    #[test]
    fn reading_and_scrolling_run_without_asking() {
        let o = observed();
        assert_eq!(risk("list_apps", json!({}), &o), Risk::Runs);
        assert_eq!(
            risk("get_app_state", json!({"app": "Spotify"}), &o),
            Risk::Runs
        );
        assert_eq!(
            risk("scroll", json!({"app": "Spotify", "direction": "down"}), &o),
            Risk::Runs
        );
    }

    #[test]
    fn native_open_tools_run_but_only_for_web_urls_and_safe_apps() {
        let o = observed();
        assert_eq!(risk("open_app", json!({"name": "Spotify"}), &o), Risk::Runs);
        assert_eq!(
            risk(
                "open_url",
                json!({"url": "https://www.google.com/search?q=x"}),
                &o
            ),
            Risk::Runs
        );
        assert_eq!(
            risk("open_url", json!({"url": "file:///etc/passwd"}), &o),
            Risk::Never
        );
        assert_eq!(
            risk("open_app", json!({"name": "1Password 7"}), &o),
            Risk::Never
        );
    }

    #[test]
    fn media_clicks_on_known_safe_labels_run() {
        let o = observed();
        assert_eq!(
            risk("click", json!({"app": "Spotify", "element_index": "2"}), &o),
            Risk::Runs
        );
        assert_eq!(
            risk("click", json!({"app": "Spotify", "element_index": "4"}), &o),
            Risk::Runs
        );
    }

    #[test]
    fn purchases_and_sending_are_never_allowed() {
        let o = observed();
        assert_eq!(
            risk("click", json!({"app": "Spotify", "element_index": "3"}), &o),
            Risk::Never
        );
        assert_eq!(
            risk("click", json!({"app": "Mail", "element_index": "2"}), &o),
            Risk::Never
        );
    }

    #[test]
    fn unknown_targets_and_non_media_clicks_need_confirmation() {
        let o = observed();
        assert_eq!(
            risk(
                "click",
                json!({"app": "Spotify", "element_index": "99"}),
                &o
            ),
            Risk::Confirm
        );
        assert_eq!(
            risk("click", json!({"app": "Mail", "element_index": "1"}), &o),
            Risk::Confirm
        );
    }

    #[test]
    fn typing_into_a_search_field_runs_and_elsewhere_confirms() {
        let mut o = observed();
        o.note_target("Spotify", "1");
        assert_eq!(
            risk("type_text", json!({"app": "Spotify", "text": "song"}), &o),
            Risk::Runs
        );
        assert_eq!(
            risk("press_key", json!({"app": "Spotify", "key": "Return"}), &o),
            Risk::Runs
        );

        o.note_target("Mail", "1");
        assert_eq!(
            risk("type_text", json!({"app": "Mail", "text": "hi"}), &o),
            Risk::Confirm
        );
        assert_eq!(
            risk("type_text", json!({"app": "Notes", "text": "hi"}), &o),
            Risk::Confirm
        );
    }

    #[test]
    fn credentials_are_never_typed() {
        let mut o = observed();
        o.note_target("Mail", "4");
        assert_eq!(
            risk("type_text", json!({"app": "Mail", "text": "hunter2"}), &o),
            Risk::Never
        );
        assert_eq!(
            risk(
                "set_value",
                json!({"app": "Mail", "element_index": "4", "value": "x"}),
                &o
            ),
            Risk::Never
        );
    }

    #[test]
    fn return_sends_in_messaging_apps_and_submits_elsewhere() {
        let mut o = observed();
        o.note_target("Mail", "1");
        assert_eq!(
            risk("press_key", json!({"app": "Mail", "key": "Return"}), &o),
            Risk::Never
        );
        assert_eq!(
            risk(
                "press_key",
                json!({"app": "Messages", "key": "cmd+return"}),
                &o
            ),
            Risk::Never
        );
        assert_eq!(
            risk("press_key", json!({"app": "Notes", "key": "Return"}), &o),
            Risk::Confirm
        );
        o.note_target("Mail", "3");
        assert_eq!(
            risk("press_key", json!({"app": "Mail", "key": "Return"}), &o),
            Risk::Runs
        );
    }

    #[test]
    fn delete_shortcuts_are_never_allowed() {
        let o = observed();
        assert_eq!(
            risk(
                "press_key",
                json!({"app": "Finder", "key": "cmd+backspace"}),
                &o
            ),
            Risk::Never
        );
        assert_eq!(
            risk(
                "press_key",
                json!({"app": "Finder", "key": "Cmd+Delete"}),
                &o
            ),
            Risk::Never
        );
    }

    #[test]
    fn searching_and_playing_in_a_media_app_runs_without_a_search_label() {
        // Spotify's search box is a combo box labelled "What do you want to play?".
        let mut o = Observed::default();
        o.observe_tree(
            "Spotify",
            "App=com.spotify.client (pid 9)\n0 standard window Spotify\n\t5 combo box (settable, string) What do you want to play?\n\t6 button Upgrade to Premium\n",
        );
        o.note_target("Spotify", "5");
        assert_eq!(
            risk(
                "type_text",
                json!({"app": "Spotify", "text": "Blinding Lights"}),
                &o
            ),
            Risk::Runs
        );
        assert_eq!(
            risk(
                "set_value",
                json!({"app": "Spotify", "element_index": "5", "value": "Blinding Lights"}),
                &o
            ),
            Risk::Runs
        );
        assert_eq!(
            risk("press_key", json!({"app": "Spotify", "key": "Return"}), &o),
            Risk::Runs
        );
        assert_eq!(
            risk("click", json!({"app": "Spotify", "x": 640, "y": 300}), &o),
            Risk::Runs
        );
        assert_eq!(
            risk("click", json!({"app": "Spotify", "element_index": "6"}), &o),
            Risk::Never
        );
        assert_eq!(
            risk("click", json!({"app": "Notes", "x": 10, "y": 10}), &o),
            Risk::Confirm
        );
        assert_eq!(
            risk("click", json!({"app": "Safari", "x": 10, "y": 10}), &o),
            Risk::Confirm
        );
    }

    #[test]
    fn media_keys_run_in_media_apps_only() {
        let o = observed();
        assert_eq!(
            risk("press_key", json!({"app": "Spotify", "key": "space"}), &o),
            Risk::Runs
        );
        assert_eq!(
            risk("press_key", json!({"app": "Notes", "key": "space"}), &o),
            Risk::Confirm
        );
    }

    #[test]
    fn sensitive_apps_and_unknown_tools_are_never_allowed() {
        let o = observed();
        assert_eq!(
            risk("get_app_state", json!({"app": "Keychain Access"}), &o),
            Risk::Never
        );
        assert_eq!(
            risk(
                "click",
                json!({"app": "com.1password.1password", "element_index": "1"}),
                &o
            ),
            Risk::Never
        );
        assert_eq!(
            risk("scroll", json!({"app": "System Settings"}), &o),
            Risk::Never
        );
        assert_eq!(risk("run_shell", json!({}), &o), Risk::Never);
    }

    #[test]
    fn drag_and_secondary_actions_confirm() {
        let o = observed();
        assert_eq!(risk("drag", json!({"app": "Spotify"}), &o), Risk::Confirm);
        assert_eq!(
            risk(
                "perform_secondary_action",
                json!({"app": "Spotify", "element_index": "2", "action": "Raise"}),
                &o
            ),
            Risk::Confirm
        );
    }

    #[test]
    fn listing_and_arranging_windows_run_because_they_can_be_put_back() {
        let o = observed();
        assert_eq!(risk("list_windows", json!({}), &o), Risk::Runs);
        assert_eq!(
            risk("list_windows", json!({"app": "TextEdit"}), &o),
            Risk::Runs
        );
        assert_eq!(
            risk(
                "move_window",
                json!({"app": "TextEdit", "window": 3, "x": 0, "y": 0}),
                &o
            ),
            Risk::Runs
        );
        assert_eq!(
            risk(
                "resize_window",
                json!({"app": "Preview", "window": "4", "width": 600, "height": 400}),
                &o
            ),
            Risk::Runs
        );
        for layout in [
            "left_right_split",
            "top_bottom_split",
            "thirds",
            "grid2x2",
            "maximize",
            "restore_previous",
        ] {
            assert_eq!(
                risk(
                    "arrange_windows",
                    json!({"layout": layout, "windows": [{"id": 1, "app": "TextEdit"}, {"id": 2, "app": "Preview"}]}),
                    &o
                ),
                Risk::Runs,
                "{layout}"
            );
        }
    }

    #[test]
    fn a_window_of_an_off_limits_app_or_more_than_six_is_never_arranged() {
        let o = observed();
        let arrange = |windows: Value| {
            risk(
                "arrange_windows",
                json!({"layout": "left_right_split", "windows": windows}),
                &o,
            )
        };
        assert_eq!(
            arrange(json!([{"id": 1, "app": "TextEdit"}, {"id": 2, "app": "1Password 7"}])),
            Risk::Never
        );
        assert_eq!(
            arrange(json!([{"id": 1, "app": "com.apple.Terminal"}])),
            Risk::Never
        );
        let seven: Vec<Value> = (1..=7)
            .map(|id| json!({"id": id, "app": "TextEdit"}))
            .collect();
        assert_eq!(arrange(Value::Array(seven)), Risk::Never);
        assert_eq!(arrange(json!([])), Risk::Never, "no windows");
        assert_eq!(arrange(json!("all")), Risk::Never, "not a list");
        assert_eq!(arrange(json!([{"id": 1}])), Risk::Confirm, "no app named");
        assert_eq!(
            risk(
                "arrange_windows",
                json!({"layout": "cascade", "windows": [{"id": 1, "app": "TextEdit"}]}),
                &o
            ),
            Risk::Never,
            "a layout FNDR does not know"
        );
        for tool in ["list_windows", "move_window", "resize_window"] {
            assert_eq!(
                risk(tool, json!({"app": "Keychain Access", "window": 1}), &o),
                Risk::Never,
                "{tool}"
            );
        }
        assert_eq!(
            risk("move_window", json!({"window": 1, "x": 0, "y": 0}), &o),
            Risk::Confirm,
            "no app named"
        );
        assert!(is_sensitive_app("System Settings"));
        assert!(is_sensitive_app("com.apple.keychainaccess"));
        assert!(!is_sensitive_app("TextEdit"));
    }

    #[test]
    fn app_names_match_bundle_ids_in_trees() {
        let o = observed();
        assert_eq!(
            risk(
                "click",
                json!({"app": "com.spotify.client", "element_index": "2"}),
                &o
            ),
            Risk::Runs
        );
    }
}
