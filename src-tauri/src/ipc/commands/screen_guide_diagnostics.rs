//! Explicit, one-shot local diagnostics for Screen Guide.
//!
//! This module is deliberately separate from ActivityTrace. ActivityTrace is
//! safe for ordinary product UI; these artifacts can contain the exact pixels
//! and OCR text the user explicitly asked FNDR to preserve for one turn.

use super::screen_guide::ScreenGuideActivityStage;
use crate::ocr::ScreenGuideOcrLine;
use crate::AppState;
use serde::Serialize;
use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
#[cfg(target_os = "macos")]
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::State;
use uuid::Uuid;

pub(crate) const ARM_TTL: Duration = Duration::from_secs(5 * 60);
const BUNDLE_MAX_AGE: Duration = Duration::from_secs(24 * 60 * 60);
const MAINTENANCE_INTERVAL: Duration = Duration::from_secs(5 * 60);
const MAX_BUNDLES: usize = 2;
const MAX_TOTAL_BYTES: u64 = 64 * 1024 * 1024;
const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ScreenGuideDiagnosticResultKind {
    Saved,
    Error,
    Deleted,
    Cancelled,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ScreenGuideDiagnosticResultCode {
    BundleSaved,
    StorageUnavailable,
    BundleTooLarge,
    DiagnosticsDeleted,
    PrivateModeDisarmed,
    PrivacySettingsDisarmed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScreenGuideDiagnosticDisarmReason {
    PrivateMode,
    PrivacySettings,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ScreenGuideDiagnosticReceipt {
    pub kind: ScreenGuideDiagnosticResultKind,
    pub code: ScreenGuideDiagnosticResultCode,
    pub message: String,
    pub screenshot_saved: bool,
    pub ocr_saved: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ScreenGuideDiagnosticStatus {
    pub armed: bool,
    pub expires_in_ms: Option<u64>,
    pub bundle_count: usize,
    pub partial_count: usize,
    pub total_bytes: u64,
    pub last_result: Option<ScreenGuideDiagnosticReceipt>,
}

#[derive(Debug, Default)]
struct DiagnosticArm {
    expires_at: Option<Instant>,
    last_result: Option<ScreenGuideDiagnosticReceipt>,
}

impl DiagnosticArm {
    fn arm(&mut self, now: Instant) {
        self.expires_at = Some(now + ARM_TTL);
        self.last_result = None;
    }

    fn disarm(&mut self) {
        self.expires_at = None;
    }

    fn status(&mut self, now: Instant) -> ScreenGuideDiagnosticStatus {
        let remaining = self.expires_at.and_then(|expires_at| {
            expires_at
                .checked_duration_since(now)
                .filter(|remaining| !remaining.is_zero())
        });
        if remaining.is_none() {
            self.expires_at = None;
        }
        ScreenGuideDiagnosticStatus {
            armed: remaining.is_some(),
            expires_in_ms: remaining.map(duration_millis),
            bundle_count: 0,
            partial_count: 0,
            total_bytes: 0,
            last_result: self.last_result.clone(),
        }
    }

    #[cfg(test)]
    fn take(&mut self, now: Instant) -> bool {
        let armed = self.status(now).armed;
        self.disarm();
        armed
    }
}

fn receipt(
    kind: ScreenGuideDiagnosticResultKind,
    code: ScreenGuideDiagnosticResultCode,
    message: &'static str,
    screenshot_saved: bool,
    ocr_saved: bool,
) -> ScreenGuideDiagnosticReceipt {
    ScreenGuideDiagnosticReceipt {
        kind,
        code,
        message: message.to_string(),
        screenshot_saved,
        ocr_saved,
    }
}

fn storage_error_receipt(code: ScreenGuideDiagnosticResultCode) -> ScreenGuideDiagnosticReceipt {
    let message = match code {
        ScreenGuideDiagnosticResultCode::BundleTooLarge => {
            "The diagnostic exceeded the 64 MiB local limit and was not saved."
        }
        _ => "The diagnostic could not be saved to private FNDR app data.",
    };
    receipt(
        ScreenGuideDiagnosticResultKind::Error,
        code,
        message,
        false,
        false,
    )
}

fn set_last_result(result: ScreenGuideDiagnosticReceipt) {
    match lock_arm() {
        Ok(mut arm) => arm.last_result = Some(result),
        Err(error) => tracing::warn!(%error, "screen_guide:diagnostic_receipt_unavailable"),
    }
}

pub(crate) fn report_storage_failure(error: &io::Error) {
    let code = if error.kind() == io::ErrorKind::FileTooLarge {
        ScreenGuideDiagnosticResultCode::BundleTooLarge
    } else {
        ScreenGuideDiagnosticResultCode::StorageUnavailable
    };
    set_last_result(storage_error_receipt(code));
}

fn disarm_control_for_privacy(
    arm: &mut DiagnosticArm,
    reason: ScreenGuideDiagnosticDisarmReason,
) {
    let was_armed = arm.expires_at.is_some();
    arm.disarm();
    if was_armed {
        let (code, message) = match reason {
            ScreenGuideDiagnosticDisarmReason::PrivateMode => (
                ScreenGuideDiagnosticResultCode::PrivateModeDisarmed,
                "Private Mode cancelled the pending diagnostic turn.",
            ),
            ScreenGuideDiagnosticDisarmReason::PrivacySettings => (
                ScreenGuideDiagnosticResultCode::PrivacySettingsDisarmed,
                "A privacy setting change cancelled the pending diagnostic turn.",
            ),
        };
        arm.last_result = Some(receipt(
            ScreenGuideDiagnosticResultKind::Cancelled,
            code,
            message,
            false,
            false,
        ));
    }
}

pub(crate) fn disarm_for_privacy(reason: ScreenGuideDiagnosticDisarmReason) {
    match lock_arm() {
        Ok(mut arm) => disarm_control_for_privacy(&mut arm, reason),
        Err(error) => tracing::warn!(%error, "screen_guide:diagnostic_privacy_disarm_failed"),
    }
}

fn arm_control_if_not_private(
    arm: &mut DiagnosticArm,
    is_incognito: &AtomicBool,
    now: Instant,
) -> Result<(), &'static str> {
    if is_incognito.load(Ordering::SeqCst) {
        disarm_control_for_privacy(arm, ScreenGuideDiagnosticDisarmReason::PrivateMode);
        return Err("Screen Guide diagnostics are unavailable in Private Mode.");
    }
    arm.arm(now);
    Ok(())
}

fn duration_millis(duration: Duration) -> u64 {
    duration.as_millis().min(u128::from(u64::MAX)) as u64
}

fn arm_state() -> &'static Mutex<DiagnosticArm> {
    static STATE: OnceLock<Mutex<DiagnosticArm>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(DiagnosticArm::default()))
}

fn lock_arm() -> io::Result<std::sync::MutexGuard<'static, DiagnosticArm>> {
    arm_state()
        .lock()
        .map_err(|_| io::Error::other("Screen Guide diagnostic arm is unavailable"))
}

