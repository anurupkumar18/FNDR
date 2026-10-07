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
    /// Description of the element last clicked or edited, per app key.
    focus: HashMap<String, String>,
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
    }

    /// The element a click or edit just acted on, which now has focus.
    pub fn note_target(&mut self, app: &str, element_index: &str) {
        let key = self.resolve(app);
        if let Some(description) = self.element(app, element_index) {
            self.focus.insert(key, description);
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
        self.focus.get(&self.resolve(app))
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
    "submit", "sign in", "log in", "login", "sign up", "post", "reply", "publish", "share", "save",
    "confirm", "continue", "accept", "agree", "install", "allow", "upload", "download",
];

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

fn has_word(text: &str, words: &[&str]) -> bool {
    let padded = format!(" {} ", text.replace(|c: char| !c.is_alphanumeric(), " "));
    words
        .iter()
        .any(|word| padded.contains(&format!(" {word} ")))
}

fn is_secure(description: &str) -> bool {
    description.contains("secure text field") || has_word(description, &["password", "passcode"])
}

fn is_search_field(description: &str) -> bool {
    let field = ["text field", "search field", "combo box", "text box"]
        .iter()
        .any(|role| description.contains(role));
    field && has_word(description, SEARCH_WORDS)
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
                Some(_) if in_list(&names, MEDIA_APPS) || in_list(&names, BROWSERS) => {
                    decision(Risk::Runs, "clicks in a media app or browser")
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
            } else if in_list(&names, MEDIA_APPS) || in_list(&names, BROWSERS) {
                decision(Risk::Runs, "a playback or navigation key")
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
