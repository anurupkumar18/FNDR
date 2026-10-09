//! macOS Accessibility API integration for auto-fill field detection and text injection.
//!
//! Uses AXUIElement APIs from the ApplicationServices framework to:
//! 1. Identify the currently focused input field's label in any app
//! 2. Inject text directly into that field without requiring keyboard focus

mod operate;
mod text_tree;

pub use operate::AxDesktop;

use crate::ocr::{OcrConfig, OcrEngine};
use objc2_app_kit::NSWorkspace;
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::ffi::{c_char, c_void, CString};
use std::time::Duration;

// ── CF/AX type aliases ────────────────────────────────────────────────────────

type CFTypeRef = *mut c_void;
type CFStringRef = CFTypeRef;
type CFAllocatorRef = *mut c_void;
type AXUIElementRef = CFTypeRef;
type AXError = i32;
type CFIndex = i64;
type PidT = i32;

const K_AX_ERROR_SUCCESS: AXError = 0;
const K_CF_STRING_ENCODING_UTF8: u32 = 0x08000100;

// ── ApplicationServices / CoreFoundation FFI ──────────────────────────────────

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXUIElementCreateSystemWide() -> AXUIElementRef;
    fn AXUIElementCreateApplication(pid: PidT) -> AXUIElementRef;
    fn AXUIElementGetPid(element: AXUIElementRef, pid: *mut PidT) -> AXError;
    fn AXUIElementCopyAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: *mut CFTypeRef,
    ) -> AXError;
    fn AXUIElementSetAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: CFTypeRef,
    ) -> AXError;
    fn AXIsProcessTrusted() -> bool;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFStringCreateWithCString(
        alloc: CFAllocatorRef,
        c_str: *const c_char,
        encoding: u32,
    ) -> CFStringRef;
    fn CFStringGetCString(
        the_string: CFStringRef,
        buffer: *mut c_char,
        buffer_size: CFIndex,
        encoding: u32,
    ) -> bool;
    fn CFStringGetLength(the_string: CFStringRef) -> CFIndex;
    fn CFStringGetMaximumSizeForEncoding(length: CFIndex, encoding: u32) -> CFIndex;
    fn CFGetTypeID(cf: CFTypeRef) -> usize;
    fn CFStringGetTypeID() -> usize;
    fn CFRelease(cf: CFTypeRef);
}

// ── Helpers ───────────────────────────────────────────────────────────────────

unsafe fn str_to_cfstring(s: &str) -> CFStringRef {
    match CString::new(s) {
        Ok(c) => {
            CFStringCreateWithCString(std::ptr::null_mut(), c.as_ptr(), K_CF_STRING_ENCODING_UTF8)
        }
        Err(_) => std::ptr::null_mut(),
    }
}

unsafe fn cfstring_to_rust(cf: CFStringRef) -> Option<String> {
    if cf.is_null() {
        return None;
    }
    if CFGetTypeID(cf) != CFStringGetTypeID() {
        return None;
    }
    let len = CFStringGetLength(cf);
    if len == 0 {
        return Some(String::new());
    }
    let max = CFStringGetMaximumSizeForEncoding(len, K_CF_STRING_ENCODING_UTF8) + 1;
    let mut buf: Vec<c_char> = vec![0; max as usize];
    if CFStringGetCString(cf, buf.as_mut_ptr(), max, K_CF_STRING_ENCODING_UTF8) {
        let s = std::ffi::CStr::from_ptr(buf.as_ptr());
        Some(s.to_string_lossy().into_owned())
    } else {
        None
    }
}

unsafe fn ax_copy_attr_value(element: AXUIElementRef, attr: &str) -> Result<CFTypeRef, AXError> {
    if element.is_null() {
        return Err(-1);
    }
    let attr_cf = str_to_cfstring(attr);
    if attr_cf.is_null() {
        return Err(-1);
    }
    let mut value: CFTypeRef = std::ptr::null_mut();
    let err = AXUIElementCopyAttributeValue(element, attr_cf, &mut value);
    CFRelease(attr_cf);
    if err == K_AX_ERROR_SUCCESS {
        Ok(value)
    } else {
        Err(err)
    }
}

/// Read a string attribute from an AXUIElement. Returns None on any error.
unsafe fn ax_string_attr(element: AXUIElementRef, attr: &str) -> Option<String> {
    if element.is_null() {
        return None;
    }
    let Ok(value) = ax_copy_attr_value(element, attr) else {
        return None;
    };
    if value.is_null() {
        return None;
    }
    let result = cfstring_to_rust(value);
    CFRelease(value);
    result
}

