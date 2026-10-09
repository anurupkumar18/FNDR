//! The Mac side of `fndr operator-mcp` (ADR 026): reads an app's front window
//! as indexed text and acts on the exact element FNDR printed.
//!
//! Presses go through AXPress and edits through AXValue on the element
//! itself, so a click is never a screen position. Text typed without an
//! element, keys and scrolling go to the app's own focus after FNDR has made
//! that app frontmost and confirmed it stayed there. No pixels are read.

use std::time::{Duration, Instant};

use objc2_app_kit::{NSApplicationActivationPolicy, NSRunningApplication};

use super::text_tree::TextTree;
use super::{
    activate_target_app, ax_copy_attr_value, ax_set_string_attr, ax_string_attr,
    enable_manual_accessibility, frontmost_pid, has_accessibility_permission, kCFBooleanTrue,
    str_to_cfstring, AXError, AXUIElementCreateApplication, AXUIElementRef,
    AXUIElementSetAttributeValue, AXUIElementSetMessagingTimeout, AxElement, AxTextTree,
    CFArrayGetCount, CFArrayGetTypeID, CFArrayGetValueAtIndex, CFGetTypeID, CFRelease, CFRetain,
    K_AX_ERROR_SUCCESS,
};
use crate::operator::mcp::{AppRef, Desktop, Entry, Tree};

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXUIElementPerformAction(element: AXUIElementRef, action: super::CFStringRef) -> AXError;
}

const MAX_VISITED: usize = 2_500;
const MAX_LINES: usize = 600;
const READ_BUDGET: Duration = Duration::from_secs(4);
const LABEL_CHARS: usize = 80;

pub struct AxDesktop;

/// The word `policy.rs` reads for a role, or `None` for roles that only group
/// other elements and are shown only when they carry a label.
fn role_word(role: &str, subrole: Option<&str>) -> Option<&'static str> {
    Some(match (role, subrole) {
        (_, Some("AXSecureTextField")) => "secure text field",
        (_, Some("AXSearchField")) => "search field",
        ("AXWindow", Some("AXStandardWindow")) => "standard window",
        ("AXRadioButton", Some("AXTabButton")) | ("AXTab", _) => "tab",
        ("AXWindow", _) => "window",
        ("AXButton", _) => "button",
        ("AXLink", _) => "link",
        ("AXTextField", _) => "text field",
        ("AXTextArea", _) => "text area",
        ("AXComboBox", _) => "combo box",
        ("AXCheckBox", _) => "checkbox",
        ("AXRadioButton", _) => "radio button",
        ("AXPopUpButton", _) => "pop up button",
        ("AXMenuButton", _) => "menu button",
        ("AXMenuItem", _) => "menu item",
        ("AXSlider", _) => "slider",
        ("AXStaticText", _) => "text",
        ("AXImage", _) => "image",
        ("AXHeading", _) => "heading",
        ("AXWebArea", _) => "web area",
        ("AXRow", _) => "row",
        ("AXCell", _) => "cell",
        _ => return None,
    })
}

/// Fields whose contents are never read: only their label is.
fn holds_typed_text(role: &str, subrole: Option<&str>) -> bool {
    subrole == Some("AXSecureTextField")
        || subrole == Some("AXSearchField")
        || matches!(role, "AXTextField" | "AXTextArea" | "AXComboBox")
}

/// One line of words, no longer than a label should be, with nothing in it
/// that could start a new line of the tree.
fn tidy(text: &str) -> String {
    let flat: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let words = flat.split_whitespace().collect::<Vec<_>>().join(" ");
    if words.chars().count() <= LABEL_CHARS {
        words
    } else {
        let cut: String = words.chars().take(LABEL_CHARS).collect();
        format!("{cut}...")
    }
}

unsafe fn label_of(element: &AxElement, role: &str, subrole: Option<&str>) -> String {
    let attr = |name: &str| {
        ax_string_attr(element.0, name)
            .map(|text| tidy(&text))
            .filter(|text| !text.is_empty())
    };
    let name = attr("AXTitle")
        .or_else(|| attr("AXDescription"))
        .or_else(|| attr("AXPlaceholderValue"));
    // What a field holds stays unread; what other elements say is part of
    // their line, so a display such as Calculator's result can be read back.
    let value = if holds_typed_text(role, subrole) {
        None
    } else {
        attr("AXValue").filter(|value| Some(value) != name.as_ref())
    };
    match (name, value) {
        (Some(name), Some(value)) => format!("{name} = {value}"),
        (Some(text), None) | (None, Some(text)) => text,
        (None, None) => String::new(),
    }
}