pub(crate) fn diagnostics_root(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("diagnostics").join("screen-guide")
}

fn active_partial_paths() -> &'static Mutex<HashSet<PathBuf>> {
    static ACTIVE: OnceLock<Mutex<HashSet<PathBuf>>> = OnceLock::new();
    ACTIVE.get_or_init(|| Mutex::new(HashSet::new()))
}

fn register_active_partial(root: &Path, path: &Path) -> io::Result<()> {
    let mut active = active_partial_paths()
        .lock()
        .map_err(|_| io::Error::other("Screen Guide diagnostic sessions are unavailable"))?;
    if active
        .iter()
        .any(|active_path| active_path.parent() == Some(root))
    {
        return Err(io::Error::new(
            io::ErrorKind::WouldBlock,
            "A Screen Guide diagnostic session is already active",
        ));
    }
    active.insert(path.to_path_buf());
    Ok(())
}

fn unregister_active_partial(path: &Path) {
    match active_partial_paths().lock() {
        Ok(mut active) => {
            active.remove(path);
        }
        Err(_) => tracing::warn!("screen_guide:diagnostic_session_registry_unavailable"),
    }
}

fn active_partials_snapshot() -> io::Result<HashSet<PathBuf>> {
    active_partial_paths()
        .lock()
        .map(|active| active.clone())
        .map_err(|_| io::Error::other("Screen Guide diagnostic sessions are unavailable"))
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ScreenGuideDiagnosticDisplay {
    pub display_id: Option<u32>,
    /// Non-reversible numeric signature, if the capture backend exposes one.
    pub display_signature: Option<u64>,
    pub origin_x_points: Option<f64>,
    pub origin_y_points: Option<f64>,
    pub width_points: Option<f64>,
    pub height_points: Option<f64>,
    pub scale_factor: Option<f64>,
}

/// Aggregate signal already computed by the capture path. Reusing it avoids a
/// second full PNG decode solely for diagnostics.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ScreenGuideDiagnosticCaptureMetrics {
    pub width_pixels: u32,
    pub height_pixels: u32,
    pub mean_luma: f64,
    pub luma_variance: f64,
    pub luma_range: u8,
    pub transparent_pixel_ratio: f64,
}

