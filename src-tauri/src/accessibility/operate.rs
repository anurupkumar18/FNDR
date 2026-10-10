//! The Mac side of `fndr operator-mcp` (ADR 026): reads an app's front window
//! as indexed text and acts on the exact element FNDR printed.
//!
//! Presses go through AXPress and edits through AXValue on the element
//! itself, so a click is never a screen position. Text typed without an
//! element, keys and scrolling go to the app's own focus after FNDR has made
//! that app frontmost and confirmed it stayed there. No pixels are read.
//!
//! Windows are moved through their AXPosition and AXSize. Display frames come
//! from NSScreen on the main thread, or from CoreGraphics when the main thread
//! does not answer.

use std::ffi::c_void;
use std::time::{Duration, Instant};

use objc2_app_kit::{NSApplicationActivationPolicy, NSRunningApplication};

use super::text_tree::TextTree;
use super::{
    activate_target_app, ax_copy_attr_value, ax_set_string_attr, ax_string_attr,
    enable_manual_accessibility, frontmost_pid, has_accessibility_permission, kCFBooleanTrue,
    str_to_cfstring, AXError, AXUIElementCreateApplication, AXUIElementRef,
    AXUIElementSetAttributeValue, AXUIElementSetMessagingTimeout, AxElement, AxTextTree,
    CFArrayGetCount, CFArrayGetTypeID, CFArrayGetValueAtIndex, CFBooleanGetTypeID,
    CFBooleanGetValue, CFGetTypeID, CFRelease, CFRetain, K_AX_ERROR_SUCCESS,
};
use crate::operator::layout::{Rect, Screen, Window};
use crate::operator::mcp::{AppRef, Desktop, Entry, Tree};

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXUIElementPerformAction(element: AXUIElementRef, action: super::CFStringRef) -> AXError;
    fn AXValueCreate(kind: u32, value: *const c_void) -> super::CFTypeRef;
    fn AXValueGetValue(value: super::CFTypeRef, kind: u32, out: *mut c_void) -> bool;
}

const AX_VALUE_CG_POINT: u32 = 1;
const AX_VALUE_CG_SIZE: u32 = 2;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct CgPair {
    a: f64,
    b: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct CgRect {
    origin: CgPair,
    size: CgPair,
}

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGGetActiveDisplayList(max: u32, displays: *mut u32, count: *mut u32) -> i32;
    fn CGDisplayBounds(display: u32) -> CgRect;
}

extern "C" {
    static _dispatch_main_q: c_void;
    fn dispatch_async_f(
        queue: *const c_void,
        context: *mut c_void,
        work: extern "C" fn(*mut c_void),
    );
}

/// A point or size attribute of a window.
unsafe fn ax_pair(element: AXUIElementRef, attr: &str, kind: u32) -> Option<CgPair> {
    let value = ax_copy_attr_value(element, attr).ok()?;
    if value.is_null() {
        return None;
    }
    let mut pair = CgPair::default();
    let read = AXValueGetValue(value, kind, &mut pair as *mut CgPair as *mut c_void);
    CFRelease(value);
    read.then_some(pair)
}

unsafe fn set_ax_pair(
    element: AXUIElementRef,
    attr: &str,
    kind: u32,
    pair: CgPair,
) -> Result<(), AXError> {
    let value = AXValueCreate(kind, &pair as *const CgPair as *const c_void);
    if value.is_null() {
        return Err(-1);
    }
    let name = str_to_cfstring(attr);
    let code = AXUIElementSetAttributeValue(element, name, value);
    CFRelease(name);
    CFRelease(value);
    if code == K_AX_ERROR_SUCCESS {
        Ok(())
    } else {
        Err(code)
    }
}

unsafe fn window_frame(window: AXUIElementRef) -> Option<Rect> {
    let position = ax_pair(window, "AXPosition", AX_VALUE_CG_POINT)?;
    let size = ax_pair(window, "AXSize", AX_VALUE_CG_SIZE)?;
    Some(Rect::new(position.a, position.b, size.a, size.b))
}

