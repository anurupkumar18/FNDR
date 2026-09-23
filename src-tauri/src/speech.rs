use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Mutex as StdMutex, OnceLock};
use std::time::{Duration, Instant};

use tokio::process::Command as AsyncCommand;
use tokio::sync::Mutex as AsyncMutex;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TranscriptionHint {
    Default,
    VoiceCommand,
}

impl TranscriptionHint {
    fn env_value(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::VoiceCommand => "voice-command",
        }
    }

    fn sidecar_flag(self) -> Option<&'static str> {
        match self {
            Self::Default => None,
            Self::VoiceCommand => Some("--voice-command"),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum SpeechModelKind {
    WhisperBaseEn,
    Orpheus3B,
}

#[derive(Debug, Clone, Copy)]
struct SpeechModelDefinition {
    id: &'static str,
    folder: &'static str,
    filename: &'static str,
    download_url: &'static str,
}

const WHISPER_MODEL: SpeechModelDefinition = SpeechModelDefinition {
    id: "whisper-small-ggml",
    folder: "whisper-small",
    filename: "ggml-small.bin",
    download_url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin",
};

const ORPHEUS_MODEL: SpeechModelDefinition = SpeechModelDefinition {
    id: "orpheus-3b-0.1-ft-q4-k-m",
    folder: "orpheus-3b-0.1-ft",
    filename: "orpheus-3b-0.1-ft-Q4_K_M.gguf",
    download_url:
        "https://huggingface.co/unsloth/orpheus-3b-0.1-ft-GGUF/resolve/main/orpheus-3b-0.1-ft-Q4_K_M.gguf",
};

static SPEECH_DOWNLOAD_LOCK: OnceLock<AsyncMutex<()>> = OnceLock::new();
static SPEECH_BOOTSTRAP_LOCK: OnceLock<AsyncMutex<()>> = OnceLock::new();
static SPEECH_SHUTTING_DOWN: AtomicBool = AtomicBool::new(false);
static ACTIVE_TRANSCRIPTION_COMMANDS: AtomicUsize = AtomicUsize::new(0);
static ACTIVE_VOICE_INPUTS: OnceLock<StdMutex<HashSet<PathBuf>>> = OnceLock::new();
const TRANSCRIPTION_COMMAND_TIMEOUT: Duration = Duration::from_secs(120);
const AUDIO_CONVERSION_TIMEOUT: Duration = Duration::from_secs(30);

fn active_voice_inputs() -> &'static StdMutex<HashSet<PathBuf>> {
    ACTIVE_VOICE_INPUTS.get_or_init(|| StdMutex::new(HashSet::new()))
}

fn download_lock() -> &'static AsyncMutex<()> {
    SPEECH_DOWNLOAD_LOCK.get_or_init(|| AsyncMutex::new(()))
}

fn bootstrap_lock() -> &'static AsyncMutex<()> {
    SPEECH_BOOTSTRAP_LOCK.get_or_init(|| AsyncMutex::new(()))
}

fn definition(kind: SpeechModelKind) -> &'static SpeechModelDefinition {
    match kind {
        SpeechModelKind::WhisperBaseEn => &WHISPER_MODEL,
        SpeechModelKind::Orpheus3B => &ORPHEUS_MODEL,
    }
}

pub fn speech_models_dir(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("speech_models")
}

fn model_dir(app_data_dir: &Path, kind: SpeechModelKind) -> PathBuf {
    speech_models_dir(app_data_dir).join(definition(kind).folder)
}

pub fn model_path(app_data_dir: &Path, kind: SpeechModelKind) -> PathBuf {
    model_dir(app_data_dir, kind).join(definition(kind).filename)
}

fn partial_model_path(app_data_dir: &Path, kind: SpeechModelKind) -> PathBuf {
    model_dir(app_data_dir, kind).join(format!("{}.partial", definition(kind).filename))
}

pub fn voice_cache_dir(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("voice")
}

pub fn make_voice_input_path(app_data_dir: &Path, extension: &str) -> PathBuf {
    voice_cache_dir(app_data_dir).join("input").join(format!(
        "voice-input-{}.{}",
        Uuid::new_v4(),
        extension
    ))
}

pub fn make_tts_output_path(app_data_dir: &Path) -> PathBuf {
    voice_cache_dir(app_data_dir)
        .join("tts")
        .join(format!("speech-{}.wav", Uuid::new_v4()))
}

/// Owns a transient microphone recording and removes it on normal completion,
/// error, timeout, or async task cancellation.
struct TemporaryVoiceInput {
    path: PathBuf,
}

