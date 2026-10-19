//! EmbeddingGemma 2's files: ggml-org's Q8_0 GGUF conversion at one pinned
//! revision (made from google/embeddinggemma-2, Apache 2.0), the files DigUp
//! ships. Never F16: the model's activations overflow float16. Downloaded
//! only after the person turns Files search on, and checked by SHA-256 before
//! they get their final name. Kept apart from `models::MODEL_CATALOG`, whose
//! entries are offered as FNDR's own chat and capture models.

use std::path::{Path, PathBuf};

pub const REPOSITORY: &str = "ggml-org/embeddinggemma-2-GGUF";
pub const REVISION: &str = "bfcd298762cc34d0357ece5ebdd31791a3a374d8";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelFile {
    pub name: &'static str,
    pub bytes: u64,
    pub sha256: &'static str,
}

impl ModelFile {
    pub fn url(&self) -> String {
        format!(
            "https://huggingface.co/{REPOSITORY}/resolve/{REVISION}/{}",
            self.name
        )
    }
}

/// The text model (310 MB): all that queries need.
pub const TEXT: ModelFile = ModelFile {
    name: "embeddinggemma-2-Q8_0.gguf",
    bytes: 309_855_456,
    sha256: "2188ac1deca4b77dffefd603c2776a9d76d9d74ec01841392982ebb840b09135",
};

/// The image and audio encoders (555 MB): needed only while indexing pictures, sound and video.
pub const MEDIA: ModelFile = ModelFile {
    name: "mmproj-embeddinggemma-2-Q8_0.gguf",
    bytes: 554_821_024,
    sha256: "c4a8a52691ecef40618438928bdf9e68379b854e24166f292592353db0aab64f",
};

/// In download order: the text model first, so search works while the encoders arrive.
pub const ALL: [ModelFile; 2] = [TEXT, MEDIA];

pub fn total_bytes() -> u64 {
    ALL.iter().map(|file| file.bytes).sum()
}

/// Where the files live: their own folder under FNDR's models directory.
pub fn dir(app_data_dir: &Path) -> PathBuf {
    crate::models::models_dir(app_data_dir).join("embeddinggemma-2")
}

/// The files not in `dir` yet. Only verified files get their final name, so being there is enough.
pub fn missing(dir: &Path) -> Vec<ModelFile> {
    ALL.into_iter()
        .filter(|file| !dir.join(file.name).is_file())
        .collect()
}

/// Progress of one download, for status events.
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct DownloadProgress {
    pub file: &'static str,
    pub bytes: u64,
    pub total: u64,
}

/// Downloads every missing file into `dir`, resuming a partial one, and
/// checks each against its pinned SHA-256 before giving it its final name. A
/// file that fails the check is deleted and the download fails.
pub async fn download(
    dir: &Path,
    mut progress: impl FnMut(DownloadProgress),
) -> Result<(), String> {
    use futures_util::StreamExt;
    use tokio::io::AsyncWriteExt;

    tokio::fs::create_dir_all(dir)
        .await
        .map_err(|error| error.to_string())?;
    let client = reqwest::Client::builder()
        .user_agent("FNDR/1.0")
        .connect_timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|error| error.to_string())?;
    for file in missing(dir) {
        let partial = dir.join(format!("{}.part", file.name));
        let have = tokio::fs::metadata(&partial)
            .await
            .map(|meta| meta.len())
            .unwrap_or(0);
        let have = if have > file.bytes { 0 } else { have };
        let mut request = client.get(file.url());
        if have > 0 {
            request = request.header(reqwest::header::RANGE, format!("bytes={have}-"));
        }
        let response = request
            .send()
            .await
            .map_err(|error| format!("{}: {error}", file.name))?;
        let status = response.status();
        if !status.is_success() {
            return Err(format!(
                "{}: the download was refused ({status})",
                file.name
            ));
        }
        // A server that ignores the range sends the whole file again.
        let resumed = have > 0 && status == reqwest::StatusCode::PARTIAL_CONTENT;
        let mut out = tokio::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .append(resumed)
            .truncate(!resumed)
            .open(&partial)
            .await
            .map_err(|error| error.to_string())?;
        let mut bytes = if resumed { have } else { 0 };
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|error| format!("{}: {error}", file.name))?;
            out.write_all(&chunk)
                .await
                .map_err(|error| error.to_string())?;
            bytes += chunk.len() as u64;
            progress(DownloadProgress {
                file: file.name,
                bytes,
                total: file.bytes,
            });
        }
        out.flush().await.map_err(|error| error.to_string())?;
        drop(out);
        let check = partial.clone();
        let expected = file.sha256;
        let verified = tokio::task::spawn_blocking(move || {
            crate::models::verify_file_sha256(&check, expected)
        })
        .await
        .map_err(|error| error.to_string())?;
        if let Err(error) = verified {
            let _ = tokio::fs::remove_file(&partial).await;
            return Err(error);
        }
        tokio::fs::rename(&partial, dir.join(file.name))
            .await
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_are_pinned() {
        for file in ALL {
            assert_eq!(file.sha256.len(), 64);
            assert!(file.url().contains(REVISION), "{}", file.url());
            assert!(file.name.contains("Q8_0"), "never F16: {}", file.name);
        }
        assert_eq!(total_bytes(), 864_676_480);
    }

    #[test]
    fn only_finished_files_count() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(missing(dir.path()), ALL.to_vec());
        std::fs::write(dir.path().join(format!("{}.part", TEXT.name)), b"x").unwrap();
        assert_eq!(missing(dir.path()), ALL.to_vec());
        std::fs::write(dir.path().join(TEXT.name), b"x").unwrap();
        assert_eq!(missing(dir.path()), vec![MEDIA]);
    }
}
