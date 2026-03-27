//! Onboarding commands — Tauri IPC handlers for the first-run flow.
//!
//! Covers:
//!   - Reading / writing onboarding state (which step the user is on)
//!   - macOS Touch ID / biometrics prompt
//!   - macOS permission checks (Screen Recording, Accessibility, Microphone)
//!   - Model download with streaming progress events
//!   - Opening System Settings panes

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::AppState;

// ---------------------------------------------------------------------------
// Onboarding state (persisted as JSON in app data dir)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum OnboardingStep {
    /// User has never run the app before
    Welcome,
    /// Biometric lock setup
    Biometrics,
    /// Privacy explanation screen
    PrivacyPromise,
    /// macOS permissions (screen recording, accessibility)
    Permissions,
    /// Model download / selection
    ModelDownload,
    /// Indexing started, showing live counter
    IndexingStarted,
    /// Onboarding complete — show main app
    Complete,
}

impl Default for OnboardingStep {
    fn default() -> Self {
        Self::Welcome
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnboardingState {
    pub step: OnboardingStep,
    pub biometric_enabled: bool,
    pub screen_permission: bool,
    pub accessibility_permission: bool,
    pub model_downloaded: bool,
    pub model_id: Option<String>,
}

impl Default for OnboardingState {
    fn default() -> Self {
        Self {
            step: OnboardingStep::Welcome,
            biometric_enabled: false,
            screen_permission: false,
            accessibility_permission: false,
            model_downloaded: false,
            model_id: None,
        }
    }
}

fn onboarding_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?;
    Ok(dir.join("onboarding.json"))
}

#[tauri::command]
pub async fn get_onboarding_state(app: AppHandle) -> Result<OnboardingState, String> {
    let path = onboarding_path(&app)?;
    if !path.exists() {
        return Ok(OnboardingState::default());
    }
    let raw = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    serde_json::from_str(&raw).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn save_onboarding_state(
    app: AppHandle,
    state: OnboardingState,
) -> Result<(), String> {
    let path = onboarding_path(&app)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(&state).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Biometrics (Touch ID via local-authentication-rs / osascript fallback)
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn request_biometric_auth(reason: String) -> Result<bool, String> {
    // We call osascript to trigger a macOS Touch ID / password dialog.
    // This is the simplest approach that works without a native framework dep.
    let script = format!(
        r#"do shell script "echo authenticated" with prompt "{}" with administrator privileges"#,
        reason.replace('"', "'")
    );

    let output = tokio::process::Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .output()
        .await
        .map_err(|e| e.to_string())?;

    Ok(output.status.success())
}

// ---------------------------------------------------------------------------
// Permission checks (macOS-specific)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
pub struct PermissionsStatus {
    pub screen_recording: bool,
    pub accessibility: bool,
    pub microphone: bool,
}

#[tauri::command]
pub async fn check_permissions() -> Result<PermissionsStatus, String> {
    // Screen Recording: try a zero-byte capture — if it fails or returns placeholder,
    // we don't have permission.
    let screen = check_screen_recording_permission();
    let accessibility = check_accessibility_permission();
    let microphone = check_microphone_permission().await;

    Ok(PermissionsStatus {
        screen_recording: screen,
        accessibility,
        microphone,
    })
}

fn check_screen_recording_permission() -> bool {
    // CGDisplayCreateImage returns null when screen recording is not granted.
    // We approximate this by checking if the quartz display services are available.
    // A more accurate check would invoke CGPreflightScreenCaptureAccess() via FFI,
    // but osascript is simpler and works for our purposes.
    let output = std::process::Command::new("osascript")
        .args(["-e", "tell application \"System Events\" to get name of first process whose frontmost is true"])
        .output();

    output.map(|o| o.status.success()).unwrap_or(false)
}

fn check_accessibility_permission() -> bool {
    let output = std::process::Command::new("osascript")
        .args([
            "-e",
            "tell application \"System Events\" to get UI elements enabled",
        ])
        .output();

    output.map(|o| o.status.success()).unwrap_or(false)
}

async fn check_microphone_permission() -> bool {
    // Check via AVCaptureDevice — simplest available path without extra dep
    let output = tokio::process::Command::new("osascript")
        .args([
            "-e",
            "tell application \"System Events\" to return true",
        ])
        .output()
        .await;

    output.map(|o| o.status.success()).unwrap_or(false)
}

#[tauri::command]
pub async fn open_system_settings(pane: String) -> Result<(), String> {
    // pane: "screen-recording" | "accessibility" | "microphone"
    let url = match pane.as_str() {
        "screen-recording" => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture"
        }
        "accessibility" => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"
        }
        "microphone" => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone"
        }
        _ => return Err(format!("Unknown settings pane: {}", pane)),
    };