/// Set a string attribute on an AXUIElement. Returns Ok(()) on success.
unsafe fn ax_set_string_attr(
    element: AXUIElementRef,
    attr: &str,
    text: &str,
) -> Result<(), AXError> {
    if element.is_null() {
        return Err(-1);
    }
    let attr_cf = str_to_cfstring(attr);
    if attr_cf.is_null() {
        return Err(-1);
    }
    let value_cf = str_to_cfstring(text);
    if value_cf.is_null() {
        CFRelease(attr_cf);
        return Err(-1);
    }
    let err = AXUIElementSetAttributeValue(element, attr_cf, value_cf);
    CFRelease(attr_cf);
    CFRelease(value_cf);

    if err == K_AX_ERROR_SUCCESS {
        Ok(())
    } else {
        Err(err)
    }
}

// ── Public types ──────────────────────────────────────────────────────────────

/// Contextual information about the currently focused input field.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldContext {
    /// The field's human-readable label (AXTitle / AXDescription / AXPlaceholderValue).
    pub label: String,
    /// AXPlaceholderValue if found separately from the label.
    pub placeholder: String,
    /// Name of the application that owns the focused field.
    pub app_name: String,
    /// Bundle identifier of the target application when available.
    pub bundle_id: Option<String>,
    /// Window title of the currently frontmost target app.
    pub window_title: String,
    /// Current field value, if readable via accessibility.
    pub current_value: String,
    /// OCR excerpt from the current screen used as a last-resort hint.
    pub screen_context: String,
    /// Best-effort label inferred from OCR when AX metadata is sparse.
    pub inferred_label: String,
}

/// Internal state preserved between the hotkey trigger and the inject call.
#[derive(Debug, Clone)]
struct AutofillTarget {
    /// PID of the application that owned the field at trigger time.
    pid: PidT,
    /// AXRole of the captured element, for identity verification at inject time.
    element_role: String,
}

#[derive(Debug, Clone)]
struct StoredAutofillTarget {
    request_id: Option<u64>,
    target: AutofillTarget,
}

#[derive(Debug)]
pub(crate) struct PendingAutofillTarget(Option<AutofillTarget>);

pub(crate) fn clear_autofill_target() {
    *AUTOFILL_TARGET.lock() = None;
}

pub(crate) fn clear_autofill_target_for_request(request_id: u64) {
    let mut stored = AUTOFILL_TARGET.lock();
    if stored
        .as_ref()
        .is_some_and(|target| target.request_id == Some(request_id))
    {
        *stored = None;
    }
}

pub(crate) fn commit_autofill_target(request_id: u64, pending: PendingAutofillTarget) {
    *AUTOFILL_TARGET.lock() = pending.0.map(|target| StoredAutofillTarget {
        request_id: Some(request_id),
        target,
    });
}

static AUTOFILL_TARGET: Lazy<Mutex<Option<StoredAutofillTarget>>> = Lazy::new(|| Mutex::new(None));

fn truncate_chars(input: &str, max_chars: usize) -> String {
    let mut chars = input.chars();
    let head: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{head}...")
    } else {
        head
    }
}

fn normalize_context_excerpt(text: &str) -> String {
    let mut lines = Vec::new();
    for line in text.lines() {
        let compact = line.split_whitespace().collect::<Vec<_>>().join(" ");
        let compact = compact.trim();
        if compact.len() < 2 {
            continue;
        }
        if matches!(
            compact.to_ascii_lowercase().as_str(),
            "new tab" | "dashboard" | "home" | "settings" | "preferences"
        ) {
            continue;
        }
        if lines
            .last()
            .is_some_and(|prev: &String| prev.eq_ignore_ascii_case(compact))
        {
            continue;
        }
        lines.push(compact.to_string());
        if lines.len() >= 12 {
            break;
        }
    }
    truncate_chars(&lines.join("\n"), 1200)
}

