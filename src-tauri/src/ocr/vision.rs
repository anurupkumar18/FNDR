//! Apple Vision OCR integration
//!
//! Uses VNRecognizeTextRequest for text extraction.

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{class, msg_send, msg_send_id};
use objc2_foundation::{NSArray, NSData, NSDictionary, NSString};
use std::ffi::c_void;

/// OCR Engine using Apple Vision
pub struct OcrEngine {
    // Reusable request object
    _initialized: bool,
}

impl OcrEngine {
    pub fn new() -> Result<Self, String> {
        // Vision framework is loaded dynamically
        Ok(Self { _initialized: true })
    }

    /// Recognize text from image data (PNG format)
    pub fn recognize(&self, image_data: &[u8]) -> Result<String, String> {
        unsafe {
            // Create NSData from image bytes
            let ns_data: Retained<NSData> =
                NSData::dataWithBytes_length(image_data.as_ptr() as *mut c_void, image_data.len());

            // Create VNImageRequestHandler
            let handler = create_image_request_handler(&ns_data)?;

            // Create and configure VNRecognizeTextRequest
            let request = create_text_request()?;

            // Perform request
            perform_request(&handler, &request)?;

            // Extract results
            let text = extract_results(&request)?;

            // Normalize text
            let normalized = normalize_text(&text);

            Ok(normalized)
        }
    }
}

unsafe fn create_image_request_handler(data: &NSData) -> Result<Retained<AnyObject>, String> {
    let cls = class!(VNImageRequestHandler);
    let options = NSDictionary::<AnyObject, AnyObject>::new();

    let handler = msg_send_id![cls, alloc];
    let handler: Retained<AnyObject> = msg_send_id![handler, initWithData:data options:&*options];
    Ok(handler)
}

unsafe fn create_text_request() -> Result<Retained<AnyObject>, String> {
    let cls = class!(VNRecognizeTextRequest);

    let request = msg_send_id![cls, alloc];
    let request: Retained<AnyObject> = msg_send_id![request, init];

    // Set recognition level to accurate (1)
    let _: () = msg_send![&request, setRecognitionLevel: 1i64];
    // Enable language correction
    let _: () = msg_send![&request, setUsesLanguageCorrection: true];

    Ok(request)
}

unsafe fn perform_request(handler: &AnyObject, request: &AnyObject) -> Result<(), String> {
    let request_retained =
        unsafe { Retained::retain(request as *const AnyObject as *mut AnyObject).unwrap() };
    let requests = NSArray::from_id_slice(&[request_retained]);

    let mut error: *mut AnyObject = std::ptr::null_mut();
    let success: bool = msg_send![handler, performRequests:&*requests error:&mut error];

    if !success {
        return Err("VNImageRequestHandler performRequests failed".to_string());
    }
    Ok(())
}

unsafe fn extract_results(request: &AnyObject) -> Result<String, String> {
    let results: *const AnyObject = msg_send![request, results];
    if results.is_null() {
        return Ok(String::new());
    }

    let count: usize = msg_send![results, count];
    let mut text_parts = Vec::with_capacity(count);

    for i in 0..count {
        let observation: *const AnyObject = msg_send![results, objectAtIndex: i];
        if observation.is_null() {
            continue;
        }

        // Get top candidate
        let candidates: *const AnyObject = msg_send![observation, topCandidates: 1usize];
        if candidates.is_null() {
            continue;
        }

        let candidate_count: usize = msg_send![candidates, count];
        if candidate_count == 0 {
            continue;
        }

        let candidate: *const AnyObject = msg_send![candidates, objectAtIndex: 0usize];
        if candidate.is_null() {
            continue;
        }

        let ns_string: *const NSString = msg_send![candidate, string];
        if !ns_string.is_null() {
            let rust_string = (*ns_string).to_string();
            text_parts.push(rust_string);
        }
    }

    Ok(text_parts.join("\n"))
}

/// Normalize OCR text: collapse whitespace, remove garbage, and filter UI noise
fn normalize_text(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut last_line = String::new();

    // Common macOS/App UI noise to ignore
    let noise_patterns = [
        "File Edit View Window Help",
        "Mon Jan",
        "Tue Feb",
        "Wed Mar",
        "Thu Apr",
        "Fri May",
        "Sat Jun",
        "Sun Jul", // Date prefixes
        "Apple Inc.",
        "System Settings",
        "Finder",
        "com.apple.",
        "com.google.",
        "Antigravity", // Filter out FNDR's own UI text
        "FNDR",
        "Login",
        "Password",
        "Sign in",
        "Sign up", // Login screens
        "Version",
        "Build",
        "Copyright", // Metadata
        "PM",
        "AM", // Time markers usually alone on a line
        "%",
        "KB",
        "MB",
        "GB", // File sizes alone
    ];

    for line in text.lines() {
        let trimmed = line.trim();

        // Skip empty lines or single characters
        if trimmed.len() < 2 {
            continue;
        }

        // Filter out common UI noise strings
        let mut is_noise = false;
        for pattern in noise_patterns {
            if trimmed.contains(pattern) {
                is_noise = true;
                break;
            }
        }
        if is_noise {
            continue;
        }

        // Skip duplicate consecutive lines
        if trimmed == last_line {
            continue;
        }

        // Skip lines that are mostly non-alphanumeric (garbage or icons)
        let alnum_count = trimmed.chars().filter(|c| c.is_alphanumeric()).count();
        if alnum_count < trimmed.len() / 2 {
            continue;
        }

        if !result.is_empty() {
            result.push('\n');
        }
        result.push_str(trimmed);
        last_line = trimmed.to_string();
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_text() {
        let input = "  Hello World  \n\nHello World\n  Test  \n!!!###\n";
        let result = normalize_text(input);
        assert_eq!(result, "Hello World\nTest");
    }
}
