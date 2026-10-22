//! The focused text field as Smart Fill sees it: whether it may be written,
//! where its caret is on screen, and how text gets in.
//!
//! Text goes in word by word through the field's AXValue when the app honours
//! that (native text fields and views do), and as one paste when it does not
//! (some web and Electron fields). A paste restores the clipboard it replaced.

use std::ffi::c_void;
use std::process::Command;
use std::time::Duration;

use super::{
    ax_copy_attr_value, ax_set_string_attr, ax_string_attr, enable_manual_accessibility,
    frontmost_pid, has_accessibility_permission, paste_into_frontmost_app, str_to_cfstring,
    AXUIElementCreateApplication, AXUIElementRef, CFRelease, CFTypeRef, K_AX_ERROR_SUCCESS,
};

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXValueCreate(kind: u32, value: *const c_void) -> CFTypeRef;
    fn AXValueGetValue(value: CFTypeRef, kind: u32, out: *mut c_void) -> bool;
    fn AXUIElementCopyParameterizedAttributeValue(
        element: AXUIElementRef,
        attribute: super::CFStringRef,
        parameter: CFTypeRef,
        result: *mut CFTypeRef,
    ) -> super::AXError;
}

const AX_VALUE_CG_POINT: u32 = 1;
const AX_VALUE_CG_SIZE: u32 = 2;
const AX_VALUE_CG_RECT: u32 = 3;
const AX_VALUE_CF_RANGE: u32 = 4;

/// Roles of elements a person types a sentence into.
const WRITABLE_ROLES: [&str; 3] = ["AXTextField", "AXTextArea", "AXComboBox"];

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

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct CfRange {
    location: isize,
    length: isize,
}

/// A rectangle in global screen points, origin at the top left of the main
/// display (the space Accessibility reports and Tauri positions windows in).
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct ScreenRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// The field that had focus when Smart Fill was asked.
#[derive(Debug, Clone)]
pub struct FocusedField {
    pub pid: i32,
    pub role: String,
    pub value: String,
    /// A field in web content (a browser or an Electron app). Setting its
    /// AXValue changes what is drawn but not what the page's script holds,
    /// so these fields are written by paste only.
    pub web: bool,
}

/// Why a field was refused. `show` is whether the person is told: a password
/// field or a missing permission is worth a line; a double tap with no text
/// box focused is just a stray tap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    pub message: &'static str,
    pub show: bool,
}

fn refusal(role: &str, subrole: &str) -> Option<Refusal> {
    if subrole == "AXSecureTextField" {
        return Some(Refusal {
            message: "Smart Fill never writes into a password field.",
            show: true,
        });
    }
    if !WRITABLE_ROLES.contains(&role) {
        return Some(Refusal {
            message: "Click into a text box first, then press Command twice.",
            show: false,
        });
    }
    None
}

unsafe fn focused_element(pid: i32) -> Result<AXUIElementRef, String> {
    let application = AXUIElementCreateApplication(pid);
    if application.is_null() {
        return Err("The app in front can't be read.".to_string());
    }
    // Electron and Chromium build their accessibility tree only when asked.
    enable_manual_accessibility(application);
    let focused = ax_copy_attr_value(application, "AXFocusedUIElement");
    CFRelease(application);
    match focused {
        Ok(element) if !element.is_null() => Ok(element),
        _ => Err("Click into a text box first, then press Command twice.".to_string()),
    }
}

/// The app in front, when there is one.
pub fn frontmost() -> Option<i32> {
    unsafe { frontmost_pid() }.or_else(super::workspace_frontmost_pid)
}

/// Runs `work`, then puts back the plain text the clipboard held before it
/// (pasting into several fields leaves the last value there).
pub fn with_clipboard_restored<T>(work: impl FnOnce() -> T) -> T {
    let previous = read_clipboard_text().filter(|text| !text.is_empty());
    let result = work();
    if let Some(previous) = previous {
        std::thread::sleep(Duration::from_millis(250));
        write_clipboard_text(&previous);
    }
    result
}