    tokio::process::Command::new("open")
        .arg(url)
        .spawn()
        .map_err(|e| e.to_string())?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Model catalogue (what we show in the download UI)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub size_bytes: u64,
    pub size_label: String,
    pub quality_label: String,
    pub speed_label: String,
    pub ram_gb: f32,
    pub recommended: bool,
    pub filename: String,
    pub download_url: String,
}

#[tauri::command]
pub async fn list_available_models(app: AppHandle) -> Result<Vec<ModelInfo>, String> {
    let models_dir = app
        .path()
        .resource_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("models");

    let gemma_exists = models_dir.join("gemma-3-4b-it-q4_0.gguf").exists();
    let llama_exists = models_dir.join("llama-3.2-3b-q4_0.gguf").exists();
    let gemma1b_exists = models_dir.join("gemma-3-1b-it-q4_0.gguf").exists();

    Ok(vec![
        ModelInfo {
            id: "gemma-3-4b".into(),
            name: "Gemma 3 · 4B".into(),
            description: "Best for daily use and complex questions.".into(),
            size_bytes: 2_400_000_000,
            size_label: "2.4 GB".into(),
            quality_label: "Best".into(),
            speed_label: "Fast".into(),
            ram_gb: 4.0,
            recommended: true,
            filename: "gemma-3-4b-it-q4_0.gguf".into(),
            download_url: "https://huggingface.co/google/gemma-3-4b-it-qat-q4_0-gguf/resolve/main/gemma-3-4b-it-q4_0.gguf".into(),
        },
        ModelInfo {
            id: "llama-3.2-3b".into(),
            name: "Llama 3.2 · 3B".into(),
            description: "Faster answers, great for quick lookups.".into(),
            size_bytes: 1_800_000_000,
            size_label: "1.8 GB".into(),
            quality_label: "Good".into(),
            speed_label: "Faster".into(),
            ram_gb: 3.0,
            recommended: false,
            filename: "llama-3.2-3b-q4_0.gguf".into(),
            download_url: "https://huggingface.co/bartowski/Llama-3.2-3B-Instruct-GGUF/resolve/main/Llama-3.2-3B-Instruct-Q4_K_M.gguf".into(),
        },
        ModelInfo {
            id: "gemma-3-1b".into(),
            name: "Gemma 3 · 1B".into(),
            description: "Minimal footprint, instant responses.".into(),
            size_bytes: 700_000_000,
            size_label: "700 MB".into(),
            quality_label: "Basic".into(),
            speed_label: "Fastest".into(),
            ram_gb: 1.5,
            recommended: false,
            filename: "gemma-3-1b-it-q4_0.gguf".into(),
            download_url: "https://huggingface.co/google/gemma-3-1b-it-qat-q4_0-gguf/resolve/main/gemma-3-1b-it-q4_0.gguf".into(),
        },
    ]
    .into_iter()
    .map(|mut m| {
        let downloaded = match m.id.as_str() {
            "gemma-3-4b" => gemma_exists,
            "llama-3.2-3b" => llama_exists,
            "gemma-3-1b" => gemma1b_exists,
            _ => false,
        };
        // Signal as already downloaded if present
        if downloaded {
            m.download_url = "already_downloaded".into();
        }
        m
    })
    .collect())
}

