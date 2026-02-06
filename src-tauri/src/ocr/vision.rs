//! Apple Vision OCR integration
//!
//! Uses VNRecognizeTextRequest for high-quality text extraction from screenshots.
//! Provides configurable filtering and intelligent text normalization.

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{class, msg_send, msg_send_id};
use objc2_foundation::{NSArray, NSData, NSDictionary, NSString};
use regex::Regex;
use std::ffi::c_void;
use std::sync::Arc;

/// Errors that can occur during OCR operations
#[derive(Debug, thiserror::Error)]
pub enum OcrError {
    #[error("Vision framework initialization failed: {0}")]
    InitializationError(String),

    #[error("Image processing failed: {0}")]
    ImageProcessingError(String),

    #[error("Text recognition failed: {0}")]
    RecognitionError(String),

    #[error("Result extraction failed: {0}")]
    ExtractionError(String),
}

/// Recognition quality level
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecognitionLevel {
    /// Fast recognition, lower accuracy
    Fast = 0,
    /// Accurate recognition, slower
    Accurate = 1,
}

/// OCR configuration options
#[derive(Debug, Clone)]
pub struct OcrConfig {
    /// Recognition quality level
    pub recognition_level: RecognitionLevel,

    /// Enable automatic language correction
    pub language_correction: bool,

    /// Minimum confidence threshold (0.0 to 1.0)
    pub min_confidence: f32,

    /// Enable aggressive noise filtering
    pub aggressive_filtering: bool,

    /// Minimum line length to keep (characters)
    pub min_line_length: usize,

    /// Custom noise patterns to filter
    pub custom_noise_patterns: Vec<String>,

    /// Enable duplicate line removal
    pub remove_duplicates: bool,

    /// Preserve formatting (newlines, spacing)
    pub preserve_formatting: bool,
}

impl Default for OcrConfig {
    fn default() -> Self {
        Self {
            recognition_level: RecognitionLevel::Accurate,
            language_correction: true,
            min_confidence: 0.5,
            aggressive_filtering: true,
            min_line_length: 2,
            custom_noise_patterns: Vec::new(),
            remove_duplicates: true,
            preserve_formatting: false,
        }
    }
}

impl OcrConfig {
    /// Create a fast configuration for real-time processing
    pub fn fast() -> Self {
        Self {
            recognition_level: RecognitionLevel::Fast,
            language_correction: false,
            min_confidence: 0.3,
            aggressive_filtering: false,
            min_line_length: 1,
            custom_noise_patterns: Vec::new(),
            remove_duplicates: false,
            preserve_formatting: true,
        }
    }

    /// Create a high-quality configuration for document processing
    pub fn high_quality() -> Self {
        Self {
            recognition_level: RecognitionLevel::Accurate,
            language_correction: true,
            min_confidence: 0.7,
            aggressive_filtering: false,
            min_line_length: 1,
            custom_noise_patterns: Vec::new(),
            remove_duplicates: false,
            preserve_formatting: true,
        }
    }
}

/// Recognized text with metadata
#[derive(Debug, Clone)]
pub struct RecognizedText {
    /// The extracted text content
    pub text: String,

    /// Average confidence score
    pub confidence: f32,

    /// Number of text blocks recognized
    pub block_count: usize,

    /// Original text before normalization
    pub raw_text: String,
}

/// OCR Engine using Apple Vision framework
pub struct OcrEngine {
    config: Arc<OcrConfig>,
    noise_filter: NoiseFilter,
}

impl OcrEngine {
    /// Create a new OCR engine with default configuration
    pub fn new() -> Result<Self, OcrError> {
        Self::with_config(OcrConfig::default())
    }

