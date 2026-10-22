//! The fillable fields of the window in front, with their labels.
//!
//! Smart Fill uses this to fill a whole form at once. A field is a text box,
//! a pop-up menu (a web `<select>`) or a date box; password fields, buttons,
//! checkboxes and anything disabled are left out. A label comes from the
//! element's own title, description or placeholder, then from the element the
//! page says labels it, then from the last piece of text before it.

use std::time::{Duration, Instant};

use serde::Serialize;

use super::text_tree::TextTree;
use super::{
    ax_copy_attr_value, ax_string_attr, enable_manual_accessibility, kCFBooleanTrue,
    paste_into_frontmost_app, str_to_cfstring, AXUIElementCreateApplication,
    AXUIElementSetAttributeValue, AXUIElementSetMessagingTimeout, AxElement, AxTextTree, CFRelease,
    K_AX_ERROR_SUCCESS,
};

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXUIElementIsAttributeSettable(
        element: super::AXUIElementRef,
        attribute: super::CFStringRef,
        settable: *mut bool,
    ) -> super::AXError;
}

/// Whether the person could type into the element: its value can be set.
unsafe fn value_is_settable(element: &AxElement) -> bool {
    let attribute = str_to_cfstring("AXValue");
    let mut settable = false;
    let code = AXUIElementIsAttributeSettable(element.0, attribute, &mut settable);
    CFRelease(attribute);
    code == K_AX_ERROR_SUCCESS && settable
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldKind {
    Text,
    Select,
    Date,
}

#[derive(Clone)]
pub struct FormField {
    /// Position in reading order, the id the model refers to.
    pub index: usize,
    pub kind: FieldKind,
    pub label: String,
    pub placeholder: String,
    /// The web page's own id for the field (`AXDOMIdentifier`), when it has one.
    pub dom_id: String,
    pub value: String,
    /// Menu choices of a pop-up, in order.
    pub options: Vec<String>,
    pub(crate) element: AxElement,
}

// An AXUIElement is a thread-safe CoreFoundation object; the Accessibility API
// may be called from any thread, and a field is only ever used by one task at a
// time (found on one blocking thread, written on another).
unsafe impl Send for FormField {}

impl std::fmt::Debug for FormField {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FormField")
            .field("index", &self.index)
            .field("kind", &self.kind)
            .field("label", &self.label)
            .field("dom_id", &self.dom_id)
            .finish()
    }
}

const TEXT_ROLES: [&str; 3] = ["AXTextField", "AXTextArea", "AXComboBox"];
const DATE_ROLES: [&str; 2] = ["AXDateField", "AXDatePicker"];
const LABEL_ROLES: [&str; 2] = ["AXStaticText", "AXHeading"];

/// Role and subrole of an element decide whether it is a field and what kind.
/// `None` for anything a form does not ask the person to type or pick.
pub(crate) fn field_kind(role: &str, subrole: &str) -> Option<FieldKind> {
    if subrole == "AXSecureTextField" {
        return None;
    }
    if role == "AXPopUpButton" {
        return Some(FieldKind::Select);
    }
    if DATE_ROLES.contains(&role) {
        return Some(FieldKind::Date);
    }
    TEXT_ROLES.contains(&role).then_some(FieldKind::Text)
}

/// A date box shows its format as the value until it is filled.
pub(crate) fn looks_like_date_placeholder(value: &str) -> bool {
    let value = value.to_ascii_lowercase();
    value.contains("mm") && value.contains("dd") && value.contains("yyyy")
}

/// A date box's calendar button is a pop-up in the tree but not a field.
pub(crate) fn is_picker_button(label: &str, dom_id: &str) -> bool {
    dom_id == "picker" || label.to_ascii_lowercase().starts_with("show date picker")
}

/// `value` is empty, or only a date box's format hint.
pub(crate) fn is_blank(value: &str) -> bool {
    value.trim().is_empty() || looks_like_date_placeholder(value)
}