fn infer_label_from_screen_context(text: &str) -> String {
    const FIELD_HINTS: [&str; 18] = [
        "policy",
        "member",
        "claim",
        "group",
        "tax",
        "ein",
        "subscriber",
        "routing",
        "account",
        "phone",
        "email",
        "address",
        "zip",
        "postal",
        "birth",
        "dob",
        "number",
        "id",
    ];

    let mut best = String::new();
    let mut best_score = 0i32;

    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.len() > 72 {
            continue;
        }

        let candidate = line
            .split(':')
            .next()
            .unwrap_or(line)
            .split("  ")
            .next()
            .unwrap_or(line)
            .trim();

        if candidate.len() < 3 || candidate.len() > 40 {
            continue;
        }

        let lower = candidate.to_ascii_lowercase();
        if lower.starts_with("http")
            || lower.contains('@')
            || lower
                .chars()
                .all(|ch| ch.is_ascii_digit() || matches!(ch, '-' | '/' | '.'))
        {
            continue;
        }

        let word_count = candidate.split_whitespace().count();
        if !(1..=5).contains(&word_count) {
            continue;
        }

        let mut score = 0i32;
        if line.contains(':') {
            score += 3;
        }
        if candidate
            .chars()
            .all(|ch| ch.is_alphanumeric() || matches!(ch, ' ' | '#' | '/' | '-' | '(' | ')'))
        {
            score += 1;
        }
        if FIELD_HINTS.iter().any(|hint| lower.contains(hint)) {
            score += 4;
        }
        if lower.contains("number") || lower.ends_with(" id") || lower.ends_with(" code") {
            score += 2;
        }
        if lower.split_whitespace().count() >= 2 {
            score += 1;
        }

        if score > best_score {
            best_score = score;
            best = candidate.to_string();
        }
    }

    if best_score >= 4 {
        best
    } else {
        String::new()
    }
}

fn capture_screen_context_image() -> Option<Vec<u8>> {
    match crate::capture::macos::capture_screen() {
        Ok(image) => Some(image),
        Err(err) => {
            tracing::debug!("autofill: screen OCR capture failed: {err}");
            None
        }
    }
}

fn recognize_screen_context(image: &[u8]) -> String {
    let engine = match OcrEngine::with_config(OcrConfig::high_quality()) {
        Ok(engine) => engine,
        Err(err) => {
            tracing::debug!("autofill: OCR engine unavailable: {err}");
            return String::new();
        }
    };

    match engine.recognize_with_metadata(&image) {
        Ok(text) => normalize_context_excerpt(&text.0.text),
        Err(err) => {
            tracing::debug!("autofill: OCR inference failed: {err}");
            String::new()
        }
    }
}

unsafe fn frontmost_pid() -> Option<PidT> {
    let system_el = AXUIElementCreateSystemWide();
    if system_el.is_null() {
        return None;
    }

    let focused_app = match ax_copy_attr_value(system_el, "AXFocusedApplication") {
        Ok(focused_app) => focused_app,
        Err(_) => {
            CFRelease(system_el);
            return None;
        }
    };
    CFRelease(system_el);

    if focused_app.is_null() {
        return None;
    }

    let mut pid: PidT = 0;
    let err = AXUIElementGetPid(focused_app, &mut pid);
    CFRelease(focused_app);
    if err == K_AX_ERROR_SUCCESS && pid > 0 {
        Some(pid)
    } else {
        None
    }
}

/// Paste text into the frontmost app via the clipboard, replacing its content.
fn type_text_into_frontmost_app(text: &str) -> Result<(), String> {
    paste_into_frontmost_app(text, true)
}

/// Paste text into the frontmost app via the clipboard, after selecting all
/// when `replace` is set. The clipboard keeps the text afterwards.
/// Uses pbcopy + osascript keystroke — avoids enigo CGEventPost which can SIGABRT
/// on macOS when called outside the right event context.
fn paste_into_frontmost_app(text: &str, replace: bool) -> Result<(), String> {
    use std::io::Write;

    // 1. Write text to clipboard via pbcopy.
    let mut child = std::process::Command::new("pbcopy")
        .stdin(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to start pbcopy: {e}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(text.as_bytes())
            .map_err(|e| format!("Failed to write to pbcopy: {e}"))?;
    }
    child
        .wait()
        .map_err(|e| format!("pbcopy did not finish: {e}"))?;

    std::thread::sleep(Duration::from_millis(30));

    // 2. Select all existing content when replacing, then paste.
    let select_all = if replace {
        "keystroke \"a\" using command down\ndelay 0.03\n"
    } else {
        ""
    };
    let script = format!(
        "tell application \"System Events\"\n{select_all}keystroke \"v\" using command down\nend tell"
    );

    let status = std::process::Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .status()
        .map_err(|e| format!("osascript paste failed to start: {e}"))?;

    if status.success() {
        Ok(())
    } else {
        Err("osascript keystroke paste returned non-zero exit code".to_string())
    }
}