    /// Create a new OCR engine with custom configuration
    pub fn with_config(config: OcrConfig) -> Result<Self, OcrError> {
        // Verify Vision framework is available
        // Verify Vision framework is available (logic simplified for objc2 safety)
        let _cls = unsafe { class!(VNImageRequestHandler) };

        let noise_filter = NoiseFilter::new(
            config.aggressive_filtering,
            config.custom_noise_patterns.clone(),
        );

        tracing::debug!("OCR engine initialized with config: {:?}", config);

        Ok(Self {
            config: Arc::new(config),
            noise_filter,
        })
    }

    /// Update the configuration
    pub fn update_config(&mut self, config: OcrConfig) {
        self.noise_filter = NoiseFilter::new(
            config.aggressive_filtering,
            config.custom_noise_patterns.clone(),
        );
        self.config = Arc::new(config);
    }

    /// Get the current configuration
    pub fn config(&self) -> &OcrConfig {
        &self.config
    }

    /// Recognize text from image data (PNG format)
    pub fn recognize(&self, image_data: &[u8]) -> Result<String, OcrError> {
        let result = self.recognize_with_metadata(image_data)?;
        Ok(result.text)
    }

    /// Recognize text with full metadata
    pub fn recognize_with_metadata(&self, image_data: &[u8]) -> Result<RecognizedText, OcrError> {
        unsafe {
            // Create NSData from image bytes
            let ns_data =
                NSData::dataWithBytes_length(image_data.as_ptr() as *mut c_void, image_data.len());

            // Create VNImageRequestHandler
            let handler = self.create_image_request_handler(&ns_data)?;

            // Create and configure VNRecognizeTextRequest
            let request = self.create_text_request()?;

            // Perform request
            self.perform_request(&handler, &request)?;

            // Extract results with confidence scores
            let (raw_text, avg_confidence, block_count) = self.extract_results(&request)?;

            // Normalize text according to config
            let normalized = self.normalize_text(&raw_text);

            Ok(RecognizedText {
                text: normalized,
                confidence: avg_confidence,
                block_count,
                raw_text,
            })
        }
    }

    unsafe fn create_image_request_handler(
        &self,
        data: &NSData,
    ) -> Result<Retained<AnyObject>, OcrError> {
        let cls = class!(VNImageRequestHandler);
        let options = NSDictionary::<AnyObject, AnyObject>::new();

        let handler = msg_send_id![cls, alloc];
        let handler: Retained<AnyObject> =
            msg_send_id![handler, initWithData:data options:&*options];

        Ok(handler)
    }

    unsafe fn create_text_request(&self) -> Result<Retained<AnyObject>, OcrError> {
        let cls = class!(VNRecognizeTextRequest);

        let request = msg_send_id![cls, alloc];
        let request: Retained<AnyObject> = msg_send_id![request, init];

        // Set recognition level
        let level = self.config.recognition_level as i64;
        let _: () = msg_send![&request, setRecognitionLevel: level];

        // Set language correction
        let _: () = msg_send![&request, setUsesLanguageCorrection: self.config.language_correction];

        // Set minimum text height (helps filter noise)
        let min_height: f32 = 0.0;
        let _: () = msg_send![&request, setMinimumTextHeight: min_height];

        Ok(request)
    }

    unsafe fn perform_request(
        &self,
        handler: &AnyObject,
        request: &AnyObject,
    ) -> Result<(), OcrError> {
        let request_retained = Retained::retain(request as *const AnyObject as *mut AnyObject)
            .ok_or_else(|| OcrError::RecognitionError("Failed to retain request".to_string()))?;

        let requests = NSArray::from_id_slice(&[request_retained]);

        let mut error: *mut AnyObject = std::ptr::null_mut();
        let success: bool = msg_send![handler, performRequests:&*requests error:&mut error];

        if !success {
            let error_msg = if !error.is_null() {
                let description: *const NSString = msg_send![error, localizedDescription];
                if !description.is_null() {
                    (*description).to_string()
                } else {
                    "Unknown error".to_string()
                }
            } else {
                "Request failed".to_string()
            };

            return Err(OcrError::RecognitionError(error_msg));
        }

        Ok(())
    }