unsafe fn label_text(element: &AxElement) -> String {
    for attribute in ["AXTitle", "AXDescription", "AXPlaceholderValue", "AXHelp"] {
        if let Some(text) = ax_string_attr(element.0, attribute).filter(|t| !t.trim().is_empty()) {
            return text.trim().to_string();
        }
    }
    if let Ok(labelled_by) = ax_copy_attr_value(element.0, "AXTitleUIElement") {
        if !labelled_by.is_null() {
            let text = ax_string_attr(labelled_by, "AXValue")
                .or_else(|| ax_string_attr(labelled_by, "AXTitle"))
                .unwrap_or_default();
            CFRelease(labelled_by);
            if !text.trim().is_empty() {
                return text.trim().to_string();
            }
        }
    }
    String::new()
}

fn menu_options(tree: &AxTextTree, select: &AxElement) -> Vec<String> {
    let mut options = Vec::new();
    let mut stack: Vec<(AxElement, usize)> = tree
        .children(select)
        .into_iter()
        .rev()
        .map(|child| (child, 1))
        .collect();
    while let Some((node, depth)) = stack.pop() {
        let role = tree.role(&node).unwrap_or_default();
        if role == "AXMenuItem" {
            let title = unsafe { ax_string_attr(node.0, "AXTitle") }.unwrap_or_default();
            if !title.trim().is_empty() {
                options.push(title.trim().to_string());
            }
        }
        if depth < 4 && options.len() < 60 {
            for child in tree.children(&node).into_iter().rev() {
                stack.push((child, depth + 1));
            }
        }
    }
    options
}

/// The fillable fields of `pid`'s focused window in reading order, within
/// `max_time` and `max_nodes`. Empty without Accessibility permission or when
/// the window has no such fields.
pub fn discover(pid: i32, max_time: Duration, max_nodes: usize) -> Vec<FormField> {
    if !super::has_accessibility_permission() {
        return Vec::new();
    }
    let started = Instant::now();
    let tree = AxTextTree;
    let mut fields: Vec<FormField> = Vec::new();
    unsafe {
        let application = AxElement(AXUIElementCreateApplication(pid));
        if application.0.is_null() {
            return fields;
        }
        let messaging_timeout = (max_time.as_secs_f32() / 4.0).max(0.05);
        AXUIElementSetMessagingTimeout(application.0, messaging_timeout);
        enable_manual_accessibility(application.0);
        let Ok(window) = ax_copy_attr_value(application.0, "AXFocusedWindow") else {
            return fields;
        };
        if window.is_null() {
            return fields;
        }
        let root = AxElement(window);
        AXUIElementSetMessagingTimeout(root.0, messaging_timeout);

        let mut last_text = String::new();
        let mut visited = 0_usize;
        let mut stack = vec![root];
        while let Some(node) = stack.pop() {
            visited += 1;
            if visited > max_nodes || started.elapsed() > max_time {
                break;
            }
            let role = tree.role(&node).unwrap_or_default();
            let subrole = tree.subrole(&node).unwrap_or_default();
            if LABEL_ROLES.contains(&role.as_str()) {
                if let Some(text) = tree.text(&node).filter(|t| !t.trim().is_empty()) {
                    last_text = text.trim().to_string();
                }
            }
            if let Some(kind) = field_kind(&role, &subrole) {
                let enabled = ax_string_attr(node.0, "AXEnabled")
                    .map(|v| v != "0" && v != "false")
                    .unwrap_or(true);
                let editable = kind != FieldKind::Text || value_is_settable(&node);
                if enabled && editable {
                    let mut label = label_text(&node);
                    if label.is_empty() {
                        label = last_text.clone();
                    }
                    let dom_id = ax_string_attr(node.0, "AXDOMIdentifier").unwrap_or_default();
                    if is_picker_button(&label, &dom_id) {
                        continue;
                    }
                    let placeholder =
                        ax_string_attr(node.0, "AXPlaceholderValue").unwrap_or_default();
                    let value = ax_string_attr(node.0, "AXValue").unwrap_or_default();
                    let options = if kind == FieldKind::Select {
                        menu_options(&tree, &node)
                    } else {
                        Vec::new()
                    };
                    fields.push(FormField {
                        index: fields.len(),
                        kind,
                        label,
                        placeholder,
                        dom_id,
                        value,
                        options,
                        element: node.clone(),
                    });
                }
            }
            if role != "AXPopUpButton" {
                for child in tree.children(&node).into_iter().rev() {
                    stack.push(child);
                }
            }
        }
    }
    fields
}

