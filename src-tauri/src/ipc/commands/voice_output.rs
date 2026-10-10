//! The voice FNDR speaks with. The webview's speech registry does the
//! speaking; this only keeps the person's choice in `config.toml`.

use crate::config::VoiceOutputConfig;
use crate::AppState;
use std::sync::Arc;
use tauri::State;

#[tauri::command]
pub async fn get_voice_output_settings(
    state: State<'_, Arc<AppState>>,
) -> Result<VoiceOutputConfig, String> {
    Ok(state
        .inner()
        .config
        .read()
        .voice_output
        .clone()
        .normalized())
}

#[tauri::command]
pub async fn set_voice_output_settings(
    state: State<'_, Arc<AppState>>,
    settings: VoiceOutputConfig,
) -> Result<VoiceOutputConfig, String> {
    let normalized = settings.normalized();
    let mut config = state.inner().config.write();
    let previous = std::mem::replace(&mut config.voice_output, normalized.clone());
    if let Err(err) = config.save() {
        config.voice_output = previous;
        return Err(err.to_string());
    }
    Ok(normalized)
}