fn activate_target_app(pid: PidT) -> Result<(), String> {
    let script = format!(
        r#"tell application "System Events"
set frontmost of (first process whose unix id is {pid}) to true
end tell"#
    );

    let status = std::process::Command::new("osascript")
        .arg("-e")
        .arg(script)
        .status()
        .map_err(|err| format!("Failed to run activation script: {err}"))?;

    if !status.success() {
        return Err("Failed to activate target app for autofill".to_string());
    }

    // Let macOS complete focus handoff before synthesizing keystrokes.
    std::thread::sleep(Duration::from_millis(100));
    Ok(())
}

pub fn restore_target_app_focus() {
    if let Some(stored) = AUTOFILL_TARGET.lock().clone() {
        let _ = activate_target_app(stored.target.pid);
    }
}

pub(crate) fn restore_target_app_focus_for_request(request_id: u64) {
    let target = AUTOFILL_TARGET
        .lock()
        .as_ref()
        .filter(|stored| stored.request_id == Some(request_id))
        .map(|stored| stored.target.clone());
    if let Some(target) = target {
        let _ = activate_target_app(target.pid);
    }
}

/// Extract a label from AXLinkedUIElements (the `<label>` elements associated with an input).
unsafe fn ax_linked_label(element: AXUIElementRef) -> Option<String> {
    // AXLinkedUIElements is a CFArrayRef of AXUIElementRefs
    let attr_cf = str_to_cfstring("AXLinkedUIElements");
    if attr_cf.is_null() {
        return None;
    }
    let mut value: CFTypeRef = std::ptr::null_mut();
    let err = AXUIElementCopyAttributeValue(element, attr_cf, &mut value);
    CFRelease(attr_cf);

    if err != K_AX_ERROR_SUCCESS || value.is_null() {
        return None;
    }

    // We need CFArrayGetCount and CFArrayGetValueAtIndex — declare them inline
    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFArrayGetCount(array: CFTypeRef) -> CFIndex;
        fn CFArrayGetValueAtIndex(array: CFTypeRef, idx: CFIndex) -> CFTypeRef;
    }

    let count = CFArrayGetCount(value);
    let mut result = None;
    for i in 0..count {
        let el = CFArrayGetValueAtIndex(value, i);
        if el.is_null() {
            continue;
        }
        // Each linked element is an AXUIElementRef — try to get its title
        if let Some(t) = ax_string_attr(el, "AXTitle").filter(|s| !s.trim().is_empty()) {
            result = Some(t);
            break;
        }
        if let Some(t) = ax_string_attr(el, "AXValue").filter(|s| !s.trim().is_empty()) {
            result = Some(t);
            break;
        }
    }
    CFRelease(value);
    result
}

// ── Core public functions ─────────────────────────────────────────────────────

