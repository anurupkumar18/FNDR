//! A child process that answers one JSON line with one JSON line: how the app
//! talks to `fndr-files-helper` (the model) and `fndr-files-reader` (macOS's
//! readers). The child prints a ready line first and quits when its stdin
//! closes, so dropping a [`LineProcess`] ends it. A child that dies or answers
//! garbage fails the call; the caller starts a new one next time.
//!
//! Blocking: run it on a blocking thread, never on an async worker.

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

pub struct LineProcess {
    name: &'static str,
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl LineProcess {
    /// Starts `program` and reads its ready line. A ready line without
    /// `"ready": true` is a failure, with its `error` as the reason.
    pub fn start(
        name: &'static str,
        program: &Path,
        args: &[&std::ffi::OsStr],
    ) -> Result<(Self, Value), String> {
        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|error| format!("can't start {}: {error}", program.display()))?;
        let stdin = child.stdin.take().ok_or("no stdin")?;
        let mut stdout = BufReader::new(child.stdout.take().ok_or("no stdout")?);
        let mut line = String::new();
        let _ = stdout.read_line(&mut line);
        let ready: Option<Value> = serde_json::from_str(line.trim()).ok();
        match ready {
            Some(ready) if ready.get("ready").and_then(Value::as_bool) == Some(true) => Ok((
                Self {
                    name,
                    child,
                    stdin,
                    stdout,
                    next_id: 1,
                },
                ready,
            )),
            other => {
                let _ = child.kill();
                let _ = child.wait();
                let why = other
                    .as_ref()
                    .and_then(|ready| {
                        ready
                            .get("error")
                            .and_then(Value::as_str)
                            .map(str::to_string)
                    })
                    .unwrap_or_else(|| line.trim().to_string());
                Err(format!("the {name} did not start: {why}"))
            }
        }
    }

    /// Sends `request` with a fresh id and returns the reply, or its `error`.
    pub fn call(&mut self, mut request: Value) -> Result<Value, String> {
        let id = self.next_id;
        self.next_id += 1;
        request["id"] = json!(id);
        let stopped = || format!("the {} stopped", self.name);
        writeln!(self.stdin, "{request}")
            .and_then(|_| self.stdin.flush())
            .map_err(|_| stopped())?;
        let mut line = String::new();
        if self
            .stdout
            .read_line(&mut line)
            .map_err(|error| error.to_string())?
            == 0
        {
            return Err(stopped());
        }
        let reply: Value = serde_json::from_str(line.trim())
            .map_err(|_| format!("the {} answered garbage", self.name))?;
        if reply.get("id").and_then(Value::as_u64) != Some(id) {
            return Err(format!("the {} answered out of turn", self.name));
        }
        if reply.get("ok").and_then(Value::as_bool) != Some(true) {
            return Err(reply
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("unknown error")
                .to_string());
        }
        Ok(reply)
    }
}

impl Drop for LineProcess {
    fn drop(&mut self) {
        // Closing stdin asks it to quit; one stuck in a long call is killed.
        let _ = self.stdin.flush();
        if matches!(self.child.try_wait(), Ok(None)) {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }
}

/// A binary FNDR ships beside its own executable (Tauri's external binaries),
/// or that the build staged in `src-tauri/binaries` for development.
pub fn locate(name: &str) -> Option<std::path::PathBuf> {
    let triple = if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        "aarch64-apple-darwin"
    } else if cfg!(target_os = "macos") {
        "x86_64-apple-darwin"
    } else {
        "x86_64-unknown-linux-gnu"
    };
    let suffixed = format!("{name}-{triple}");
    let mut candidates = Vec::new();
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
    {
        candidates.push(dir.join(name));
        candidates.push(dir.join(&suffixed));
    }
    candidates.push(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("binaries")
            .join(&suffixed),
    );
    candidates.into_iter().find(|path| path.is_file())
}

#[cfg(all(test, unix))]
pub(crate) mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    /// Writes an executable Python stand-in that prints `ready`, then answers
    /// each request with `answer(request)` (a Python expression). Real helpers
    /// are checked by `make files-check` and on a Mac.
    pub fn fake(dir: &Path, name: &str, ready: &str, answer: &str) -> std::path::PathBuf {
        let script = dir.join(name);
        std::fs::write(
            &script,
            format!(
                "#!/usr/bin/env python3\nimport json, sys\nprint(json.dumps({ready}), flush=True)\n\
                 for line in sys.stdin:\n    request = json.loads(line)\n    reply = {answer}\n    \
                 reply['id'] = request['id']\n    print(json.dumps(reply), flush=True)\n"
            ),
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        script
    }

    #[test]
    fn replies_and_errors_come_back() {
        let dir = tempfile::tempdir().unwrap();
        let program = fake(
            dir.path(),
            "echo",
            r#"{"ready": True}"#,
            r#"{"ok": request["op"] == "hi", "echo": request["op"], "error": "no"}"#,
        );
        let (mut process, _) = LineProcess::start("echo", &program, &[]).unwrap();
        assert_eq!(process.call(json!({"op": "hi"})).unwrap()["echo"], "hi");
        assert_eq!(process.call(json!({"op": "bye"})).unwrap_err(), "no");
    }

    #[test]
    fn a_child_that_is_not_ready_says_why() {
        let dir = tempfile::tempdir().unwrap();
        let program = fake(
            dir.path(),
            "sad",
            r#"{"ready": False, "error": "no model"}"#,
            "{}",
        );
        let error = LineProcess::start("helper", &program, &[]).err().unwrap();
        assert_eq!(error, "the helper did not start: no model");
    }

    #[test]
    fn a_missing_program_fails_to_start() {
        assert!(LineProcess::start("helper", Path::new("/nonexistent/helper"), &[]).is_err());
    }
}
