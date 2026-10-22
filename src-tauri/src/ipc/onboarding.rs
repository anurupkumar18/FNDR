//! Onboarding commands — Tauri IPC handlers for the first-run flow.
//!
//! Covers:
//!   - Reading / writing onboarding state (which step the user is on)
//!   - macOS Touch ID / biometrics prompt
//!   - macOS permission checks (Screen Recording, Accessibility, Microphone)
//!   - Model download with streaming progress events
//!   - Opening System Settings panes

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use tauri::{AppHandle, Emitter, Manager, State};

// ---------------------------------------------------------------------------
// macOS framework bindings for permission checks
// ---------------------------------------------------------------------------

// CGPreflightScreenCaptureAccess — returns true if the calling process has
// Screen Recording (screen capture) permission. Available since macOS 10.15.
// Using the proper CoreGraphics API instead of osascript, which was incorrectly
// testing Accessibility permission rather than Screen Recording.
#[cfg(target_os = "macos")]
#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGPreflightScreenCaptureAccess() -> bool;
}

// Force-link AVFoundation so that the AVCaptureDevice ObjC class is available
// at runtime when we call it through the objc2 message-send machinery.
#[cfg(target_os = "macos")]
#[link(name = "AVFoundation", kind = "framework")]
extern "C" {}

#[cfg(target_os = "macos")]
#[link(name = "Speech", kind = "framework")]
extern "C" {}

use crate::{load_ai_engines, models, AppState};

// ---------------------------------------------------------------------------
// Onboarding state (persisted as JSON in app data dir)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum OnboardingStep {
    /// User has never run the app before
    #[default]
    Welcome,
    /// Biometric lock setup
    Biometrics,
    /// Privacy explanation screen
    PrivacyPromise,
    /// macOS permissions (screen recording, accessibility)
    Permissions,
    /// Model download / selection
    ModelDownload,
    /// Optional local imports and guided retrieval
    BringPast,
    /// Indexing started, showing live counter
    IndexingStarted,
    /// Onboarding complete — show main app
    Complete,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnboardingState {
    pub step: OnboardingStep,
    pub biometric_enabled: bool,
    pub screen_permission: bool,
    pub accessibility_permission: bool,
    pub model_downloaded: bool,
    pub model_id: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
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
            display_name: None,
        }
    }
}

fn normalize_onboarding_state(mut state: OnboardingState) -> OnboardingState {
    if !state.model_downloaded
        && matches!(
            state.step,
            OnboardingStep::IndexingStarted | OnboardingStep::Complete
        )
    {
        state.step = OnboardingStep::ModelDownload;
    }

    state.display_name = state
        .display_name
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty());

    state
}

fn onboarding_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = crate::config::fndr_app_data_dir(app.path()).map_err(|e| e.to_string())?;
    Ok(dir.join("onboarding.json"))
}

#[tauri::command]
pub async fn get_onboarding_state(app: AppHandle) -> Result<OnboardingState, String> {
    let path = onboarding_path(&app)?;
    if !path.exists() {
        return Ok(OnboardingState::default());
    }

    let raw = match std::fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(err) => {
            tracing::warn!("Failed to read onboarding state at {:?}: {}", path, err);
            return Ok(OnboardingState::default());
        }
    };

    match serde_json::from_str::<OnboardingState>(&raw) {
        Ok(state) => Ok(normalize_onboarding_state(state)),
        Err(err) => {
            tracing::warn!("Failed to parse onboarding state at {:?}: {}", path, err);
            Ok(OnboardingState::default())
        }
    }
}

#[tauri::command]
pub async fn save_onboarding_state(app: AppHandle, state: OnboardingState) -> Result<(), String> {
    let path = onboarding_path(&app)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let normalized_state = normalize_onboarding_state(state);
    let json = serde_json::to_string_pretty(&normalized_state).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())
}

