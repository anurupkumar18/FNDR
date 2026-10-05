//! Persistent MCP API token.
//!
//! On first launch, generates a UUID v4 bearer token and writes it to
//! `~/.fndr/mcp_token` (mode 0o600). Subsequent calls return the cached token.
//! The token is used to authenticate all MCP HTTP requests.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

static TOKEN: OnceLock<String> = OnceLock::new();

fn token_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".fndr")
        .join("mcp_token")
}

/// Load the existing token from disk, or generate and persist a new one.
pub fn load_or_create() -> String {
    TOKEN
        .get_or_init(|| load_or_create_at(&token_path()))
        .clone()
}

/// The token at `path`, created if missing or empty. The file is readable by
/// the owner only (0o600) from the moment it exists, and a token file that
/// another user can read is tightened on load (VS-61): the token unlocks every
/// memory over MCP.
fn load_or_create_at(path: &Path) -> String {
    // Try to load existing token
    if let Ok(mut file) = fs::File::open(path) {
        let mut existing = String::new();
        match restrict_to_owner(&file).and_then(|_| file.read_to_string(&mut existing)) {
            Ok(_) => {
                let token = existing.trim().to_string();
                if !token.is_empty() {
                    tracing::debug!("Loaded existing MCP token from {:?}", path);
                    return token;
                }
            }
            Err(e) => tracing::warn!("Failed to securely load MCP token: {}", e),
        }
    }

    // Generate a fresh token
    let token = uuid::Uuid::new_v4().to_string();

    // Ensure directory exists
    if let Some(parent) = path.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            tracing::warn!("Failed to create ~/.fndr dir: {}", e);
        }
    }

    match write_owner_only(path, &token) {
        Ok(()) => tracing::info!("Generated new MCP token, saved to {:?}", path),
        Err(e) => tracing::warn!("Failed to persist MCP token: {}", e),
    }

    token
}

fn write_owner_only(path: &Path, token: &str) -> std::io::Result<()> {
    let mut file = open_owner_only(path)?;
    file.set_len(0)?;
    file.write_all(token.as_bytes())
}

fn open_owner_only(path: &Path) -> std::io::Result<fs::File> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(path)?;
    // Creation mode does not change existing files. Restrict the same open
    // descriptor before truncating or writing any secret bytes.
    restrict_to_owner(&file)?;
    Ok(file)
}

fn restrict_to_owner(file: &fs::File) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let too_open = file.metadata()?.permissions().mode() & 0o077 != 0;
        if too_open {
            file.set_permissions(fs::Permissions::from_mode(0o600))?;
        }
    }
    #[cfg(not(unix))]
    let _ = file;
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn mode(path: &Path) -> u32 {
        fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn a_new_token_file_is_readable_by_the_owner_only() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".fndr").join("mcp_token");
        let token = load_or_create_at(&path);
        assert!(!token.is_empty());
        assert_eq!(fs::read_to_string(&path).unwrap(), token);
        assert_eq!(mode(&path), 0o600);
        // Loading again returns the same token.
        assert_eq!(load_or_create_at(&path), token);
    }

    #[test]
    fn a_token_file_others_can_read_is_tightened_on_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcp_token");
        fs::write(&path, "existing-token\n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(load_or_create_at(&path), "existing-token");
        assert_eq!(mode(&path), 0o600);
    }

    #[test]
    fn an_empty_token_file_gets_a_new_token() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcp_token");
        fs::write(&path, "  \n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        let token = load_or_create_at(&path);
        assert!(!token.trim().is_empty());
        assert_eq!(fs::read_to_string(&path).unwrap(), token);
        assert_eq!(mode(&path), 0o600);
    }

    #[test]
    fn an_existing_token_file_is_private_before_secret_bytes_are_written() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcp_token");
        fs::write(&path, "  \n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();

        let mut file = open_owner_only(&path).unwrap();
        assert_eq!(file.metadata().unwrap().permissions().mode() & 0o777, 0o600);
        assert!(fs::read_to_string(&path).unwrap().trim().is_empty());
        file.set_len(0).unwrap();
        file.write_all(b"new-secret-token").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "new-secret-token");
    }
}