/// A privacy-reduced active-window sample. Titles and URLs are intentionally
/// represented only by presence/verification flags, never their raw values.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ScreenGuideDiagnosticContextProbe {
    pub attempt: u32,
    pub app_name: Option<String>,
    pub bundle_id: Option<String>,
    pub title_present: bool,
    pub title_verified: bool,
    pub browser_url_present: bool,
    pub is_internal: bool,
    pub matched_latched_target: bool,
    pub rank: u8,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ScreenGuideDiagnosticOutcome {
    Completed,
    PermissionDenied,
    ContextUnavailable,
    PrivacyBlocked,
    CaptureFailed,
    TargetChanged,
    OcrFailed,
    NoReadableText,
    ModelUnavailable,
    Failed,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TimedStage {
    stage: ScreenGuideActivityStage,
    offset_ms: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TimedContextProbe {
    #[serde(flatten)]
    probe: ScreenGuideDiagnosticContextProbe,
    offset_ms: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CaptureSummary {
    byte_count: usize,
    width_pixels: u32,
    height_pixels: u32,
    mean_luma: f64,
    luma_variance: f64,
    luma_range: u8,
    transparent_pixel_ratio: f64,
    raw_capture_saved: bool,
    display: ScreenGuideDiagnosticDisplay,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct OcrSummary {
    minimum_text_height: f32,
    plain_text_chars: usize,
    plain_text_line_count: usize,
    positioned_line_count: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DiagnosticManifest {
    schema_version: u32,
    outcome: ScreenGuideDiagnosticOutcome,
    duration_ms: u64,
    stages: Vec<TimedStage>,
    context_probes: Vec<TimedContextProbe>,
    capture: Option<CaptureSummary>,
    ocr: Option<OcrSummary>,
}

#[derive(Debug, Serialize)]
struct StoredOcrLine<'a> {
    text: &'a str,
    x: f64,
    y: f64,
}

/// A consumed one-shot diagnostic turn. Raw artifacts remain inside its
/// private partial directory until `finish` atomically publishes the bundle.
pub(crate) struct ScreenGuideDiagnosticSession {
    root: PathBuf,
    partial_dir: PathBuf,
    final_dir: PathBuf,
    started: Instant,
    stages: Vec<TimedStage>,
    context_probes: Vec<TimedContextProbe>,
    capture: Option<CaptureSummary>,
    ocr: Option<OcrSummary>,
    finished: bool,
}

impl ScreenGuideDiagnosticSession {
    fn new(app_data_dir: &Path, wall_time: SystemTime, started: Instant) -> io::Result<Self> {
        let root = diagnostics_root(app_data_dir);
        create_secure_dir(&root)?;
        cleanup_bundles(&root, wall_time)?;

        let bundle_name = format!("{}-{}", unix_millis(wall_time), Uuid::new_v4());
        let partial_dir = root.join(format!("{bundle_name}.partial"));
        let final_dir = root.join(bundle_name);
        create_secure_dir(&partial_dir)?;
        if let Err(error) = register_active_partial(&root, &partial_dir) {
            let _ = fs::remove_dir_all(&partial_dir);
            return Err(error);
        }

        Ok(Self {
            root,
            partial_dir,
            final_dir,
            started,
            stages: Vec::new(),
            context_probes: Vec::new(),
            capture: None,
            ocr: None,
            finished: false,
        })
    }

    pub(crate) fn record_stage(&mut self, stage: ScreenGuideActivityStage) {
        self.stages.push(TimedStage {
            stage,
            offset_ms: duration_millis(self.started.elapsed()),
        });
    }

    pub(crate) fn record_context_probe(&mut self, probe: ScreenGuideDiagnosticContextProbe) {
        self.context_probes.push(TimedContextProbe {
            probe,
            offset_ms: duration_millis(self.started.elapsed()),
        });
    }

    fn record_capture_summary(
        &mut self,
        byte_count: usize,
        display: ScreenGuideDiagnosticDisplay,
        metrics: ScreenGuideDiagnosticCaptureMetrics,
        raw_capture_saved: bool,
    ) {
        self.capture = Some(CaptureSummary {
            byte_count,
            width_pixels: metrics.width_pixels,
            height_pixels: metrics.height_pixels,
            mean_luma: metrics.mean_luma,
            luma_variance: metrics.luma_variance,
            luma_range: metrics.luma_range,
            transparent_pixel_ratio: metrics.transparent_pixel_ratio,
            raw_capture_saved,
            display,
        });
    }

    /// Record aggregate capture evidence without retaining pixels. Failure and
    /// privacy outcomes can remain useful without publishing raw artifacts.
    pub(crate) fn record_capture_without_raw(
        &mut self,
        byte_count: usize,
        display: ScreenGuideDiagnosticDisplay,
        metrics: ScreenGuideDiagnosticCaptureMetrics,
    ) {
        self.record_capture_summary(byte_count, display, metrics, false);
    }

    /// Persist the exact encoded image passed to Vision OCR after OCR and the
    /// post-OCR privacy decision have completed.
    pub(crate) fn write_capture(
        &mut self,
        png: &[u8],
        display: ScreenGuideDiagnosticDisplay,
        metrics: ScreenGuideDiagnosticCaptureMetrics,
    ) -> io::Result<()> {
        if png.len() as u64 > MAX_TOTAL_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::FileTooLarge,
                "Screen Guide diagnostic capture exceeds the local size limit",
            ));
        }
        self.write_bounded_artifact("capture.png", png)?;
        self.record_capture_summary(png.len(), display, metrics, true);
        Ok(())
    }

    /// Persist the exact OCR text and point-eligible lines. Neither value is
    /// copied into the aggregate manifest.
    pub(crate) fn write_ocr(
        &mut self,
        plain_text: &str,
        lines: &[ScreenGuideOcrLine],
        minimum_text_height: f32,
    ) -> io::Result<()> {
        self.write_bounded_artifact("ocr.txt", plain_text.as_bytes())?;
        let stored_lines: Vec<_> = lines
            .iter()
            .map(|line| StoredOcrLine {
                text: line.text.as_str(),
                x: line.x,
                y: line.y,
            })
            .collect();
        let encoded = serde_json::to_vec_pretty(&stored_lines).map_err(io::Error::other)?;
        self.write_bounded_artifact("ocr-lines.json", &encoded)?;
        self.ocr = Some(OcrSummary {
            minimum_text_height,
            plain_text_chars: plain_text.chars().count(),
            plain_text_line_count: plain_text
                .lines()
                .filter(|line| !line.trim().is_empty())
                .count(),
            positioned_line_count: lines.len(),
        });
        Ok(())
    }

    fn write_bounded_artifact(&self, name: &str, contents: &[u8]) -> io::Result<()> {
        let projected = directory_size(&self.partial_dir)?.saturating_add(contents.len() as u64);
        if projected > MAX_TOTAL_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::FileTooLarge,
                "Screen Guide diagnostic bundle exceeds the local size limit",
            ));
        }
        write_secure_file(&self.partial_dir.join(name), contents)?;
        cleanup_bundles(&self.root, SystemTime::now())
    }

    /// Remove any raw pixels and OCR written before a later privacy gate
    /// rejected the turn. Aggregate context and stage evidence remains so a
    /// privacy-blocked manifest can still explain where the turn stopped.
    pub(crate) fn discard_raw_artifacts(&mut self) -> io::Result<()> {
        let mut first_error = None;
        for name in ["capture.png", "ocr.txt", "ocr-lines.json"] {
            match fs::remove_file(self.partial_dir.join(name)) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) if first_error.is_none() => first_error = Some(error),
                Err(_) => {}
            }
        }
        self.capture = None;
        self.ocr = None;
        match first_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    /// Remove identity-bearing context from terminal privacy manifests. The
    /// remaining booleans/ranks are aggregate execution evidence only.
    pub(crate) fn prepare_privacy_blocked_manifest(&mut self) -> io::Result<()> {
        self.discard_raw_artifacts()?;
        for timed in &mut self.context_probes {
            timed.probe.app_name = None;
            timed.probe.bundle_id = None;
        }
        Ok(())
    }

    pub(crate) fn finish(mut self, outcome: ScreenGuideDiagnosticOutcome) -> io::Result<PathBuf> {
        let screenshot_saved = self
            .capture
            .as_ref()
            .is_some_and(|capture| capture.raw_capture_saved);
        let ocr_saved = self.ocr.is_some();
        let result = self.finish_inner(outcome);
        match &result {
            Ok(_) => set_last_result(receipt(
                ScreenGuideDiagnosticResultKind::Saved,
                ScreenGuideDiagnosticResultCode::BundleSaved,
                "The one-turn Screen Guide diagnostic was saved locally.",
                screenshot_saved,
                ocr_saved,
            )),
            Err(error) => report_storage_failure(error),
        }
        result
    }

    fn finish_inner(&mut self, outcome: ScreenGuideDiagnosticOutcome) -> io::Result<PathBuf> {
        let manifest = DiagnosticManifest {
            schema_version: SCHEMA_VERSION,
            outcome,
            duration_ms: duration_millis(self.started.elapsed()),
            stages: std::mem::take(&mut self.stages),
            context_probes: std::mem::take(&mut self.context_probes),
            capture: self.capture.take(),
            ocr: self.ocr.take(),
        };
        let encoded = serde_json::to_vec_pretty(&manifest).map_err(io::Error::other)?;
        self.write_bounded_artifact("manifest.json", &encoded)?;
        if directory_size(&self.partial_dir)? > MAX_TOTAL_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::FileTooLarge,
                "Screen Guide diagnostic bundle exceeds the local size limit",
            ));
        }
        fs::rename(&self.partial_dir, &self.final_dir)?;
        unregister_active_partial(&self.partial_dir);
        self.finished = true;
        cleanup_bundles(&self.root, SystemTime::now())?;
        if !self.final_dir.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::FileTooLarge,
                "Screen Guide diagnostic bundle was removed by retention",
            ));
        }
        Ok(self.final_dir.clone())
    }
}