impl TemporaryVoiceInput {
    fn new(path: PathBuf) -> Self {
        if let Ok(mut active) = active_voice_inputs().lock() {
            active.insert(path.clone());
        }
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TemporaryVoiceInput {
    fn drop(&mut self) {
        if let Ok(mut active) = active_voice_inputs().lock() {
            active.remove(&self.path);
        }
        if let Err(error) = std::fs::remove_file(&self.path) {
            if error.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!("Failed to remove temporary voice input: {}", error);
            }
        }
    }
}

struct ActiveTranscriptionCommand;

impl ActiveTranscriptionCommand {
    fn new() -> Self {
        ACTIVE_TRANSCRIPTION_COMMANDS.fetch_add(1, Ordering::SeqCst);
        Self
    }
}

impl Drop for ActiveTranscriptionCommand {
    fn drop(&mut self) {
        ACTIVE_TRANSCRIPTION_COMMANDS.fetch_sub(1, Ordering::SeqCst);
    }
}

async fn wait_for_speech_shutdown() {
    while !SPEECH_SHUTTING_DOWN.load(Ordering::SeqCst) {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// Stop transient speech work before the desktop process exits. FNDR installs
/// a deliberately process-lifetime Tokio runtime, so normal runtime drop cannot
/// be relied on to cancel child processes or delete current microphone input.
pub fn shutdown_speech() {
    SPEECH_SHUTTING_DOWN.store(true, Ordering::SeqCst);
    if let Ok(active) = active_voice_inputs().lock() {
        for path in active.iter() {
            let _ = std::fs::remove_file(path);
        }
    }

    let deadline = Instant::now() + Duration::from_secs(2);
    while ACTIVE_TRANSCRIPTION_COMMANDS.load(Ordering::SeqCst) > 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    if let Ok(active) = active_voice_inputs().lock() {
        for path in active.iter() {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// Removes microphone recordings left behind by an interrupted previous app
/// process. Call this during startup, before a new transcription can begin.
pub fn cleanup_stale_voice_inputs(app_data_dir: &Path) -> Result<usize, String> {
    let input_dir = voice_cache_dir(app_data_dir).join("input");
    let entries = match std::fs::read_dir(&input_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(error) => {
            return Err(format!(
                "Failed to inspect temporary voice input directory: {}",
                error
            ))
        }
    };

    let mut removed = 0;
    for entry in entries {
        let entry = entry
            .map_err(|error| format!("Failed to inspect temporary voice input entry: {}", error))?;
        if !entry
            .file_name()
            .to_string_lossy()
            .starts_with("voice-input-")
        {
            continue;
        }

        let file_type = entry
            .file_type()
            .map_err(|error| format!("Failed to inspect temporary voice input: {}", error))?;
        if !file_type.is_file() && !file_type.is_symlink() {
            continue;
        }

        std::fs::remove_file(entry.path())
            .map_err(|error| format!("Failed to remove temporary voice input: {}", error))?;
        removed += 1;
    }

    Ok(removed)
}

pub async fn ensure_model_downloaded(
    app_data_dir: &Path,
    kind: SpeechModelKind,
) -> Result<PathBuf, String> {
    let _guard = download_lock().lock().await;
    let final_path = model_path(app_data_dir, kind);
    if final_path.exists() {
        return Ok(final_path);
    }

    let model_dir = model_dir(app_data_dir, kind);
    std::fs::create_dir_all(&model_dir).map_err(|e| e.to_string())?;
    let partial_path = partial_model_path(app_data_dir, kind);
    let definition = definition(kind);

    tracing::info!(
        "Downloading speech model {} to {:?}",
        definition.id,
        final_path
    );

    download_with_resume(
        definition.download_url,
        &partial_path,
        &final_path,
        definition.id,
    )
    .await?;

    Ok(final_path)
}

async fn download_with_resume(
    url: &str,
    partial_path: &Path,
    final_path: &Path,
    label: &str,
) -> Result<(), String> {
    use futures_util::StreamExt;
    use tokio::io::AsyncWriteExt;

    let client = reqwest::Client::builder()
        .user_agent("FNDR/1.0")
        .connect_timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("Failed to create HTTP client for {}: {}", label, e))?;

    let resume_from = partial_path.metadata().map(|meta| meta.len()).unwrap_or(0);
    let mut request = client.get(url);
    if resume_from > 0 {
        request = request.header("Range", format!("bytes={}-", resume_from));
    }

    if let Ok(parsed_url) = url.parse::<reqwest::Url>() {
        if let Some(host) = parsed_url.host_str() {
            crate::privacy_proof::record_egress(host);
        }
    }
    let response = request
        .send()
        .await
        .map_err(|e| format!("Failed downloading {}: {}", label, e))?;
    let status_code = response.status().as_u16();
    if !response.status().is_success() && status_code != 206 {
        let body_preview = response.text().await.unwrap_or_default().replace('\n', " ");
        return Err(format!(
            "Failed downloading {}: {} {}",
            label,
            status_code,
            body_preview.chars().take(200).collect::<String>()
        ));
    }

    let resume_from = if resume_from > 0 && status_code == 200 {
        let _ = tokio::fs::remove_file(partial_path).await;
        0
    } else {
        resume_from
    };

    let raw_file = tokio::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(resume_from > 0)
        .truncate(resume_from == 0)
        .open(partial_path)
        .await
        .map_err(|e| format!("Failed opening partial file for {}: {}", label, e))?;
    let mut file = tokio::io::BufWriter::new(raw_file);

    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("Download stream failed for {}: {}", label, e))?;
        file.write_all(&chunk)
            .await
            .map_err(|e| format!("Failed writing {} partial: {}", label, e))?;
    }

    file.flush()
        .await
        .map_err(|e| format!("Failed flushing {} partial: {}", label, e))?;
    drop(file);

    tokio::fs::rename(partial_path, final_path)
        .await
        .map_err(|e| format!("Failed finalizing {}: {}", label, e))?;

    Ok(())
}

pub fn resolve_sidecar(script_name: &str) -> Option<PathBuf> {
    let packaged = std::env::current_exe().ok().and_then(|exe| {
        exe.parent()
            .map(|dir| dir.join("../Resources/sidecars").join(script_name))
    });
    if let Some(path) = packaged {
        if path.exists() {
            return Some(path);
        }
    }

    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("sidecars")
        .join(script_name);
    dev.exists().then_some(dev)
}

fn venv_python_path(venv_dir: &Path) -> PathBuf {
    if cfg!(target_os = "windows") {
        venv_dir.join("Scripts").join("python.exe")
    } else {
        venv_dir.join("bin").join("python3")
    }
}

fn parse_python_version(raw: &str) -> Option<(u32, u32)> {
    let version = raw
        .split_whitespace()
        .find(|part| part.chars().next().is_some_and(|ch| ch.is_ascii_digit()))?;
    let mut components = version.split('.');
    let major = components.next()?.parse().ok()?;
    let minor = components.next()?.parse().ok()?;
    Some((major, minor))
}

fn supported_python_version((major, minor): (u32, u32)) -> bool {
    major == 3 && minor <= 13
}

fn python_version(python: &Path) -> Option<(u32, u32)> {
    let output = Command::new(python).arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    parse_python_version(&stdout).or_else(|| parse_python_version(&stderr))
}

fn python_is_supported(python: &Path) -> bool {
    python_version(python)
        .map(supported_python_version)
        .unwrap_or(false)
}

fn speech_venv_is_usable(venv_dir: &Path) -> bool {
    python_is_supported(&venv_python_path(venv_dir))
}

fn speech_venv_has_pip(venv_dir: &Path) -> bool {
    Command::new(venv_python_path(venv_dir))
        .args(["-m", "pip", "--version"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

pub fn python_for_sidecar(app_data_dir: &Path) -> Option<PathBuf> {
    let venv_dir = prepare_speech_venv_dir(app_data_dir);
    if speech_venv_is_usable(&venv_dir) {
        return Some(venv_python_path(&venv_dir));
    }
    None
}

fn speech_venv_dir(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("speech").join("venv")
}

fn legacy_speech_venv_dir() -> Option<PathBuf> {
    dirs::document_dir().map(|root| root.join("FNDR Speech").join("venv"))
}

fn migrate_owned_speech_venv(legacy: &Path, target: &Path) -> Result<bool, String> {
    if target.exists() || !legacy.is_dir() {
        return Ok(false);
    }
    let parent = target
        .parent()
        .ok_or_else(|| "FNDR speech environment has no parent directory.".to_string())?;
    std::fs::create_dir_all(parent)
        .map_err(|error| format!("Failed preparing private speech storage: {}", error))?;
    std::fs::rename(legacy, target).map_err(|error| {
        format!(
            "Failed moving the legacy FNDR speech environment: {}",
            error
        )
    })?;
    Ok(true)
}

fn prepare_speech_venv_dir(app_data_dir: &Path) -> PathBuf {
    let target = speech_venv_dir(app_data_dir);
    if target.exists() {
        return target;
    }
    if let Some(legacy) = legacy_speech_venv_dir() {
        match migrate_owned_speech_venv(&legacy, &target) {
            Ok(true) => {
                tracing::info!("Moved the legacy FNDR speech environment into private app storage")
            }
            Ok(false) => {}
            Err(error) => tracing::warn!("{}", error),
        }
    }
    target
}

/// Probe for a usable Python 3 interpreter, preferring versions ≤ 3.13
/// (whisper-cpp-python has no 3.14+ wheels). Checks Homebrew paths first
/// because macOS Dock-launched apps don't inherit the user's shell PATH.
fn find_python3() -> Option<PathBuf> {
    // Ordered list of (binary name, full path to try)
    let candidates: &[&str] = &[
        "/opt/homebrew/bin/python3.13",
        "/opt/homebrew/bin/python3.12",
        "/opt/homebrew/bin/python3.11",
        "/opt/homebrew/bin/python3.10",
        "/usr/local/bin/python3.13",
        "/usr/local/bin/python3.12",
        "/usr/local/bin/python3.11",
        "/usr/local/bin/python3.10",
        "/opt/homebrew/bin/python3",
        "/usr/local/bin/python3",
        "/usr/bin/python3",
        "python3",
    ];
    for &path in candidates {
        let candidate = PathBuf::from(path);
        if python_is_supported(&candidate) {
            return Some(candidate);
        }
    }
    None
}

fn resolve_binary_from_candidates(
    explicit: Option<PathBuf>,
    path_dirs: &[PathBuf],
    fixed_candidates: &[PathBuf],
    binary_name: &str,
) -> Option<PathBuf> {
    explicit
        .into_iter()
        .chain(path_dirs.iter().map(|dir| dir.join(binary_name)))
        .chain(fixed_candidates.iter().cloned())
        .find(|candidate| binary_is_executable(candidate))
        .map(|candidate| {
            if candidate.is_absolute() {
                candidate
            } else {
                std::env::current_dir()
                    .map(|cwd| cwd.join(&candidate))
                    .unwrap_or(candidate)
            }
        })
}

fn binary_is_executable(candidate: &Path) -> bool {
    let Ok(metadata) = candidate.metadata() else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

fn resolve_ffmpeg_binary() -> Option<PathBuf> {
    let explicit = std::env::var_os("FNDR_FFMPEG_PATH").map(PathBuf::from);
    let path_dirs = std::env::var_os("PATH")
        .map(|value| std::env::split_paths(&value).collect::<Vec<_>>())
        .unwrap_or_default();
    let fixed_candidates = [
        "/opt/homebrew/bin/ffmpeg",
        "/usr/local/bin/ffmpeg",
        "/opt/local/bin/ffmpeg",
        "/usr/bin/ffmpeg",
    ]
    .map(PathBuf::from);

    resolve_binary_from_candidates(explicit, &path_dirs, &fixed_candidates, "ffmpeg")
}

async fn run_transcription_command(
    mut command: AsyncCommand,
    timeout: Duration,
    label: &str,
) -> Result<Output, String> {
    let _active = ActiveTranscriptionCommand::new();
    if SPEECH_SHUTTING_DOWN.load(Ordering::SeqCst) {
        return Err(format!("{} cancelled during shutdown", label));
    }
    command.kill_on_drop(true);
    let execution = tokio::time::timeout(timeout, command.output());
    tokio::pin!(execution);
    let result = tokio::select! {
        result = &mut execution => result,
        () = wait_for_speech_shutdown() => {
            return Err(format!("{} cancelled during shutdown", label));
        }
    };
    match result {
        Ok(Ok(output)) => Ok(output),
        Ok(Err(error)) => Err(format!("{} failed to start: {}", label, error)),
        Err(_) => Err(format!(
            "{} timed out after {} seconds",
            label,
            timeout.as_secs()
        )),
    }
}

fn audio_is_pcm_wav_candidate(audio_path: &Path) -> bool {
    audio_path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("wav"))
}

async fn convert_audio_to_pcm_wav(
    app_data_dir: &Path,
    audio_path: &Path,
    ffmpeg_path: &Path,
) -> Result<TemporaryVoiceInput, String> {
    let output_path = make_voice_input_path(app_data_dir, "wav");
    if let Some(parent) = output_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|error| format!("Failed to create voice input cache directory: {}", error))?;
    }
    let output = TemporaryVoiceInput::new(output_path);

    let mut command = AsyncCommand::new(ffmpeg_path);
    command
        .args(["-nostdin", "-y", "-i"])
        .arg(audio_path)
        .args(["-ar", "16000", "-ac", "1", "-c:a", "pcm_s16le"])
        .arg(output.path());
    let result =
        run_transcription_command(command, AUDIO_CONVERSION_TIMEOUT, "ffmpeg voice conversion")
            .await?;
    if !result.status.success() {
        let detail = String::from_utf8_lossy(&result.stderr).trim().to_string();
        return Err(if detail.is_empty() {
            format!("ffmpeg voice conversion exited with {}", result.status)
        } else {
            format!("ffmpeg voice conversion failed: {}", detail)
        });
    }

    let output_is_nonempty = std::fs::metadata(output.path())
        .map(|metadata| metadata.len() > 0)
        .unwrap_or(false);
    if !output_is_nonempty {
        return Err("ffmpeg voice conversion did not produce a PCM WAV".to_string());
    }

    Ok(output)
}

fn whisper_cli_command(
    cli_path: &Path,
    model_path: &Path,
    audio_path: &Path,
    cpu_only: bool,
) -> AsyncCommand {
    let mut command = AsyncCommand::new(cli_path);
    command
        .arg("-m")
        .arg(model_path)
        .arg("-f")
        .arg(audio_path)
        .args(["-l", "en", "--no-timestamps", "-nt"]);
    if cpu_only {
        command.arg("-ng");
    }
    command
}

fn transcript_from_successful_output(output: &Output) -> Option<String> {
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!text.is_empty()).then_some(text)
}

async fn try_whisper_cli_binary_with_candidates(
    model_path: &Path,
    audio_path: &Path,
    cli_paths: &[PathBuf],
) -> Option<String> {
    try_whisper_cli_binary_with_candidates_within(
        model_path,
        audio_path,
        cli_paths,
        TRANSCRIPTION_COMMAND_TIMEOUT,
    )
    .await
}

async fn try_whisper_cli_binary_with_candidates_within(
    model_path: &Path,
    audio_path: &Path,
    cli_paths: &[PathBuf],
    overall_timeout: Duration,
) -> Option<String> {
    let mut seen = HashSet::new();
    let unique_cli_paths = cli_paths
        .iter()
        .filter(|candidate| binary_is_executable(candidate))
        .filter_map(|candidate| candidate.canonicalize().ok())
        .filter(|candidate| seen.insert(candidate.clone()))
        .collect::<Vec<_>>();

    tokio::time::timeout(overall_timeout, async {
        for cli_path in unique_cli_paths {
            let gpu_result = run_transcription_command(
                whisper_cli_command(&cli_path, model_path, audio_path, false),
                TRANSCRIPTION_COMMAND_TIMEOUT,
                "whisper-cli",
            )
            .await;

            match gpu_result {
                Ok(output) if output.status.success() => {
                    if let Some(text) = transcript_from_successful_output(&output) {
                        return Some(text);
                    }
                }
                Ok(_) => {
                    let cpu_result = run_transcription_command(
                        whisper_cli_command(&cli_path, model_path, audio_path, true),
                        TRANSCRIPTION_COMMAND_TIMEOUT,
                        "whisper-cli CPU fallback",
                    )
                    .await;
                    if let Ok(output) = cpu_result {
                        if let Some(text) = transcript_from_successful_output(&output) {
                            return Some(text);
                        }
                    }
                }
                Err(_) => {}
            }
        }
        None
    })
    .await
    .ok()
    .flatten()
}

/// Try to run whisper via the `whisper-cli` binary (from `brew install whisper-cpp`).
/// Returns the transcript on success, or None if the binary is unavailable / fails.
async fn try_whisper_cli_binary(model_path: &Path, audio_path: &Path) -> Option<String> {
    let path_dirs = std::env::var_os("PATH")
        .map(|value| std::env::split_paths(&value).collect::<Vec<_>>())
        .unwrap_or_default();
    let mut cli_paths = path_dirs
        .iter()
        .map(|directory| directory.join("whisper-cli"))
        .collect::<Vec<_>>();
    cli_paths.extend([
        PathBuf::from("/opt/homebrew/bin/whisper-cli"),
        PathBuf::from("/usr/local/bin/whisper-cli"),
    ]);
    try_whisper_cli_binary_with_candidates(model_path, audio_path, &cli_paths).await
}

fn python_imports_ok(python: &Path, imports: &str) -> bool {
    Command::new(python)
        .args(["-c", imports])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn whisper_imports_ok(python: &Path) -> bool {
    python_imports_ok(python, "import whisper_cpp_python")
}

fn orpheus_imports_ok(python: &Path) -> bool {
    python_imports_ok(
        python,
        "import llama_cpp, numpy, huggingface_hub, onnxruntime",
    )
}

fn llama_cpp_extra_index() -> &'static str {
    if cfg!(target_os = "macos") && cfg!(target_arch = "aarch64") {
        "https://abetlen.github.io/llama-cpp-python/whl/metal"
    } else {
        "https://abetlen.github.io/llama-cpp-python/whl/cpu"
    }
}

fn ensure_venv_ready(app_data_dir: &Path) -> Result<PathBuf, String> {
    let venv_dir = prepare_speech_venv_dir(app_data_dir);

    if venv_dir.exists() && !speech_venv_is_usable(&venv_dir) {
        tracing::warn!(
            "Recreating unusable FNDR-owned speech environment at {:?}",
            venv_dir
        );
        std::fs::remove_dir_all(&venv_dir)
            .map_err(|error| format!("Failed removing unusable FNDR Speech venv: {}", error))?;
    }

    if !venv_dir.exists() {
        let python3 = find_python3().ok_or_else(|| {
            "python3 (≤3.13) is required for speech features. Install it with: brew install python@3.13".to_string()
        })?;
        if let Some(parent) = venv_dir.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let status = Command::new(&python3)
            .args(["-m", "venv", &venv_dir.to_string_lossy()])
            .status()
            .map_err(|e| format!("Failed creating FNDR Speech venv: {}", e))?;
        if !status.success() {
            return Err("Failed creating FNDR Speech venv".to_string());
        }
    }

    let python = venv_python_path(&venv_dir);
    if !speech_venv_is_usable(&venv_dir) {
        return Err(format!(
            "FNDR Speech venv is missing a usable Python ≤3.13 at {:?}",
            python
        ));
    }
    if !speech_venv_has_pip(&venv_dir) {
        let ensure_pip = Command::new(&python)
            .args(["-m", "ensurepip", "--upgrade"])
            .status()
            .map_err(|e| format!("Failed restoring speech pip: {}", e))?;
        if !ensure_pip.success() || !speech_venv_has_pip(&venv_dir) {
            return Err("FNDR Speech needs pip to install its local backend.".to_string());
        }
    }

    let upgrade = Command::new(&python)
        .args([
            "-m",
            "pip",
            "install",
            "--upgrade",
            "pip",
            "setuptools",
            "wheel",
        ])
        .status()
        .map_err(|e| format!("Failed upgrading speech pip: {}", e))?;
    if !upgrade.success() {
        return Err("Failed upgrading speech pip toolchain".to_string());
    }

    Ok(venv_dir)
}

fn ensure_whisper_backend_blocking(app_data_dir: &Path) -> Result<(), String> {
    if let Some(python) = python_for_sidecar(app_data_dir) {
        if whisper_imports_ok(&python) {
            return Ok(());
        }
    }

    let venv_dir = ensure_venv_ready(app_data_dir)?;
    let python = venv_python_path(&venv_dir);

    let whisper = Command::new(&python)
        .env(
            "CMAKE_ARGS",
            "-DCMAKE_POLICY_VERSION_MINIMUM=3.5 -DWHISPER_METAL=1",
        )
        .args(["-m", "pip", "install", "whisper-cpp-python"])
        .status()
        .map_err(|e| format!("Failed installing whisper-cpp-python: {}", e))?;
    if !whisper.success() {
        return Err("Failed installing whisper-cpp-python".to_string());
    }

    // Workaround: whisper-cpp-python on MacOS expects .so but often builds .dylib
    let patch_script = "
import sys, os
site_packages = [p for p in sys.path if 'site-packages' in p]
if site_packages:
    dylib = os.path.join(site_packages[0], 'whisper_cpp_python', 'libwhisper.dylib')
    so = os.path.join(site_packages[0], 'whisper_cpp_python', 'libwhisper.so')
    if os.path.exists(dylib) and not os.path.exists(so):
        import shutil
        shutil.copy(dylib, so)
";
    let _ = Command::new(&python).args(["-c", patch_script]).status();

    if !whisper_imports_ok(&python) {
        return Err("Whisper backend is still unavailable after install".to_string());
    }

    Ok(())
}

fn ensure_orpheus_backend_blocking(app_data_dir: &Path) -> Result<(), String> {
    if let Some(python) = python_for_sidecar(app_data_dir) {
        if orpheus_imports_ok(&python) {
            return Ok(());
        }
    }

    let venv_dir = ensure_venv_ready(app_data_dir)?;
    let python = venv_python_path(&venv_dir);

    let llama = Command::new(&python)
        .args([
            "-m",
            "pip",
            "install",
            "llama-cpp-python",
            "--extra-index-url",
            llama_cpp_extra_index(),
        ])
        .status()
        .map_err(|e| format!("Failed installing llama-cpp-python: {}", e))?;
    if !llama.success() {
        return Err("Failed installing llama-cpp-python".to_string());
    }

    let deps = Command::new(&python)
        .args([
            "-m",
            "pip",
            "install",
            "huggingface_hub",
            "numpy",
            "onnxruntime",
        ])
        .status()
        .map_err(|e| format!("Failed installing Orpheus dependencies: {}", e))?;
    if !deps.success() {
        return Err("Failed installing Orpheus dependencies".to_string());
    }

    if !orpheus_imports_ok(&python) {
        return Err("Orpheus backend dependencies are still unavailable after install".to_string());
    }

    Ok(())
}

pub async fn ensure_whisper_backend(app_data_dir: &Path) -> Result<(), String> {
    let _guard = bootstrap_lock().lock().await;
    let app_data_dir = app_data_dir.to_path_buf();
    tokio::task::spawn_blocking(move || ensure_whisper_backend_blocking(&app_data_dir))
        .await
        .map_err(|e| e.to_string())?
}

pub async fn ensure_orpheus_backend(app_data_dir: &Path) -> Result<(), String> {
    let _guard = bootstrap_lock().lock().await;
    let app_data_dir = app_data_dir.to_path_buf();
    tokio::task::spawn_blocking(move || ensure_orpheus_backend_blocking(&app_data_dir))
        .await
        .map_err(|e| e.to_string())?
}

fn extension_from_mime(mime_type: Option<&str>) -> &'static str {
    let mime = mime_type.unwrap_or_default().to_ascii_lowercase();
    if mime.contains("wav") {
        "wav"
    } else if mime.contains("webm") {
        "webm"
    } else if mime.contains("ogg") {
        "ogg"
    } else if mime.contains("mp4") || mime.contains("m4a") || mime.contains("aac") {
        "m4a"
    } else if mime.contains("mpeg") || mime.contains("mp3") {
        "mp3"
    } else {
        "wav"
    }
}

fn normalize_transcript_text(raw: &str) -> String {
    let mut cleaned_tokens = Vec::new();

    for token in raw.split_whitespace() {
        let trimmed = token.trim();
        if trimmed.is_empty() {
            continue;
        }

        let marker = trimmed
            .trim_matches(|ch: char| {
                matches!(
                    ch,
                    '[' | ']' | '(' | ')' | '{' | '}' | '<' | '>' | '"' | '\''
                )
            })
            .to_ascii_lowercase();

        if matches!(
            marker.as_str(),
            "music"
                | "applause"
                | "laughing"
                | "laughter"
                | "noise"
                | "silence"
                | "inaudible"
                | "background"
                | "ambient"
        ) {
            continue;
        }

        cleaned_tokens.push(trimmed.to_string());
    }

    cleaned_tokens.join(" ").trim().to_string()
}

pub async fn transcribe_audio_bytes(
    app_data_dir: &Path,
    audio_bytes: &[u8],
    mime_type: Option<&str>,
) -> Result<String, String> {
    if audio_bytes.is_empty() {
        return Err("Cannot transcribe empty audio input".to_string());
    }
    if SPEECH_SHUTTING_DOWN.load(Ordering::SeqCst) {
        return Err("Voice transcription is shutting down.".to_string());
    }

    let extension = extension_from_mime(mime_type);
    let input_path = make_voice_input_path(app_data_dir, extension);
    if let Some(parent) = input_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("Failed to create voice input cache directory: {}", e))?;
    }

