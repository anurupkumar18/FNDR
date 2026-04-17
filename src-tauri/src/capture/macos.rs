//! macOS-specific capture functionality
//!
//! Uses Core Graphics for screen capture and AppKit for app info.

use image::ImageEncoder;
use objc2_app_kit::NSWorkspace;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct FrontmostAppContext {
    pub app_name: String,
    pub bundle_id: Option<String>,
    pub window_title: String,
}

#[derive(Clone)]
struct ScriptCacheEntry {
    app_name: String,
    value: Option<String>,
    cached_at: Instant,
}

static WINDOW_TITLE_CACHE: OnceLock<Mutex<Option<ScriptCacheEntry>>> = OnceLock::new();
static URL_CACHE: OnceLock<Mutex<Option<ScriptCacheEntry>>> = OnceLock::new();

fn cache_get(
    cache: &OnceLock<Mutex<Option<ScriptCacheEntry>>>,
    app_name: &str,
    ttl: Duration,
) -> Option<String> {
    let guard = cache.get_or_init(|| Mutex::new(None)).lock().ok()?;
    let entry = guard.as_ref()?;
    if !entry.app_name.eq_ignore_ascii_case(app_name) {
        return None;
    }
    if entry.cached_at.elapsed() > ttl {
        return None;
    }
    entry.value.clone()
}

fn cache_put(
    cache: &OnceLock<Mutex<Option<ScriptCacheEntry>>>,
    app_name: &str,
    value: Option<String>,
) {
    if let Ok(mut guard) = cache.get_or_init(|| Mutex::new(None)).lock() {
        *guard = Some(ScriptCacheEntry {
            app_name: app_name.to_string(),
            value,
            cached_at: Instant::now(),
        });
    }
}

/// Capture the main screen and return PNG data
pub fn capture_screen() -> Result<Vec<u8>, String> {
    unsafe {
        // Get main display
        let _display_id = core_graphics::display::CGMainDisplayID();

        // Create image from display
        let image = core_graphics::display::CGDisplay::screenshot(
            core_graphics::geometry::CGRect::null(),
            core_graphics::window::K_CGWINDOW_LIST_OPTION_ON_SCREEN_ONLY,
            core_graphics::window::K_CGNULL_WINDOW_ID,
            core_graphics::display::K_CGWINDOW_IMAGE_DEFAULT,
        );

        let image = image.ok_or("Failed to capture screen")?;

        // Convert to PNG data using ImageIO
        let data = image_to_png(&image)?;
        Ok(data)
    }
}

/// Convert CGImage to PNG data
fn image_to_png(image: &core_graphics::image::CGImage) -> Result<Vec<u8>, String> {
    // Get image dimensions
    let width = image.width();
    let height = image.height();
    let bytes_per_row = image.bytes_per_row();

    // Get raw data
    let data_provider = image.data_provider().ok_or("No data provider")?;
    let raw_data = data_provider.copy_data();
    let bytes = raw_data.bytes();

    // The data often has padding at the end of each row.
    // We need to strip this to create a clean RGBA buffer.
    let mut clean_data = Vec::with_capacity(width * height * 4);
    for row in 0..height {
        let start = row * bytes_per_row;
        let end = start + (width * 4);
        if end <= bytes.len() {
            // ScreenCaptureKit/CoreGraphics usually returns BGRA.
            // We need to convert it to RGBA for the image crate and OCR.
            let row_bytes = &bytes[start..end];
            for chunk in row_bytes.chunks_exact(4) {
                // BGRA -> RGBA
                clean_data.push(chunk[2]); // R
                clean_data.push(chunk[1]); // G
                clean_data.push(chunk[0]); // B
                clean_data.push(chunk[3]); // A
            }
        }
    }

    if clean_data.len() != width * height * 4 {
        return Err(format!(
            "Data size mismatch: expected {} got {}",
            width * height * 4,
            clean_data.len()
        ));
    }

    let img_buffer = image::RgbaImage::from_raw(width as u32, height as u32, clean_data)
        .ok_or("Failed to create image buffer")?;

    // Encode as PNG
    let mut png_data = Vec::new();
    let encoder = image::codecs::png::PngEncoder::new(&mut png_data);
    encoder
        .write_image(
            &img_buffer,
            width as u32,
            height as u32,
            image::ColorType::Rgba8,
        )
        .map_err(|e| format!("PNG encode failed: {}", e))?;

    Ok(png_data)
}

