//! The files the on-device voice needs, fetched once with the person's
//! consent: Kokoro-82M v1.0 (8-bit ONNX export), a curated set of English
//! voice packs, and misaki's pronunciation dictionaries. Every file is pinned
//! to a commit and a SHA-256; a download resumes from its `.partial` file and
//! is checked before it is moved into place. Nothing is fetched after that.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use once_cell::sync::Lazy;
use serde::Serialize;

pub const MODEL_FOLDER: &str = "kokoro-82m-v1.0";
pub const MODEL_FILE: &str = "model_quantized.onnx";
const MARKER_FILE: &str = "install.json";

const HF: &str =
    "https://huggingface.co/onnx-community/Kokoro-82M-v1.0-ONNX/resolve/1939ad2a8e416c0acfeecc08a694d14ef25f2231";
const MISAKI: &str =
    "https://raw.githubusercontent.com/hexgrad/misaki/fba1236595f2d2bf21d414ba6e57d25256afada3/misaki/data";

#[derive(Debug, Clone, Copy)]
pub struct Artifact {
    /// Path under the model folder.
    pub path: &'static str,
    pub url_base: &'static str,
    /// Path under `url_base`.
    pub remote: &'static str,
    pub sha256: &'static str,
    pub size: u64,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceInfo {
    pub id: &'static str,
    pub name: &'static str,
    pub accent: &'static str,
    pub gender: &'static str,
    /// The model card's overall grade for the voice's training data.
    pub grade: &'static str,
}

/// Voices graded C or better on the model card (hexgrad/Kokoro-82M
/// VOICES.md), American and British English only, since the pronunciation
/// step is English.
pub const VOICES: &[VoiceInfo] = &[
    VoiceInfo {
        id: "af_heart",
        name: "Heart",
        accent: "us",
        gender: "female",
        grade: "A",
    },
    VoiceInfo {
        id: "af_bella",
        name: "Bella",
        accent: "us",
        gender: "female",
        grade: "A-",
    },
    VoiceInfo {
        id: "af_nicole",
        name: "Nicole",
        accent: "us",
        gender: "female",
        grade: "B-",
    },
    VoiceInfo {
        id: "af_aoede",
        name: "Aoede",
        accent: "us",
        gender: "female",
        grade: "C+",
    },
    VoiceInfo {
        id: "af_kore",
        name: "Kore",
        accent: "us",
        gender: "female",
        grade: "C+",
    },
    VoiceInfo {
        id: "af_sarah",
        name: "Sarah",
        accent: "us",
        gender: "female",
        grade: "C+",
    },
    VoiceInfo {
        id: "am_fenrir",
        name: "Fenrir",
        accent: "us",
        gender: "male",
        grade: "C+",
    },
    VoiceInfo {
        id: "am_michael",
        name: "Michael",
        accent: "us",
        gender: "male",
        grade: "C+",
    },
    VoiceInfo {
        id: "am_puck",
        name: "Puck",
        accent: "us",
        gender: "male",
        grade: "C+",
    },
    VoiceInfo {
        id: "bf_emma",
        name: "Emma",
        accent: "gb",
        gender: "female",
        grade: "B-",
    },
    VoiceInfo {
        id: "bf_isabella",
        name: "Isabella",
        accent: "gb",
        gender: "female",
        grade: "C",
    },
    VoiceInfo {
        id: "bm_fable",
        name: "Fable",
        accent: "gb",
        gender: "male",
        grade: "C",
    },
    VoiceInfo {
        id: "bm_george",
        name: "George",
        accent: "gb",
        gender: "male",
        grade: "C",
    },
];

pub const DEFAULT_VOICE: &str = "af_heart";

const VOICE_BYTES: u64 = 522_240;

macro_rules! voice {
    ($id:literal, $sha:literal) => {
        Artifact {
            path: concat!("voices/", $id, ".bin"),
            url_base: HF,
            remote: concat!("voices/", $id, ".bin"),
            sha256: $sha,
            size: VOICE_BYTES,
        }
    };
}

pub const ARTIFACTS: &[Artifact] = &[
    Artifact {
        path: MODEL_FILE,
        url_base: HF,
        remote: "onnx/model_quantized.onnx",
        sha256: "fbae9257e1e05ffc727e951ef9b9c98418e6d79f1c9b6b13bd59f5c9028a1478",
        size: 92_361_116,
    },
    Artifact {
        path: "us_gold.json",
        url_base: MISAKI,
        remote: "us_gold.json",
        sha256: "dc414872a49a28ae6c141463d502fd945f3b2fde040484fdc47d00cc4612686f",
        size: 3_000_469,
    },
    Artifact {
        path: "us_silver.json",
        url_base: MISAKI,
        remote: "us_silver.json",
        sha256: "de8f67be911bb6c659187b4a65fd966b6a30e56350e0f790d763210b053ac475",
        size: 3_099_517,
    },
    Artifact {
        path: "gb_gold.json",
        url_base: MISAKI,
        remote: "gb_gold.json",
        sha256: "29e62f4b60261c88f7f3c2c7811ca3825978948090b72d2b27d565b729282f71",
        size: 2_838_552,
    },
    Artifact {
        path: "gb_silver.json",
        url_base: MISAKI,
        remote: "gb_silver.json",
        sha256: "48131e2d92ccc41655f4543e87e0f938e71463eb5a54be7f0693bb712ebb6bce",
        size: 3_663_898,
    },
    voice!(
        "af_heart",
        "d583ccff3cdca2f7fae535cb998ac07e9fcb90f09737b9a41fa2734ec44a8f0b"
    ),
    voice!(
        "af_bella",
        "f69d836209b78eb8c66e75e3cda491e26ea838a3674257e9d4e5703cbaf55c8b"
    ),
    voice!(
        "af_nicole",
        "cd2191ab31b914ed7b318416b0e4440fdf392ddad9106a060819aa600a64f59a"
    ),
    voice!(
        "af_aoede",
        "4a004c33430762e2461eedb2013fad808ef4ab3121f5300f554476caf58d8361"
    ),
    voice!(
        "af_kore",
        "9be5221b6a941c04b561959b8ff0b06e809444dcc4ab7e75a7b23606f691819e"
    ),
    voice!(
        "af_sarah",
        "4409fbc125afabacc615d94db5398d847006a737b0247d6892b7a9a0007a2f0a"
    ),
    voice!(
        "am_fenrir",
        "c27989f741f7ee34d273a39d8a595cc0837d35f5ced9a29b7cc162614616df43"
    ),
    voice!(
        "am_michael",
        "1d1f21dd8da39c30705cd4c75d039d265e9bc4a2a93ed09bc9e1b1225eb95ba1"
    ),
    voice!(
        "am_puck",
        "fcf73c989033e9233e0b98713eca600c8c74dcc1614b37009d5450ff4a2274a0"
    ),
    voice!(
        "bf_emma",
        "669fe0647f9dd04fcab92f1439a40eeb4c8b4ab1f82e4996fe3d918ce4a63b73"
    ),
    voice!(
        "bf_isabella",
        "3754352c4aaa46d17f27654ab7518d65b62ad6163a0f55a5f4330c2da2c4e94f"
    ),
    voice!(
        "bm_fable",
        "f889083196807b4adb15e9204252165f503b8d33d3982e681c52443c49d798f1"
    ),
    voice!(
        "bm_george",
        "c4b235a4c1f2cd3b939fed08b899ce9385638b763f7b73a59616c4fc9bd6c9bc"
    ),
];

pub fn total_bytes() -> u64 {
    ARTIFACTS.iter().map(|a| a.size).sum()
}

pub fn model_dir(app_data_dir: &Path) -> PathBuf {
    crate::models::models_dir(app_data_dir).join(MODEL_FOLDER)
}

pub fn voice_path(dir: &Path, voice: &str) -> PathBuf {
    dir.join("voices").join(format!("{voice}.bin"))
}

pub fn voice_info(id: &str) -> Option<&'static VoiceInfo> {
    VOICES.iter().find(|v| v.id == id)
}