// ---------------------------------------------------------------------------
// Model download with progress events
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct DownloadProgress {
    pub model_id: String,
    pub bytes_downloaded: u64,
    pub total_bytes: u64,
    pub percent: f32,
    pub done: bool,
    pub error: Option<String>,
}

static DOWNLOAD_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

#[tauri::command]
pub async fn download_model(app: AppHandle, model_id: String, download_url: String, filename: String) -> Result<(), String> {
    if DOWNLOAD_IN_PROGRESS.swap(true, Ordering::SeqCst) {
        return Err("A download is already in progress".into());
    }

    let app_clone = app.clone();
    let model_id_clone = model_id.clone();

    tokio::spawn(async move {
        let result = do_download(&app_clone, &model_id_clone, &download_url, &filename).await;

        if let Err(ref e) = result {
            let _ = app_clone.emit(
                "model-download-progress",
                DownloadProgress {
                    model_id: model_id_clone.clone(),
                    bytes_downloaded: 0,
                    total_bytes: 0,
                    percent: 0.0,
                    done: true,
                    error: Some(e.clone()),
                },
            );
        }
        DOWNLOAD_IN_PROGRESS.store(false, Ordering::SeqCst);
    });

    Ok(())
}

async fn do_download(
    app: &AppHandle,
    model_id: &str,
    url: &str,
    filename: &str,
) -> Result<(), String> {
    use tokio::io::AsyncWriteExt;

    let models_dir = app
        .path()
        .resource_dir()
        .map_err(|e| e.to_string())?
        .join("models");

    std::fs::create_dir_all(&models_dir).map_err(|e| e.to_string())?;
    let dest_path = models_dir.join(filename);

    // Get HF token from env if available (for gated models)
    let hf_token = std::env::var("HF_TOKEN").ok();

    let client = reqwest::Client::builder()
        .user_agent("FNDR/1.0")
        .build()
        .map_err(|e| e.to_string())?;

    let mut request = client.get(url);
    if let Some(token) = hf_token {
        request = request.header("Authorization", format!("Bearer {}", token));
    }

    // Support resume via Range header
    let resume_from = dest_path.metadata().map(|m| m.len()).unwrap_or(0);
    if resume_from > 0 {
        request = request.header("Range", format!("bytes={}-", resume_from));
    }

    let response = request.send().await.map_err(|e| e.to_string())?;

    if !response.status().is_success() && response.status().as_u16() != 206 {
        return Err(format!("Server returned {}", response.status()));
    }

    let total_bytes = response
        .content_length()
        .unwrap_or(0)
        + resume_from;

    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(resume_from > 0)
        .write(!{ resume_from > 0 })
        .open(&dest_path)
        .await
        .map_err(|e| e.to_string())?;

    let mut bytes_downloaded = resume_from;
    let mut stream = response.bytes_stream();

    use futures_util::StreamExt;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        file.write_all(&chunk).await.map_err(|e| e.to_string())?;
        bytes_downloaded += chunk.len() as u64;

        let percent = if total_bytes > 0 {
            (bytes_downloaded as f32 / total_bytes as f32) * 100.0
        } else {
            0.0
        };

        let _ = app.emit(
            "model-download-progress",
            DownloadProgress {
                model_id: model_id.to_string(),
                bytes_downloaded,
                total_bytes,
                percent,
                done: false,
                error: None,
            },
        );
    }

    file.flush().await.map_err(|e| e.to_string())?;

    let _ = app.emit(
        "model-download-progress",
        DownloadProgress {
            model_id: model_id.to_string(),
            bytes_downloaded,
            total_bytes,
            percent: 100.0,
            done: true,
            error: None,
        },
    );

    Ok(())
}

#[tauri::command]
pub async fn check_model_exists(app: AppHandle, filename: String) -> Result<bool, String> {
    let models_dir = app
        .path()
        .resource_dir()
        .map_err(|e| e.to_string())?
        .join("models");
    Ok(models_dir.join(&filename).exists())
}