/// Get information about the frontmost application
pub fn get_frontmost_app_info() -> FrontmostAppContext {
    unsafe {
        let workspace = NSWorkspace::sharedWorkspace();
        let app = workspace.frontmostApplication();

        let app_name = app
            .as_ref()
            .and_then(|a| a.localizedName())
            .map(|s| s.to_string())
            .unwrap_or_else(|| "Unknown".to_string());

        let bundle_id = app
            .as_ref()
            .and_then(|a| a.bundleIdentifier())
            .map(|s| s.to_string());

        let window_title = get_front_window_title(&app_name)
            .or_else(|| bundle_id.clone())
            .unwrap_or_default();

        FrontmostAppContext {
            app_name,
            bundle_id,
            window_title,
        }
    }
}

/// Best-effort active window title via AppleScript (requires Accessibility permissions for generic fallback).
fn get_front_window_title(app_name: &str) -> Option<String> {
    if let Some(cached) = cache_get(&WINDOW_TITLE_CACHE, app_name, Duration::from_millis(900)) {
        return Some(cached);
    }

    let app_lower = app_name.to_lowercase();
    let script = if app_lower.contains("safari") {
        r#"tell application "Safari" to get name of current tab of front window"#
    } else if app_lower.contains("chrome") {
        r#"tell application "Google Chrome" to get title of active tab of front window"#
    } else if app_lower.contains("arc") {
        r#"tell application "Arc" to get title of active tab of front window"#
    } else if app_lower.contains("brave") {
        r#"tell application "Brave Browser" to get title of active tab of front window"#
    } else if app_lower.contains("edge") {
        r#"tell application "Microsoft Edge" to get title of active tab of front window"#
    } else {
        r#"tell application "System Events"
                tell (first process whose frontmost is true)
                    if (count of windows) > 0 then
                        return name of front window
                    end if
                end tell
            end tell"#
    };

    let result = match std::process::Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
    {
        Ok(output) if output.status.success() => {
            let title = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if title.is_empty() {
                None
            } else {
                Some(title)
            }
        }
        _ => None,
    };

    cache_put(&WINDOW_TITLE_CACHE, app_name, result.clone());
    result
}

/// Get the current URL from the frontmost browser window using AppleScript
pub fn get_browser_url(app_name: &str) -> Option<String> {
    if let Some(cached) = cache_get(&URL_CACHE, app_name, Duration::from_millis(1200)) {
        return Some(cached);
    }

    let app_lower = app_name.to_lowercase();

    let script = if app_lower.contains("safari") {
        r#"tell application "Safari" to get URL of current tab of front window"#
    } else if app_lower.contains("chrome") {
        r#"tell application "Google Chrome" to get URL of active tab of front window"#
    } else if app_lower.contains("firefox") {
        // Firefox doesn't support AppleScript well, try via UI scripting
        return None;
    } else if app_lower.contains("arc") {
        r#"tell application "Arc" to get URL of active tab of front window"#
    } else if app_lower.contains("brave") {
        r#"tell application "Brave Browser" to get URL of active tab of front window"#
    } else if app_lower.contains("edge") {
        r#"tell application "Microsoft Edge" to get URL of active tab of front window"#
    } else {
        cache_put(&URL_CACHE, app_name, None);
        return None;
    };

    // Run osascript to get the URL
    let result = match std::process::Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
    {
        Ok(output) => {
            if output.status.success() {
                let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if url.starts_with("http://") || url.starts_with("https://") {
                    Some(url)
                } else {
                    None
                }
            } else {
                None
            }
        }
        Err(_) => None,
    };

    cache_put(&URL_CACHE, app_name, result.clone());
    result
}

// Core Graphics bindings
mod core_graphics {
    pub mod display {
        use std::ffi::c_void;

        pub type CGDirectDisplayID = u32;
        pub const K_CGWINDOW_IMAGE_DEFAULT: u32 = 0;