impl Drop for ScreenGuideDiagnosticSession {
    fn drop(&mut self) {
        if !self.finished {
            let _ = fs::remove_dir_all(&self.partial_dir);
        }
        unregister_active_partial(&self.partial_dir);
    }
}

/// Consume the one-shot arm before Screen Guide starts any capture work.
/// Private Mode always disarms and returns no session.
pub(crate) fn take_session(
    app_data_dir: &Path,
    private_mode: bool,
) -> io::Result<Option<ScreenGuideDiagnosticSession>> {
    let now = Instant::now();
    let mut arm = lock_arm()?;
    take_session_with_control(&mut arm, app_data_dir, private_mode, now, SystemTime::now())
}

fn take_session_with_control(
    arm: &mut DiagnosticArm,
    app_data_dir: &Path,
    private_mode: bool,
    now: Instant,
    wall_time: SystemTime,
) -> io::Result<Option<ScreenGuideDiagnosticSession>> {
    if private_mode {
        disarm_control_for_privacy(arm, ScreenGuideDiagnosticDisarmReason::PrivateMode);
        return Ok(None);
    }
    if !arm.status(now).armed {
        return Ok(None);
    }

    // Create private storage before consuming consent. If the disk is full or
    // unavailable, the arm remains live for its original remaining TTL.
    match ScreenGuideDiagnosticSession::new(app_data_dir, wall_time, now) {
        Ok(session) => {
            arm.disarm();
            arm.last_result = None;
            Ok(Some(session))
        }
        Err(error) => {
            arm.last_result = Some(storage_error_receipt(
                ScreenGuideDiagnosticResultCode::StorageUnavailable,
            ));
            Err(error)
        }
    }
}

#[tauri::command]
pub fn arm_screen_guide_diagnostic(
    state: State<'_, Arc<AppState>>,
) -> Result<ScreenGuideDiagnosticStatus, String> {
    if state.is_incognito.load(Ordering::SeqCst) {
        return Err("Screen Guide diagnostics are unavailable in Private Mode.".to_string());
    }
    let root = diagnostics_root(&state.app_data_dir);
    create_secure_dir(&root).map_err(command_error)?;
    cleanup_bundles(&root, SystemTime::now()).map_err(command_error)?;
    let mut arm = lock_arm().map_err(command_error)?;
    // The first check avoids unnecessary disk work. This second check is the
    // privacy boundary: Private Mode cancellation takes the same arm mutex,
    // so a concurrent transition either wins here or immediately disarms the
    // newly-created consent before it can be consumed.
    arm_control_if_not_private(&mut arm, &state.is_incognito, Instant::now())
        .map_err(str::to_string)?;
    status_with_storage(&mut arm, &root).map_err(command_error)
}

#[tauri::command]
pub fn get_screen_guide_diagnostic_status(
    state: State<'_, Arc<AppState>>,
) -> Result<ScreenGuideDiagnosticStatus, String> {
    let root = diagnostics_root(&state.app_data_dir);
    if root.exists() {
        cleanup_bundles(&root, SystemTime::now()).map_err(command_error)?;
    }
    let mut arm = lock_arm().map_err(command_error)?;
    if state.is_incognito.load(Ordering::SeqCst) {
        disarm_control_for_privacy(&mut arm, ScreenGuideDiagnosticDisarmReason::PrivateMode);
    }
    status_with_storage(&mut arm, &root).map_err(command_error)
}