    let input = TemporaryVoiceInput::new(input_path);
    // Voice inputs are short. Keeping this write synchronous ensures no detached
    // filesystem task can recreate the path after cancellation drops the guard.
    std::fs::write(input.path(), audio_bytes)
        .map_err(|e| format!("Failed to persist voice input: {}", e))?;
    if SPEECH_SHUTTING_DOWN.load(Ordering::SeqCst) {
        return Err("Voice transcription is shutting down.".to_string());
    }

    transcribe_audio_file_with_hint(app_data_dir, input.path(), TranscriptionHint::VoiceCommand)
        .await
}

pub async fn transcribe_audio_file(
    app_data_dir: &Path,
    audio_path: &Path,
) -> Result<String, String> {
    transcribe_audio_file_with_hint(app_data_dir, audio_path, TranscriptionHint::Default).await
}

pub async fn transcribe_audio_file_voice_command(
    app_data_dir: &Path,
    audio_path: &Path,
) -> Result<String, String> {
    transcribe_audio_file_with_hint(app_data_dir, audio_path, TranscriptionHint::VoiceCommand).await
}

async fn transcribe_audio_file_with_hint(
    app_data_dir: &Path,
    audio_path: &Path,
    hint: TranscriptionHint,
) -> Result<String, String> {
    let started = Instant::now();
    tracing::info!(hint = ?hint, "speech:transcribe_started");

    let model_path = match ensure_model_downloaded(app_data_dir, SpeechModelKind::WhisperBaseEn).await {
        Ok(path) => {
            tracing::info!(elapsed_ms = started.elapsed().as_millis() as u64, "speech:model_ready");
            path
        }
        Err(err) => {
            tracing::warn!(elapsed_ms = started.elapsed().as_millis() as u64, error = %err, "speech:model_download_failed");
            return Err(err);
        }
    };
    let cli_started = Instant::now();
    let ffmpeg_path = resolve_ffmpeg_binary();

    // Fast path: use the whisper-cli binary (brew install whisper-cpp) — no Python needed.
    let cli_transcript = if audio_is_pcm_wav_candidate(audio_path) {
        try_whisper_cli_binary(&model_path, audio_path).await
    } else if let Some(ffmpeg_path) = ffmpeg_path.as_deref() {
        match convert_audio_to_pcm_wav(app_data_dir, audio_path, ffmpeg_path).await {
            Ok(normalized_audio) => {
                try_whisper_cli_binary(&model_path, normalized_audio.path()).await
            }
            Err(error) => {
                if SPEECH_SHUTTING_DOWN.load(Ordering::SeqCst) {
                    return Err(error);
                }
                tracing::warn!("Could not normalize voice input for whisper-cli: {}", error);
                None
            }
        }
    } else {
        tracing::warn!("Could not find ffmpeg to normalize voice input for whisper-cli");
        None
    };
    if SPEECH_SHUTTING_DOWN.load(Ordering::SeqCst) {
        return Err("Voice transcription is shutting down.".to_string());
    }
    if let Some(text) = cli_transcript {
        let cleaned = normalize_transcript_text(&text);
        if !cleaned.is_empty() {
            tracing::info!(
                elapsed_ms = started.elapsed().as_millis() as u64,
                backend = "whisper-cli",
                "speech:transcribe_finished"
            );
            return Ok(cleaned);
        }
    }
    tracing::info!(
        elapsed_ms = cli_started.elapsed().as_millis() as u64,
        "speech:whisper_cli_unavailable_or_empty_falling_back_to_python"
    );

    if let Err(err) = ensure_whisper_backend(app_data_dir).await {
        tracing::warn!(elapsed_ms = started.elapsed().as_millis() as u64, error = %err, "speech:python_backend_setup_failed");
        return Err(err);
    }

    if let Ok(custom_cmd) = std::env::var("FNDR_WHISPER_GGUF_COMMAND") {
        let mut command = AsyncCommand::new("sh");
        command
            .arg("-c")
            .arg(custom_cmd)
            .env("FNDR_AUDIO_PATH", audio_path)
            .env("FNDR_WHISPER_MODEL_PATH", &model_path)
            .env("FNDR_TRANSCRIBE_HINT", hint.env_value());
        if let Some(ffmpeg_path) = ffmpeg_path.as_deref() {
            command.env("FNDR_FFMPEG_PATH", ffmpeg_path);
        }
        let output = run_transcription_command(
            command,
            TRANSCRIPTION_COMMAND_TIMEOUT,
            "FNDR_WHISPER_GGUF_COMMAND",
        )
        .await?;

        if output.status.success() {
            let text = normalize_transcript_text(&String::from_utf8_lossy(&output.stdout));
            if !text.is_empty() {
                tracing::info!(
                    elapsed_ms = started.elapsed().as_millis() as u64,
                    backend = "custom_command",
                    "speech:transcribe_finished"
                );
                return Ok(text);
            }
        }
        let err = String::from_utf8_lossy(&output.stderr).trim().to_string();
        tracing::warn!(elapsed_ms = started.elapsed().as_millis() as u64, error = %err, "speech:transcribe_failed");
        return Err(err);
    }

    let sidecar = resolve_sidecar("whisper_gguf_runner.py")
        .ok_or_else(|| "Could not locate whisper_gguf_runner.py".to_string())?;
    let python = python_for_sidecar(app_data_dir)
        .or_else(find_python3)
        .ok_or_else(|| "No usable python3 found for Whisper transcription. Install with: brew install python@3.13".to_string())?;
    let mut command = AsyncCommand::new(python);
    command.arg(sidecar).arg(&model_path).arg(audio_path);
    if let Some(ffmpeg_path) = ffmpeg_path.as_deref() {
        command.env("FNDR_FFMPEG_PATH", ffmpeg_path);
    }
    if let Some(flag) = hint.sidecar_flag() {
        command.arg(flag);
    }
    let output = run_transcription_command(
        command,
        TRANSCRIPTION_COMMAND_TIMEOUT,
        "Whisper GGUF runner",
    )
    .await?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr).trim().to_string();
        tracing::warn!(elapsed_ms = started.elapsed().as_millis() as u64, error = %err, "speech:transcribe_failed");
        return Err(err);
    }

    let text = normalize_transcript_text(&String::from_utf8_lossy(&output.stdout));
    if text.is_empty() {
        tracing::warn!(elapsed_ms = started.elapsed().as_millis() as u64, "speech:transcribe_empty_result");
        return Err("Whisper GGUF runner returned empty transcript".to_string());
    }
    tracing::info!(
        elapsed_ms = started.elapsed().as_millis() as u64,
        backend = "whisper_gguf_sidecar",
        "speech:transcribe_finished"
    );
    Ok(text)
}

