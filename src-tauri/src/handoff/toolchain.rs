//! FNDR's own Python for hand-off workers (Part 7): Python 3.12 managed by
//! uv inside the toolchain folder, and the packages the skills need, from a
//! hash-pinned lock. Installed only when Hand-off is turned on; the worker
//! profile makes the folder read-only (spike S5: 199 MB, 45 s on an M1).

use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest, Sha256};

/// The packages, every file pinned by hash (`uv pip compile --universal
/// --generate-hashes`, Python 3.12).
pub const LOCK: &str = include_str!("../../sidecars/hand-off/requirements.lock");

const INSTALLED_MARKER: &str = "installed-lock.sha256";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Missing,
    /// Installed from a different lock; installing again replaces it.
    Outdated,
    Installed,
}

pub fn python(dir: &Path) -> PathBuf {
    dir.join("venv").join("bin").join("python")
}

/// The lock's hash, written next to the venv once an install finishes.
pub fn lock_hash() -> String {
    Sha256::digest(LOCK.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

pub fn status(dir: &Path) -> Status {
    if !python(dir).is_file() {
        return Status::Missing;
    }
    match std::fs::read_to_string(dir.join(INSTALLED_MARKER)) {
        Ok(hash) if hash.trim() == lock_hash() => Status::Installed,
        _ => Status::Outdated,
    }
}

/// Installs into `dir` with `uv`, replacing what is there. Blocking: run it
/// off the async runtime. Downloads Python and the locked packages; nothing
/// is installed that is not in the lock with its hash.
pub fn install(uv: &Path, dir: &Path) -> Result<(), String> {
    let _ = std::fs::remove_dir_all(dir);
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let lock = dir.join("requirements.lock");
    std::fs::write(&lock, LOCK).map_err(|e| e.to_string())?;
    let venv = dir.join("venv");
    run(
        uv,
        dir,
        &[
            "venv".as_ref(),
            venv.as_os_str(),
            "--python".as_ref(),
            "3.12".as_ref(),
            "--python-preference".as_ref(),
            "only-managed".as_ref(),
        ],
    )?;
    let python = python(dir);
    run(
        uv,
        dir,
        &[
            "pip".as_ref(),
            "install".as_ref(),
            "--python".as_ref(),
            python.as_os_str(),
            "--require-hashes".as_ref(),
            "-r".as_ref(),
            lock.as_os_str(),
        ],
    )?;
    let _ = std::fs::remove_dir_all(dir.join("cache"));
    std::fs::write(dir.join(INSTALLED_MARKER), lock_hash()).map_err(|e| e.to_string())
}

pub fn remove(dir: &Path) -> Result<(), String> {
    match std::fs::remove_dir_all(dir) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

/// `uv` with its Python and cache kept inside the toolchain folder, so
/// nothing lands in the person's own uv or Python setup.
fn run(uv: &Path, dir: &Path, args: &[&std::ffi::OsStr]) -> Result<(), String> {
    let output = Command::new(uv)
        .args(args)
        .env("UV_PYTHON_INSTALL_DIR", dir.join("python"))
        .env("UV_CACHE_DIR", dir.join("cache"))
        .env("UV_NO_PROGRESS", "1")
        .env_remove("VIRTUAL_ENV")
        .output()
        .map_err(|e| format!("Could not run uv: {e}"))?;
    if output.status.success() {
        return Ok(());
    }
    let detail = String::from_utf8_lossy(&output.stderr);
    let last = detail
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("no detail");
    Err(format!("Installing the document tools failed: {last}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lock_pins_every_package_by_hash() {
        let packages: Vec<&str> = LOCK
            .lines()
            .filter(|l| l.chars().next().is_some_and(|c| c.is_ascii_lowercase()))
            .collect();
        assert!(
            packages.iter().all(|l| l.contains("==")),
            "every package has a version"
        );
        for name in [
            "pandas==",
            "matplotlib==",
            "python-docx==",
            "openpyxl==",
            "python-pptx==",
            "pypdf==",
        ] {
            assert!(packages.iter().any(|l| l.starts_with(name)), "{name}");
        }
        assert!(LOCK.matches("--hash=sha256:").count() >= packages.len());
        assert_eq!(lock_hash().len(), 64);
    }

    #[test]
    fn status_follows_the_python_and_the_lock_it_came_from() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(status(dir.path()), Status::Missing);
        std::fs::create_dir_all(dir.path().join("venv/bin")).unwrap();
        std::fs::write(python(dir.path()), "").unwrap();
        assert_eq!(status(dir.path()), Status::Outdated, "no marker");
        std::fs::write(dir.path().join(INSTALLED_MARKER), "0".repeat(64)).unwrap();
        assert_eq!(status(dir.path()), Status::Outdated, "another lock");
        std::fs::write(dir.path().join(INSTALLED_MARKER), lock_hash()).unwrap();
        assert_eq!(status(dir.path()), Status::Installed);
        remove(dir.path()).unwrap();
        assert_eq!(status(dir.path()), Status::Missing);
        assert!(remove(dir.path()).is_ok(), "removing twice is fine");
    }

    /// Downloads Python 3.12 and the locked packages (about 200 MB).
    #[test]
    #[ignore = "Downloads about 200 MB; run: cargo test --lib live_toolchain_installs_from_the_lock -- --ignored --nocapture"]
    fn live_toolchain_installs_from_the_lock() {
        let uv = which("uv").expect("uv on PATH");
        let dir = tempfile::tempdir().unwrap();
        let toolchain = dir.path().join("toolchain");
        let started = std::time::Instant::now();
        install(&uv, &toolchain).unwrap();
        eprintln!("installed in {:?}", started.elapsed());
        assert_eq!(status(&toolchain), Status::Installed);
        assert!(
            !toolchain.join("cache").exists(),
            "the download cache is removed"
        );
        let imported = Command::new(python(&toolchain))
            .args([
                "-I",
                "-c",
                "import pandas, matplotlib, docx, openpyxl, pptx, pypdf; print('ok')",
            ])
            .output()
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&imported.stdout).trim(), "ok");
    }

    fn which(name: &str) -> Option<PathBuf> {
        std::env::split_paths(&std::env::var_os("PATH")?)
            .map(|dir| dir.join(name))
            .find(|path| path.is_file())
    }
}