#[tauri::command]
pub fn delete_screen_guide_diagnostics(
    state: State<'_, Arc<AppState>>,
) -> Result<ScreenGuideDiagnosticStatus, String> {
    let root = diagnostics_root(&state.app_data_dir);
    {
        let mut arm = lock_arm().map_err(command_error)?;
        arm.disarm();
        arm.last_result = Some(receipt(
            ScreenGuideDiagnosticResultKind::Deleted,
            ScreenGuideDiagnosticResultCode::DiagnosticsDeleted,
            "All Screen Guide diagnostics were deleted.",
            false,
            false,
        ));
    }
    if root.exists() {
        fs::remove_dir_all(&root).map_err(command_error)?;
    }
    Ok(ScreenGuideDiagnosticStatus {
        armed: false,
        expires_in_ms: None,
        bundle_count: 0,
        partial_count: 0,
        total_bytes: 0,
        last_result: Some(receipt(
            ScreenGuideDiagnosticResultKind::Deleted,
            ScreenGuideDiagnosticResultCode::DiagnosticsDeleted,
            "All Screen Guide diagnostics were deleted.",
            false,
            false,
        )),
    })
}

/// Run once during app startup and then independently of whether the Screen
/// Guide panel is opened. Incomplete directories from a prior process are not
/// active in this process and are removed on the first pass.
pub fn start_screen_guide_diagnostic_maintenance(app_data_dir: PathBuf) {
    let root = diagnostics_root(&app_data_dir);
    if root.exists() {
        if let Err(error) = cleanup_bundles(&root, SystemTime::now()) {
            tracing::warn!(%error, "screen_guide:diagnostic_startup_cleanup_failed");
        }
    }

    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(MAINTENANCE_INTERVAL).await;
            let root = root.clone();
            let cleanup = tokio::task::spawn_blocking(move || {
                if root.exists() {
                    cleanup_bundles(&root, SystemTime::now())
                } else {
                    Ok(())
                }
            })
            .await;
            match cleanup {
                Ok(Ok(())) => {}
                Ok(Err(error)) => {
                    tracing::warn!(%error, "screen_guide:diagnostic_periodic_cleanup_failed")
                }
                Err(error) => tracing::warn!(
                    %error,
                    "screen_guide:diagnostic_periodic_cleanup_stopped"
                ),
            }
        }
    });
}

#[tauri::command]
pub fn reveal_screen_guide_diagnostics(
    state: State<'_, Arc<AppState>>,
) -> Result<ScreenGuideDiagnosticStatus, String> {
    let root = diagnostics_root(&state.app_data_dir);
    if root.exists() {
        cleanup_bundles(&root, SystemTime::now()).map_err(command_error)?;
    }
    let mut arm = lock_arm().map_err(command_error)?;
    let status = status_with_storage(&mut arm, &root).map_err(command_error)?;
    if status.bundle_count == 0 && status.partial_count == 0 {
        return Err("There are no Screen Guide diagnostics to reveal.".to_string());
    }
    reveal_diagnostics_root(&root)?;
    Ok(status)
}

#[cfg(target_os = "macos")]
fn reveal_diagnostics_root(root: &Path) -> Result<(), String> {
    Command::new("open")
        .arg(root)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("Could not reveal Screen Guide diagnostics: {error}"))
}

#[cfg(not(target_os = "macos"))]
fn reveal_diagnostics_root(_root: &Path) -> Result<(), String> {
    Err("Revealing Screen Guide diagnostics is available on macOS.".to_string())
}

fn command_error(error: io::Error) -> String {
    report_storage_failure(&error);
    tracing::warn!(error = %error, "Screen Guide diagnostic storage failed");
    "Screen Guide diagnostic storage is unavailable.".to_string()
}

fn status_with_storage(
    arm: &mut DiagnosticArm,
    root: &Path,
) -> io::Result<ScreenGuideDiagnosticStatus> {
    let mut status = arm.status(Instant::now());
    let storage = diagnostic_storage_summary(root)?;
    status.bundle_count = storage.bundle_count;
    status.partial_count = storage.partial_count;
    status.total_bytes = storage.total_bytes;
    Ok(status)
}

