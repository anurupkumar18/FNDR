//! Encryption at rest for the files FNDR writes outside its database:
//! screenshots, and exports the person chooses to keep sealed.
//!
//! What this covers and what it does not, in one place, because the Privacy
//! Proof screen quotes it:
//!
//! - Covered: image files in `frames/` and `screenshots/`, and export files
//!   written with `encrypt: true`. They are sealed with AES-256-GCM under a
//!   random 256-bit key that lives in the macOS Keychain, never on disk.
//! - Not covered: the LanceDB database (`lancedb/`). Memory text, embeddings
//!   and the graph are stored as plain Lance columns because column
//!   encryption would break full-text and vector search. Disk-level protection
//!   for the database is FileVault, whose state this module only reports.
//! - Not covered: Screen Guide diagnostic bundles, which the person asks for
//!   by hand and may need to open.

use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM, NONCE_LEN};
use ring::rand::{SecureRandom, SystemRandom};
use serde::Serialize;
use std::fmt;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

const MAGIC: &[u8; 8] = b"FNDRENC1";
const KEY_LEN: usize = 32;
/// Extension appended to a sealed file: `shot.png` becomes `shot.png.fndrenc`.
pub const SEALED_EXTENSION: &str = "fndrenc";
const KEYCHAIN_SERVICE: &str = "com.fndr.vault";
const KEYCHAIN_ACCOUNT: &str = "data-key-v1";
/// A file this young may still be in use by capture; the sweep leaves it.
const MIN_AGE_BEFORE_SEALING: Duration = Duration::from_secs(60);
const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "heic", "webp", "tif", "tiff"];
/// Folders under the app data directory that hold captured images.
pub const CAPTURE_IMAGE_DIRS: &[&str] = &["frames", "screenshots"];