/// `button Play`, the form `read_tree` prints and `describe` checks.
/// `None` when the element has neither a known role nor a label.
unsafe fn describe_element(element: &AxElement) -> Option<(String, bool)> {
    let role = ax_string_attr(element.0, "AXRole")?;
    let subrole = ax_string_attr(element.0, "AXSubrole");
    let label = label_of(element, &role, subrole.as_deref());
    let word = role_word(&role, subrole.as_deref());
    let shown = word.is_some() || !label.is_empty();
    let word = word
        .map(str::to_string)
        .unwrap_or_else(|| tidy(role.trim_start_matches("AX")).to_lowercase());
    let line = if label.is_empty() {
        word
    } else {
        format!("{word} {label}")
    };
    Some((line, shown))
}

unsafe fn is_secure(element: AXUIElementRef) -> bool {
    ax_string_attr(element, "AXSubrole").as_deref() == Some("AXSecureTextField")
}

fn ax_failure(code: AXError, what: &str) -> String {
    match code {
        -25211 => "Accessibility is not allowed for FNDR. Turn it on in System Settings > Privacy & Security.".to_string(),
        -25206 | -25205 | -25212 => format!("That element cannot be {what}."),
        -25204 | -25202 => format!("The app did not answer while {what}."),
        other => format!("macOS refused ({other}) while {what}."),
    }
}

fn need_permission() -> Result<(), String> {
    if has_accessibility_permission() {
        Ok(())
    } else {
        Err("Accessibility is not allowed for FNDR. Turn it on in System Settings > Privacy & Security.".to_string())
    }
}

/// The pid of the app in front. LaunchServices answers correctly even in a
/// process whose main run loop is not spinning, where the Accessibility
/// system-wide focus can lag.
fn front_pid() -> Option<i32> {
    crate::operator::native::lsappinfo_front()
        .map(|(pid, _, _)| pid)
        .or_else(|| unsafe { frontmost_pid() })
}

/// Makes `app` frontmost and confirms it stayed there, so keys and pasted
/// text cannot land in another app.
fn bring_to_front(app: &AppRef) -> Result<(), String> {
    need_permission()?;
    if front_pid() != Some(app.pid) {
        activate_target_app(app.pid)?;
    }
    if front_pid() == Some(app.pid) {
        Ok(())
    } else {
        Err(format!("{} would not come to the front.", app.name))
    }
}

unsafe fn focused_is_secure(pid: i32) -> bool {
    let application = AXUIElementCreateApplication(pid);
    if application.is_null() {
        return false;
    }
    let secure = match ax_copy_attr_value(application, "AXFocusedUIElement") {
        Ok(focused) if !focused.is_null() => {
            let secure = is_secure(focused);
            CFRelease(focused);
            secure
        }
        _ => false,
    };
    CFRelease(application);
    secure
}

const MODIFIERS: [(&[&str], &str); 4] = [
    (&["cmd", "command", "super", "meta"], "command down"),
    (&["ctrl", "control"], "control down"),
    (&["alt", "option", "opt"], "option down"),
    (&["shift"], "shift down"),
];

fn key_code(name: &str) -> Option<u16> {
    Some(match name {
        "return" | "enter" => 36,
        "tab" => 48,
        "space" => 49,
        "delete" | "backspace" => 51,
        "escape" | "esc" => 53,
        "left" => 123,
        "right" => 124,
        "down" => 125,
        "up" => 126,
        "pageup" | "page_up" => 116,
        "pagedown" | "page_down" => 121,
        "home" => 115,
        "end" => 119,
        "forwarddelete" => 117,
        _ => return None,
    })
}

