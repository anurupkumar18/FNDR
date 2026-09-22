use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};

static LAYA_BOOTSTRAP_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

pub fn laya_venv_dir() -> Option<PathBuf> {
    dirs::document_dir().map(|root| root.join("FNDR Laya").join("venv"))
}

fn laya_bootstrap_lock() -> &'static Mutex<()> {
    LAYA_BOOTSTRAP_LOCK.get_or_init(|| Mutex::new(()))
}

fn pip_for_venv(venv_dir: &Path) -> PathBuf {
    venv_dir.join("bin").join("pip")
}

fn python_for_venv(venv_dir: &Path) -> PathBuf {
    venv_dir.join("bin").join("python3")
}

fn find_python3() -> Option<PathBuf> {
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
    candidates.iter().map(PathBuf::from).find(|candidate| {
        Command::new(candidate)
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    })
}

fn laya_import_is_ready(python: &Path) -> bool {
    Command::new(python)
        .args(["-c", "from laya import Router"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

pub fn ensure_laya_venv_ready() -> Result<PathBuf, String> {
    let _guard = laya_bootstrap_lock()
        .lock()
        .map_err(|_| "FNDR Laya bootstrap lock was poisoned".to_string())?;
    let venv_dir = laya_venv_dir().ok_or("Could not determine Documents directory")?;
    let python = python_for_venv(&venv_dir);

    if !python.exists() {
        let python3 = find_python3().ok_or("python3 is required for the Laya decider")?;
        if let Some(parent) = venv_dir.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let status = Command::new(&python3)
            .args(["-m", "venv", &venv_dir.to_string_lossy()])
            .status()
            .map_err(|error| format!("Failed creating FNDR Laya venv: {error}"))?;
        if !status.success() {
            return Err("Failed creating FNDR Laya venv".to_string());
        }
    }

    if !laya_import_is_ready(&python) {
        let pip = pip_for_venv(&venv_dir);
        if !pip.exists() {
            return Err(format!("Pip binary missing at {pip:?}"));
        }
        let install = Command::new(&pip)
            .args(["install", "laya"])
            .status()
            .map_err(|error| format!("Failed installing laya: {error}"))?;
        if !install.success() || !laya_import_is_ready(&python) {
            return Err("Failed installing laya into the FNDR Laya venv".to_string());
        }
    }

    Ok(venv_dir)
}

fn laya_sidecar_path() -> Option<PathBuf> {
    let packaged = std::env::current_exe().ok().and_then(|exe| {
        exe.parent()
            .map(|directory| directory.join("../Resources/sidecars/laya_decide_runner.py"))
    });
    if let Some(path) = packaged.filter(|path| path.exists()) {
        return Some(path);
    }

    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("sidecars")
        .join("laya_decide_runner.py");
    dev.exists().then_some(dev)
}

pub fn run_laya_predict(
    state: &serde_json::Value,
    questions: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let venv_dir = ensure_laya_venv_ready()?;
    let python = python_for_venv(&venv_dir);
    let sidecar = laya_sidecar_path().ok_or("Could not locate laya_decide_runner.py")?;
    let payload = serde_json::json!({"state": state, "questions": questions}).to_string();
    let output = Command::new(&python)
        .arg(sidecar)
        .arg(payload)
        .output()
        .map_err(|error| format!("Failed to run laya sidecar: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "Laya sidecar failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("Bad Laya sidecar output: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn development_sidecar_exists() {
        assert!(laya_sidecar_path().is_some());
    }

    #[test]
    #[ignore = "requires network, the Laya package, and a downloaded checkpoint"]
    fn laya_predict_returns_a_choice_and_confidence_for_a_simple_routing_question() {
        let state = serde_json::json!({"body": "We were billed twice, please refund today."});
        let questions = serde_json::json!({
            "department": {
                "type": "choice",
                "instructions": "Which department should handle this?",
                "criteria": {"billing": "invoices, payments, refunds", "technical": "bugs, outages"}
            }
        });
        let result = run_laya_predict(&state, &questions).unwrap();
        let choice = result["answers"]["department"]["choice"].as_str().unwrap();
        assert_eq!(choice, "billing");
    }
}
