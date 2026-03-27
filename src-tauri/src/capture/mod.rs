//! Capture pipeline
//!
//! Handles screen capture, deduplication, and frame processing.
//! FastVLM (Apple FastVLM-0.5B) runs as an async sidecar on each screenshot
//! to augment the stored snippet with true visual understanding.

mod dedupe;
mod macos;
mod sampling;

pub use dedupe::PerceptualHasher;
pub use sampling::AdaptiveSampler;

use crate::embed::{ClipEmbedder, Embedder};
use crate::ocr::OcrEngine;
use crate::privacy::Blocklist;
use crate::store::MemoryRecord;
use crate::AppState;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Resolve the FastVLM sidecar Python script path.
/// Checks both the packaged app bundle and the dev-time source tree.
fn resolve_fastvlm_sidecar() -> Option<PathBuf> {
    // Packaged: <exe>/../Resources/sidecar/fastvlm_runner.py
    let packaged = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("../Resources/sidecar/fastvlm_runner.py")));
    if let Some(ref p) = packaged {
        if p.exists() {
            return Some(p.clone());
        }
    }

    // Dev: relative to Cargo manifest root
    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("sidecar/fastvlm_runner.py");
    if dev.exists() {
        return Some(dev);
    }

    None
}

/// Find the best Python executable (prefer venv, fall back to system python3).
fn python_cmd_for_sidecar() -> PathBuf {
    if let Some(docs) = dirs::document_dir() {
        let venv_py = docs.join("FNDR Meetings/venv/bin/python3");
        if venv_py.exists() {
            return venv_py;
        }
    }
    PathBuf::from("python3")
}