/// Returns true if FNDR has accessibility permission.
pub fn has_accessibility_permission() -> bool {
    unsafe { AXIsProcessTrusted() }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct FocusedWindowSnapshot {
    pub title: Option<String>,
    pub document_url: Option<String>,
}

/// The focused app's pid and window, read through Accessibility. Unlike
/// `NSWorkspace.frontmostApplication`, this does not depend on the main run
/// loop having processed activation notifications.
pub(crate) fn ax_frontmost() -> Option<(PidT, FocusedWindowSnapshot)> {
    if !has_accessibility_permission() {
        return None;
    }
    unsafe {
        let pid = frontmost_pid()?;
        let application = AXUIElementCreateApplication(pid);
        if application.is_null() {
            return None;
        }
        let snapshot = window_snapshot_for_application(application);
        CFRelease(application);
        Some((pid, snapshot))
    }
}

fn workspace_frontmost_pid() -> Option<PidT> {
    unsafe {
        NSWorkspace::sharedWorkspace()
            .frontmostApplication()
            .map(|app| app.processIdentifier())
            .filter(|pid| *pid > 0)
    }
}

fn accessibility_text_target_allowed(pid: PidT, own_pid: PidT) -> bool {
    pid > 0 && own_pid > 0 && pid != own_pid
}

fn expected_pid_remained_frontmost(
    expected_pid: PidT,
    before_snapshot: Option<PidT>,
    after_snapshot: Option<PidT>,
) -> bool {
    expected_pid > 0
        && before_snapshot == Some(expected_pid)
        && after_snapshot == Some(expected_pid)
}

unsafe fn window_snapshot_for_application(application: AXUIElementRef) -> FocusedWindowSnapshot {
    let (window_title, window_document_url) = ax_copy_attr_value(application, "AXFocusedWindow")
        .ok()
        .map(|window| {
            let title = ax_string_attr(window, "AXTitle");
            let document_url = ax_string_attr(window, "AXDocument");
            CFRelease(window);
            (title, document_url)
        })
        .unwrap_or_default();
    let title = window_title.or_else(|| ax_string_attr(application, "AXTitle"));
    let document_url = window_document_url.or_else(|| ax_string_attr(application, "AXDocument"));

    FocusedWindowSnapshot {
        title,
        document_url,
    }
}

/// Read the active window title and document URL without launching a helper process.
/// An expected process ID prevents a stale Accessibility focus target from being used.
pub(crate) fn focused_window_snapshot(expected_pid: Option<PidT>) -> Option<FocusedWindowSnapshot> {
    if !has_accessibility_permission() {
        return None;
    }

    unsafe {
        if let Some(expected_pid) = expected_pid {
            let before_snapshot = workspace_frontmost_pid();
            if before_snapshot != Some(expected_pid) || expected_pid <= 0 {
                return None;
            }

            // Query the known application directly. In some host environments the
            // system-wide AXFocusedApplication lookup returns kAXErrorCannotComplete
            // even though the frontmost application's own AX tree is available.
            let application = AXUIElementCreateApplication(expected_pid);
            if application.is_null() {
                return None;
            }
            let snapshot = window_snapshot_for_application(application);
            CFRelease(application);

            let after_snapshot = workspace_frontmost_pid();
            return expected_pid_remained_frontmost(expected_pid, before_snapshot, after_snapshot)
                .then_some(snapshot);
        }

        let system_el = AXUIElementCreateSystemWide();
        if system_el.is_null() {
            return None;
        }
        let focused_app = match ax_copy_attr_value(system_el, "AXFocusedApplication") {
            Ok(focused_app) => focused_app,
            Err(_) => {
                CFRelease(system_el);
                return None;
            }
        };
        CFRelease(system_el);
        if focused_app.is_null() {
            return None;
        }

        let mut pid: PidT = 0;
        let pid_matches = AXUIElementGetPid(focused_app, &mut pid) == K_AX_ERROR_SUCCESS && pid > 0;
        if !pid_matches {
            CFRelease(focused_app);
            return None;
        }

        let snapshot = window_snapshot_for_application(focused_app);
        CFRelease(focused_app);

        Some(snapshot)
    }
}

// ── Focused window text (VS-15) ───────────────────────────────────────────────

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXUIElementSetMessagingTimeout(element: AXUIElementRef, seconds: f32) -> AXError;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRetain(cf: CFTypeRef) -> CFTypeRef;
    fn CFArrayGetCount(array: CFTypeRef) -> CFIndex;
    fn CFArrayGetValueAtIndex(array: CFTypeRef, idx: CFIndex) -> CFTypeRef;
    fn CFArrayGetTypeID() -> usize;
    static kCFBooleanTrue: CFTypeRef;
}

/// Chromium and Electron apps build their Accessibility tree only after a
/// client asks for it. AXManualAccessibility is the documented opt-in that
/// does not change what the person sees (unlike AXEnhancedUserInterface).
unsafe fn enable_manual_accessibility(application: AXUIElementRef) {
    for name in ["AXManualAccessibility", "AXEnhancedUserInterface"] {
        let attr = str_to_cfstring(name);
        if !attr.is_null() {
            let _ = AXUIElementSetAttributeValue(application, attr, kCFBooleanTrue);
            CFRelease(attr);
        }
    }
}

/// An owned AXUIElement reference, released on drop.
pub struct AxElement(AXUIElementRef);

impl Drop for AxElement {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { CFRelease(self.0) };
        }
    }
}

struct AxTextTree;

impl text_tree::TextTree for AxTextTree {
    type Node = AxElement;

    fn role(&self, node: &AxElement) -> Option<String> {
        unsafe { ax_string_attr(node.0, "AXRole") }
    }

    fn subrole(&self, node: &AxElement) -> Option<String> {
        unsafe { ax_string_attr(node.0, "AXSubrole") }
    }

    fn text(&self, node: &AxElement) -> Option<String> {
        unsafe {
            ax_string_attr(node.0, "AXValue")
                .filter(|text| !text.trim().is_empty())
                .or_else(|| ax_string_attr(node.0, "AXTitle"))
        }
    }