/// The focused field of the app in front, or the reason Smart Fill must not
/// touch it. Password fields and anything that is not a text box are refused.
pub fn focused_field() -> Result<FocusedField, Refusal> {
    const STRAY: Refusal = Refusal {
        message: "Click into a text box first, then press Command twice.",
        show: false,
    };
    if !has_accessibility_permission() {
        return Err(Refusal {
            message: "Smart Fill needs the Accessibility permission for FNDR.",
            show: true,
        });
    }
    unsafe {
        let pid = frontmost_pid().ok_or(STRAY)?;
        let element = focused_element(pid).map_err(|_| STRAY)?;
        let role = ax_string_attr(element, "AXRole").unwrap_or_default();
        let subrole = ax_string_attr(element, "AXSubrole").unwrap_or_default();
        let value = ax_string_attr(element, "AXValue").unwrap_or_default();
        let web = is_web_element(element);
        CFRelease(element);
        if let Some(reason) = refusal(&role, &subrole) {
            return Err(reason);
        }
        Ok(FocusedField {
            pid,
            role,
            value,
            web,
        })
    }
}

/// Web content exposes DOM attributes that native controls do not.
unsafe fn is_web_element(element: AXUIElementRef) -> bool {
    ["AXDOMClassList", "AXDOMIdentifier"].iter().any(|name| {
        match ax_copy_attr_value(element, name) {
            Ok(value) if !value.is_null() => {
                CFRelease(value);
                true
            }
            _ => false,
        }
    })
}

unsafe fn pair(element: AXUIElementRef, attr: &str, kind: u32) -> Option<CgPair> {
    let value = ax_copy_attr_value(element, attr).ok()?;
    if value.is_null() {
        return None;
    }
    let mut out = CgPair::default();
    let ok = AXValueGetValue(value, kind, &mut out as *mut CgPair as *mut c_void);
    CFRelease(value);
    ok.then_some(out)
}

unsafe fn bounds_for_range(element: AXUIElementRef, range: CfRange) -> Option<CgRect> {
    let parameter = AXValueCreate(AX_VALUE_CF_RANGE, &range as *const CfRange as *const c_void);
    if parameter.is_null() {
        return None;
    }
    let attribute = str_to_cfstring("AXBoundsForRange");
    let mut result: CFTypeRef = std::ptr::null_mut();
    let code =
        AXUIElementCopyParameterizedAttributeValue(element, attribute, parameter, &mut result);
    CFRelease(attribute);
    CFRelease(parameter);
    if code != K_AX_ERROR_SUCCESS || result.is_null() {
        return None;
    }
    let mut rect = CgRect::default();
    let ok = AXValueGetValue(
        result,
        AX_VALUE_CG_RECT,
        &mut rect as *mut CgRect as *mut c_void,
    );
    CFRelease(result);
    (ok && rect.size.b > 0.0).then_some(rect)
}