/// Gives the field keyboard focus.
pub fn focus(field: &FormField) -> bool {
    unsafe {
        let attribute = str_to_cfstring("AXFocused");
        let code = AXUIElementSetAttributeValue(field.element.0, attribute, kCFBooleanTrue);
        CFRelease(attribute);
        code == K_AX_ERROR_SUCCESS
    }
}

/// What the field holds now.
pub fn current_value(field: &FormField) -> String {
    unsafe { ax_string_attr(field.element.0, "AXValue") }.unwrap_or_default()
}

/// Escapes text for an AppleScript string literal.
pub(crate) fn applescript_quote(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Types `text` as key presses into the focused control, the way a person
/// would: dates fill digit by digit and a pop-up jumps to the choice that
/// starts with it.
pub(crate) fn type_keystrokes(text: &str) -> Result<(), String> {
    let script = format!(
        "tell application \"System Events\" to keystroke {}",
        applescript_quote(text)
    );
    let status = std::process::Command::new("osascript")
        .arg("-e")
        .arg(script)
        .status()
        .map_err(|error| format!("Could not type into the form: {error}"))?;
    status
        .success()
        .then_some(())
        .ok_or_else(|| "Typing into the form was refused.".to_string())
}

/// Puts `text` in the field: pasted into a text box (so the page sees a real
/// paste), typed into a date box or a pop-up. The focus is set first.
pub fn write(field: &FormField, text: &str) -> Result<(), String> {
    if !focus(field) {
        return Err("The field would not take focus.".to_string());
    }
    std::thread::sleep(Duration::from_millis(60));
    match field.kind {
        FieldKind::Text => paste_into_frontmost_app(text, false),
        FieldKind::Date | FieldKind::Select => type_keystrokes(text),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applescript_text_is_quoted_and_escaped() {
        assert_eq!(applescript_quote("Sam"), "\"Sam\"");
        assert_eq!(applescript_quote("a\"b"), "\"a\\\"b\"");
        assert_eq!(applescript_quote("a\\b"), "\"a\\\\b\"");
    }

    #[test]
    fn the_date_pickers_button_is_not_a_field() {
        assert!(is_picker_button(
            "Show date picker Date of birth bday",
            "picker"
        ));
        assert!(is_picker_button("Show date picker", ""));
        assert!(!is_picker_button("Country country", "country"));
    }

    #[test]
    fn text_boxes_pop_ups_and_dates_are_fields() {
        assert_eq!(field_kind("AXTextField", ""), Some(FieldKind::Text));
        assert_eq!(field_kind("AXTextArea", ""), Some(FieldKind::Text));
        assert_eq!(field_kind("AXPopUpButton", ""), Some(FieldKind::Select));
        assert_eq!(field_kind("AXDateField", ""), Some(FieldKind::Date));
    }

    #[test]
    fn passwords_buttons_and_checkboxes_are_not() {
        assert_eq!(field_kind("AXTextField", "AXSecureTextField"), None);
        assert_eq!(field_kind("AXButton", ""), None);
        assert_eq!(field_kind("AXCheckBox", ""), None);
        assert_eq!(field_kind("AXStaticText", ""), None);
    }

    #[test]
    fn a_date_format_hint_counts_as_blank() {
        assert!(is_blank(""));
        assert!(is_blank("  "));
        assert!(is_blank("mm/dd/yyyy"));
        assert!(!is_blank("03/14/2002"));
        assert!(!is_blank("Sam"));
    }
}