pub async fn synthesize_speech(
    app_data_dir: &Path,
    text: &str,
    voice_id: Option<&str>,
) -> Result<PathBuf, String> {
    if text.trim().is_empty() {
        return Err("Cannot synthesize empty text".to_string());
    }

    let model_path = ensure_model_downloaded(app_data_dir, SpeechModelKind::Orpheus3B).await?;
    ensure_orpheus_backend(app_data_dir).await?;

    let output_path = make_tts_output_path(app_data_dir);
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let voice = voice_id.unwrap_or("tara").to_string();

    if let Ok(custom_cmd) = std::env::var("FNDR_ORPHEUS_COMMAND") {
        let model = model_path.clone();
        let output = output_path.clone();
        let text = text.to_string();
        let voice_for_cmd = voice.clone();
        let custom_output = tokio::task::spawn_blocking(move || {
            Command::new("sh")
                .arg("-c")
                .arg(custom_cmd)
                .env("FNDR_TTS_MODEL_PATH", model.to_string_lossy().to_string())
                .env("FNDR_TTS_OUTPUT_PATH", output.to_string_lossy().to_string())
                .env("FNDR_TTS_TEXT", text)
                .env("FNDR_TTS_VOICE", voice_for_cmd)
                .output()
        })
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("FNDR_ORPHEUS_COMMAND failed to start: {}", e))?;

        if !custom_output.status.success() {
            return Err(String::from_utf8_lossy(&custom_output.stderr)
                .trim()
                .to_string());
        }
        return Ok(output_path);
    }

    let sidecar = resolve_sidecar("orpheus_tts_runner.py")
        .ok_or_else(|| "Could not locate orpheus_tts_runner.py".to_string())?;
    let python = python_for_sidecar(app_data_dir)
        .or_else(find_python3)
        .ok_or_else(|| {
            "No usable python3 found for Orpheus TTS. Install with: brew install python@3.13"
                .to_string()
        })?;
    let model = model_path.clone();
    let output = output_path.clone();
    let text = text.to_string();

    let runner_output = tokio::task::spawn_blocking(move || {
        Command::new(python)
            .arg(sidecar)
            .arg(model.to_string_lossy().to_string())
            .arg(output.to_string_lossy().to_string())
            .arg(voice)
            .arg(text)
            .output()
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| format!("Failed launching Orpheus TTS runner: {}", e))?;

    if !runner_output.status.success() {
        return Err(String::from_utf8_lossy(&runner_output.stderr)
            .trim()
            .to_string());
    }
    if !output_path.exists() {
        return Err("Orpheus TTS runner did not produce an output WAV".to_string());
    }

    Ok(output_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn development_build_resolves_bundled_whisper_sidecar() {
        let sidecar = resolve_sidecar("whisper_gguf_runner.py")
            .expect("the checked-in Whisper sidecar should be resolvable in development");

        assert!(sidecar.is_file());
        assert!(sidecar.ends_with("sidecars/whisper_gguf_runner.py"));
    }

    #[test]
    fn python_version_filter_rejects_python_3_14() {
        assert_eq!(parse_python_version("Python 3.13.7\n"), Some((3, 13)));
        assert!(supported_python_version((3, 13)));
        assert!(!supported_python_version((3, 14)));
        assert!(!supported_python_version((2, 7)));
    }

    #[test]
    fn compressed_browser_audio_requires_pcm_normalization() {
        assert!(audio_is_pcm_wav_candidate(Path::new("voice.WAV")));
        assert!(!audio_is_pcm_wav_candidate(Path::new("voice.webm")));
        assert!(!audio_is_pcm_wav_candidate(Path::new("voice.m4a")));
        assert!(!audio_is_pcm_wav_candidate(Path::new("voice.ogg")));
    }

    #[cfg(unix)]
    fn make_executable(path: &Path) {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = std::fs::metadata(path)
            .expect("executable metadata")
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(path, permissions).expect("mark test command executable");
    }

    #[cfg(unix)]
    #[test]
    fn binary_resolver_finds_fixed_candidate_without_a_shell_path() {
        let temp = tempfile::tempdir().expect("temporary binary directory");
        let ffmpeg = temp.path().join("ffmpeg");
        std::fs::write(&ffmpeg, b"#!/bin/sh\nexit 0\n").expect("write fake ffmpeg");
        make_executable(&ffmpeg);

        let resolved =
            resolve_binary_from_candidates(None, &[], std::slice::from_ref(&ffmpeg), "ffmpeg");

        assert_eq!(resolved.as_deref(), Some(ffmpeg.as_path()));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn compressed_audio_is_normalized_to_owned_pcm_wav() {
        let app_data = tempfile::tempdir().expect("temporary app data directory");
        let ffmpeg = app_data.path().join("fake-ffmpeg");
        std::fs::write(&ffmpeg, b"#!/bin/sh\ncp \"$4\" \"${11}\"\n").expect("write fake ffmpeg");
        make_executable(&ffmpeg);
        let compressed = app_data.path().join("recording.webm");
        std::fs::write(&compressed, b"compressed microphone input")
            .expect("write compressed input");

        let normalized = convert_audio_to_pcm_wav(app_data.path(), &compressed, &ffmpeg)
            .await
            .expect("normalize input");
        let normalized_path = normalized.path().to_path_buf();

        assert_eq!(
            normalized_path.extension().and_then(|value| value.to_str()),
            Some("wav")
        );
        assert_eq!(
            std::fs::read(&normalized_path).expect("read normalized input"),
            b"compressed microphone input"
        );
        drop(normalized);
        assert!(
            !normalized_path.exists(),
            "normalized microphone input should be transient"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn whisper_cli_retries_on_cpu_after_gpu_failure() {
        let temp = tempfile::tempdir().expect("temporary whisper-cli directory");
        let cli = temp.path().join("whisper-cli");
        std::fs::write(
            &cli,
            b"#!/bin/sh\nfor arg in \"$@\"; do\n  if [ \"$arg\" = \"-ng\" ]; then\n    printf 'cpu transcript\\n'\n    exit 0\n  fi\ndone\nexit 23\n",
        )
        .expect("write fake whisper-cli");
        make_executable(&cli);

        let transcript = try_whisper_cli_binary_with_candidates(
            Path::new("model.bin"),
            Path::new("voice.wav"),
            &[cli],
        )
        .await;

        assert_eq!(transcript.as_deref(), Some("cpu transcript"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn whisper_cli_deduplicates_aliases_to_the_same_executable() {
        let temp = tempfile::tempdir().expect("temporary whisper-cli directory");
        let cli = temp.path().join("whisper-cli");
        let alias = temp.path().join("whisper-cli-alias");
        let attempts = temp.path().join("attempts");
        std::fs::write(
            &cli,
            format!("#!/bin/sh\nprintf x >> '{}'\nexit 23\n", attempts.display()),
        )
        .expect("write fake whisper-cli");
        make_executable(&cli);
        std::os::unix::fs::symlink(&cli, &alias).expect("create whisper-cli alias");

        let transcript = try_whisper_cli_binary_with_candidates(
            Path::new("model.bin"),
            Path::new("voice.wav"),
            &[cli, alias],
        )
        .await;

        assert_eq!(transcript, None);
        assert_eq!(
            std::fs::read(&attempts).expect("read attempt count"),
            b"xx",
            "one executable should receive one GPU attempt and one CPU fallback"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn whisper_cli_candidates_share_one_overall_timeout() {
        let temp = tempfile::tempdir().expect("temporary whisper-cli directory");
        let cli = temp.path().join("whisper-cli");
        let finished = temp.path().join("finished");
        std::fs::write(
            &cli,
            format!(
                "#!/bin/sh\nsleep 1\nprintf done > '{}'\n",
                finished.display()
            ),
        )
        .expect("write slow whisper-cli");
        make_executable(&cli);

        let transcript = try_whisper_cli_binary_with_candidates_within(
            Path::new("model.bin"),
            Path::new("voice.wav"),
            &[cli],
            Duration::from_millis(25),
        )
        .await;

        assert_eq!(transcript, None);
        tokio::time::sleep(Duration::from_millis(1_100)).await;
        assert!(!finished.exists(), "timed-out whisper-cli kept running");
    }

    #[cfg(unix)]
    #[test]
    fn owned_venv_with_broken_python_is_not_usable() {
        let temp = tempfile::tempdir().expect("temporary venv directory");
        let bin = temp.path().join("bin");
        std::fs::create_dir_all(&bin).expect("create venv bin directory");
        std::os::unix::fs::symlink(temp.path().join("removed-python"), bin.join("python3"))
            .expect("create broken venv python link");

        assert!(!speech_venv_is_usable(temp.path()));
    }

    #[cfg(unix)]
    #[test]
    fn missing_pip_does_not_make_a_working_runtime_disposable() {
        let temp = tempfile::tempdir().expect("temporary venv directory");
        let bin = temp.path().join("bin");
        std::fs::create_dir_all(&bin).expect("create venv bin directory");
        let python = bin.join("python3");
        std::fs::write(
            &python,
            b"#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then printf 'Python 3.13.7\\n'; exit 0; fi\nexit 1\n",
        )
        .expect("write fake Python runtime");
        make_executable(&python);

        assert!(speech_venv_is_usable(temp.path()));
        assert!(!speech_venv_has_pip(temp.path()));
    }

    #[test]
    fn speech_environment_stays_inside_private_app_data() {
        let app_data = tempfile::tempdir().expect("temporary app data directory");
        let venv = speech_venv_dir(app_data.path());

        assert_eq!(venv, app_data.path().join("speech").join("venv"));
        assert!(venv.starts_with(app_data.path()));
    }

    #[test]
    fn legacy_speech_environment_moves_without_overwriting_private_storage() {
        let temp = tempfile::tempdir().expect("temporary speech storage");
        let legacy = temp.path().join("Documents/FNDR Speech/venv");
        let target = temp
            .path()
            .join("Library/Application Support/FNDR/speech/venv");
        std::fs::create_dir_all(&legacy).expect("create legacy speech environment");
        std::fs::write(legacy.join("marker"), b"owned runtime").expect("write legacy marker");

        assert!(migrate_owned_speech_venv(&legacy, &target).expect("migrate legacy runtime"));
        assert!(!legacy.exists());
        assert_eq!(
            std::fs::read(target.join("marker")).expect("read migrated marker"),
            b"owned runtime"
        );

        std::fs::create_dir_all(&legacy).expect("recreate legacy directory");
        assert!(
            !migrate_owned_speech_venv(&legacy, &target).expect("existing private runtime wins")
        );
        assert!(
            legacy.exists(),
            "legacy data must not be overwritten or deleted"
        );
    }

    #[tokio::test]
    async fn cancelling_an_async_task_removes_its_temporary_voice_input() {
        let app_data = tempfile::tempdir().expect("temporary app data directory");
        let input_path = make_voice_input_path(app_data.path(), "wav");
        std::fs::create_dir_all(input_path.parent().expect("voice input parent"))
            .expect("create voice input directory");
        std::fs::write(&input_path, b"private voice input").expect("write voice input");
        let input = TemporaryVoiceInput::new(input_path.clone());

        let task = tokio::spawn(async move {
            std::future::pending::<()>().await;
            drop(input);
        });
        tokio::task::yield_now().await;
        task.abort();
        let _ = task.await;

        assert!(!input_path.exists());
    }

    #[test]
    fn startup_cleanup_removes_only_abandoned_voice_inputs() {
        let app_data = tempfile::tempdir().expect("temporary app data directory");
        let input_dir = voice_cache_dir(app_data.path()).join("input");
        std::fs::create_dir_all(&input_dir).expect("create voice input directory");
        let abandoned = input_dir.join("voice-input-crashed.webm");
        let unrelated = input_dir.join("keep-me.txt");
        std::fs::write(&abandoned, b"private voice input").expect("write abandoned input");
        std::fs::write(&unrelated, b"not managed by speech cleanup").expect("write unrelated file");

        let removed = cleanup_stale_voice_inputs(app_data.path()).expect("clean stale inputs");

        assert_eq!(removed, 1);
        assert!(!abandoned.exists());
        assert!(unrelated.exists());
    }

    #[tokio::test]
    async fn transcription_command_times_out_without_finishing_later() {
        let temp = tempfile::tempdir().expect("temporary command directory");
        let marker = temp.path().join("finished");
        let mut command = tokio::process::Command::new("/bin/sh");
        command
            .arg("-c")
            .arg("sleep 1; touch \"$FNDR_TEST_MARKER\"")
            .env("FNDR_TEST_MARKER", &marker);

        let error = run_transcription_command(
            command,
            std::time::Duration::from_millis(25),
            "test transcription",
        )
        .await
        .expect_err("command should time out");

        assert!(error.contains("timed out"));
        tokio::time::sleep(std::time::Duration::from_millis(1_100)).await;
        assert!(!marker.exists(), "timed-out command kept running");
    }

    #[tokio::test]
    async fn transcription_command_stops_when_its_async_task_is_cancelled() {
        let temp = tempfile::tempdir().expect("temporary command directory");
        let started = temp.path().join("started");
        let finished = temp.path().join("finished");
        let mut command = tokio::process::Command::new("/bin/sh");
        command
            .arg("-c")
            .arg("touch \"$FNDR_TEST_STARTED\"; sleep 1; touch \"$FNDR_TEST_FINISHED\"")
            .env("FNDR_TEST_STARTED", &started)
            .env("FNDR_TEST_FINISHED", &finished);

        let task = tokio::spawn(run_transcription_command(
            command,
            std::time::Duration::from_secs(5),
            "test transcription",
        ));
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            while !started.exists() {
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("test command should start");

        task.abort();
        let _ = task.await;
        tokio::time::sleep(std::time::Duration::from_millis(1_100)).await;
        assert!(!finished.exists(), "cancelled command kept running");
    }
}
