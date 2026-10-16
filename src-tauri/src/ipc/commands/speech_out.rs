//! FNDR's natural voice from the ChatGPT plan (ADR 028). The webview owns the
//! WebRTC peer connection; these commands relay its offer and hand over the
//! text to speak. Private Mode refuses every send.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use crate::speech_out::{self, VoiceOutStatus, VoiceOutVoices};
use crate::AppState;

fn private_mode(state: &AppState) -> bool {
    state.is_incognito.load(Ordering::SeqCst)
}

#[tauri::command]
pub async fn voice_out_status(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<VoiceOutStatus, String> {
    Ok(speech_out::shared().status(private_mode(&state)).await)
}

#[tauri::command]
pub async fn voice_out_voices() -> Result<VoiceOutVoices, String> {
    Ok(speech_out::shared().voices().await)
}

/// Sends the webview's SDP offer and returns Codex's SDP answer.
#[tauri::command]
pub async fn voice_out_start(
    state: tauri::State<'_, Arc<AppState>>,
    sdp_offer: String,
    voice: Option<String>,
) -> Result<String, String> {
    speech_out::shared()
        .start(&sdp_offer, voice.as_deref(), private_mode(&state))
        .await
}

#[tauri::command]
pub async fn voice_out_speak(
    state: tauri::State<'_, Arc<AppState>>,
    text: String,
) -> Result<(), String> {
    speech_out::shared()
        .speak(&text, private_mode(&state))
        .await
}

#[tauri::command]
pub async fn voice_out_cancel() -> Result<(), String> {
    speech_out::shared().cancel().await;
    Ok(())
}

#[tauri::command]
pub async fn voice_out_stop() -> Result<(), String> {
    speech_out::shared().stop().await;
    Ok(())
}
