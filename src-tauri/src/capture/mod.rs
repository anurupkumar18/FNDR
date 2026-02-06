//! Capture pipeline
//!
//! Handles screen capture, deduplication, and frame processing.

mod dedupe;
mod macos;
mod sampling;

pub use dedupe::PerceptualHasher;
pub use sampling::AdaptiveSampler;

use crate::ocr::OcrEngine;
use crate::privacy::Blocklist;
use crate::store::MemoryRecord;
use crate::AppState;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Run the main capture loop
pub async fn run_capture_loop(state: Arc<AppState>) -> Result<(), Box<dyn std::error::Error>> {
    tracing::info!("Initializing capture pipeline...");

    // Initialize components
    let mut hasher = PerceptualHasher::new();
    let sampler = AdaptiveSampler::new();
    let ocr = OcrEngine::new()?;
    // Embedder is replaced by AI-enhanced summaries for this phase
    // let embedder = Embedder::new()?;

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
        let (app_name, window_title) = macos::get_frontmost_app_info();

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

        // AI Analysis (VLM if available, LLM summarization as fallback)
        let snippet = if let Some(ref vlm) = state.vlm {
            // Use VLM for intelligent screen analysis
            let vlm_start = std::time::Instant::now();
            let analysis = vlm.analyze_screen(&text, &app_name).await;
            tracing::info!("VLM analysis ({:?}): {}", vlm_start.elapsed(), &analysis);
            if analysis.is_empty() {
                text.clone()
            } else {
                analysis
            }
        } else {
            // Fallback to LLM summarization
            let summary = state.inference.summarize(&text).await;
            tracing::info!("LLM Summary: {}", summary);
            if summary.is_empty() {
                text.clone()
            } else {
                summary
            }
        };

        // Create record
        let now = chrono::Utc::now();
        let record = MemoryRecord {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: now.timestamp_millis(),
            day_bucket: now.format("%Y-%m-%d").to_string(),
            app_name: app_name.clone(),
            window_title: window_title.clone(),
            text: text.clone(),
            snippet,
            embedding: vec![0.0; 384], // Placeholder for now, simple_store uses keyword search
        };
        batch.push(record);

        state.frames_captured.fetch_add(1, Ordering::Relaxed);
        state
            .last_capture_time
            .store(now.timestamp_millis() as u64, Ordering::Relaxed);

        // Drop image data immediately (important for memory)
        drop(image_data);

        tokio::time::sleep(sleep_duration).await;
    }
}