/// Every file is in place at its pinned size and the install finished.
pub fn is_installed(dir: &Path) -> bool {
    dir.join(MARKER_FILE).is_file()
        && ARTIFACTS.iter().all(|a| {
            std::fs::metadata(dir.join(a.path))
                .map(|m| m.len() == a.size)
                .unwrap_or(false)
        })
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Phase {
    #[default]
    Idle,
    Downloading,
    Failed(String),
}

#[derive(Debug, Clone, Default)]
pub struct Progress {
    pub phase: Phase,
    pub downloaded: u64,
}

static PROGRESS: Lazy<Mutex<Progress>> = Lazy::new(|| Mutex::new(Progress::default()));

pub fn progress() -> Progress {
    PROGRESS.lock().map(|p| p.clone()).unwrap_or_default()
}

fn set_progress(update: impl FnOnce(&mut Progress)) {
    if let Ok(mut progress) = PROGRESS.lock() {
        update(&mut progress);
    }
}

/// Marks the install as started, or says why it cannot start.
pub fn begin(consent: bool) -> Result<(), String> {
    if !consent {
        return Err(
            "The on-device voice is downloaded only after you agree to the download.".to_string(),
        );
    }
    let mut progress = PROGRESS
        .lock()
        .map_err(|_| "install state is unavailable".to_string())?;
    if progress.phase == Phase::Downloading {
        return Err("The on-device voice is already downloading.".to_string());
    }
    *progress = Progress {
        phase: Phase::Downloading,
        downloaded: 0,
    };
    Ok(())
}

fn existing_ok(path: &Path, artifact: &Artifact) -> bool {
    std::fs::metadata(path).is_ok_and(|m| m.len() == artifact.size)
        && crate::models::verify_file_sha256(path, artifact.sha256).is_ok()
}

async fn fetch(
    client: &reqwest::Client,
    dir: &Path,
    artifact: &Artifact,
    done_before: u64,
) -> Result<(), String> {
    use futures_util::StreamExt;
    use tokio::io::AsyncWriteExt;

    let dest = dir.join(artifact.path);
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
    }
    let partial = dest.with_extension(format!(
        "{}.partial",
        dest.extension().and_then(|e| e.to_str()).unwrap_or("bin")
    ));
    let url = format!("{}/{}", artifact.url_base, artifact.remote);
    let resume_from = std::fs::metadata(&partial).map(|m| m.len()).unwrap_or(0);
    let resume_from = if resume_from > artifact.size {
        let _ = std::fs::remove_file(&partial);
        0
    } else {
        resume_from
    };
    if resume_from < artifact.size {
        if let Some(host) = url
            .parse::<reqwest::Url>()
            .ok()
            .and_then(|u| u.host_str().map(str::to_string))
        {
            crate::privacy_proof::record_egress(&host);
        }
        let mut request = client.get(&url);
        if resume_from > 0 {
            request = request.header("Range", format!("bytes={resume_from}-"));
        }
        let response = request
            .send()
            .await
            .map_err(|e| format!("download {}: {e}", artifact.path))?;
        let status = response.status().as_u16();
        if !(200..300).contains(&status) {
            return Err(format!(
                "download {}: server answered {status}",
                artifact.path
            ));
        }
        let start = if resume_from > 0 && status == 206 {
            resume_from
        } else {
            0
        };
        let file = tokio::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .append(start > 0)
            .truncate(start == 0)
            .open(&partial)
            .await
            .map_err(|e| format!("open {}: {e}", partial.display()))?;
        let mut file = tokio::io::BufWriter::new(file);
        let mut written = start;
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| format!("download {}: {e}", artifact.path))?;
            written += chunk.len() as u64;
            if written > artifact.size {
                drop(file);
                let _ = tokio::fs::remove_file(&partial).await;
                return Err(format!("{} is larger than expected", artifact.path));
            }
            file.write_all(&chunk)
                .await
                .map_err(|e| format!("write {}: {e}", partial.display()))?;
            set_progress(|p| p.downloaded = done_before + written);
        }
        file.flush()
            .await
            .map_err(|e| format!("write {}: {e}", partial.display()))?;
    }
    let check = partial.clone();
    let expected = artifact.sha256;
    let verified =
        tokio::task::spawn_blocking(move || crate::models::verify_file_sha256(&check, expected))
            .await
            .map_err(|e| format!("checksum task: {e}"))?;
    if let Err(err) = verified {
        let _ = tokio::fs::remove_file(&partial).await;
        return Err(err);
    }
    tokio::fs::rename(&partial, &dest)
        .await
        .map_err(|e| format!("move {} into place: {e}", artifact.path))
}