unsafe fn ax_bool_attr(element: AXUIElementRef, attr: &str) -> bool {
    match ax_copy_attr_value(element, attr) {
        Ok(value) if !value.is_null() => {
            let truth = CFGetTypeID(value) == CFBooleanGetTypeID() && CFBooleanGetValue(value);
            CFRelease(value);
            truth
        }
        _ => false,
    }
}

/// Windows a person arranges: standard windows and dialogs, not palettes,
/// sheets or the invisible helper windows some apps keep.
fn is_arrangeable(role: Option<&str>, subrole: Option<&str>) -> bool {
    role == Some("AXWindow")
        && matches!(subrole, None | Some("AXStandardWindow") | Some("AXDialog"))
}

/// Converts NSScreen frames, measured up from the bottom of the main display,
/// into Accessibility ones measured down from its top. The main display is
/// the one at 0,0 and is put first.
fn screens_from_appkit(frames: &[(Rect, Rect)]) -> Vec<Screen> {
    let main_height = frames
        .iter()
        .find(|(frame, _)| frame.x == 0.0 && frame.y == 0.0)
        .or(frames.first())
        .map(|(frame, _)| frame.height)
        .unwrap_or_default();
    let flip = |rect: Rect| {
        Rect::new(
            rect.x,
            main_height - rect.y - rect.height,
            rect.width,
            rect.height,
        )
    };
    let mut screens: Vec<Screen> = frames
        .iter()
        .map(|(frame, visible)| Screen {
            frame: flip(*frame),
            visible: flip(*visible),
        })
        .collect();
    screens.sort_by(|a, b| {
        let main = |screen: &Screen| !(screen.frame.x == 0.0 && screen.frame.y == 0.0);
        main(a)
            .cmp(&main(b))
            .then(a.frame.x.total_cmp(&b.frame.x))
            .then(a.frame.y.total_cmp(&b.frame.y))
    });
    screens
}

/// Whole frame and visible frame of every NSScreen. Main thread only.
fn appkit_frames() -> Option<Vec<(Rect, Rect)>> {
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};
    objc2_foundation::MainThreadMarker::new()?;
    let rect = |r: objc2_foundation::NSRect| {
        Rect::new(r.origin.x, r.origin.y, r.size.width, r.size.height)
    };
    let mut frames = Vec::new();
    unsafe {
        // Walked with an enumerator: on macOS 27 the array's `count` has an
        // encoding objc2 0.2 rejects (see `notch.rs`).
        let screens: *mut AnyObject = msg_send![class!(NSScreen), screens];
        if screens.is_null() {
            return None;
        }
        let walker: *mut AnyObject = msg_send![screens, objectEnumerator];
        if walker.is_null() {
            return None;
        }
        loop {
            let screen: *mut AnyObject = msg_send![walker, nextObject];
            if screen.is_null() {
                break;
            }
            let screen = &*(screen as *const objc2_app_kit::NSScreen);
            frames.push((rect(screen.frame()), rect(screen.visibleFrame())));
        }
    }
    (!frames.is_empty()).then_some(frames)
}

extern "C" fn send_appkit_frames(context: *mut c_void) {
    let sender = unsafe {
        Box::from_raw(context as *mut std::sync::mpsc::SyncSender<Option<Vec<(Rect, Rect)>>>)
    };
    let _ = sender.send(appkit_frames());
}

/// The menu bar's height where AppKit cannot be asked. The Dock is not
/// known there, so a window may then reach under it.
const MENU_BAR_GUESS: f64 = 25.0;