    fn children(&self, node: &AxElement) -> Vec<AxElement> {
        unsafe {
            let Ok(array) = ax_copy_attr_value(node.0, "AXChildren") else {
                return Vec::new();
            };
            if array.is_null() {
                return Vec::new();
            }
            let mut children = Vec::new();
            if CFGetTypeID(array) == CFArrayGetTypeID() {
                for index in 0..CFArrayGetCount(array) {
                    let child = CFArrayGetValueAtIndex(array, index);
                    if !child.is_null() {
                        children.push(AxElement(CFRetain(child)));
                    }
                }
            }
            CFRelease(array);
            children
        }
    }
}

/// Text read from the focused window's Accessibility tree, counts included so
/// callers can log without keeping content.
#[derive(Debug, Clone)]
pub struct FocusedText {
    pub text: String,
    pub nodes_visited: usize,
    pub secure_fields_skipped: usize,
    pub from_web_area: bool,
    pub role_counts: std::collections::BTreeMap<String, usize>,
    pub truncated: bool,
    pub elapsed_ms: u64,
}

/// `focused_text` for whichever process is frontmost right now.
pub fn frontmost_focused_text(max_chars: usize) -> Option<FocusedText> {
    focused_text(workspace_frontmost_pid()?, max_chars)
}

/// Read the focused window's on-screen text for process `pid` (reading order,
/// at most `max_chars`, at most 50 ms or 4,000 nodes). Returns `None` without
/// permission, when `pid` is not the frontmost process before and after the
/// read, or when the window exposes no text. Secure text fields are never read.
/// Callers run the capture privacy gates before calling this.
pub fn focused_text(pid: i32, max_chars: usize) -> Option<FocusedText> {
    if !accessibility_text_target_allowed(pid, std::process::id() as PidT)
        || !has_accessibility_permission()
        || workspace_frontmost_pid() != Some(pid)
    {
        return None;
    }
    let started = std::time::Instant::now();
    let outcome = unsafe {
        let application = AxElement(AXUIElementCreateApplication(pid));
        if application.0.is_null() {
            return None;
        }
        AXUIElementSetMessagingTimeout(application.0, 0.05);
        enable_manual_accessibility(application.0);
        let window = AxElement(ax_copy_attr_value(application.0, "AXFocusedWindow").ok()?);
        if window.0.is_null() {
            return None;
        }
        AXUIElementSetMessagingTimeout(window.0, 0.05);
        text_tree::collect_text(&AxTextTree, &window, text_tree::Budget::new(max_chars))
    };
    if !expected_pid_remained_frontmost(pid, Some(pid), workspace_frontmost_pid()) {
        return None;
    }
    if outcome.text.trim().is_empty() && outcome.nodes_visited == 0 {
        return None;
    }
    Some(FocusedText {
        role_counts: outcome.role_counts,
        truncated: outcome.stop != text_tree::StopReason::Complete,
        text: outcome.text,
        nodes_visited: outcome.nodes_visited,
        secure_fields_skipped: outcome.secure_fields_skipped,
        from_web_area: outcome.from_web_area,
        elapsed_ms: started.elapsed().as_millis() as u64,
    })
}