/// The System Events line that sends `key` ("Return", "cmd+shift+t").
fn key_script(key: &str) -> Result<String, String> {
    let lowered = key.trim().to_lowercase().replace([' ', '-'], "+");
    let mut parts: Vec<&str> = lowered.split('+').filter(|part| !part.is_empty()).collect();
    let main = parts.pop().ok_or("press_key needs a key.")?;
    let mut flags = Vec::new();
    for part in parts {
        let flag = MODIFIERS
            .iter()
            .find(|(names, _)| names.contains(&part))
            .map(|(_, flag)| *flag)
            .ok_or_else(|| format!("{part} is not a modifier key."))?;
        flags.push(flag);
    }
    let using = if flags.is_empty() {
        String::new()
    } else {
        format!(" using {{{}}}", flags.join(", "))
    };
    let action = if let Some(code) = key_code(main) {
        format!("key code {code}")
    } else {
        let mut chars = main.chars();
        match (chars.next(), chars.next()) {
            (Some(c), None) if c.is_ascii_graphic() => {
                let escaped = match c {
                    '"' => "\\\"".to_string(),
                    '\\' => "\\\\".to_string(),
                    other => other.to_string(),
                };
                format!("keystroke \"{escaped}\"")
            }
            _ => return Err(format!("FNDR does not know the key {key}.")),
        }
    };
    Ok(format!(
        "tell application \"System Events\" to {action}{using}"
    ))
}