/// Persist the user's preferred local LLM/VLM choice and (when the model
/// is on disk) load it eagerly so the very next capture frame already has
/// structured-memory extraction available.
///
/// Persistence target: `onboarding.json#model_id`. We deliberately reuse
/// the existing field instead of introducing a second source of truth in
/// `config.toml` — `models::inference_preferred_model_id` already reads
/// from onboarding and maps tier rules (`config.vlm_model_size`) over it.
///
/// `vlm_model_size` continues to gate VLM tier (1B vs 4B). LLM GGUF
/// selection uses this persisted id directly.
#[tauri::command]
pub async fn set_preferred_inference_model(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    model_id: String,
) -> Result<bool, String> {
    let trimmed = model_id.trim().to_string();
    if trimmed.is_empty() {
        return Err("model_id must not be empty".to_string());
    }
    if models::model_by_id(&trimmed).is_none() {
        return Err(format!("Unknown model id: {trimmed}"));
    }

    let path = onboarding_path(&app)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut current = if path.exists() {
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|raw| serde_json::from_str::<OnboardingState>(&raw).ok())
            .unwrap_or_default()
    } else {
        OnboardingState::default()
    };
    current.model_id = Some(trimmed.clone());
    let normalized = normalize_onboarding_state(current);
    let json = serde_json::to_string_pretty(&normalized).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())?;

    // If the GGUF is on disk, swap the live engine so capture sees the
    // new model immediately. Otherwise just persist the choice — the
    // next download or restart will pick it up via
    // `models::inference_preferred_model_id`.
    let app_data_dir = crate::config::fndr_app_data_dir(app.path()).map_err(|e| e.to_string())?;
    let config = state.inner().config.read().clone();
    if models::resolve_model(Some(trimmed.as_str()), Some(app_data_dir.as_path())).is_some() {
        let loaded = load_ai_engines(app_data_dir.as_path(), &config).await;
        state.inner().replace_ai_engines(loaded.inference);
        Ok(true)
    } else {
        tracing::info!(
            "Preferred model {} saved but file not on disk yet — engine not reloaded",
            trimmed
        );
        Ok(false)
    }
}

// ---------------------------------------------------------------------------
// Biometrics (compiled LocalAuthentication helper)
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn request_biometric_auth(app: AppHandle, reason: String) -> Result<bool, String> {
    let helper = biometric_helper_path(&app)?;
    let output = tokio::process::Command::new(helper)
        .arg(reason)
        .stdin(Stdio::null())
        .output()
        .await
        .map_err(|error| format!("Could not launch the native authentication helper: {error}"))?;

    if !output.status.success() {
        return Err("The native authentication helper did not complete.".to_string());
    }
    parse_biometric_helper_output(&String::from_utf8_lossy(&output.stdout))
}

fn biometric_helper_path(app: &AppHandle) -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os("FNDR_AUTH_HELPER") {
        return Ok(path.into());
    }
    let target = if cfg!(target_arch = "aarch64") {
        "aarch64-apple-darwin"
    } else {
        "x86_64-apple-darwin"
    };
    let resource_dir = app.path().resource_dir().ok();
    let executable_dir = std::env::current_exe()
        .ok()
        .and_then(|executable| executable.parent().map(Path::to_path_buf));
    biometric_helper_candidates(
        resource_dir.as_deref(),
        executable_dir.as_deref(),
        Path::new(env!("CARGO_MANIFEST_DIR")),
        target,
    )
    .into_iter()
    .find(|candidate| candidate.is_file())
    .ok_or_else(|| {
        "The native authentication helper is unavailable. Reinstall FNDR and try again.".to_string()
    })
}

/// Where the Touch ID helper may be, best first. The bundled app carries it
/// as `fndr-auth` beside the executable (Tauri drops the target suffix from an
/// external binary); a development build has the suffixed name in `binaries/`.
/// The source-tree path is last: a packaged app must never depend on it.
fn biometric_helper_candidates(
    resource_dir: Option<&Path>,
    executable_dir: Option<&Path>,
    manifest_dir: &Path,
    target: &str,
) -> Vec<PathBuf> {
    let suffixed = format!("fndr-auth-{target}");
    let mut candidates = Vec::new();
    if let Some(dir) = executable_dir {
        candidates.push(dir.join("fndr-auth"));
        candidates.push(dir.join(&suffixed));
    }
    if let Some(dir) = resource_dir {
        candidates.push(dir.join("fndr-auth"));
        candidates.push(dir.join(&suffixed));
    }
    candidates.push(manifest_dir.join("binaries").join(&suffixed));
    candidates
}