/// Capture the focused input field's context from the currently frontmost application.
/// Stores the target PID for later use by `inject_text`.
///
/// Must be called while the target field still has focus (i.e., in the hotkey handler
/// before the FNDR window is raised).
pub(crate) fn capture_focused_context_snapshot(
) -> Result<(FieldContext, Option<Vec<u8>>, PendingAutofillTarget), String> {
    if !has_accessibility_permission() {
        return Err("Accessibility permission not granted. Enable FNDR in System Settings → Privacy → Accessibility.".to_string());
    }

    unsafe {
        let frontmost = crate::capture::macos::get_frontmost_app_info();
        // 1. Get the system-wide focused application element
        let system_el = AXUIElementCreateSystemWide();
        if system_el.is_null() {
            return Err("Failed to create system-wide AX element".to_string());
        }

        // 2. Get the focused application element
        let focused_app = match ax_copy_attr_value(system_el, "AXFocusedApplication") {
            Ok(focused_app) => {
                CFRelease(system_el);
                focused_app
            }
            Err(_) => {
                CFRelease(system_el);
                return Err("No focused application found".to_string());
            }
        };

        if focused_app.is_null() {
            return Err("No focused application found".to_string());
        }

        // 3. Get the focused UI element within that app
        let focused_el =
            ax_copy_attr_value(focused_app, "AXFocusedUIElement").unwrap_or(std::ptr::null_mut());

        // Get PID from the focused application AXUIElement
        let mut pid: PidT = 0;
        let pid_err = AXUIElementGetPid(focused_app, &mut pid);
        let pid_opt = if pid_err == K_AX_ERROR_SUCCESS && pid > 0 {
            Some(pid)
        } else {
            None
        };
        CFRelease(focused_app);

        if focused_el.is_null() {
            return Err("No focused UI element found — click into a text field first".to_string());
        }

        // 4. Try label attributes in priority order (direct, then linked label element, then parent)
        let label = None
            .or_else(|| ax_string_attr(focused_el, "AXTitle").filter(|s| !s.trim().is_empty()))
            .or_else(|| {
                ax_string_attr(focused_el, "AXDescription").filter(|s| !s.trim().is_empty())
            })
            .or_else(|| {
                ax_string_attr(focused_el, "AXPlaceholderValue").filter(|s| !s.trim().is_empty())
            })
            .or_else(|| ax_string_attr(focused_el, "AXLabelValue").filter(|s| !s.trim().is_empty()))
            // AXLinkedUIElements: label elements associated with this input (<label for="…">)
            .or_else(|| ax_linked_label(focused_el))
            // Parent element title as last resort (catches some Electron/web patterns)
            .or_else(|| {
                let Ok(parent) = ax_copy_attr_value(focused_el, "AXParent") else {
                    return None;
                };
                if parent.is_null() {
                    return None;
                }
                let t = ax_string_attr(parent, "AXTitle")
                    .filter(|s| !s.trim().is_empty())
                    .or_else(|| {
                        ax_string_attr(parent, "AXDescription").filter(|s| !s.trim().is_empty())
                    });
                CFRelease(parent);
                t
            })
            .unwrap_or_default();

        let placeholder = ax_string_attr(focused_el, "AXPlaceholderValue").unwrap_or_default();
        let current_value = ax_string_attr(focused_el, "AXValue").unwrap_or_default();
        // Read identity attrs before releasing — ax_string_attr reads from the pointer.
        let element_role = ax_string_attr(focused_el, "AXRole").unwrap_or_default();

        CFRelease(focused_el);

        // 5. Return target identity for an epoch-guarded commit by the caller.
        // A superseded capture must never overwrite the field owned by a
        // newer Autofill request.
        let pending_target = PendingAutofillTarget(pid_opt.map(|p| AutofillTarget {
            pid: p,
            element_role: element_role.clone(),
        }));

        // Snapshot pixels only when AX metadata is absent. OCR is deliberately
        // deferred so callers can release short native window-exclusion gates
        // before the 3-5 second Vision pass.
        let screen_image = label
            .is_empty()
            .then(capture_screen_context_image)
            .flatten();

        // label may be empty for web content (Chrome, Electron) — callers handle that case.
        Ok((
            FieldContext {
                label: if !label.is_empty() {
                    label
                } else {
                    placeholder.clone()
                },
                placeholder,
                app_name: frontmost.app_name,
                bundle_id: frontmost.bundle_id,
                window_title: frontmost.window_title,
                current_value,
                screen_context: String::new(),
                inferred_label: String::new(),
            },
            screen_image,
            pending_target,
        ))
    }
}

pub(crate) fn finish_focused_context_ocr(
    mut context: FieldContext,
    screen_image: Option<Vec<u8>>,
) -> FieldContext {
    if let Some(image) = screen_image {
        context.screen_context = recognize_screen_context(&image);
        context.inferred_label = infer_label_from_screen_context(&context.screen_context);
    }
    context
}

pub fn capture_focused_context() -> Result<FieldContext, String> {
    clear_autofill_target();
    let (context, screen_image, pending_target) = capture_focused_context_snapshot()?;
    *AUTOFILL_TARGET.lock() = pending_target.0.map(|target| StoredAutofillTarget {
        request_id: None,
        target,
    });
    Ok(finish_focused_context_ocr(context, screen_image))
}

/// Inject text into the field that was focused at trigger time.
/// Prefers system-style typing when the target app is still frontmost, with an
/// AX value-set fallback for cases where FNDR had to take focus for preview.
pub fn inject_text_into_field(text: &str, prefer_typed: bool) -> Result<(), String> {
    let stored = AUTOFILL_TARGET
        .lock()
        .clone()
        .ok_or_else(|| "No autofill target stored — trigger Option+F first".to_string())?;
    inject_text_into_target(text, prefer_typed, stored.target)
}