#[derive(Debug)]
pub enum VaultError {
    Key(String),
    Crypto(&'static str),
    Io(std::io::Error),
}

impl fmt::Display for VaultError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Key(message) => write!(f, "encryption key unavailable: {message}"),
            Self::Crypto(message) => write!(f, "{message}"),
            Self::Io(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for VaultError {}

impl From<std::io::Error> for VaultError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

/// Where the key comes from. The app uses [`KeychainKey`]; tests use
/// [`StaticKey`] so no test touches the person's Keychain.
pub trait KeyProvider: Send + Sync {
    fn key(&self) -> Result<[u8; KEY_LEN], VaultError>;
}

/// A fixed key, for tests.
pub struct StaticKey(pub [u8; KEY_LEN]);

impl KeyProvider for StaticKey {
    fn key(&self) -> Result<[u8; KEY_LEN], VaultError> {
        Ok(self.0)
    }
}

/// The key held in the macOS Keychain under FNDR's own service name. Created
/// on first use.
pub struct KeychainKey {
    service: String,
    account: String,
}

impl Default for KeychainKey {
    fn default() -> Self {
        Self {
            service: KEYCHAIN_SERVICE.to_string(),
            account: KEYCHAIN_ACCOUNT.to_string(),
        }
    }
}

impl KeychainKey {
    pub fn named(service: &str, account: &str) -> Self {
        Self {
            service: service.to_string(),
            account: account.to_string(),
        }
    }

    /// Remove the stored key. Anything sealed under it can no longer be opened.
    #[cfg(target_os = "macos")]
    pub fn delete(&self) -> Result<(), VaultError> {
        security_framework::passwords::delete_generic_password(&self.service, &self.account)
            .map_err(|error| VaultError::Key(error.to_string()))
    }
}

#[cfg(target_os = "macos")]
impl KeyProvider for KeychainKey {
    fn key(&self) -> Result<[u8; KEY_LEN], VaultError> {
        use security_framework::passwords::{get_generic_password, set_generic_password};
        const ERR_SEC_ITEM_NOT_FOUND: i32 = -25300;

        match get_generic_password(&self.service, &self.account) {
            Ok(bytes) => <[u8; KEY_LEN]>::try_from(bytes.as_slice())
                .map_err(|_| VaultError::Key("the stored key has the wrong length".to_string())),
            Err(error) if error.code() == ERR_SEC_ITEM_NOT_FOUND => {
                let mut key = [0u8; KEY_LEN];
                SystemRandom::new()
                    .fill(&mut key)
                    .map_err(|_| VaultError::Key("no random source".to_string()))?;
                set_generic_password(&self.service, &self.account, &key)
                    .map_err(|error| VaultError::Key(error.to_string()))?;
                Ok(key)
            }
            Err(error) => Err(VaultError::Key(error.to_string())),
        }
    }
}

#[cfg(not(target_os = "macos"))]
impl KeyProvider for KeychainKey {
    fn key(&self) -> Result<[u8; KEY_LEN], VaultError> {
        Err(VaultError::Key(
            "the Keychain is only available on macOS".to_string(),
        ))
    }
}

fn aead_key(key: &[u8; KEY_LEN]) -> Result<LessSafeKey, VaultError> {
    UnboundKey::new(&AES_256_GCM, key)
        .map(LessSafeKey::new)
        .map_err(|_| VaultError::Crypto("the key was rejected"))
}

/// Whether the bytes are a sealed file written by [`seal`].
pub fn is_sealed(bytes: &[u8]) -> bool {
    bytes.starts_with(MAGIC)
}

/// Seal `plaintext`. `context` is authenticated but not stored: the same text
/// must be passed to [`open`], so a sealed file cannot be moved under another
/// name and still open.
pub fn seal(key: &[u8; KEY_LEN], plaintext: &[u8], context: &str) -> Result<Vec<u8>, VaultError> {
    let sealing = aead_key(key)?;
    let mut nonce_bytes = [0u8; NONCE_LEN];
    SystemRandom::new()
        .fill(&mut nonce_bytes)
        .map_err(|_| VaultError::Crypto("no random source"))?;
    let mut in_out = plaintext.to_vec();
    sealing
        .seal_in_place_append_tag(
            Nonce::assume_unique_for_key(nonce_bytes),
            Aad::from(context.as_bytes()),
            &mut in_out,
        )
        .map_err(|_| VaultError::Crypto("sealing failed"))?;
    let mut out = Vec::with_capacity(MAGIC.len() + NONCE_LEN + in_out.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&in_out);
    Ok(out)
}

/// Open bytes written by [`seal`] with the same key and context.
pub fn open(key: &[u8; KEY_LEN], sealed: &[u8], context: &str) -> Result<Vec<u8>, VaultError> {
    if !is_sealed(sealed) || sealed.len() < MAGIC.len() + NONCE_LEN + AES_256_GCM.tag_len() {
        return Err(VaultError::Crypto("not a sealed FNDR file"));
    }
    let opening = aead_key(key)?;
    let nonce_start = MAGIC.len();
    let mut nonce_bytes = [0u8; NONCE_LEN];
    nonce_bytes.copy_from_slice(&sealed[nonce_start..nonce_start + NONCE_LEN]);
    let mut in_out = sealed[nonce_start + NONCE_LEN..].to_vec();
    let plain = opening
        .open_in_place(
            Nonce::assume_unique_for_key(nonce_bytes),
            Aad::from(context.as_bytes()),
            &mut in_out,
        )
        .map_err(|_| VaultError::Crypto("wrong key, or the file was changed"))?;
    Ok(plain.to_vec())
}

pub(crate) fn sealed_path_for(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".{SEALED_EXTENSION}"));
    path.with_file_name(name)
}

fn context_for(plain_path: &Path) -> String {
    plain_path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Write `bytes` sealed to `<path>.fndrenc`, atomically, mode 0600.
pub fn write_sealed(
    keys: &dyn KeyProvider,
    path: &Path,
    bytes: &[u8],
) -> Result<PathBuf, VaultError> {
    let sealed = seal(&keys.key()?, bytes, &context_for(path))?;
    let target = sealed_path_for(path);
    let tmp = target.with_extension("fndrenc.tmp");
    {
        use std::io::Write;
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&tmp)?;
        file.write_all(&sealed)?;
        file.sync_all()?;
    }
    std::fs::rename(&tmp, &target)?;
    Ok(target)
}

/// Read a file written by [`write_sealed`]; `sealed_path` ends in `.fndrenc`.
pub fn read_sealed(keys: &dyn KeyProvider, sealed_path: &Path) -> Result<Vec<u8>, VaultError> {
    let plain_path = sealed_path.with_extension("");
    let sealed = std::fs::read(sealed_path)?;
    open(&keys.key()?, &sealed, &context_for(&plain_path))
}

/// Seal one plaintext file in place: the sealed copy is written first and the
/// original removed only after the sealed copy is on disk.
pub fn seal_file(keys: &dyn KeyProvider, path: &Path) -> Result<PathBuf, VaultError> {
    let bytes = std::fs::read(path)?;
    let target = write_sealed(keys, path, &bytes)?;
    std::fs::remove_file(path)?;
    Ok(target)
}