fn parse_biometric_helper_output(output: &str) -> Result<bool, String> {
    #[derive(Deserialize)]
    struct AuthenticationResult {
        r#type: String,
    }

    let result: AuthenticationResult = serde_json::from_str(output.trim())
        .map_err(|_| "The native authentication helper returned an invalid result.".to_string())?;
    Ok(result.r#type == "authenticated")
}

// ---------------------------------------------------------------------------
// Permission checks (macOS-specific)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PermissionState {
    NotDetermined,
    Restricted,
    Denied,
    Granted,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PermissionsStatus {
    pub screen_recording: PermissionState,
    pub accessibility: PermissionState,
    pub microphone: PermissionState,
    pub speech_recognition: PermissionState,
}

#[tauri::command]
pub async fn check_permissions() -> Result<PermissionsStatus, String> {
    let screen = bool_permission_state(check_screen_recording_permission());
    let accessibility = bool_permission_state(check_accessibility_permission());
    let microphone = check_microphone_permission();
    let speech_recognition = check_speech_recognition_permission();

    Ok(PermissionsStatus {
        screen_recording: screen,
        accessibility,
        microphone,
        speech_recognition,
    })
}

fn bool_permission_state(granted: bool) -> PermissionState {
    if granted { PermissionState::Granted } else { PermissionState::Denied }
}

fn av_authorization_state(status: i64) -> PermissionState {
    match status {
        0 => PermissionState::NotDetermined,
        1 => PermissionState::Restricted,
        2 => PermissionState::Denied,
        3 => PermissionState::Granted,
        _ => PermissionState::Denied,
    }
}

pub(crate) fn check_screen_recording_permission() -> bool {
    // Use CGPreflightScreenCaptureAccess() — the correct macOS API for checking
    // Screen Recording permission without triggering a system prompt.
    // Previously this used osascript talking to System Events, which only succeeds
    // when *Accessibility* permission is granted, not Screen Recording, causing
    // users with Screen Recording (but no Accessibility) to be stuck on this step.
    #[cfg(target_os = "macos")]
    {
        unsafe { CGPreflightScreenCaptureAccess() }
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

fn check_accessibility_permission() -> bool {
    // osascript querying System Events requires Accessibility permission,
    // so success/failure accurately reflects that grant.
    let output = std::process::Command::new("osascript")
        .args([
            "-e",
            "tell application \"System Events\" to get UI elements enabled",
        ])
        .output();

    output.map(|o| o.status.success()).unwrap_or(false)
}

fn check_microphone_permission() -> PermissionState {
    // Call +[AVCaptureDevice authorizationStatusForMediaType:] via the ObjC runtime.
    // The AVFoundation framework is linked above.
    // AVAuthorizationStatus: notDetermined=0, restricted=1, denied=2, authorized=3.
    // Previously this osascript always returned true regardless of actual permission.
    #[cfg(target_os = "macos")]
    {
        use objc2::runtime::AnyClass;
        use objc2_foundation::ns_string;

        let Some(cls) = AnyClass::get("AVCaptureDevice") else {
            // AVFoundation unavailable (very old macOS) — treat as unknown/false
            return PermissionState::Denied;
        };

        // AVMediaTypeAudio constant value is the string literal "soun"
        let media_type = ns_string!("soun");

        let status: i64 =
            unsafe { objc2::msg_send![cls, authorizationStatusForMediaType: media_type] };

        av_authorization_state(status)
    }
    #[cfg(not(target_os = "macos"))]
    {
        PermissionState::Granted
    }
}

fn check_speech_recognition_permission() -> PermissionState {
    #[cfg(target_os = "macos")]
    {
        use objc2::runtime::AnyClass;

        let Some(cls) = AnyClass::get("SFSpeechRecognizer") else {
            return PermissionState::Denied;
        };
        let status: i64 = unsafe { objc2::msg_send![cls, authorizationStatus] };
        av_authorization_state(status)
    }
    #[cfg(not(target_os = "macos"))]
    {
        PermissionState::Granted
    }
}

#[tauri::command]
pub async fn open_system_settings(pane: String) -> Result<(), String> {
    let url = system_settings_url(&pane)?;

    tokio::process::Command::new("open")
        .arg(url)
        .spawn()
        .map_err(|e| e.to_string())?;

    Ok(())
}

fn system_settings_url(pane: &str) -> Result<&'static str, String> {
    match pane {
        "screen-recording" => {
            Ok("x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture")
        }
        "accessibility" => {
            Ok("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
        }
        "microphone" => {
            Ok("x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone")
        }
        "speech-recognition" => {
            Ok("x-apple.systempreferences:com.apple.preference.security?Privacy_SpeechRecognition")
        }
        "automation" => {
            Ok("x-apple.systempreferences:com.apple.preference.security?Privacy_Automation")
        }
        _ => Err(format!("Unknown settings pane: {pane}")),
    }
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
    pub required: bool,
    pub filename: String,
    pub download_url: String,
}

#[tauri::command]
pub async fn list_available_models(app: AppHandle) -> Result<Vec<ModelInfo>, String> {
    let app_data_dir =
        crate::config::fndr_app_data_dir(app.path()).unwrap_or_else(|_| PathBuf::from("."));

    Ok(models::catalog()
        .iter()
        .map(|model| {
            let downloaded = models::is_model_available(model.id, Some(app_data_dir.as_path()));
            ModelInfo {
                id: model.id.to_string(),
                name: model.name.to_string(),
                description: model.description.to_string(),
                size_bytes: model.size_bytes,
                size_label: model.size_label.to_string(),
                quality_label: model.quality_label.to_string(),
                speed_label: model.speed_label.to_string(),
                ram_gb: model.ram_gb,
                recommended: model.recommended,
                required: model.required,
                filename: model.filename.to_string(),
                download_url: if downloaded {
                    "already_downloaded".to_string()
                } else {
                    model.download_url.to_string()
                },
            }
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelDownloadStatus {
    pub state: String,
    pub model_id: Option<String>,
    pub filename: Option<String>,
    pub download_url: Option<String>,
    pub destination_path: Option<String>,
    pub temp_path: Option<String>,
    pub bytes_downloaded: u64,
    pub total_bytes: u64,
    pub percent: f32,
    pub done: bool,
    pub error: Option<String>,
    pub logs: Vec<String>,
    pub updated_at_ms: i64,
}

impl Default for ModelDownloadStatus {
    fn default() -> Self {
        Self {
            state: "idle".to_string(),
            model_id: None,
            filename: None,
            download_url: None,
            destination_path: None,
            temp_path: None,
            bytes_downloaded: 0,
            total_bytes: 0,
            percent: 0.0,
            done: false,
            error: None,
            logs: Vec::new(),
            updated_at_ms: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiRuntimeStatus {
    pub ai_model_available: bool,
    pub ai_model_loaded: bool,
    pub vlm_loaded: bool,
    pub loaded_model_id: Option<String>,
    pub loaded_model_path: Option<String>,
    /// "ocr_only" | "lightweight_vlm" | "heavy_vlm" | "disabled"
    pub model_mode: String,
    /// "unavailable" | "loaded" | "fallback"
    pub vlm_status: String,
    /// ID of the VLM model if loaded ("smolvlm-500m", "qwen3-vl-4b", or None)
    pub vlm_model_id: Option<String>,
}

static DOWNLOAD_IN_PROGRESS: AtomicBool = AtomicBool::new(false);
static DOWNLOAD_STATUS: OnceLock<Mutex<ModelDownloadStatus>> = OnceLock::new();

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn download_status_store() -> &'static Mutex<ModelDownloadStatus> {
    DOWNLOAD_STATUS.get_or_init(|| Mutex::new(ModelDownloadStatus::default()))
}

fn mutate_download_status<F>(app: &AppHandle, f: F)
where
    F: FnOnce(&mut ModelDownloadStatus),
{
    let snapshot = {
        let mut status = download_status_store().lock();
        f(&mut status);
        status.updated_at_ms = now_ms();
        status.clone()
    };
    let _ = app.emit("model-download-status", snapshot);
}

fn replace_download_status(app: &AppHandle, status: ModelDownloadStatus) {
    let snapshot = {
        let mut current = download_status_store().lock();
        *current = status;
        current.updated_at_ms = now_ms();
        current.clone()
    };
    let _ = app.emit("model-download-status", snapshot);
}

fn emit_download_log(app: &AppHandle, msg: &str) {
    tracing::info!("[MODEL DOWNLOAD] {}", msg);

    let snapshot = {
        let mut status = download_status_store().lock();
        status.logs.push(msg.to_string());
        if status.logs.len() > 50 {
            let overflow = status.logs.len() - 50;
            status.logs.drain(0..overflow);
        }
        status.updated_at_ms = now_ms();
        status.clone()
    };

    let _ = app.emit("model-download-log", msg.to_string());
    let _ = app.emit("model-download-status", snapshot);
}

fn emit_download_progress(app: &AppHandle, progress: DownloadProgress) {
    let snapshot = {
        let mut status = download_status_store().lock();
        status.model_id = Some(progress.model_id.clone());
        status.bytes_downloaded = progress.bytes_downloaded;
        status.total_bytes = progress.total_bytes;
        status.percent = progress.percent;
        status.done = progress.done;
        status.error = progress.error.clone();

        if progress.error.is_some() {
            status.state = "failed".to_string();
        } else if progress.done {
            status.state = "completed".to_string();
        } else {
            status.state = "downloading".to_string();
        }

        status.updated_at_ms = now_ms();
        status.clone()
    };

    let _ = app.emit("model-download-progress", progress);
    let _ = app.emit("model-download-status", snapshot);
}

fn emit_download_failure(app: &AppHandle, model_id: &str, error: String) {
    let (bytes_downloaded, total_bytes, percent) = {
        let mut status = download_status_store().lock();
        status.state = "failed".to_string();
        status.model_id = Some(model_id.to_string());
        status.done = true;
        status.error = Some(error.clone());
        status.updated_at_ms = now_ms();
        (status.bytes_downloaded, status.total_bytes, status.percent)
    };

    emit_download_progress(
        app,
        DownloadProgress {
            model_id: model_id.to_string(),
            bytes_downloaded,
            total_bytes,
            percent,
            done: true,
            error: Some(error),
        },
    );
}

#[tauri::command]
pub async fn download_model(
    app: AppHandle,
    model_id: String,
    download_url: String,
    filename: String,
) -> Result<(), String> {
    let app_data_dir = crate::config::fndr_app_data_dir(app.path()).map_err(|e| e.to_string())?;
    if let Some(definition) = models::model_by_id(&model_id) {
        let available = available_disk_bytes(&app_data_dir)?;
        let required = definition.size_bytes.saturating_add(definition.size_bytes / 10);
        if available < required {
            return Err(format!(
                "Not enough free disk space for {}. Free up at least {} more before downloading.",
                definition.name,
                required.saturating_sub(available),
            ));
        }
    }
    if DOWNLOAD_IN_PROGRESS.swap(true, Ordering::SeqCst) {
        return Err("A download is already in progress".into());
    }
    let dest_path = models::models_dir(app_data_dir.as_path()).join(&filename);
    let temp_path = models::partial_model_path(app_data_dir.as_path(), &filename);

    replace_download_status(
        &app,
        ModelDownloadStatus {
            state: "preparing".to_string(),
            model_id: Some(model_id.clone()),
            filename: Some(filename.clone()),
            download_url: Some(download_url.clone()),
            destination_path: Some(dest_path.display().to_string()),
            temp_path: Some(temp_path.display().to_string()),
            bytes_downloaded: 0,
            total_bytes: 0,
            percent: 0.0,
            done: false,
            error: None,
            logs: Vec::new(),
            updated_at_ms: now_ms(),
        },
    );

    let app_clone = app.clone();
    let model_id_clone = model_id.clone();

    tokio::spawn(async move {
        let result =
            download_model_files(&app_clone, &model_id_clone, &download_url, &filename).await;

        if let Err(ref e) = result {
            emit_download_failure(&app_clone, &model_id_clone, e.clone());
        }
        DOWNLOAD_IN_PROGRESS.store(false, Ordering::SeqCst);
    });

    Ok(())
}

fn available_disk_bytes(path: &std::path::Path) -> Result<u64, String> {
    #[cfg(unix)]
    {
        use std::ffi::CString;
        use std::os::unix::ffi::OsStrExt;

        let path = CString::new(path.as_os_str().as_bytes()).map_err(|e| e.to_string())?;
        let mut stats: libc::statvfs = unsafe { std::mem::zeroed() };
        if unsafe { libc::statvfs(path.as_ptr(), &mut stats) } != 0 {
            return Err(format!("Could not check free disk space: {}", std::io::Error::last_os_error()));
        }
        Ok((stats.f_bavail as u64).saturating_mul(stats.f_frsize as u64))
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Ok(u64::MAX)
    }
}

/// Download every file a model needs: pinned extra files first (they are
/// small), then the main weights file last so its completion event marks the
/// whole model ready.
async fn download_model_files(
    app: &AppHandle,
    model_id: &str,
    download_url: &str,
    filename: &str,
) -> Result<(), String> {
    if let Some(definition) = models::model_by_id(model_id) {
        let app_data_dir =
            crate::config::fndr_app_data_dir(app.path()).map_err(|e| e.to_string())?;
        let models_dir = models::models_dir(app_data_dir.as_path());
        for extra in definition.extra_files {
            let dest = models_dir.join(extra.filename);
            let already_valid = dest.is_file()
                && match extra.sha256 {
                    Some(expected) => models::verify_file_sha256(&dest, expected).is_ok(),
                    None => true,
                };
            if already_valid {
                continue;
            }
            if dest.is_file() {
                emit_download_log(
                    app,
                    &format!(
                        "Existing {} does not match the pinned checksum; re-downloading.",
                        extra.filename
                    ),
                );
                let _ = tokio::fs::remove_file(&dest).await;
            }
            do_download(app, model_id, extra.download_url, extra.filename, false).await?;
        }
    }
    do_download(app, model_id, download_url, filename, true).await
}

#[tauri::command]
pub async fn get_model_download_status() -> Result<ModelDownloadStatus, String> {
    Ok(download_status_store().lock().clone())
}

#[tauri::command]
pub fn get_apple_model_status(
    state: State<'_, Arc<AppState>>,
) -> crate::inference::foundation::AppleModelStatus {
    crate::inference::foundation::status(state.app_data_dir.as_path())
}

#[tauri::command]
pub async fn set_apple_model_enabled(
    state: State<'_, Arc<AppState>>,
    enabled: bool,
) -> Result<crate::inference::foundation::AppleModelStatus, String> {
    crate::inference::foundation::write_setting(state.app_data_dir.as_path(), enabled)
        .map_err(|error| error.to_string())?;
    let config = state.config.read().clone();
    let loaded = load_ai_engines(state.app_data_dir.as_path(), &config).await;
    state.replace_ai_engines(loaded.inference);
    Ok(crate::inference::foundation::status(
        state.app_data_dir.as_path(),
    ))
}

#[tauri::command]
pub async fn refresh_ai_models(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<AiRuntimeStatus, String> {
    let app_data_dir = crate::config::fndr_app_data_dir(app.path()).map_err(|e| e.to_string())?;
    let config = state.inner().config.read().clone();
    let preferred_model_id = models::inference_preferred_model_id(app_data_dir.as_path(), &config);
    let loaded_ai = load_ai_engines(app_data_dir.as_path(), &config).await;
    let ai_model_available = loaded_ai.inference.is_some()
        || models::resolve_model(preferred_model_id.as_deref(), Some(app_data_dir.as_path()))
            .is_some();

    let loaded_model_id = loaded_ai
        .inference
        .as_ref()
        .map(|engine| engine.model_id().to_string());
    let loaded_model_path = loaded_ai.inference.as_ref().and_then(|engine| {
        (!engine.model_path().as_os_str().is_empty())
            .then(|| engine.model_path().display().to_string())
    });
    let ai_model_loaded = loaded_ai.inference.is_some();
    let vlm_loaded = crate::inference::pixel_vlm_loaded();

    let model_mode = if !config.use_vlm {
        "disabled".to_string()
    } else if vlm_loaded {
        "lightweight_vlm".to_string()
    } else {
        "ocr_only".to_string()
    };

    let vlm_status = if vlm_loaded {
        "loaded".to_string()
    } else if config.use_vlm {
        "unavailable".to_string()
    } else {
        "fallback".to_string()
    };

    let vlm_model_id: Option<String> = None;

    state.inner().replace_ai_engines(loaded_ai.inference);

    Ok(AiRuntimeStatus {
        ai_model_available,
        ai_model_loaded,
        vlm_loaded,
        loaded_model_id,
        loaded_model_path,
        model_mode,
        vlm_status,
        vlm_model_id,
    })
}

async fn do_download(
    app: &AppHandle,
    model_id: &str,
    url: &str,
    filename: &str,
    is_final_file: bool,
) -> Result<(), String> {
    use tokio::io::AsyncWriteExt;

    emit_download_log(
        app,
        &format!("Starting download task for {} ({})", model_id, filename),
    );
    emit_download_log(app, "Checking local app data directories...");

    let app_data_dir = crate::config::fndr_app_data_dir(app.path()).map_err(|e| e.to_string())?;
    let models_dir = models::models_dir(app_data_dir.as_path());

    std::fs::create_dir_all(&models_dir).map_err(|e| e.to_string())?;
    let dest_path = models_dir.join(filename);
    let partial_path = models::partial_model_path(app_data_dir.as_path(), filename);

    mutate_download_status(app, |status| {
        status.destination_path = Some(dest_path.display().to_string());
        status.temp_path = Some(partial_path.display().to_string());
    });

    if dest_path.exists() {
        let file_size = dest_path.metadata().map(|meta| meta.len()).unwrap_or(0);
        emit_download_log(
            app,
            "Model file already exists. Marking download as complete.",
        );
        emit_download_progress(
            app,
            DownloadProgress {
                model_id: model_id.to_string(),
                bytes_downloaded: file_size,
                total_bytes: file_size,
                percent: 100.0,
                done: is_final_file,
                error: None,
            },
        );
        return Ok(());
    }

    // Get HF token from env if available (for gated models)
    let hf_token = std::env::var("HF_TOKEN").ok();
    if hf_token.is_some() {
        emit_download_log(app, "Found HF_TOKEN in environment mapped variables.");
    } else {
        emit_download_log(app, "No HF_TOKEN found in environment. Public models only.");
    }

    emit_download_log(app, "Building reqwest HTTP client (15s connect timeout)...");
    let client = reqwest::Client::builder()
        .user_agent("FNDR/1.0")
        .connect_timeout(std::time::Duration::from_secs(15))
        .timeout(std::time::Duration::from_secs(7200))
        .build()
        .map_err(|e| {
            let msg = format!("HTTP client build failed: {}", e);
            emit_download_log(app, &msg);
            msg
        })?;

    let mut request = client.get(url);
    if let Some(token) = hf_token {
        // Automatically trim any stray quotes the user may have left in their .env
        let clean_token = token.trim_matches(|c| c == '\'' || c == '"');
        request = request.header("Authorization", format!("Bearer {}", clean_token));
    }

    // Support resume via Range header
    let resume_from = partial_path.metadata().map(|m| m.len()).unwrap_or(0);
    if resume_from > 0 {
        emit_download_log(
            app,
            &format!(
                "Found existing partial file, attempting resume from byte {}",
                resume_from
            ),
        );
        request = request.header("Range", format!("bytes={}-", resume_from));
    }

    emit_download_log(app, &format!("Sending HTTP GET request to {}...", url));
    if let Ok(parsed_url) = url.parse::<reqwest::Url>() {
        if let Some(host) = parsed_url.host_str() {
            crate::privacy_proof::record_egress(host);
        }
    }
    let response = request.send().await.map_err(|e| {
        let msg = format!("HTTP request failed or hung: {}", e);
        emit_download_log(app, &msg);
        msg
    })?;

    let response_status = response.status();
    let status_code = response_status.as_u16();
    emit_download_log(app, &format!("Received HTTP status code: {}", status_code));

    if !response_status.is_success() && status_code != 206 {
        let body_preview = response.text().await.unwrap_or_default().replace('\n', " ");
        let body_preview = body_preview.chars().take(240).collect::<String>();
        let msg = if body_preview.trim().is_empty() {
            format!("Server returned {}", response_status)
        } else {
            format!("Server returned {}: {}", response_status, body_preview)
        };
        emit_download_log(app, &format!("Fatal Error: {}", msg));
        return Err(msg);
    }

    // If we sent a Range header but the server responded with 200 (full content)
    // instead of 206 (partial content), the server doesn't support resume.
    // Delete the partial file and restart from scratch to avoid file corruption.
    let resume_from = if resume_from > 0 && status_code == 200 {
        tracing::warn!("Server does not support range requests; restarting download from scratch");
        emit_download_log(
            app,
            "Server ignored the resume request. Restarting from byte 0 to avoid corruption.",
        );
        let _ = tokio::fs::remove_file(&partial_path).await;
        0u64
    } else {
        resume_from
    };

    let total_bytes = response.content_length().unwrap_or(0) + resume_from;

    let raw_file = tokio::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(resume_from > 0)
        .truncate(resume_from == 0)
        .open(&partial_path)
        .await
        .map_err(|e| e.to_string())?;

    // BufWriter batches small chunk writes into larger I/O operations,
    // reducing syscall overhead for the thousands of chunks in a multi-GB download.
    let mut file = tokio::io::BufWriter::new(raw_file);

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

        emit_download_progress(
            app,
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

    mutate_download_status(app, |status| {
        status.state = "finalizing".to_string();
    });
    emit_download_log(app, "Flushing model file to disk...");
    file.flush().await.map_err(|e| e.to_string())?;
    drop(file);

    if let Some(expected) = models::expected_sha256_for(model_id, filename) {
        emit_download_log(app, "Verifying SHA-256 checksum...");
        let verify_path = partial_path.clone();
        let expected = expected.to_string();
        let verified = tokio::task::spawn_blocking(move || {
            models::verify_file_sha256(&verify_path, &expected)
        })
        .await
        .map_err(|e| format!("Checksum task failed: {e}"))?;
        if let Err(err) = verified {
            // A complete-but-corrupt partial can never be repaired by resume.
            let _ = tokio::fs::remove_file(&partial_path).await;
            emit_download_log(app, &format!("Checksum verification failed: {err}"));
            return Err(err);
        }
        emit_download_log(app, "Checksum verified.");
    }

    emit_download_log(
        app,
        "Promoting partial download into the live models directory...",
    );
    tokio::fs::rename(&partial_path, &dest_path)
        .await
        .map_err(|e| format!("Failed to finalize model file: {}", e))?;
    emit_download_log(app, &format!("Model ready at {}", dest_path.display()));

    emit_download_progress(
        app,
        DownloadProgress {
            model_id: model_id.to_string(),
            bytes_downloaded,
            total_bytes,
            percent: 100.0,
            done: is_final_file,
            error: None,
        },
    );

    Ok(())
}

#[tauri::command]
pub async fn check_model_exists(app: AppHandle, filename: String) -> Result<bool, String> {
    let app_data_dir = crate::config::fndr_app_data_dir(app.path()).map_err(|e| e.to_string())?;
    Ok(models::models_dir(app_data_dir.as_path())
        .join(&filename)
        .exists())
}

#[tauri::command]
pub async fn delete_ai_model(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    filename: String,
) -> Result<(), String> {
    // Sanitize: only allow bare filenames, no path traversal
    let fname = std::path::Path::new(&filename);
    if fname.components().count() != 1 || filename.contains('/') || filename.contains('\\') {
        return Err("Invalid filename".into());
    }
    let app_data_dir = crate::config::fndr_app_data_dir(app.path()).map_err(|e| e.to_string())?;
    let models_dir = models::models_dir(app_data_dir.as_path());
    let final_path = models_dir.join(fname);
    let partial_path = models::partial_model_path(app_data_dir.as_path(), &filename);

    let should_unload = state
        .inner()
        .inference_engine()
        .map(|engine| engine.model_path() == final_path.as_path())
        .unwrap_or(false);
    if should_unload {
        state.inner().replace_ai_engines(None);
    }

    if final_path.exists() {
        tokio::fs::remove_file(&final_path)
            .await
            .map_err(|e| e.to_string())?;
    }
    if partial_path.exists() {
        tokio::fs::remove_file(&partial_path)
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        available_disk_bytes, biometric_helper_candidates, parse_biometric_helper_output,
        system_settings_url,
    };
    use std::path::Path;

    #[cfg(unix)]
    #[test]
    fn reads_available_disk_space_before_model_downloads() {
        assert!(available_disk_bytes(&std::env::temp_dir()).unwrap() > 0);
    }

    #[test]
    fn resolves_voice_permission_settings_panes() {
        assert_eq!(
            system_settings_url("microphone").unwrap(),
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone"
        );
        assert_eq!(
            system_settings_url("speech-recognition").unwrap(),
            "x-apple.systempreferences:com.apple.preference.security?Privacy_SpeechRecognition"
        );
        assert!(system_settings_url("unknown").is_err());
    }

    #[test]
    fn the_bundled_helper_beside_the_executable_is_found_before_the_source_tree() {
        let found = biometric_helper_candidates(
            Some(Path::new("/App/Contents/Resources")),
            Some(Path::new("/App/Contents/MacOS")),
            Path::new("/src/src-tauri"),
            "aarch64-apple-darwin",
        );
        assert_eq!(found[0], Path::new("/App/Contents/MacOS/fndr-auth"));
        assert_eq!(
            found.last().unwrap(),
            Path::new("/src/src-tauri/binaries/fndr-auth-aarch64-apple-darwin")
        );
    }

    #[test]
    fn biometric_helper_only_authenticates_explicit_success() {
        assert!(parse_biometric_helper_output("{\"type\":\"authenticated\"}\n").unwrap());
        assert!(!parse_biometric_helper_output("{\"type\":\"cancelled\"}\n").unwrap());
        assert!(!parse_biometric_helper_output("{\"type\":\"unavailable\"}\n").unwrap());
        assert!(parse_biometric_helper_output("not-json\n").is_err());
    }
}