/// Downloads every missing or damaged file, then records the consent and the
/// finished install. `begin` must have succeeded first.
pub async fn run(app_data_dir: PathBuf) -> Result<(), String> {
    let result = run_inner(&app_data_dir).await;
    set_progress(|p| {
        p.phase = match &result {
            Ok(()) => Phase::Idle,
            Err(err) => Phase::Failed(err.clone()),
        }
    });
    result
}

async fn run_inner(app_data_dir: &Path) -> Result<(), String> {
    let dir = model_dir(app_data_dir);
    std::fs::create_dir_all(dir.join("voices"))
        .map_err(|e| format!("create {}: {e}", dir.display()))?;
    let client = reqwest::Client::builder()
        .user_agent("FNDR/1.0")
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(1800))
        .build()
        .map_err(|e| format!("http client: {e}"))?;
    let mut done = 0u64;
    for artifact in ARTIFACTS {
        let dest = dir.join(artifact.path);
        let check = dest.clone();
        let artifact_copy = *artifact;
        let present = tokio::task::spawn_blocking(move || existing_ok(&check, &artifact_copy))
            .await
            .unwrap_or(false);
        if !present {
            let _ = std::fs::remove_file(&dest);
            fetch(&client, &dir, artifact, done).await?;
        }
        done += artifact.size;
        set_progress(|p| p.downloaded = done);
    }
    let marker = serde_json::json!({
        "model": "hexgrad/Kokoro-82M v1.0 (onnx-community export, 8-bit)",
        "consented_at_ms": chrono::Utc::now().timestamp_millis(),
        "files": ARTIFACTS.iter().map(|a| a.path).collect::<Vec<_>>(),
    });
    std::fs::write(dir.join(MARKER_FILE), marker.to_string())
        .map_err(|e| format!("record the install: {e}"))
}

