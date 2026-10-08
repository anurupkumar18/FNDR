use std::{
    env, fs, io,
    path::{Path, PathBuf},
    process::Command,
};

const HERMES_SKIP_DIRS: &[&str] = &[
    ".git",
    ".github",
    ".plans",
    "__pycache__",
    ".pytest_cache",
    ".mypy_cache",
    ".ruff_cache",
    "node_modules",
];

const HERMES_SKIP_FILES: &[&str] = &[".DS_Store"];

fn main() {
    if let Err(err) = stage_vendored_hermes_bundle() {
        panic!("failed to stage vendored Hermes bundle: {err}");
    }
    if let Err(err) = build_speech_helper() {
        panic!("failed to build native speech helper: {err}");
    }
    if let Err(err) = build_auth_helper() {
        panic!("failed to build native authentication helper: {err}");
    }
    tauri_build::build();
}

fn build_auth_helper() -> io::Result<()> {
    let target = env::var("TARGET").unwrap_or_default();
    if target != "aarch64-apple-darwin" {
        return Ok(());
    }
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap_or_default());
    let source = manifest_dir
        .join("helpers")
        .join("fndr-auth")
        .join("main.swift");
    let output = manifest_dir
        .join("binaries")
        .join(format!("fndr-auth-{target}"));
    println!("cargo:rerun-if-changed={}", source.display());
    if output_is_fresh(&source, &output)? {
        return Ok(());
    }
    fs::create_dir_all(output.parent().expect("authentication helper has a parent"))?;
    let status = Command::new("xcrun")
        .args([
            "swiftc",
            "-O",
            source.to_string_lossy().as_ref(),
            "-framework",
            "LocalAuthentication",
            "-o",
            output.to_string_lossy().as_ref(),
        ])
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "xcrun swiftc exited with {status}"
        )))
    }
}

fn build_speech_helper() -> io::Result<()> {
    let target = env::var("TARGET").unwrap_or_default();
    if target != "aarch64-apple-darwin" {
        return Ok(());
    }

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap_or_default());
    let helper_dir = manifest_dir.join("helpers").join("fndr-speech");
    let source = helper_dir.join("main.swift");
    let transcript_source = helper_dir.join("StreamingTranscript.swift");
    let privacy_plist = helper_dir.join("HelperInfo.plist");
    let binaries = manifest_dir.join("binaries");
    let output = binaries.join(format!("fndr-speech-{target}"));
    let helper_app = binaries.join("FNDR Speech Helper.app");
    let helper_contents = helper_app.join("Contents");
    let helper_macos = helper_contents.join("MacOS");
    let helper_executable = helper_macos.join("fndr-speech");
    let helper_info = helper_contents.join("Info.plist");

    println!("cargo:rerun-if-changed={}", source.display());
    println!("cargo:rerun-if-changed={}", transcript_source.display());
    println!("cargo:rerun-if-changed={}", privacy_plist.display());
    let binary_is_fresh = output_is_fresh(&source, &output)?
        && output_is_fresh(&transcript_source, &output)?
        && output_is_fresh(&privacy_plist, &output)?;
    if !binary_is_fresh {
        fs::create_dir_all(&binaries)?;
        let status = Command::new("xcrun")
            .args([
                "swiftc",
                "-O",
                source.to_string_lossy().as_ref(),
                transcript_source.to_string_lossy().as_ref(),
                "-framework",
                "AVFoundation",
                "-framework",
                "Speech",
                "-Xlinker",
                "-sectcreate",
                "-Xlinker",
                "__TEXT",
                "-Xlinker",
                "__info_plist",
                "-Xlinker",
                privacy_plist.to_string_lossy().as_ref(),
                "-o",
                output.to_string_lossy().as_ref(),
            ])
            .status()?;
        if !status.success() {
            return Err(io::Error::other(format!(
                "xcrun swiftc exited with {status}"
            )));
        }
    }

    let app_is_fresh = output_is_fresh(&output, &helper_executable)?
        && output_is_fresh(&privacy_plist, &helper_info)?;
    if app_is_fresh {
        return Ok(());
    }

    fs::create_dir_all(&helper_macos)?;
    fs::copy(&output, &helper_executable)?;
    fs::copy(&privacy_plist, &helper_info)?;
    let status = Command::new("codesign")
        .args([
            "--force",
            "--sign",
            "-",
            "--timestamp=none",
            helper_app.to_string_lossy().as_ref(),
        ])
        .status()?;
    if !status.success() {
        return Err(io::Error::other(format!("codesign exited with {status}")));
    }
    Ok(())
}

fn output_is_fresh(source: &Path, output: &Path) -> io::Result<bool> {
    let source_modified = fs::metadata(source)?.modified()?;
    let output_modified = match fs::metadata(output) {
        Ok(metadata) => metadata.modified()?,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(err) => return Err(err),
    };
    Ok(output_modified >= source_modified)
}

fn stage_vendored_hermes_bundle() -> io::Result<()> {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap_or_default());
    let source_dir = manifest_dir
        .parent()
        .unwrap_or(manifest_dir.as_path())
        .join("hermes-agent");
    let staging_dir = manifest_dir.join("target").join("hermes-agent-bundle");

    println!("cargo:rerun-if-changed={}", source_dir.display());

    reset_dir(&staging_dir)?;

    if source_dir.exists() {
        copy_dir_filtered(&source_dir, &staging_dir)?;
    }

    Ok(())
}

fn reset_dir(path: &Path) -> io::Result<()> {
    if path.exists() {
        fs::remove_dir_all(path)?;
    }
    fs::create_dir_all(path)
}

fn should_skip(name: &str, is_dir: bool) -> bool {
    if is_dir {
        HERMES_SKIP_DIRS.contains(&name)
    } else {
        HERMES_SKIP_FILES.contains(&name)
    }
}

fn copy_dir_filtered(source: &Path, destination: &Path) -> io::Result<()> {
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let file_name = entry.file_name();
        let file_name = file_name.to_string_lossy();

        if should_skip(&file_name, file_type.is_dir()) {
            continue;
        }

        let source_path = entry.path();
        let destination_path = destination.join(file_name.as_ref());

        if file_type.is_dir() {
            fs::create_dir_all(&destination_path)?;
            copy_dir_filtered(&source_path, &destination_path)?;
        } else if file_type.is_file() {
            fs::copy(&source_path, &destination_path)?;
            let permissions = fs::metadata(&source_path)?.permissions();
            fs::set_permissions(&destination_path, permissions)?;
        }
    }

    Ok(())
}
