//! Files search (ADR 033): settings, status, search, and acting on a found
//! file. The service is one per app; `start` runs at launch. Open, reveal,
//! copy and preview act only on files in the Files index, so a script in a
//! window cannot point them at anything else.

use crate::files::model;
use crate::files::reader::FilesReader;
use crate::files::service::{FilesSearch, FilesService, FilesStatus, Hooks, Programs};
use crate::files::settings::FilesSettings;
use crate::files::FileKind;
use crate::AppState;
use chrono::Timelike;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use tauri::{AppHandle, Emitter};

static SERVICE: OnceLock<Arc<FilesService>> = OnceLock::new();
static DOWNLOADING: AtomicBool = AtomicBool::new(false);

/// Starts Files search's worker. It stays off until the person turns it on.
pub fn start(app: AppHandle, state: Arc<AppState>) {
    let emitter = app.clone();
    let hooks = Hooks {
        hold: Box::new(move || {
            let (skip, reason) =
                crate::telemetry::system_metrics::pressure_recommends_skipping_heavy_models();
            if skip {
                return Some(format!("waiting for memory ({reason})"));
            }
            // Qwen holds this while it reads a capture: the GPU is busy, so wait.
            if state.model_pipeline_lock.try_lock().is_err() {
                return Some("waiting for FNDR's own model".into());
            }
            None
        }),
        hour: Box::new(|| chrono::Local::now().hour() as u8),
        emit: Box::new(move |status: &FilesStatus| {
            let _ = emitter.emit("files://status", status);
        }),
    };
    let app_data_dir = match crate::config::fndr_app_data_dir(tauri::Manager::path(&app)) {
        Ok(dir) => dir,
        Err(error) => {
            tracing::warn!("Files search did not start: {error}");
            return;
        }
    };
    let _ = SERVICE.set(FilesService::start(app_data_dir, Programs::locate(), hooks));
}

fn service() -> Result<&'static Arc<FilesService>, String> {
    SERVICE
        .get()
        .ok_or_else(|| "Files search is not running".to_string())
}

fn indexed(path: &str) -> Result<PathBuf, String> {
    if service()?.is_indexed(path) {
        Ok(PathBuf::from(path))
    } else {
        Err("That file is not in Files search.".into())
    }
}

fn reader() -> Result<FilesReader, String> {
    let program = service()?
        .programs()
        .reader
        .clone()
        .ok_or("The Files reader is not installed.")?;
    FilesReader::start(&program)
}

#[tauri::command]
pub async fn files_status() -> Result<FilesStatus, String> {
    Ok(service()?.status())
}

#[tauri::command]
pub async fn files_settings_get() -> Result<FilesSettings, String> {
    Ok(service()?.settings())
}

#[tauri::command]
pub async fn files_settings_set(settings: FilesSettings) -> Result<FilesStatus, String> {
    let service = service()?;
    tauri::async_runtime::spawn_blocking(move || service.set_settings(settings))
        .await
        .map_err(|error| error.to_string())??;
    Ok(service.status())
}

/// Files matching `query`. `kinds` narrows to some kinds (`pdf`, `image`, ...).
/// `words_only` answers at once while typing; the full search follows on a pause.
#[tauri::command]
pub async fn files_search(
    query: String,
    kinds: Option<Vec<String>>,
    limit: Option<usize>,
    words_only: Option<bool>,
) -> Result<FilesSearch, String> {
    let service = service()?;
    let kinds: Option<HashSet<FileKind>> = kinds.map(|kinds| {
        kinds
            .iter()
            .filter_map(|kind| FileKind::parse(kind))
            .collect()
    });
    let limit = limit.unwrap_or(20).clamp(1, 100);
    let words_only = words_only.unwrap_or(false);
    tauri::async_runtime::spawn_blocking(move || service.search(&query, kinds, limit, words_only))
        .await
        .map_err(|error| error.to_string())?
}

/// Downloads EmbeddingGemma 2 (865 MB, once), with `files://download` progress events.
#[tauri::command]
pub async fn files_model_download(app: AppHandle) -> Result<(), String> {
    let service = service()?;
    if DOWNLOADING.swap(true, Ordering::SeqCst) {
        return Err("The Files model is already downloading.".into());
    }
    let dir = model::dir(service.app_data_dir());
    tauri::async_runtime::spawn(async move {
        let total = model::total_bytes();
        let mut done_before = 0_u64;
        let mut last_file = "";
        let progress_app = app.clone();
        let result = model::download(&dir, |progress| {
            if progress.file != last_file {
                if !last_file.is_empty() {
                    done_before += model::ALL.iter().find(|file| file.name == last_file).map_or(0, |file| file.bytes);
                }
                last_file = progress.file;
            }
            let _ = progress_app.emit(
                "files://download",
                serde_json::json!({ "bytes": done_before + progress.bytes, "total": total, "done": false }),
            );
        })
        .await;
        DOWNLOADING.store(false, Ordering::SeqCst);
        let error = result.err();
        let _ = app.emit(
            "files://download",
            serde_json::json!({ "done": true, "error": error, "total": total }),
        );
        service.models_changed();
    });
    Ok(())
}