/// Deletes the model folder, consent record included.
pub fn remove(app_data_dir: &Path) -> Result<(), String> {
    if progress().phase == Phase::Downloading {
        return Err("Wait for the download to finish before removing the voice.".to_string());
    }
    let dir = model_dir(app_data_dir);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(|e| format!("remove {}: {e}", dir.display()))?;
    }
    set_progress(|p| *p = Progress::default());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_artifact_is_pinned() {
        for artifact in ARTIFACTS {
            assert_eq!(artifact.sha256.len(), 64, "{}", artifact.path);
            assert!(artifact.sha256.chars().all(|c| c.is_ascii_hexdigit()));
            assert!(
                artifact.url_base.contains("1939ad2a8e")
                    || artifact.url_base.contains("fba1236595")
            );
            assert!(artifact.size > 0);
        }
        for voice in VOICES {
            assert!(
                ARTIFACTS
                    .iter()
                    .any(|a| a.path == format!("voices/{}.bin", voice.id)),
                "{} has no download",
                voice.id
            );
        }
        assert!(voice_info(DEFAULT_VOICE).is_some());
        assert_eq!(ARTIFACTS.len(), 5 + VOICES.len());
    }

    #[test]
    fn the_download_is_about_a_hundred_and_twelve_megabytes() {
        let mb = total_bytes() / 1_000_000;
        assert!((105..=120).contains(&mb), "{mb} MB");
    }

    #[test]
    #[serial_test::serial(tts_install)]
    fn install_needs_consent_and_one_at_a_time() {
        assert!(begin(false).is_err());
        assert!(begin(true).is_ok());
        assert!(begin(true).is_err());
        set_progress(|p| *p = Progress::default());
    }

    #[test]
    #[serial_test::serial(tts_install)]
    fn a_partial_folder_does_not_count_as_installed() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = model_dir(tmp.path());
        std::fs::create_dir_all(dir.join("voices")).unwrap();
        std::fs::write(dir.join(MODEL_FILE), b"not a model").unwrap();
        assert!(!is_installed(&dir));
        assert!(remove(tmp.path()).is_ok());
        assert!(!dir.exists());
    }
}