fn create_secure_dir(path: &Path) -> io::Result<()> {
    fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn write_secure_file(path: &Path, contents: &[u8]) -> io::Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(contents)?;
    file.sync_all()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

#[derive(Debug, Clone, Copy)]
struct RetentionPolicy {
    max_bundles: usize,
    max_age: Duration,
    max_bytes: u64,
}

fn production_retention() -> RetentionPolicy {
    RetentionPolicy {
        max_bundles: MAX_BUNDLES,
        max_age: BUNDLE_MAX_AGE,
        max_bytes: MAX_TOTAL_BYTES,
    }
}

fn cleanup_bundles(root: &Path, now: SystemTime) -> io::Result<()> {
    cleanup_bundles_with_policy(root, now, production_retention()).map(|_| ())
}

#[derive(Debug)]
struct BundleEntry {
    path: PathBuf,
    created_ms: u128,
    size: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct DiagnosticStorageSummary {
    bundle_count: usize,
    partial_count: usize,
    total_bytes: u64,
}

fn cleanup_bundles_with_policy(
    root: &Path,
    now: SystemTime,
    policy: RetentionPolicy,
) -> io::Result<DiagnosticStorageSummary> {
    if !root.exists() {
        return Ok(DiagnosticStorageSummary::default());
    }
    let active_paths = active_partials_snapshot()?;
    let now_ms = unix_millis(now);
    let mut complete = Vec::new();
    let mut active_bytes = 0_u64;
    let mut active_count = 0_usize;
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let Some((created_ms, partial)) = parse_bundle_name(&name) else {
            continue;
        };
        let age_ms = now_ms.saturating_sub(created_ms);
        if partial {
            if active_paths.contains(&entry.path()) {
                active_count += 1;
                active_bytes = active_bytes.saturating_add(directory_size(&entry.path())?);
            } else {
                // A .partial directory not registered in this process is an
                // abandoned/crash artifact. It is never published or retained.
                fs::remove_dir_all(entry.path())?;
            }
            continue;
        }
        if age_ms > policy.max_age.as_millis() {
            fs::remove_dir_all(entry.path())?;
            continue;
        }
        complete.push(BundleEntry {
            size: directory_size(&entry.path())?,
            path: entry.path(),
            created_ms,
        });
    }

    complete.sort_by(|left, right| {
        right
            .created_ms
            .cmp(&left.created_ms)
            .then_with(|| right.path.cmp(&left.path))
    });
    let mut kept = 0_usize;
    let mut kept_bytes = active_bytes;
    for entry in complete {
        let within_count = kept < policy.max_bundles;
        let within_bytes = kept_bytes.saturating_add(entry.size) <= policy.max_bytes;
        if within_count && within_bytes {
            kept += 1;
            kept_bytes = kept_bytes.saturating_add(entry.size);
        } else {
            fs::remove_dir_all(entry.path)?;
        }
    }
    Ok(DiagnosticStorageSummary {
        bundle_count: kept,
        partial_count: active_count,
        total_bytes: kept_bytes,
    })
}

fn parse_bundle_name(name: &str) -> Option<(u128, bool)> {
    let (base, partial) = match name.strip_suffix(".partial") {
        Some(base) => (base, true),
        None => (name, false),
    };
    let (created, id) = base.split_once('-')?;
    let created_ms = created.parse().ok()?;
    Uuid::parse_str(id).ok()?;
    Some((created_ms, partial))
}

fn diagnostic_storage_summary(root: &Path) -> io::Result<DiagnosticStorageSummary> {
    if !root.exists() {
        return Ok(DiagnosticStorageSummary::default());
    }
    let active_paths = active_partials_snapshot()?;
    let mut summary = DiagnosticStorageSummary::default();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        match parse_bundle_name(&name) {
            Some((_, false)) => summary.bundle_count += 1,
            Some((_, true)) if active_paths.contains(&entry.path()) => summary.partial_count += 1,
            _ => continue,
        }
        summary.total_bytes = summary
            .total_bytes
            .saturating_add(directory_size(&entry.path())?);
    }
    Ok(summary)
}

fn directory_size(path: &Path) -> io::Result<u64> {
    let mut bytes = 0_u64;
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if file_type.is_file() {
            bytes = bytes.saturating_add(entry.metadata()?.len());
        } else if file_type.is_dir() {
            bytes = bytes.saturating_add(directory_size(&entry.path())?);
        }
    }
    Ok(bytes)
}