fn coregraphics_screens() -> Vec<Screen> {
    let mut ids = [0u32; 16];
    let mut count = 0u32;
    if unsafe { CGGetActiveDisplayList(ids.len() as u32, ids.as_mut_ptr(), &mut count) } != 0 {
        return Vec::new();
    }
    let mut screens: Vec<Screen> = ids[..count as usize]
        .iter()
        .map(|id| {
            let bounds = unsafe { CGDisplayBounds(*id) };
            let frame = Rect::new(
                bounds.origin.a,
                bounds.origin.b,
                bounds.size.a,
                bounds.size.b,
            );
            Screen {
                frame,
                visible: Rect::new(
                    frame.x,
                    frame.y + MENU_BAR_GUESS,
                    frame.width,
                    (frame.height - MENU_BAR_GUESS).max(1.0),
                ),
            }
        })
        .collect();
    screens.sort_by(|a, b| {
        let main = |screen: &Screen| !(screen.frame.x == 0.0 && screen.frame.y == 0.0);
        main(a).cmp(&main(b)).then(a.frame.x.total_cmp(&b.frame.x))
    });
    screens
}

/// The displays, asked of AppKit on the main thread so the menu bar and the
/// Dock are left out exactly.
pub(crate) fn current_screens() -> Vec<Screen> {
    if let Some(frames) = appkit_frames() {
        return screens_from_appkit(&frames);
    }
    if objc2_foundation::MainThreadMarker::new().is_none() {
        let (sender, receiver) = std::sync::mpsc::sync_channel::<Option<Vec<(Rect, Rect)>>>(1);
        let context = Box::into_raw(Box::new(sender)) as *mut c_void;
        unsafe {
            dispatch_async_f(
                &_dispatch_main_q as *const c_void,
                context,
                send_appkit_frames,
            )
        };
        if let Ok(Some(frames)) = receiver.recv_timeout(Duration::from_millis(500)) {
            return screens_from_appkit(frames.as_slice());
        }
    }
    coregraphics_screens()
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    /// Equal for two references to the same on-screen element.
    fn CFHash(cf: super::CFTypeRef) -> usize;
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

    fn windows(&mut self, app: &AppRef) -> Result<Vec<Window<AxElement>>, String> {
        need_permission()?;
        unsafe {
            let application = AxElement(AXUIElementCreateApplication(app.pid));
            if application.0.is_null() {
                return Err(format!("{} cannot be read.", app.name));
            }
            AXUIElementSetMessagingTimeout(application.0, 1.5);
            let list = ax_copy_attr_value(application.0, "AXWindows")
                .map_err(|code| ax_failure(code, "listing windows"))?;
            if list.is_null() {
                return Ok(Vec::new());
            }
            let mut windows = Vec::new();
            if CFGetTypeID(list) == CFArrayGetTypeID() {
                for at in 0..CFArrayGetCount(list) {
                    let raw = CFArrayGetValueAtIndex(list, at);
                    if raw.is_null() {
                        continue;
                    }
                    let window = AxElement(CFRetain(raw));
                    let role = ax_string_attr(window.0, "AXRole");
                    let subrole = ax_string_attr(window.0, "AXSubrole");
                    if !is_arrangeable(role.as_deref(), subrole.as_deref()) {
                        continue;
                    }
                    let Some(frame) = window_frame(window.0) else {
                        continue;
                    };
                    windows.push(Window {
                        title: ax_string_attr(window.0, "AXTitle").unwrap_or_default(),
                        frame,
                        minimized: ax_bool_attr(window.0, "AXMinimized"),
                        handle: window,
                    });
                }
            }
            CFRelease(list);
            Ok(windows)
        }
    }

    fn screens(&mut self) -> Vec<Screen> {
        current_screens()
    }

    fn frame(&mut self, window: &AxElement) -> Option<Rect> {
        unsafe { window_frame(window.0) }
    }

    /// Size, then position, then size again: a window moved to a smaller
    /// display may have been cut to fit before it got there.
    fn set_frame(&mut self, window: &AxElement, frame: Rect) -> Result<(), String> {
        need_permission()?;
        let size = CgPair {
            a: frame.width,
            b: frame.height,
        };
        let position = CgPair {
            a: frame.x,
            b: frame.y,
        };
        unsafe {
            set_ax_pair(window.0, "AXSize", AX_VALUE_CG_SIZE, size)
                .and_then(|()| set_ax_pair(window.0, "AXPosition", AX_VALUE_CG_POINT, position))
                .and_then(|()| set_ax_pair(window.0, "AXSize", AX_VALUE_CG_SIZE, size))
                .map_err(|code| ax_failure(code, "moved"))
        }
    }

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
            // Apps report one element under several parents (Chrome lists its
            // toolbar four times over); each is shown once.
            let mut seen = std::collections::HashSet::new();
            let mut stack = vec![(root, 0usize)];
            while let Some((node, depth)) = stack.pop() {
                if !seen.insert(CFHash(node.0)) {
                    continue;
                }
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
    fn appkit_frames_flip_into_accessibility_coordinates_with_the_main_display_first() {
        // An external display above-right of a 1512x982 laptop, listed first.
        let screens = screens_from_appkit(&[
            (
                Rect::new(1512.0, 102.0, 1920.0, 1080.0),
                Rect::new(1512.0, 102.0, 1920.0, 1055.0),
            ),
            (
                Rect::new(0.0, 0.0, 1512.0, 982.0),
                Rect::new(0.0, 70.0, 1512.0, 879.0),
            ),
        ]);
        assert_eq!(screens[0].frame, Rect::new(0.0, 0.0, 1512.0, 982.0));
        assert_eq!(
            screens[0].visible,
            Rect::new(0.0, 33.0, 1512.0, 879.0),
            "menu bar on top, Dock below"
        );
        assert_eq!(screens[1].frame, Rect::new(1512.0, -200.0, 1920.0, 1080.0));
        assert_eq!(
            screens[1].visible,
            Rect::new(1512.0, -175.0, 1920.0, 1055.0)
        );
    }

    #[test]
    fn only_windows_a_person_arranges_are_listed() {
        assert!(is_arrangeable(Some("AXWindow"), Some("AXStandardWindow")));
        assert!(is_arrangeable(Some("AXWindow"), Some("AXDialog")));
        assert!(is_arrangeable(Some("AXWindow"), None));
        assert!(!is_arrangeable(Some("AXWindow"), Some("AXFloatingWindow")));
        assert!(!is_arrangeable(Some("AXSheet"), None));
        assert!(!is_arrangeable(None, None));
    }

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

    /// Reads a real app without touching it:
    /// `FNDR_LIVE_APP="Google Chrome" cargo test --lib live_read_lists_each_element_once -- --ignored --nocapture`
    #[test]
    #[ignore = "live: reads a running app's window on this Mac"]
    fn live_read_lists_each_element_once() {
        let name = std::env::var("FNDR_LIVE_APP").unwrap_or_else(|_| "Google Chrome".to_string());
        let mut desktop = AxDesktop;
        let app = desktop
            .list_apps()
            .into_iter()
            .find(|app| app.name == name)
            .expect("the app is running");
        let started = Instant::now();
        let tree = desktop.read_tree(&app).expect("the window can be read");
        let lines: Vec<&str> = tree.entries.iter().map(|entry| entry.description.as_str()).collect();
        println!("{} lines in {:?}, truncated: {}", lines.len(), started.elapsed(), tree.truncated);
        let mut counts = std::collections::BTreeMap::new();
        for line in &lines {
            *counts.entry(*line).or_insert(0usize) += 1;
        }
        // Report roles only: the lines themselves are whatever is on screen.
        let repeated: Vec<_> = counts
            .iter()
            .filter(|(line, count)| **count > 1 && line.starts_with("text field"))
            .collect();
        println!("text fields listed more than once: {}", repeated.len());
        assert!(repeated.is_empty(), "an address bar or field is listed twice");
    }
}
