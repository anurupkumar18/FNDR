//! Manual import of photos (e.g. from Meta AI glasses via phone → Mac).

use super::common::shared_embedder;
use crate::embedding::{embed_imported_image, EMBEDDING_DIM};
use crate::memory_compaction::{
    build_lexical_shadow, compact_summary_embedding_text, mean_pool_embeddings, support_embedding_texts,
};
use crate::models;
use crate::ocr::OcrEngine;
use crate::storage::MemoryRecord;
use crate::AppState;
use chrono::Local;
use image::ImageFormat;
use std::io::Cursor;
use std::path::Path;
use std::sync::Arc;
use tauri::State;

const APP_LABEL: &str = "Meta glasses import";

fn allowed_image_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| {
            matches!(
                e.to_ascii_lowercase().as_str(),
                "jpg" | "jpeg" | "png" | "heic" | "heif"
            )
        })
        .unwrap_or(false)
}

fn dynamic_image_to_png_bytes(img: &image::DynamicImage) -> Result<Vec<u8>, String> {
    let mut buf = Vec::new();
    img.write_to(&mut Cursor::new(&mut buf), ImageFormat::Png)
        .map_err(|e| format!("encode png for OCR: {e}"))?;
    Ok(buf)
}

/// Import a photo into the memory store (CLIP vision + Apple OCR + BGE text).
/// If `path` is `None`, opens a native file picker (blocking on a worker thread).
#[tauri::command]
pub async fn import_meta_glasses_photo(
    state: State<'_, Arc<AppState>>,
    path: Option<String>,
) -> Result<String, String> {
    let resolved_path = match path.filter(|p| !p.trim().is_empty()) {
        Some(p) => std::path::PathBuf::from(p),
        None => tokio::task::spawn_blocking(|| {
            rfd::FileDialog::new()
                .add_filter("Images", &["png", "jpg", "jpeg", "heic", "HEIC"])
                .pick_file()
        })
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "No file selected".to_string())?,
    };

    if !resolved_path.is_file() {
        return Err(format!("Not a file: {}", resolved_path.display()));
    }
    if !allowed_image_extension(&resolved_path) {
        return Err("Unsupported image type (use JPEG, PNG, or HEIC).".to_string());
    }

    let bytes = tokio::fs::read(&resolved_path)
        .await
        .map_err(|e| format!("read file: {e}"))?;

    let filename = resolved_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("photo")
        .to_string();

    let models_dir = models::models_dir(state.app_data_dir.as_path());

    let (ocr_text, ocr_confidence, ocr_blocks, image_embedding) =
        tokio::task::spawn_blocking(move || -> Result<(String, f32, usize, Vec<f32>), String> {
            let dynamic =
                image::load_from_memory(&bytes).map_err(|e| format!("decode image: {e}"))?;

            let png_bytes = dynamic_image_to_png_bytes(&dynamic)?;
            let engine = OcrEngine::new().map_err(|e| format!("OCR init: {e}"))?;
            let recognized = engine
                .recognize_with_metadata(&png_bytes)
                .map_err(|e| format!("OCR: {e}"))?;
            let ocr_text = recognized.0.text.clone();
            let ocr_confidence = recognized.0.confidence;
            let ocr_blocks = recognized.0.block_count;

            let image_embedding = embed_imported_image(&dynamic, &models_dir)?;

            Ok((ocr_text, ocr_confidence, ocr_blocks, image_embedding))
        })
        .await
        .map_err(|e| e.to_string())??;

    let clean_text = if ocr_text.trim().is_empty() {
        format!("{APP_LABEL}: {filename} (no text detected in image)")
    } else {
        format!("{APP_LABEL}: {filename}\n\n{}", ocr_text.trim())
    };

    let snippet = if ocr_text.trim().is_empty() {
        format!("Photo: {filename}")
    } else {
        let mut s = ocr_text.trim().replace('\n', " ");
        if s.chars().count() > 180 {
            s = s.chars().take(177).collect::<String>() + "...";
        }
        s
    };

    let lexical_shadow = build_lexical_shadow(APP_LABEL, &snippet, &clean_text, None);
    let compact_summary_text =
        compact_summary_embedding_text("import", &snippet, &clean_text, &lexical_shadow);
    let support_texts = support_embedding_texts(APP_LABEL, &filename, &clean_text, &lexical_shadow);

    let embedder = shared_embedder()?;
    let mut contexts = vec![
        (
            APP_LABEL.to_string(),
            filename.clone(),
            clean_text.clone(),
        ),
        (
            APP_LABEL.to_string(),
            filename.clone(),
            compact_summary_text,
        ),
    ];
    contexts.extend(
        support_texts
            .iter()
            .cloned()
            .map(|value| (APP_LABEL.to_string(), filename.clone(), value)),
    );
    let vectors = embedder
        .embed_batch_with_context(&contexts)
        .map_err(|e| format!("embed: {e}"))?;
    let embedding = vectors
        .first()
        .cloned()
        .unwrap_or_else(|| vec![0.0; EMBEDDING_DIM]);
    let snippet_embedding = vectors
        .get(1)
        .cloned()
        .unwrap_or_else(|| vec![0.0; EMBEDDING_DIM]);
    let support_embedding = if vectors.len() > 2 {
        mean_pool_embeddings(&vectors[2..])
    } else {
        vec![0.0; EMBEDDING_DIM]
    };

    let now = Local::now();
    let record = MemoryRecord {
        id: uuid::Uuid::new_v4().to_string(),
        timestamp: now.timestamp_millis(),
        day_bucket: now.format("%Y-%m-%d").to_string(),
        app_name: APP_LABEL.to_string(),
        bundle_id: None,
        window_title: filename.clone(),
        session_id: format!("{}-glasses-import", now.format("%Y%m%d")),
        text: ocr_text.clone(),
        clean_text: clean_text.clone(),
        ocr_confidence,
        ocr_block_count: ocr_blocks.min(u32::MAX as usize) as u32,
        snippet: snippet.clone(),
        display_summary: snippet.clone(),
        summary_source: "import".to_string(),
        noise_score: 0.0,
        session_key: "import:meta_glasses".to_string(),
        lexical_shadow,
        embedding,
        image_embedding,
        screenshot_path: None,
        url: None,
        snippet_embedding,
        support_embedding,
        decay_score: 1.0,
        last_accessed_at: now.timestamp_millis(),
        source_type: "import".to_string(),
        ..Default::default()
    };

    state
        .store
        .add_batch(&[record.clone()])
        .await
        .map_err(|e| e.to_string())?;

    if let Err(err) =
        crate::context_runtime::sync_memory_record(state.as_ref(), &record, Some("import")).await
    {
        tracing::warn!("glasses import: context_runtime sync failed: {err}");
    }

    state.invalidate_memory_derived_caches();

    Ok(record.id)
}