/// Where the caret is, as a thin rectangle one line tall, or the whole field's
/// frame when the app doesn't say. `None` when neither is readable.
pub fn caret_rect(pid: i32) -> Option<ScreenRect> {
    unsafe {
        let element = focused_element(pid).ok()?;
        let selection = ax_copy_attr_value(element, "AXSelectedTextRange")
            .ok()
            .filter(|value| !value.is_null());
        let mut caret = None;
        if let Some(selection) = selection {
            let mut range = CfRange::default();
            let ok = AXValueGetValue(
                selection,
                AX_VALUE_CF_RANGE,
                &mut range as *mut CfRange as *mut c_void,
            );
            CFRelease(selection);
            if ok {
                // A caret has no width; measure the character before it and
                // take that box's right edge.
                caret = if range.location > 0 {
                    bounds_for_range(
                        element,
                        CfRange {
                            location: range.location - 1,
                            length: 1,
                        },
                    )
                    .map(|rect| ScreenRect {
                        x: rect.origin.a + rect.size.a,
                        y: rect.origin.b,
                        width: 2.0,
                        height: rect.size.b,
                    })
                } else {
                    bounds_for_range(
                        element,
                        CfRange {
                            location: 0,
                            length: 0,
                        },
                    )
                    .or_else(|| {
                        bounds_for_range(
                            element,
                            CfRange {
                                location: 0,
                                length: 1,
                            },
                        )
                    })
                    .map(|rect| ScreenRect {
                        x: rect.origin.a,
                        y: rect.origin.b,
                        width: 2.0,
                        height: rect.size.b,
                    })
                };
            }
        }
        let frame = || {
            let position = pair(element, "AXPosition", AX_VALUE_CG_POINT)?;
            let size = pair(element, "AXSize", AX_VALUE_CG_SIZE)?;
            Some(ScreenRect {
                x: position.a + 10.0,
                y: position.b + 6.0,
                width: 2.0,
                height: 18.0_f64.min(size.b),
            })
        };
        let rect = caret.or_else(frame);
        CFRelease(element);
        rect
    }
}

/// Sets the focused field's whole value through Accessibility and reads it
/// back. `Err` when the field changed, the app refused, or it kept something
/// else (so the caller can paste instead).
pub fn set_value(pid: i32, expected_role: &str, text: &str) -> Result<(), String> {
    unsafe {
        if frontmost_pid() != Some(pid) {
            return Err("The app lost focus.".to_string());
        }
        let element = focused_element(pid)?;
        let role = ax_string_attr(element, "AXRole").unwrap_or_default();
        if role != expected_role {
            CFRelease(element);
            return Err("The focus moved to another field.".to_string());
        }
        let written = ax_set_string_attr(element, "AXValue", text);
        let kept = ax_string_attr(element, "AXValue").unwrap_or_default();
        CFRelease(element);
        match written {
            Ok(()) if kept == text => Ok(()),
            Ok(()) => Err("The app did not keep the text.".to_string()),
            Err(code) => Err(format!("The app refused the text (AX error {code}).")),
        }
    }
}

fn read_clipboard_text() -> Option<String> {
    let output = Command::new("pbpaste").output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8(output.stdout).ok())
        .flatten()
}

fn write_clipboard_text(text: &str) {
    use std::io::Write;
    let Ok(mut child) = Command::new("pbcopy")
        .stdin(std::process::Stdio::piped())
        .spawn()
    else {
        return;
    };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(text.as_bytes());
    }
    let _ = child.wait();
    crate::capture::clipboard::note_self_copy(text);
}

/// Replaces the field's text with one paste, then puts back the text the
/// clipboard held before. A clipboard that held something other than plain
/// text (an image, files) can't be put back and is left holding the reply.
pub fn paste_replacing(pid: i32, text: &str) -> Result<(), String> {
    unsafe {
        if frontmost_pid() != Some(pid) {
            return Err("The app lost focus.".to_string());
        }
    }
    let previous = read_clipboard_text();
    let result = paste_into_frontmost_app(text, true);
    if let Some(previous) = previous.filter(|text| !text.is_empty()) {
        std::thread::sleep(Duration::from_millis(250));
        write_clipboard_text(&previous);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::refusal;

    #[test]
    fn password_fields_are_refused_with_a_visible_reason() {
        let refused = refusal("AXTextField", "AXSecureTextField").unwrap();
        assert!(refused.show);
        assert!(refused.message.contains("password"));
    }

    #[test]
    fn text_boxes_are_allowed() {
        for role in ["AXTextField", "AXTextArea", "AXComboBox"] {
            assert!(refusal(role, "").is_none(), "{role}");
        }
    }

    #[test]
    fn buttons_and_pages_are_refused_quietly() {
        for role in ["AXButton", "AXWebArea", "AXStaticText", ""] {
            let refused = refusal(role, "").expect(role);
            assert!(!refused.show, "{role}");
        }
    }
}