/// Call the FastVLM sidecar with a screenshot path.
/// Returns the visual description on success, or None if the sidecar is
/// unavailable / times out / returns a sentinel error string.
async fn call_fastvlm(screenshot_path: &str) -> Option<String> {
    let sidecar = resolve_fastvlm_sidecar()?;
    let python = python_cmd_for_sidecar();

    let result = tokio::time::timeout(
        Duration::from_secs(15),
        tokio::process::Command::new(&python)
            .arg(&sidecar)
            .arg(screenshot_path)
            .output(),
    )
    .await;

    let output = match result {
        Ok(Ok(o)) => o,
        Ok(Err(e)) => {
            tracing::debug!("FastVLM sidecar launch failed: {}", e);
            return None;
        }
        Err(_) => {
            tracing::debug!("FastVLM sidecar timed out");
            return None;
        }
    };

    if !output.status.success() {
        tracing::debug!(
            "FastVLM sidecar non-zero exit: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
        return None;
    }

    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();

    // Discard sentinel error strings written by the sidecar
    if text.is_empty() || text.starts_with("[fastvlm") {
        return None;
    }

    tracing::info!("FastVLM visual description: {}", text);
    Some(text)
}

/// Run the main capture loop
pub async fn run_capture_loop(state: Arc<AppState>) -> Result<(), Box<dyn std::error::Error>> {
    tracing::info!("Initializing capture pipeline...");

    // Initialize components
    let mut hasher = PerceptualHasher::new();
    let sampler = AdaptiveSampler::new();
    let ocr = OcrEngine::new()?;
    let text_embedder = Embedder::new()?;
    let image_embedder = ClipEmbedder::new();

    // Batch buffer
    let mut batch: Vec<MemoryRecord> = Vec::new();
    let mut last_flush = Instant::now();
    let flush_interval = Duration::from_secs(30);
    let max_batch_size = 100;

    // Force capture timer
    let mut last_forced_capture = Instant::now();

    tracing::info!("Capture loop started");

    loop {
        let config = state.config.read().clone();

        // Flush batch if needed
        let should_flush = batch.len() >= max_batch_size || last_flush.elapsed() >= flush_interval;
        if should_flush && !batch.is_empty() {
            let flush_start = Instant::now();
            if let Err(e) = state.store.add_batch(&batch) {
                tracing::error!("Failed to flush batch: {}", e);
            } else {
                tracing::info!(
                    "Flushed {} records in {:?}",
                    batch.len(),
                    flush_start.elapsed()
                );
            }
            batch.clear();
            last_flush = Instant::now();
        }

        // Check if paused
        if !state.is_capturing() {
            tokio::time::sleep(Duration::from_millis(500)).await;
            continue;
        }

        // Calculate sleep duration based on FPS
        let fps = sampler.get_current_fps(&config);
        let sleep_duration = Duration::from_secs_f64(1.0 / fps);

        // Get active application info
        let app_context = macos::get_frontmost_app_info();
        let app_name = app_context.app_name.clone();
        let window_title = app_context.window_title.clone();

        // Check blocklist
        if Blocklist::is_blocked(&app_name, &config.blocklist) {
            tokio::time::sleep(sleep_duration).await;
            continue;
        }

        // Capture screen
        let capture_result = macos::capture_screen();
        let image_data = match capture_result {
            Ok(data) => data,
            Err(e) => {
                tracing::warn!("Screen capture failed: {}", e);
                tokio::time::sleep(sleep_duration).await;
                continue;
            }
        };

        // Deduplication check
        let force_capture =
            last_forced_capture.elapsed().as_secs() >= config.forced_capture_interval;
        let is_duplicate = hasher.is_duplicate(&image_data, config.dedupe_threshold);

        if is_duplicate && !force_capture {
            state.frames_dropped.fetch_add(1, Ordering::Relaxed);
            tokio::time::sleep(sleep_duration).await;
            continue;
        }

        tracing::info!("Processing new frame from {}", app_name);

        if force_capture {
            last_forced_capture = Instant::now();
        }

        // OCR
        let ocr_start = Instant::now();
        let text = match ocr.recognize(&image_data) {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!("OCR failed: {}", e);
                tokio::time::sleep(sleep_duration).await;
                continue;
            }
        };
        let ocr_latency = ocr_start.elapsed();
        tracing::info!("OCR result: {} chars in {:?}", text.len(), ocr_latency);

        // Skip if text too short
        if text.len() < config.min_text_length {
            tokio::time::sleep(sleep_duration).await;
            continue;
        }

        // AI Analysis (VLM on OCR text if available, LLM summarization as fallback)
        let snippet = if let Some(ref vlm) = state.vlm {
            let vlm_start = std::time::Instant::now();
            let analysis = vlm
                .analyze_screen(&text, &app_name)
                .await
                .unwrap_or_default();
            tracing::info!("VLM analysis ({:?}): {}", vlm_start.elapsed(), &analysis);
            if analysis.is_empty() { text.clone() } else { analysis }
        } else {
            let summary = if let Some(engine) = &state.inference {
                engine.summarize(&text).await
            } else {
                String::new()
            };
            tracing::info!("LLM Summary: {}", summary);
            if summary.is_empty() { text.clone() } else { summary }
        };

        // Persist screenshot first (needed for FastVLM)
        let now = chrono::Utc::now();
        let url = macos::get_browser_url(&app_name);
        if let Some(ref u) = url {
            tracing::info!("Captured URL: {}", u);
        }

        let screenshot_path = persist_screenshot(
            &state.store.data_dir(),
            &now.format("%Y%m%d").to_string(),
            &image_data,
        );

        // ── FastVLM augmentation ─────────────────────────────────────────────
        // Run on the actual PNG for true visual understanding. This enriches
        // the snippet with content that OCR alone misses (charts, images, UI).
        // Runs async with a 15-second timeout so it never blocks the pipeline.
        let visual_description = if let Some(ref path) = screenshot_path {
            call_fastvlm(path).await
        } else {
            None
        };

        // Merge: keep the structural snippet + append visual context
        let final_snippet = match visual_description {
            Some(visual) => format!("{} | Visual: {}", snippet, visual),
            None => snippet,
        };
        // ── End FastVLM augmentation ─────────────────────────────────────────

        let record = MemoryRecord {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: now.timestamp_millis(),
            day_bucket: now.format("%Y-%m-%d").to_string(),
            app_name: app_name.clone(),
            bundle_id: app_context.bundle_id.clone(),
            window_title: window_title.clone(),
            session_id: format!(
                "{}-{}",
                now.format("%Y%m%d"),
                app_context
                    .bundle_id
                    .clone()
                    .unwrap_or_else(|| app_name.to_lowercase().replace(' ', "_"))
            ),
            text: text.clone(),
            snippet: final_snippet,
            embedding: text_embedder
                .embed_batch(&[text.clone()])
                .ok()
                .and_then(|mut vectors| vectors.drain(..).next())
                .unwrap_or_else(|| vec![0.0; 384]),
            image_embedding: image_embedder.embed_image(&image_data),
            screenshot_path,
            url,
        };
        batch.push(record);
        if let Some(last) = batch.last() {
            if let Err(err) = state.graph.ingest_memory(last) {
                tracing::warn!("Failed to ingest memory into graph: {}", err);
            }
        }

        state.frames_captured.fetch_add(1, Ordering::Relaxed);
        state
            .last_capture_time
            .store(now.timestamp_millis() as u64, Ordering::Relaxed);

        // Drop image data immediately (important for memory)
        drop(image_data);

        tokio::time::sleep(sleep_duration).await;
    }
}

fn persist_screenshot(
    data_dir: &std::path::Path,
    day_bucket: &str,
    image_data: &[u8],
) -> Option<String> {
    let frames_dir = data_dir.join("frames").join(day_bucket);
    if std::fs::create_dir_all(&frames_dir).is_err() {
        return None;
    }
    let file_name = format!("{}.png", uuid::Uuid::new_v4());
    let path = frames_dir.join(file_name);
    if std::fs::write(&path, image_data).is_ok() {
        Some(path.to_string_lossy().to_string())
    } else {
        None
    }
}