#[derive(Debug, Default, Clone, Serialize, PartialEq, Eq)]
pub struct SealReport {
    pub sealed: usize,
    pub already_sealed: usize,
    pub skipped_recent: usize,
    pub failed: usize,
}

fn is_image(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase)
        .is_some_and(|ext| IMAGE_EXTENSIONS.contains(&ext.as_str()))
}

fn is_sealed_name(path: &Path) -> bool {
    path.extension().and_then(|ext| ext.to_str()) == Some(SEALED_EXTENSION)
}

/// Seal every plaintext image under `dir`, recursively. A missing directory is
/// an empty sweep. Files modified within the last minute are left for the next
/// sweep.
pub fn seal_image_directory(keys: &dyn KeyProvider, dir: &Path, now: SystemTime) -> SealReport {
    let mut report = SealReport::default();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(current) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_dir() {
                pending.push(path);
                continue;
            }
            if is_sealed_name(&path) {
                report.already_sealed += 1;
                continue;
            }
            if !is_image(&path) {
                continue;
            }
            let recent = entry
                .metadata()
                .and_then(|meta| meta.modified())
                .ok()
                .and_then(|modified| now.duration_since(modified).ok())
                .is_some_and(|age| age < MIN_AGE_BEFORE_SEALING);
            if recent {
                report.skipped_recent += 1;
                continue;
            }
            match seal_file(keys, &path) {
                Ok(_) => report.sealed += 1,
                Err(error) => {
                    tracing::warn!(%error, "vault:seal_failed");
                    report.failed += 1;
                }
            }
        }
    }
    report
}

/// Seal the image folders FNDR owns under `app_data_dir`.
pub fn seal_capture_images(keys: &dyn KeyProvider, app_data_dir: &Path) -> SealReport {
    let now = SystemTime::now();
    let mut total = SealReport::default();
    for name in CAPTURE_IMAGE_DIRS {
        let report = seal_image_directory(keys, &app_data_dir.join(name), now);
        total.sealed += report.sealed;
        total.already_sealed += report.already_sealed;
        total.skipped_recent += report.skipped_recent;
        total.failed += report.failed;
    }
    total
}

/// Whether the Mac's startup disk is encrypted by FileVault.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FileVaultStatus {
    On,
    Off,
    Unknown,
}

/// Read `fdesetup status` output: "FileVault is On." or "FileVault is Off."
/// An encryption or decryption in progress is reported as `Unknown`, because
/// the disk is not fully protected yet.
pub fn parse_fdesetup_status(output: &str) -> FileVaultStatus {
    let first = output.lines().next().unwrap_or("").trim().to_lowercase();
    if first.starts_with("filevault is on") {
        if output.to_lowercase().contains("in progress") {
            FileVaultStatus::Unknown
        } else {
            FileVaultStatus::On
        }
    } else if first.starts_with("filevault is off") {
        FileVaultStatus::Off
    } else {
        FileVaultStatus::Unknown
    }
}

/// Ask macOS. `fdesetup status` needs no administrator rights.
pub fn file_vault_status() -> FileVaultStatus {
    #[cfg(target_os = "macos")]
    {
        match std::process::Command::new("/usr/bin/fdesetup")
            .arg("status")
            .output()
        {
            Ok(out) if out.status.success() => {
                parse_fdesetup_status(&String::from_utf8_lossy(&out.stdout))
            }
            _ => FileVaultStatus::Unknown,
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        FileVaultStatus::Unknown
    }
}

/// What is and is not encrypted, as shown on Privacy Proof. One source for
/// the screen and the evidence file.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct EncryptionStatement {
    pub filevault: FileVaultStatus,
    pub encrypted: Vec<&'static str>,
    pub not_encrypted: Vec<&'static str>,
}

pub fn encryption_statement(filevault: FileVaultStatus) -> EncryptionStatement {
    EncryptionStatement {
        filevault,
        encrypted: vec![
            "A background sweep attempts to seal the few screenshots it finds in FNDR's capture folders with a Keychain key.",
            "Archives saved from Privacy activity are sealed with the same Keychain key.",
        ],
        not_encrypted: vec![
            "The memory database is not encrypted by FNDR. It holds text, search indexes, embeddings and graph data. FileVault can protect the disk when it is on.",
            "Other exports such as PDFs can be saved as plain files.",
            "Screen Guide diagnostic bundles you ask FNDR to save.",
        ],
    }
}