        #[link(name = "CoreGraphics", kind = "framework")]
        extern "C" {
            pub fn CGMainDisplayID() -> CGDirectDisplayID;
            pub fn CGDisplayCreateImage(display: CGDirectDisplayID) -> *mut c_void;
        }

        pub struct CGDisplay;

        impl CGDisplay {
            pub unsafe fn screenshot(
                _bounds: super::geometry::CGRect,
                _list_option: u32,
                _window_id: u32,
                _image_option: u32,
            ) -> Option<super::image::CGImage> {
                let display_id = CGMainDisplayID();
                let image_ref = CGDisplayCreateImage(display_id);
                if image_ref.is_null() {
                    None
                } else {
                    Some(super::image::CGImage { ptr: image_ref })
                }
            }
        }
    }

    pub mod geometry {
        #[repr(C)]
        #[derive(Clone, Copy)]
        pub struct CGRect {
            pub origin: CGPoint,
            pub size: CGSize,
        }

        #[repr(C)]
        #[derive(Clone, Copy)]
        pub struct CGPoint {
            pub x: f64,
            pub y: f64,
        }

        #[repr(C)]
        #[derive(Clone, Copy)]
        pub struct CGSize {
            pub width: f64,
            pub height: f64,
        }

        impl CGRect {
            pub fn null() -> Self {
                Self {
                    origin: CGPoint { x: 0.0, y: 0.0 },
                    size: CGSize {
                        width: 0.0,
                        height: 0.0,
                    },
                }
            }
        }
    }

    pub mod window {
        pub const K_CGWINDOW_LIST_OPTION_ON_SCREEN_ONLY: u32 = 1 << 0;
        pub const K_CGNULL_WINDOW_ID: u32 = 0;
    }

    pub mod image {
        use std::ffi::c_void;

        #[link(name = "CoreGraphics", kind = "framework")]
        extern "C" {
            fn CGImageGetWidth(image: *mut c_void) -> usize;
            fn CGImageGetHeight(image: *mut c_void) -> usize;
            fn CGImageGetBytesPerRow(image: *mut c_void) -> usize;
            fn CGImageGetDataProvider(image: *mut c_void) -> *mut c_void;
            fn CGImageRelease(image: *mut c_void);
            fn CGDataProviderCopyData(provider: *mut c_void) -> *mut c_void;
            fn CFDataGetLength(data: *mut c_void) -> isize;
            fn CFDataGetBytePtr(data: *mut c_void) -> *const u8;
            fn CFRelease(cf: *mut c_void);
        }

        pub struct CGImage {
            pub ptr: *mut c_void,
        }

        impl CGImage {
            pub fn width(&self) -> usize {
                unsafe { CGImageGetWidth(self.ptr) }
            }

            pub fn height(&self) -> usize {
                unsafe { CGImageGetHeight(self.ptr) }
            }

            pub fn bytes_per_row(&self) -> usize {
                unsafe { CGImageGetBytesPerRow(self.ptr) }
            }

            pub fn data_provider(&self) -> Option<CGDataProvider> {
                unsafe {
                    let provider = CGImageGetDataProvider(self.ptr);
                    if provider.is_null() {
                        None
                    } else {
                        Some(CGDataProvider { ptr: provider })
                    }
                }
            }
        }

        impl Drop for CGImage {
            fn drop(&mut self) {
                unsafe { CGImageRelease(self.ptr) }
            }
        }

        pub struct CGDataProvider {
            ptr: *mut c_void,
        }

        impl CGDataProvider {
            pub fn copy_data(&self) -> CFData {
                unsafe {
                    let data = CGDataProviderCopyData(self.ptr);
                    CFData { ptr: data }
                }
            }
        }

        pub struct CFData {
            ptr: *mut c_void,
        }

        impl CFData {
            pub fn bytes(&self) -> &[u8] {
                unsafe {
                    let len = CFDataGetLength(self.ptr) as usize;
                    let ptr = CFDataGetBytePtr(self.ptr);
                    std::slice::from_raw_parts(ptr, len)
                }
            }
        }

        impl Drop for CFData {
            fn drop(&mut self) {
                if !self.ptr.is_null() {
                    unsafe { CFRelease(self.ptr) }
                }
            }
        }
    }
}