fn run_script(script: &str) -> Result<(), String> {
    let output = std::process::Command::new("/usr/bin/osascript")
        .args(["-e", script])
        .output()
        .map_err(|error| format!("Could not send the key: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "macOS refused the key press: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

unsafe fn window_root(application: AXUIElementRef) -> Option<AxElement> {
    if let Ok(window) = ax_copy_attr_value(application, "AXFocusedWindow") {
        if !window.is_null() {
            return Some(AxElement(window));
        }
    }
    let windows = ax_copy_attr_value(application, "AXWindows").ok()?;
    if windows.is_null() {
        return None;
    }
    let first = (CFGetTypeID(windows) == CFArrayGetTypeID() && CFArrayGetCount(windows) > 0)
        .then(|| CFArrayGetValueAtIndex(windows, 0))
        .filter(|window| !window.is_null())
        .map(|window| AxElement(CFRetain(window)));
    CFRelease(windows);
    first
}

impl Desktop for AxDesktop {
    type Handle = AxElement;

    fn list_apps(&mut self) -> Vec<AppRef> {
        let apps = unsafe { objc2_app_kit::NSWorkspace::sharedWorkspace().runningApplications() };
        (0..apps.count())
            .map(|at| unsafe { apps.objectAtIndex(at) })
            .filter(|app: &objc2::rc::Retained<NSRunningApplication>| unsafe {
                app.activationPolicy() == NSApplicationActivationPolicy::Regular
            })
            .map(|app| unsafe {
                AppRef {
                    pid: app.processIdentifier(),
                    name: app
                        .localizedName()
                        .map(|s| s.to_string())
                        .unwrap_or_default(),
                    bundle: app
                        .bundleIdentifier()
                        .map(|s| s.to_string())
                        .unwrap_or_default(),
                }
            })
            .collect()
    }

    fn read_tree(&mut self, app: &AppRef) -> Result<Tree<AxElement>, String> {
        need_permission()?;
        unsafe {
            let application = AxElement(AXUIElementCreateApplication(app.pid));
            if application.0.is_null() {
                return Err(format!("{} cannot be read.", app.name));
            }
            AXUIElementSetMessagingTimeout(application.0, 1.5);
            enable_manual_accessibility(application.0);
            let root = window_root(application.0)
                .ok_or_else(|| format!("{} has no open window to read.", app.name))?;
            let window = ax_string_attr(root.0, "AXTitle")
                .map(|t| tidy(&t))
                .unwrap_or_default();

            let started = Instant::now();
            let mut visited = 0;
            let mut truncated = false;
            let mut entries: Vec<Entry<AxElement>> = Vec::new();
            let mut stack = vec![(root, 0usize)];
            while let Some((node, depth)) = stack.pop() {
                visited += 1;
                if visited > MAX_VISITED
                    || entries.len() >= MAX_LINES
                    || started.elapsed() > READ_BUDGET
                {
                    truncated = true;
                    break;
                }
                let shown = describe_element(&node).filter(|(_, shown)| *shown);
                let child_depth = if shown.is_some() { depth + 1 } else { depth };
                // A secure field's subtree is never walked.
                if !is_secure(node.0) {
                    for child in AxTextTree.children(&node).into_iter().rev() {
                        stack.push((child, child_depth));
                    }
                }
                if let Some((description, _)) = shown {
                    entries.push(Entry {
                        depth,
                        description,
                        handle: node,
                    });
                }
            }
            Ok(Tree {
                window,
                entries,
                truncated,
            })
        }
    }

    fn describe(&mut self, handle: &AxElement) -> Option<String> {
        unsafe { describe_element(handle).map(|(line, _)| line) }
    }

    fn press(&mut self, handle: &AxElement) -> Result<(), String> {
        need_permission()?;
        unsafe {
            let action = str_to_cfstring("AXPress");
            let code = AXUIElementPerformAction(handle.0, action);
            CFRelease(action);
            if code == K_AX_ERROR_SUCCESS {
                Ok(())
            } else {
                Err(ax_failure(code, "pressed"))
            }
        }
    }

    fn set_value(&mut self, handle: &AxElement, text: &str) -> Result<(), String> {
        need_permission()?;
        unsafe {
            if is_secure(handle.0) {
                return Err("Credentials are never entered.".to_string());
            }
            ax_set_string_attr(handle.0, "AXValue", text).map_err(|code| ax_failure(code, "edited"))
        }
    }

    fn type_text(
        &mut self,
        app: &AppRef,
        target: Option<&AxElement>,
        text: &str,
    ) -> Result<(), String> {
        bring_to_front(app)?;
        unsafe {
            if let Some(element) = target {
                if is_secure(element.0) {
                    return Err("Credentials are never entered.".to_string());
                }
                let focus = str_to_cfstring("AXFocused");
                let code = AXUIElementSetAttributeValue(element.0, focus, kCFBooleanTrue);
                CFRelease(focus);
                if code != K_AX_ERROR_SUCCESS {
                    return Err(ax_failure(code, "focused"));
                }
            }
            if focused_is_secure(app.pid) {
                return Err("Credentials are never entered.".to_string());
            }
            if front_pid() != Some(app.pid) {
                return Err(format!("{} lost focus before typing.", app.name));
            }
        }
        super::paste_into_frontmost_app(text, false)
    }

    fn press_key(&mut self, app: &AppRef, key: &str) -> Result<(), String> {
        let script = key_script(key)?;
        bring_to_front(app)?;
        run_script(&script)
    }

    fn scroll(&mut self, app: &AppRef, direction: &str, pages: u32) -> Result<(), String> {
        let key = match direction {
            "up" => "pageup",
            "down" => "pagedown",
            "left" => "left",
            _ => "right",
        };
        let script = key_script(key)?;
        bring_to_front(app)?;
        for _ in 0..pages {
            run_script(&script)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roles_read_the_way_policy_expects() {
        assert_eq!(
            role_word("AXTextField", Some("AXSecureTextField")),
            Some("secure text field")
        );
        assert_eq!(
            role_word("AXTextField", Some("AXSearchField")),
            Some("search field")
        );
        assert_eq!(role_word("AXTextField", None), Some("text field"));
        assert_eq!(role_word("AXRadioButton", Some("AXTabButton")), Some("tab"));
        assert_eq!(
            role_word("AXWindow", Some("AXStandardWindow")),
            Some("standard window")
        );
        assert_eq!(role_word("AXGroup", None), None);
    }

    #[test]
    fn labels_stay_on_one_short_line() {
        assert_eq!(tidy("Play\n\tnow  "), "Play now");
        let long = "x".repeat(200);
        assert_eq!(tidy(&long).chars().count(), LABEL_CHARS + 3);
        assert!(!tidy("a\r\n0 button Delete account").contains('\n'));
    }

    #[test]
    fn typed_text_fields_are_labelled_but_never_read() {
        assert!(holds_typed_text("AXTextField", None));
        assert!(holds_typed_text("AXTextField", Some("AXSecureTextField")));
        assert!(holds_typed_text("AXTextArea", None));
        assert!(!holds_typed_text("AXButton", None));
        assert!(!holds_typed_text("AXStaticText", None));
    }

    #[test]
    fn keys_become_system_events_lines() {
        let sent = |key: &str| key_script(key).unwrap();
        assert_eq!(
            sent("Return"),
            "tell application \"System Events\" to key code 36"
        );
        assert_eq!(
            sent("space"),
            "tell application \"System Events\" to key code 49"
        );
        assert_eq!(
            sent("cmd+shift+T"),
            "tell application \"System Events\" to keystroke \"t\" using {command down, shift down}"
        );
        assert_eq!(
            sent("Control-Option-Left"),
            "tell application \"System Events\" to key code 123 using {control down, option down}"
        );
        assert_eq!(
            sent("cmd+\""),
            "tell application \"System Events\" to keystroke \"\\\"\" using {command down}"
        );
        assert!(key_script("hyper+a").is_err());
        assert!(key_script("f13x").is_err());
        assert!(key_script("").is_err());
    }
}