/// Opens a found file in its app, at its page or moment.
#[tauri::command]
pub async fn files_open(
    path: String,
    page: Option<usize>,
    seconds: Option<f64>,
    words: Option<Vec<String>>,
) -> Result<String, String> {
    let path = indexed(&path)?;
    tauri::async_runtime::spawn_blocking(move || {
        reader()?.open(&path, page, seconds, &words.unwrap_or_default())
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Opens code at its line in the editor chosen in Settings.
#[tauri::command]
pub async fn files_open_code(path: String, line: usize) -> Result<String, String> {
    let path = indexed(&path)?;
    let editor = service()?.settings().code_editor;
    tauri::async_runtime::spawn_blocking(move || {
        reader()?.open_code(&path, line.max(1), editor.as_deref())
    })
    .await
    .map_err(|error| error.to_string())?
}

/// The code editors installed here, for Settings.
#[tauri::command]
pub async fn files_editors() -> Result<Vec<crate::files::reader::Editor>, String> {
    tauri::async_runtime::spawn_blocking(|| reader()?.editors())
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn files_reveal(path: String) -> Result<(), String> {
    let path = indexed(&path)?;
    std::process::Command::new("/usr/bin/open")
        .arg("-R")
        .arg(&path)
        .status()
        .map_err(|error| error.to_string())
        .and_then(|status| {
            if status.success() {
                Ok(())
            } else {
                Err("Finder could not show it.".into())
            }
        })
}

#[tauri::command]
pub async fn files_copy(path: String) -> Result<(), String> {
    let path = indexed(&path)?;
    tauri::async_runtime::spawn_blocking(move || reader()?.copy(&path))
        .await
        .map_err(|error| error.to_string())?
}

/// Thumbnails Quick Look draws, as data URLs by path, from one reader and a
/// small cache. A file it can't draw is left out; the row shows its kind.
#[tauri::command]
pub async fn files_thumbnails(
    paths: Vec<String>,
    pixels: Option<u32>,
) -> Result<std::collections::HashMap<String, String>, String> {
    use base64::Engine;
    use std::collections::HashMap;
    use std::sync::Mutex;
    static CACHE: OnceLock<Mutex<HashMap<(PathBuf, u32), String>>> = OnceLock::new();
    const KEEP: usize = 400;
    let pixels = pixels.unwrap_or(144).clamp(32, 512);

    let paths: Vec<PathBuf> = paths
        .iter()
        .take(48)
        .filter_map(|path| indexed(path).ok())
        .collect();
    tauri::async_runtime::spawn_blocking(move || {
        let cache = CACHE.get_or_init(Default::default);
        let mut found = HashMap::new();
        let mut missing = Vec::new();
        {
            let cache = cache.lock().map_err(|_| "thumbnail cache poisoned")?;
            for path in paths {
                match cache.get(&(path.clone(), pixels)) {
                    Some(url) => {
                        found.insert(path.to_string_lossy().into_owned(), url.clone());
                    }
                    None => missing.push(path),
                }
            }
        }
        if missing.is_empty() {
            return Ok(found);
        }
        let mut reader = reader()?;
        let mut drawn = Vec::new();
        for path in missing {
            if let Ok(png) = reader.thumbnail(&path, pixels) {
                let url = format!(
                    "data:image/png;base64,{}",
                    base64::engine::general_purpose::STANDARD.encode(png)
                );
                found.insert(path.to_string_lossy().into_owned(), url.clone());
                drawn.push(((path, pixels), url));
            }
        }
        let mut cache = cache.lock().map_err(|_| "thumbnail cache poisoned")?;
        if cache.len() + drawn.len() > KEEP {
            cache.clear();
        }
        cache.extend(drawn);
        Ok(found)
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Quick Find hands its search to Search and Ask: Quick Find hides, the main
/// window comes forward and gets `files://open-search`.
#[tauri::command]
pub async fn files_open_in_window(
    app: AppHandle,
    query: String,
    kinds: Option<Vec<String>>,
) -> Result<(), String> {
    use tauri::Manager;
    if let Some(omnibar) = app.get_webview_window(super::omnibar::OMNIBAR_LABEL) {
        let _ = omnibar.hide();
    }
    let main = app
        .get_webview_window("main")
        .ok_or("FNDR's window is not open.")?;
    let _ = main.show();
    let _ = main.set_focus();
    app.emit_to(
        "main",
        "files://open-search",
        serde_json::json!({ "query": query, "kinds": kinds }),
    )
    .map_err(|error| error.to_string())
}

/// Quick Look for a found file, or closes the one showing.
#[tauri::command]
pub async fn files_quick_look(path: Option<String>) -> Result<bool, String> {
    use std::sync::Mutex;
    static SHOWING: Mutex<Option<std::process::Child>> = Mutex::new(None);
    let path = path.map(|path| indexed(&path)).transpose()?;
    let mut showing = SHOWING.lock().map_err(|_| "Quick Look state poisoned")?;
    if let Some(mut child) = showing.take() {
        let _ = child.kill();
        let _ = child.wait();
        if path.is_none() {
            return Ok(false);
        }
    }
    let Some(path) = path else {
        return Ok(false);
    };
    let child = std::process::Command::new("/usr/bin/qlmanage")
        .arg("-p")
        .arg(&path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|error| format!("Quick Look did not start: {error}"))?;
    *showing = Some(child);
    Ok(true)
}

/// A preview as a data URL: the picture, or the matched page or frame.
#[tauri::command]
pub async fn files_preview(
    path: String,
    kind: String,
    page: Option<usize>,
    seconds: Option<f64>,
) -> Result<String, String> {
    use base64::Engine;
    let path = indexed(&path)?;
    let scratch = service()?.app_data_dir().join("files").join("previews");
    tauri::async_runtime::spawn_blocking(move || {
        let _ = std::fs::remove_dir_all(&scratch);
        let mut reader = reader()?;
        let image: PathBuf = match kind.as_str() {
            "pdf" => reader.preview(&path, Some(page.unwrap_or(1)), None, &scratch)?,
            "video" => reader.preview(&path, None, Some(seconds.unwrap_or(0.0)), &scratch)?,
            "image" | "screenshot" => {
                use crate::files::reader::Reader;
                reader
                    .picture(&path, false, 1, &scratch)?
                    .path
                    .ok_or("This picture cannot be shown.")?
            }
            _ => return Err("No picture preview for this kind.".into()),
        };
        let bytes = std::fs::read(&image).map_err(|error| error.to_string())?;
        let _ = std::fs::remove_dir_all(&scratch);
        let mime = if image.extension().and_then(|ext| ext.to_str()) == Some("png") {
            "image/png"
        } else {
            "image/jpeg"
        };
        Ok(format!(
            "data:{mime};base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        ))
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Turns Files search off and deletes its index. The model files stay unless `model` is true.
#[tauri::command]
pub async fn files_forget(model: bool) -> Result<FilesStatus, String> {
    let service = service()?;
    let mut settings = service.settings();
    settings.enabled = false;
    service.set_settings(settings)?;
    service.forget()?;
    if model {
        let dir = crate::files::model::dir(service.app_data_dir());
        if dir.exists() {
            std::fs::remove_dir_all(&dir).map_err(|error| error.to_string())?;
        }
    }
    Ok(service.status())
}

/// The usual folders that exist here, for setup to offer: Documents, Desktop,
/// Downloads, Pictures, Movies and Music.
#[tauri::command]
pub async fn files_suggested_folders() -> Result<Vec<String>, String> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or("No home folder")?;
    Ok([
        "Documents",
        "Desktop",
        "Downloads",
        "Pictures",
        "Movies",
        "Music",
    ]
    .iter()
    .map(|name| home.join(name))
    .filter(|folder| folder.is_dir())
    .map(|folder| folder.to_string_lossy().to_string())
    .collect())
}

/// A folder the person picks, or nothing when they cancel.
#[tauri::command]
pub async fn files_pick_folder() -> Result<Option<String>, String> {
    let folder = tauri::async_runtime::spawn_blocking(|| {
        rfd::FileDialog::new()
            .set_title("Choose a folder for Files search")
            .pick_folder()
    })
    .await
    .map_err(|error| error.to_string())?;
    Ok(folder.map(|folder| folder.to_string_lossy().to_string()))
}

/// How many files a folder holds of each kind, by metadata only: setup shows
/// what a first pass involves before anything is read.
#[tauri::command]
pub async fn files_estimate(
    folders: Vec<String>,
) -> Result<crate::files::estimate::Estimate, String> {
    let settings = service()?.settings();
    tauri::async_runtime::spawn_blocking(move || {
        let roots: Vec<PathBuf> = folders.iter().map(PathBuf::from).collect();
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default();
        Ok(crate::files::estimate::estimate(
            &roots,
            &settings.index_options(),
            Path::new(&home),
        ))
    })
    .await
    .map_err(|error| error.to_string())?
}