    unsafe fn extract_results(
        &self,
        request: &AnyObject,
    ) -> Result<(String, f32, usize), OcrError> {
        let results: *const AnyObject = msg_send![request, results];
        if results.is_null() {
            return Ok((String::new(), 0.0, 0));
        }

        let count: usize = msg_send![results, count];
        if count == 0 {
            return Ok((String::new(), 0.0, 0));
        }

        let mut text_parts = Vec::with_capacity(count);
        let mut confidence_scores = Vec::with_capacity(count);

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

            // Extract confidence score
            let confidence: f32 = msg_send![candidate, confidence];

            // Filter by minimum confidence
            if confidence < self.config.min_confidence {
                tracing::trace!("Skipping low-confidence text: {:.2}", confidence);
                continue;
            }

            // Extract text
            let ns_string: *const NSString = msg_send![candidate, string];
            if !ns_string.is_null() {
                let rust_string = (*ns_string).to_string();
                text_parts.push(rust_string);
                confidence_scores.push(confidence);
            }
        }

        let avg_confidence = if confidence_scores.is_empty() {
            0.0
        } else {
            confidence_scores.iter().sum::<f32>() / confidence_scores.len() as f32
        };

        Ok((text_parts.join("\n"), avg_confidence, text_parts.len()))
    }

    /// Normalize OCR text according to configuration
    fn normalize_text(&self, text: &str) -> String {
        if self.config.preserve_formatting {
            return text.to_string();
        }

        let mut result = String::with_capacity(text.len());
        let mut last_line = String::new();

        for line in text.lines() {
            let trimmed = line.trim();

            // Skip empty lines or too-short lines
            if trimmed.len() < self.config.min_line_length {
                continue;
            }

            // Apply noise filtering
            if self.noise_filter.is_noise(trimmed) {
                tracing::trace!("Filtered noise: {}", trimmed);
                continue;
            }

            // Skip duplicate consecutive lines
            if self.config.remove_duplicates && trimmed == last_line {
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
}

impl Default for OcrEngine {
    fn default() -> Self {
        Self::new().expect("Failed to initialize OCR engine")
    }
}

/// Intelligent noise filter for OCR text
struct NoiseFilter {
    aggressive: bool,
    base_patterns: Vec<String>,
    custom_patterns: Vec<String>,
}

impl NoiseFilter {
    fn new(aggressive: bool, custom_patterns: Vec<String>) -> Self {
        let base_patterns = vec![
            // Common macOS UI elements
            "File Edit View Window Help".to_string(),
            "Apple Inc.".to_string(),
            "System Settings".to_string(),
            "System Preferences".to_string(),
            "Finder".to_string(),
            // App identifiers
            "com.apple.".to_string(),
            "com.google.".to_string(),
            "com.microsoft.".to_string(),
            // Time/date fragments (when alone)
            "AM".to_string(),
            "PM".to_string(),
            // Common metadata
            "Version".to_string(),
            "Build".to_string(),
            "Copyright ©".to_string(),
            "All Rights Reserved".to_string(),
            // Login/auth UI
            "Sign in".to_string(),
            "Sign up".to_string(),
            "Log in".to_string(),
            "Log out".to_string(),
            "Forgot password".to_string(),
            // Generic UI
            "Loading...".to_string(),
            "Please wait".to_string(),
            "OK".to_string(),
            "Cancel".to_string(),
        ];

        Self {
            aggressive,
            base_patterns,
            custom_patterns,
        }
    }

    fn is_noise(&self, text: &str) -> bool {
        // Check custom patterns first (user-defined)
        for pattern in &self.custom_patterns {
            if text.contains(pattern) {
                return true;
            }
        }

        // Check base patterns
        for pattern in &self.base_patterns {
            if text.contains(pattern) {
                return true;
            }
        }

        if !self.aggressive {
            return false;
        }

        // Aggressive filtering: additional heuristics

        // Filter lines with mostly special characters
        let alnum_count = text.chars().filter(|c| c.is_alphanumeric()).count();
        let total_count = text.chars().count();
        if total_count > 0 && (alnum_count as f32 / total_count as f32) < 0.5 {
            return true;
        }

        // Filter single words that are common UI elements
        let words: Vec<&str> = text.split_whitespace().collect();
        if words.len() == 1 {
            let word = words[0].to_lowercase();
            let ui_words = [
                "close",
                "minimize",
                "maximize",
                "menu",
                "settings",
                "preferences",
                "about",
                "help",
                "quit",
                "exit",
                "new",
                "open",
                "save",
                "print",
                "copy",
                "paste",
                "cut",
                "undo",
                "redo",
                "search",
                "find",
            ];

            if ui_words.contains(&word.as_str()) {
                return true;
            }
        }

        // Filter timestamp-like patterns (HH:MM)
        let time_pattern = regex::Regex::new(r"^\d{1,2}:\d{2}(\s*(AM|PM))?$").ok();
        if let Some(re) = time_pattern {
            if re.is_match(text) {
                return true;
            }
        }

        // Filter percentage-only lines
        if text.trim().ends_with('%')
            && text
                .trim()
                .chars()
                .all(|c| c.is_numeric() || c == '%' || c == '.')
        {
            return true;
        }

        // Filter file size indicators (e.g., "123 KB")
        let size_pattern = regex::Regex::new(r"^\d+(\.\d+)?\s*(B|KB|MB|GB|TB)$").ok();
        if let Some(re) = size_pattern {
            if re.is_match(text.trim()) {
                return true;
            }
        }

        // Filter common date patterns when alone
        let date_prefixes = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
        let month_names = [
            "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
        ];

        for prefix in &date_prefixes {
            if text.starts_with(prefix) && text.len() < 20 {
                return true;
            }
        }

        for month in &month_names {
            if text.contains(month) && text.len() < 15 {
                return true;
            }
        }

        false
    }
}

/// Statistics about OCR performance
#[derive(Debug, Clone)]
pub struct OcrStats {
    pub total_blocks: usize,
    pub filtered_blocks: usize,
    pub average_confidence: f32,
    pub processing_time_ms: u128,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_noise_filter_basic() {
        let filter = NoiseFilter::new(true, vec![]);

        assert!(filter.is_noise("File Edit View Window Help"));
        assert!(filter.is_noise("com.apple.finder"));
        assert!(filter.is_noise("Sign in"));
        assert!(!filter.is_noise("This is actual content"));
    }

    #[test]
    fn test_noise_filter_aggressive() {
        let filter = NoiseFilter::new(true, vec![]);

        // Time patterns
        assert!(filter.is_noise("3:45 PM"));
        assert!(filter.is_noise("14:30"));

        // File sizes
        assert!(filter.is_noise("123 KB"));
        assert!(filter.is_noise("45.6 MB"));

        // Percentages
        assert!(filter.is_noise("95%"));

        // Single UI words
        assert!(filter.is_noise("Close"));
        assert!(filter.is_noise("Menu"));

        // Real content should pass
        assert!(!filter.is_noise("Implement new authentication system"));
    }

    #[test]
    fn test_custom_patterns() {
        let filter = NoiseFilter::new(false, vec!["FNDR".to_string(), "CustomApp".to_string()]);

        assert!(filter.is_noise("FNDR Dashboard"));
        assert!(filter.is_noise("CustomApp Settings"));
        assert!(!filter.is_noise("Regular text"));
    }

    #[test]
    fn test_config_presets() {
        let fast = OcrConfig::fast();
        assert_eq!(fast.recognition_level, RecognitionLevel::Fast);
        assert!(!fast.language_correction);

        let hq = OcrConfig::high_quality();
        assert_eq!(hq.recognition_level, RecognitionLevel::Accurate);
        assert!(hq.language_correction);
    }
}
