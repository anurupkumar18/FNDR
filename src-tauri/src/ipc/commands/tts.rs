//! The on-device neural voice (ADR 030). The webview's `local_neural`
//! speech provider calls these; synthesis runs on this Mac and the only
//! network use is `tts_install`, after the person agreed to the download.

use crate::tts::{self, install::VoiceInfo, TtsStatus};
use crate::AppState;
use serde::Serialize;
use std::sync::Arc;
use tauri::{AppHandle, Manager, State};

/// Longer text is refused, so one call cannot hold the voice for minutes.
const MAX_TEXT_CHARS: usize = 2_000;

fn app_data_dir(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    crate::config::fndr_app_data_dir(app.path()).map_err(|e| e.to_string())
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TtsVoices {
    pub voices: Vec<VoiceInfo>,
    pub default_voice: &'static str,
}

#[tauri::command]
pub async fn tts_status(app: AppHandle) -> Result<TtsStatus, String> {
    Ok(tts::status(&app_data_dir(&app)?))
}

#[tauri::command]
pub async fn tts_voices() -> Result<TtsVoices, String> {
    Ok(TtsVoices {
        voices: tts::install::VOICES.to_vec(),
        default_voice: tts::install::DEFAULT_VOICE,
    })
}

/// Starts the one-time download. `consent` is the person's answer to the
/// download prompt; without it nothing is fetched.
#[tauri::command]
pub async fn tts_install(app: AppHandle, consent: bool) -> Result<TtsStatus, String> {
    let dir = app_data_dir(&app)?;
    tts::install::begin(consent)?;
    let target = dir.clone();
    tokio::spawn(async move {
        if let Err(err) = tts::install::run(target).await {
            tracing::warn!("tts: install failed: {err}");
        }
    });
    Ok(tts::status(&dir))
}

#[tauri::command]
pub async fn tts_remove(app: AppHandle) -> Result<TtsStatus, String> {
    let dir = app_data_dir(&app)?;
    tts::cancel();
    tts::unload();
    tts::install::remove(&dir)?;
    Ok(tts::status(&dir))
}

#[tauri::command]
pub async fn tts_cancel() -> Result<(), String> {
    tts::cancel();
    Ok(())
}

/// A 24 kHz mono 16-bit WAV of `text`, returned as raw bytes (an
/// ArrayBuffer in the webview). `voice` falls back to the saved choice,
/// then to the default voice.
#[tauri::command]
pub async fn tts_synthesize(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    text: String,
    voice: Option<String>,
    speed: Option<f32>,
) -> Result<tauri::ipc::Response, String> {
    let text = text.trim().to_string();
    if text.is_empty() {
        return Err("Nothing to say.".to_string());
    }
    if text.chars().count() > MAX_TEXT_CHARS {
        return Err(format!(
            "Text over {MAX_TEXT_CHARS} characters is spoken in parts."
        ));
    }
    let dir = app_data_dir(&app)?;
    let voice = voice
        .filter(|v| tts::install::voice_info(v).is_some())
        .unwrap_or_else(|| {
            state
                .inner()
                .config
                .read()
                .voice_output
                .neural_voice
                .clone()
        });
    let speed = speed.filter(|s| s.is_finite()).unwrap_or(1.0);
    let synthesis =
        tokio::task::spawn_blocking(move || tts::synthesize(&dir, &text, &voice, speed))
            .await
            .map_err(|e| format!("speech task: {e}"))??;
    Ok(tauri::ipc::Response::new(tts::engine::wav_bytes(
        &synthesis.samples,
    )))
}