pub(crate) fn inject_text_into_field_for_request(
    request_id: u64,
    text: &str,
    prefer_typed: bool,
) -> Result<(), String> {
    let target = AUTOFILL_TARGET
        .lock()
        .as_ref()
        .filter(|stored| stored.request_id == Some(request_id))
        .map(|stored| stored.target.clone())
        .ok_or_else(|| "This Autofill request is no longer active.".to_string())?;
    inject_text_into_target(text, prefer_typed, target)
}

fn inject_text_into_target(
    text: &str,
    prefer_typed: bool,
    target: AutofillTarget,
) -> Result<(), String> {
    let target_is_frontmost = unsafe { frontmost_pid() == Some(target.pid) };
    let mut typed_error: Option<String> = None;

    if prefer_typed {
        if !target_is_frontmost {
            if let Err(err) = activate_target_app(target.pid) {
                typed_error = Some(err);
            }
        }

        if unsafe { frontmost_pid() == Some(target.pid) } {
            match type_text_into_frontmost_app(text) {
                Ok(()) => return Ok(()),
                Err(err) => typed_error = Some(err),
            }
        } else if typed_error.is_none() {
            typed_error = Some("Target app could not be activated for typed injection".to_string());
        }
    }

    unsafe {
        let app_el = AXUIElementCreateApplication(target.pid);
        if app_el.is_null() {
            return Err(format!(
                "Failed to create AX element for PID {}",
                target.pid
            ));
        }

        let focused_el =
            ax_copy_attr_value(app_el, "AXFocusedUIElement").unwrap_or(std::ptr::null_mut());
        CFRelease(app_el);

        if focused_el.is_null() {
            return Err(
                "Target field is no longer accessible — please click the field and try again"
                    .to_string(),
            );
        }

        // Verify the focused element is still the one we captured, not a different field.
        // Compare AXRole (and label when non-empty) to detect focus drift.
        if !target.element_role.is_empty() {
            let current_role = ax_string_attr(focused_el, "AXRole").unwrap_or_default();
            if !current_role.is_empty() && current_role != target.element_role {
                CFRelease(focused_el);
                return Err(format!(
                    "The focused element changed (was {}, now {}) — focus drifted. Click the field and press ⌥F again.",
                    target.element_role, current_role
                ));
            }
        }

        let result = ax_set_string_attr(focused_el, "AXValue", text);
        CFRelease(focused_el);

        match result {
            Ok(()) => Ok(()),
            Err(code) if target_is_frontmost => match type_text_into_frontmost_app(text) {
                Ok(()) => Ok(()),
                Err(err) => Err(format!(
                    "Auto-fill failed with AX error {code}, and system typing also failed: {}",
                    typed_error.unwrap_or(err)
                )),
            },
            Err(code) => {
                if let Ok(()) = activate_target_app(target.pid) {
                    if let Ok(()) = type_text_into_frontmost_app(text) {
                        return Ok(());
                    }
                }
                if let Some(err) = typed_error {
                    Err(format!(
                        "System typing failed ({err}) and AX value injection failed with code {code}"
                    ))
                } else {
                    Err(format!(
                        "AXUIElementSetAttributeValue failed with code {code} — the target app may require manual confirmation or keyboard typing"
                    ))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{accessibility_text_target_allowed, expected_pid_remained_frontmost};

    #[test]
    fn accessibility_text_skips_fndr_own_webview_pid() {
        assert!(!accessibility_text_target_allowed(42, 42));
        assert!(accessibility_text_target_allowed(7, 42));
        assert!(!accessibility_text_target_allowed(0, 42));
    }

    #[test]
    fn direct_pid_snapshot_requires_same_frontmost_process_before_and_after() {
        assert!(expected_pid_remained_frontmost(42, Some(42), Some(42)));
    }

    #[test]
    fn direct_pid_snapshot_rejects_stale_or_changed_frontmost_process() {
        assert!(!expected_pid_remained_frontmost(42, Some(7), Some(42)));
        assert!(!expected_pid_remained_frontmost(42, Some(42), Some(7)));
        assert!(!expected_pid_remained_frontmost(42, None, Some(42)));
        assert!(!expected_pid_remained_frontmost(42, Some(42), None));
    }

    #[test]
    fn direct_pid_snapshot_rejects_invalid_expected_pid() {
        assert!(!expected_pid_remained_frontmost(0, Some(0), Some(0)));
        assert!(!expected_pid_remained_frontmost(-1, Some(-1), Some(-1)));
    }
}