fn unix_millis(time: SystemTime) -> u128 {
    time.duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_millis()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::commands::screen_guide::ScreenGuideActivityStage;
    use crate::ocr::ScreenGuideOcrLine;
    use std::fs;
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
    use tempfile::tempdir;

    fn test_capture_metrics() -> ScreenGuideDiagnosticCaptureMetrics {
        ScreenGuideDiagnosticCaptureMetrics {
            width_pixels: 1440,
            height_pixels: 900,
            mean_luma: 120.0,
            luma_variance: 64.0,
            luma_range: 220,
            transparent_pixel_ratio: 0.0,
        }
    }

    #[test]
    fn arm_is_one_shot_and_expires() {
        let now = Instant::now();
        let mut arm = DiagnosticArm::default();

        arm.arm(now);
        assert!(arm.status(now).armed);
        assert!(arm.take(now));
        assert!(!arm.take(now));

        arm.arm(now);
        assert!(!arm.status(now + ARM_TTL + Duration::from_millis(1)).armed);
        assert!(!arm.take(now + ARM_TTL + Duration::from_millis(1)));
    }

    #[test]
    fn private_mode_transition_disarms_pending_collection() {
        let now = Instant::now();
        let mut arm = DiagnosticArm::default();
        arm.arm(now);

        disarm_control_for_privacy(&mut arm, ScreenGuideDiagnosticDisarmReason::PrivateMode);

        assert!(!arm.status(now).armed);
        assert_eq!(
            arm.status(now).last_result.map(|result| result.code),
            Some(ScreenGuideDiagnosticResultCode::PrivateModeDisarmed)
        );
    }

    #[test]
    fn privacy_setting_transition_reports_its_own_reason() {
        let now = Instant::now();
        let mut arm = DiagnosticArm::default();
        arm.arm(now);

        disarm_control_for_privacy(
            &mut arm,
            ScreenGuideDiagnosticDisarmReason::PrivacySettings,
        );

        let status = arm.status(now);
        assert!(!status.armed);
        assert_eq!(
            status.last_result.map(|result| result.code),
            Some(ScreenGuideDiagnosticResultCode::PrivacySettingsDisarmed)
        );
    }

    #[test]
    fn session_creation_failure_preserves_the_pending_arm() {
        let temp = tempdir().unwrap();
        let blocker = temp.path().join("not-a-directory");
        fs::write(&blocker, b"block directory creation").unwrap();
        let now = Instant::now();
        let mut arm = DiagnosticArm::default();
        arm.arm(now);

        let result = take_session_with_control(&mut arm, &blocker, false, now, SystemTime::now());

        assert!(result.is_err());
        let status = arm.status(now);
        assert!(status.armed);
        assert_eq!(
            status.last_result.map(|result| result.code),
            Some(ScreenGuideDiagnosticResultCode::StorageUnavailable)
        );
    }

    #[test]
    fn session_writes_exact_artifacts_and_keeps_raw_values_out_of_manifest() {
        let temp = tempdir().unwrap();
        let started_at = SystemTime::now();
        let mut session =
            ScreenGuideDiagnosticSession::new(temp.path(), started_at, Instant::now()).unwrap();
        let png = b"exact-png-payload";
        let ocr = "private OCR sentinel";
        let lines = vec![ScreenGuideOcrLine {
            text: "private line sentinel".into(),
            x: 0.25,
            y: 0.75,
        }];

        session.record_stage(ScreenGuideActivityStage::Capturing);
        session.record_context_probe(ScreenGuideDiagnosticContextProbe {
            attempt: 1,
            app_name: Some("Google Chrome".into()),
            bundle_id: Some("com.google.Chrome".into()),
            title_present: true,
            title_verified: true,
            browser_url_present: true,
            is_internal: false,
            matched_latched_target: true,
            rank: 4,
        });
        session
            .write_capture(
                png,
                ScreenGuideDiagnosticDisplay::default(),
                test_capture_metrics(),
            )
            .unwrap();
        session.write_ocr(ocr, &lines, 0.015).unwrap();
        let bundle = session
            .finish(ScreenGuideDiagnosticOutcome::NoReadableText)
            .unwrap();

        assert_eq!(fs::read(bundle.join("capture.png")).unwrap(), png);
        assert_eq!(fs::read_to_string(bundle.join("ocr.txt")).unwrap(), ocr);
        let positioned = fs::read_to_string(bundle.join("ocr-lines.json")).unwrap();
        assert!(positioned.contains("private line sentinel"));

        let manifest = fs::read_to_string(bundle.join("manifest.json")).unwrap();
        assert!(!manifest.contains("private OCR sentinel"));
        assert!(!manifest.contains("private line sentinel"));
        for forbidden in [
            "\"question\":",
            "\"history\":",
            "\"answer\":",
            "\"audio\":",
            "\"windowTitle\":",
            "\"url\":",
        ] {
            assert!(
                !manifest.contains(forbidden),
                "manifest contained {forbidden}"
            );
        }
        assert!(manifest.contains("no_readable_text"));
        assert!(manifest.contains("Google Chrome"));
    }

    #[test]
    fn privacy_rejection_discards_raw_artifacts_but_keeps_aggregate_manifest() {
        let temp = tempdir().unwrap();
        let mut session =
            ScreenGuideDiagnosticSession::new(temp.path(), SystemTime::now(), Instant::now())
                .unwrap();
        session.record_context_probe(ScreenGuideDiagnosticContextProbe {
            attempt: 1,
            app_name: Some("Private Browser".into()),
            bundle_id: Some("com.example.private".into()),
            title_present: true,
            title_verified: true,
            browser_url_present: true,
            is_internal: false,
            matched_latched_target: true,
            rank: 4,
        });
        session
            .write_capture(
                b"private pixels",
                ScreenGuideDiagnosticDisplay::default(),
                test_capture_metrics(),
            )
            .unwrap();
        session
            .write_ocr(
                "private text",
                &[ScreenGuideOcrLine {
                    text: "private line".into(),
                    x: 0.5,
                    y: 0.5,
                }],
                0.015,
            )
            .unwrap();

        session.prepare_privacy_blocked_manifest().unwrap();
        let bundle = session
            .finish(ScreenGuideDiagnosticOutcome::PrivacyBlocked)
            .unwrap();

        assert!(!bundle.join("capture.png").exists());
        assert!(!bundle.join("ocr.txt").exists());
        assert!(!bundle.join("ocr-lines.json").exists());
        let manifest = fs::read_to_string(bundle.join("manifest.json")).unwrap();
        assert!(manifest.contains("privacy_blocked"));
        assert!(manifest.contains("\"capture\": null"));
        assert!(manifest.contains("\"ocr\": null"));
        assert!(!manifest.contains("private text"));
        assert!(!manifest.contains("Private Browser"));
        assert!(!manifest.contains("com.example.private"));
    }

    #[test]
    fn aggregate_capture_evidence_does_not_create_a_raw_file() {
        let temp = tempdir().unwrap();
        let mut session =
            ScreenGuideDiagnosticSession::new(temp.path(), SystemTime::now(), Instant::now())
                .unwrap();
        session.record_capture_without_raw(
            42,
            ScreenGuideDiagnosticDisplay::default(),
            test_capture_metrics(),
        );
        assert!(!session.partial_dir.join("capture.png").exists());

        let bundle = session
            .finish(ScreenGuideDiagnosticOutcome::OcrFailed)
            .unwrap();
        assert!(!bundle.join("capture.png").exists());
        let manifest = fs::read_to_string(bundle.join("manifest.json")).unwrap();
        assert!(manifest.contains("\"rawCaptureSaved\": false"));
    }

    #[test]
    fn retention_keeps_two_newest_bundles_and_removes_stale_partial() {
        let temp = tempdir().unwrap();
        let root = diagnostics_root(temp.path());
        fs::create_dir_all(&root).unwrap();
        for created_ms in [1_000_u128, 2_000, 3_000] {
            let path = root.join(format!("{created_ms}-00000000-0000-0000-0000-000000000000"));
            fs::create_dir(&path).unwrap();
            fs::write(path.join("manifest.json"), b"{}").unwrap();
        }
        let partial = root.join("1000-10000000-0000-0000-0000-000000000000.partial");
        fs::create_dir(&partial).unwrap();

        cleanup_bundles_with_policy(
            &root,
            UNIX_EPOCH + Duration::from_millis(4_000),
            RetentionPolicy {
                max_bundles: 2,
                max_age: Duration::from_millis(10_000),
                max_bytes: 1024,
            },
        )
        .unwrap();

        assert!(!root
            .join("1000-00000000-0000-0000-0000-000000000000")
            .exists());
        assert!(root
            .join("2000-00000000-0000-0000-0000-000000000000")
            .exists());
        assert!(root
            .join("3000-00000000-0000-0000-0000-000000000000")
            .exists());
        assert!(!partial.exists());
    }

    #[test]
    fn retention_enforces_total_byte_cap_newest_first() {
        let temp = tempdir().unwrap();
        let root = diagnostics_root(temp.path());
        fs::create_dir_all(&root).unwrap();
        for created_ms in [1_000_u128, 2_000] {
            let path = root.join(format!("{created_ms}-00000000-0000-0000-0000-000000000000"));
            fs::create_dir(&path).unwrap();
            fs::write(path.join("capture.png"), [0_u8; 8]).unwrap();
        }

        cleanup_bundles_with_policy(
            &root,
            UNIX_EPOCH + Duration::from_millis(3_000),
            RetentionPolicy {
                max_bundles: 2,
                max_age: Duration::from_millis(10_000),
                max_bytes: 10,
            },
        )
        .unwrap();

        assert!(!root
            .join("1000-00000000-0000-0000-0000-000000000000")
            .exists());
        assert!(root
            .join("2000-00000000-0000-0000-0000-000000000000")
            .exists());
    }

    #[test]
    fn restart_cleanup_removes_crash_partials_and_reports_no_hidden_artifacts() {
        let temp = tempdir().unwrap();
        let root = diagnostics_root(temp.path());
        let partial = root.join("1000-10000000-0000-0000-0000-000000000000.partial");
        fs::create_dir_all(&partial).unwrap();
        fs::write(partial.join("capture.png"), b"abandoned private pixels").unwrap();

        cleanup_bundles(&root, UNIX_EPOCH + Duration::from_millis(2_000)).unwrap();

        assert!(!partial.exists());
        assert_eq!(
            diagnostic_storage_summary(&root).unwrap(),
            DiagnosticStorageSummary::default()
        );
    }

    #[test]
    fn active_partial_is_visible_and_counts_against_retention_bytes() {
        let temp = tempdir().unwrap();
        let root = diagnostics_root(temp.path());
        let mut session =
            ScreenGuideDiagnosticSession::new(temp.path(), SystemTime::now(), Instant::now())
                .unwrap();
        session
            .write_capture(
                b"active pixels",
                ScreenGuideDiagnosticDisplay::default(),
                test_capture_metrics(),
            )
            .unwrap();

        let summary = cleanup_bundles_with_policy(
            &root,
            SystemTime::now(),
            RetentionPolicy {
                max_bundles: 2,
                max_age: Duration::from_secs(60),
                max_bytes: 1024,
            },
        )
        .unwrap();

        assert_eq!(summary.partial_count, 1);
        assert_eq!(summary.bundle_count, 0);
        assert!(summary.total_bytes >= b"active pixels".len() as u64);
        assert!(session.partial_dir.exists());
        session.discard_raw_artifacts().unwrap();
    }

    #[test]
    fn only_one_session_can_be_active_per_diagnostics_root() {
        let temp = tempdir().unwrap();
        let first =
            ScreenGuideDiagnosticSession::new(temp.path(), SystemTime::now(), Instant::now())
                .unwrap();

        let second = match ScreenGuideDiagnosticSession::new(
            temp.path(),
            SystemTime::now(),
            Instant::now(),
        ) {
            Ok(_) => panic!("second concurrent session unexpectedly started"),
            Err(error) => error,
        };
        assert_eq!(second.kind(), io::ErrorKind::WouldBlock);
        assert_eq!(
            fs::read_dir(diagnostics_root(temp.path()))
                .unwrap()
                .filter_map(Result::ok)
                .filter(|entry| entry.path().extension().and_then(|value| value.to_str()) == Some("partial"))
                .count(),
            1
        );

        drop(first);
        let replacement =
            ScreenGuideDiagnosticSession::new(temp.path(), SystemTime::now(), Instant::now())
                .unwrap();
        assert!(replacement.partial_dir.exists());
    }

    #[test]
    fn over_budget_bundle_is_not_published_as_saved() {
        let temp = tempdir().unwrap();
        let session =
            ScreenGuideDiagnosticSession::new(temp.path(), SystemTime::now(), Instant::now())
                .unwrap();
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(session.partial_dir.join("capture.png"))
            .unwrap()
            .set_len(MAX_TOTAL_BYTES)
            .unwrap();
        let final_dir = session.final_dir.clone();

        let error = session
            .finish(ScreenGuideDiagnosticOutcome::Completed)
            .unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::FileTooLarge);
        assert!(!final_dir.exists());
    }

    #[test]
    fn storage_failure_receipt_is_bounded_and_contains_no_path() {
        let result = storage_error_receipt(ScreenGuideDiagnosticResultCode::StorageUnavailable);

        assert_eq!(result.kind, ScreenGuideDiagnosticResultKind::Error);
        assert_eq!(
            result.code,
            ScreenGuideDiagnosticResultCode::StorageUnavailable
        );
        assert!(result.message.len() < 160);
        assert!(!result.message.contains('/'));
        assert!(!result.screenshot_saved);
        assert!(!result.ocr_saved);
    }

    #[cfg(unix)]
    #[test]
    fn bundles_and_artifacts_use_private_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempdir().unwrap();
        let mut session =
            ScreenGuideDiagnosticSession::new(temp.path(), SystemTime::now(), Instant::now())
                .unwrap();
        session
            .write_capture(
                b"pixels",
                ScreenGuideDiagnosticDisplay::default(),
                test_capture_metrics(),
            )
            .unwrap();
        let bundle = session
            .finish(ScreenGuideDiagnosticOutcome::Completed)
            .unwrap();

        assert_eq!(
            fs::metadata(&bundle).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(bundle.join("capture.png"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}
