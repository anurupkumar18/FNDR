use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use super::prompts;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    AppleFoundation,
    LocalGguf,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Availability {
    Available,
    ModelNotReady,
    AppleIntelligenceOff,
    UnsupportedDevice,
    UnsupportedOs,
    HelperUnavailable,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct AppleModelStatus {
    pub availability: Availability,
    pub enabled: bool,
}

// The third field permits Apple when GGUF is absent. Only tasks with real
// Apple runs and caller validation have that fallback. The pixel runtime never
// enters this text route.
pub const TASK_DEFAULTS: &[(&str, Backend, bool)] = &[
    ("memory_extraction", Backend::LocalGguf, true),
    ("memory_snippet", Backend::LocalGguf, true),
    ("answer", Backend::LocalGguf, true),
    ("memory_extraction_repair", Backend::LocalGguf, false),
    ("memory_review", Backend::LocalGguf, false),
    ("memory_review_repair", Backend::LocalGguf, false),
    ("card_synthesis", Backend::LocalGguf, false),
    ("query_expansion", Backend::LocalGguf, false),
    ("screen_guide", Backend::LocalGguf, false),
    ("todo_extraction", Backend::LocalGguf, false),
    ("task_suggestion", Backend::LocalGguf, false),
    ("meeting_breakdown", Backend::LocalGguf, false),
    ("vision_description", Backend::LocalGguf, false),
    ("eval_judge", Backend::LocalGguf, false),
    ("query_plan", Backend::LocalGguf, false),
];

pub fn apple_enabled(setting: Option<bool>, availability: Availability) -> bool {
    setting.unwrap_or(availability == Availability::Available)
        && availability == Availability::Available
}

pub fn route(task: &str, enabled: bool, available: bool, local_available: bool) -> Backend {
    let Some((_, preferred, apple_without_local)) =
        TASK_DEFAULTS.iter().find(|(id, _, _)| *id == task)
    else {
        return Backend::LocalGguf;
    };
    if enabled
        && available
        && (*preferred == Backend::AppleFoundation || (!local_available && *apple_without_local))
    {
        Backend::AppleFoundation
    } else {
        Backend::LocalGguf
    }
}

#[derive(Serialize, Deserialize)]
struct AppleSetting {
    enabled: bool,
}

pub fn setting_path(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("apple_model.json")
}

pub fn read_setting(app_data_dir: &Path) -> Option<bool> {
    fs::read(setting_path(app_data_dir))
        .ok()
        .and_then(|data| serde_json::from_slice::<AppleSetting>(&data).ok())
        .map(|setting| setting.enabled)
}

pub fn write_setting(app_data_dir: &Path, enabled: bool) -> std::io::Result<()> {
    fs::create_dir_all(app_data_dir)?;
    let path = setting_path(app_data_dir);
    let temp = path.with_extension("json.tmp");
    fs::write(
        &temp,
        serde_json::to_vec(&AppleSetting { enabled }).map_err(std::io::Error::other)?,
    )?;
    fs::rename(temp, path)
}

pub fn ready(app_data_dir: &Path) -> bool {
    if read_setting(app_data_dir) == Some(false) {
        return false;
    }
    status(app_data_dir).enabled
}

pub fn status(app_data_dir: &Path) -> AppleModelStatus {
    status_with_helper(app_data_dir, helper_path().as_deref())
}

fn status_with_helper(app_data_dir: &Path, helper: Option<&Path>) -> AppleModelStatus {
    let setting = read_setting(app_data_dir);
    let availability = helper
        .map(|path| Client::new(path.to_path_buf()).probe())
        .unwrap_or(Availability::HelperUnavailable);
    AppleModelStatus {
        availability,
        enabled: apple_enabled(setting, availability),
    }
}

pub fn helper_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("FNDR_FOUNDATION_HELPER") {
        return Some(PathBuf::from(path));
    }
    let suffix = if cfg!(target_arch = "aarch64") {
        "fndr-foundation-aarch64-apple-darwin"
    } else {
        "fndr-foundation-x86_64-apple-darwin"
    };
    let mut candidates = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            candidates.push(parent.join("fndr-foundation"));
            candidates.push(parent.join(suffix));
        }
    }
    candidates.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("binaries")
            .join(suffix),
    );
    candidates.into_iter().find(|path| path.is_file())
}

#[derive(Debug, Deserialize)]
struct Reply {
    id: u64,
    ok: bool,
    availability: Option<Availability>,
    output: Option<String>,
    error: Option<String>,
}

struct Process {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
}

pub struct Client {
    path: PathBuf,
    process: Option<Process>,
    next_id: u64,
}

impl Client {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            process: None,
            next_id: 1,
        }
    }

    fn start(&mut self) -> std::io::Result<()> {
        let mut child = Command::new(&self.path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let input = child
            .stdin
            .take()
            .ok_or_else(|| std::io::Error::other("helper stdin missing"))?;
        let output = child
            .stdout
            .take()
            .ok_or_else(|| std::io::Error::other("helper stdout missing"))?;
        self.process = Some(Process {
            child,
            input,
            output: BufReader::new(output),
        });
        Ok(())
    }

    fn call_once(&mut self, mut request: Value) -> Result<Reply, String> {
        if self.process.is_none() {
            self.start().map_err(|e| e.to_string())?;
        }
        let id = self.next_id;
        self.next_id += 1;
        request["id"] = json!(id);
        let process = self.process.as_mut().expect("started above");
        writeln!(process.input, "{request}").map_err(|e| e.to_string())?;
        process.input.flush().map_err(|e| e.to_string())?;
        let mut line = String::new();
        if process
            .output
            .read_line(&mut line)
            .map_err(|e| e.to_string())?
            == 0
        {
            return Err("foundation helper exited".to_string());
        }
        let reply: Reply = serde_json::from_str(&line).map_err(|e| e.to_string())?;
        if reply.id != id {
            return Err("foundation helper response id mismatch".to_string());
        }
        Ok(reply)
    }

    fn call(&mut self, request: Value) -> Result<Reply, String> {
        match self.call_once(request.clone()) {
            Ok(reply) => Ok(reply),
            Err(_) => {
                if let Some(mut process) = self.process.take() {
                    let _ = process.child.kill();
                    let _ = process.child.wait();
                }
                self.call_once(request)
            }
        }
    }

    pub fn probe(&mut self) -> Availability {
        self.call(json!({"op": "probe"}))
            .ok()
            .and_then(|reply| reply.availability)
            .unwrap_or(Availability::HelperUnavailable)
    }

    pub fn generate(
        &mut self,
        task: &str,
        instructions: &str,
        prompt: &str,
        max_tokens: i32,
    ) -> Result<String, String> {
        let fields = if task == "memory_extraction" {
            json!([
                {"name": "intent", "description": prompts::FOUNDATION_EXTRACTION_INTENT_DESCRIPTION, "type": "strings"},
                {"name": "actions", "description": prompts::FOUNDATION_EXTRACTION_ACTIONS_DESCRIPTION, "type": "strings"},
                {"name": "memory_context", "description": prompts::FOUNDATION_EXTRACTION_CONTEXT_DESCRIPTION, "type": "string"},
                {"name": "activity_type", "description": prompts::FOUNDATION_EXTRACTION_ACTIVITY_DESCRIPTION, "type": "string"},
                {"name": "confidence", "description": prompts::FOUNDATION_EXTRACTION_CONFIDENCE_DESCRIPTION, "type": "string"}
            ])
        } else {
            json!([{"name": "text", "description": prompts::FOUNDATION_TEXT_FIELD_DESCRIPTION, "type": "string"}])
        };
        let reply = self.call(json!({
            "op": "generate",
            "instructions": instructions,
            "prompt": prompt,
            "root_description": prompts::FOUNDATION_SCHEMA_ROOT_DESCRIPTION,
            "fields": fields,
            "max_tokens": max_tokens,
        }))?;
        if !reply.ok {
            return Err(reply
                .error
                .unwrap_or_else(|| "generation_failed".to_string()));
        }
        let raw = reply.output.ok_or("helper returned no output")?;
        let value: Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
        if task == "memory_extraction" {
            let intent = value["intent"]
                .as_array()
                .ok_or("helper returned no intent list")?
                .iter()
                .filter(|item| item.is_string())
                .take(2)
                .collect::<Vec<_>>();
            let actions = value["actions"]
                .as_array()
                .ok_or("helper returned no actions list")?
                .iter()
                .filter(|item| item.is_string())
                .take(4)
                .collect::<Vec<_>>();
            let context = value["memory_context"]
                .as_str()
                .ok_or("helper returned no memory context")?;
            let activity = value["activity_type"]
                .as_str()
                .ok_or("helper returned no activity type")?;
            let confidence = value["confidence"]
                .as_str()
                .and_then(|text| text.parse::<f64>().ok())
                .or_else(|| value["confidence"].as_f64())
                .filter(|number| number.is_finite() && (0.0..=1.0).contains(number))
                .unwrap_or(0.0);
            return Ok(json!({
                "source_refs": {"intent": intent, "actions": actions},
                "memory_context": context,
                "activity_type": activity,
                "confidence": confidence,
            })
            .to_string());
        }
        value
            .get("text")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| "helper returned an invalid schema".to_string())
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        if let Some(mut process) = self.process.take() {
            let sent = writeln!(process.input, "{{\"id\":0,\"op\":\"exit\"}}")
                .and_then(|_| process.input.flush())
                .is_ok();
            if sent {
                for _ in 0..50 {
                    if matches!(process.child.try_wait(), Ok(Some(_))) {
                        return;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
            }
            let _ = process.child.kill();
            let _ = process.child.wait();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_uses_apple_for_supported_text_tasks_only() {
        assert_eq!(
            route("memory_extraction", true, true, false),
            Backend::AppleFoundation
        );
        assert_eq!(
            route("memory_snippet", true, true, false),
            Backend::AppleFoundation
        );
        assert_eq!(route("answer", true, true, false), Backend::AppleFoundation);
        assert_eq!(route("answer", true, true, true), Backend::LocalGguf);
        assert_eq!(
            route("memory_extraction", true, true, true),
            Backend::LocalGguf
        );
        assert_eq!(
            route("memory_snippet", true, true, true),
            Backend::LocalGguf
        );
        assert_eq!(
            route("query_expansion", true, true, false),
            Backend::LocalGguf
        );
        assert_eq!(
            route("vision_description", true, true, false),
            Backend::LocalGguf
        );
        assert_eq!(route("answer", false, true, false), Backend::LocalGguf);
        assert_eq!(route("answer", true, false, false), Backend::LocalGguf);
    }

    #[test]
    fn availability_and_setting_default_are_explicit() {
        assert!(apple_enabled(None, Availability::Available));
        assert!(!apple_enabled(None, Availability::ModelNotReady));
        assert!(!apple_enabled(Some(false), Availability::Available));
        assert!(!apple_enabled(None, Availability::AppleIntelligenceOff));
        assert!(!apple_enabled(None, Availability::UnsupportedDevice));
        assert!(!apple_enabled(None, Availability::UnsupportedOs));
        assert!(!apple_enabled(None, Availability::HelperUnavailable));
    }

    #[cfg(unix)]
    #[test]
    fn fake_probe_preserves_each_unavailable_state() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().expect("temp dir");
        let helper = dir.path().join("helper.py");
        for (wire, expected) in [
            ("model_not_ready", Availability::ModelNotReady),
            ("apple_intelligence_off", Availability::AppleIntelligenceOff),
            ("unsupported_device", Availability::UnsupportedDevice),
            ("unsupported_os", Availability::UnsupportedOs),
        ] {
            let script = format!(
                "#!/usr/bin/env python3\nimport json, sys\nrequest = json.loads(sys.stdin.readline())\nprint(json.dumps({{'id': request['id'], 'ok': True, 'availability': '{wire}'}}), flush=True)\n"
            );
            fs::write(&helper, script).expect("write helper");
            fs::set_permissions(&helper, fs::Permissions::from_mode(0o755)).expect("executable");
            let status = status_with_helper(dir.path(), Some(&helper));
            assert_eq!(status.availability, expected);
            assert!(!status.enabled);
        }
    }

    #[cfg(unix)]
    #[test]
    fn extraction_fields_become_host_schema_with_typed_confidence() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().expect("temp dir");
        let helper = dir.path().join("helper.py");
        fs::write(
            &helper,
            "#!/usr/bin/env python3\nimport json, sys\nrequest = json.loads(sys.stdin.readline())\nassert request['fields'][0]['name'] == 'intent' and request['fields'][0]['type'] == 'strings'\nfields = {'intent': ['1: Review the draft.'], 'actions': [], 'memory_context': 'Reviewed a draft.', 'activity_type': 'reviewing_agent_output', 'confidence': '0.7'}\nprint(json.dumps({'id': request['id'], 'ok': True, 'output': json.dumps(fields)}), flush=True)\n",
        )
        .expect("write helper");
        fs::set_permissions(&helper, fs::Permissions::from_mode(0o755)).expect("executable");

        let mut client = Client::new(helper);
        let raw = client
            .generate(
                "memory_extraction",
                "Synthetic instructions",
                "Synthetic screen",
                640,
            )
            .expect("host schema");
        let value: Value = serde_json::from_str(&raw).expect("valid JSON");
        assert_eq!(value["source_refs"]["intent"][0], "1: Review the draft.");
        assert_eq!(value["source_refs"]["actions"], json!([]));
        assert_eq!(value["memory_context"], "Reviewed a draft.");
        assert_eq!(value["confidence"], 0.7);
    }

    #[cfg(unix)]
    #[test]
    fn fake_helper_restarts_after_exit_and_decodes_generation() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().expect("temp dir");
        let helper = dir.path().join("helper.sh");
        fs::write(
            &helper,
            "#!/usr/bin/env python3\nimport json, sys\nrequest = json.loads(sys.stdin.readline())\nreply = {'id': request['id'], 'ok': True}\nif request['op'] == 'probe':\n    reply['availability'] = 'available'\nelse:\n    reply['output'] = json.dumps({'text': 'ok'})\nprint(json.dumps(reply), flush=True)\n",
        )
        .expect("write fake helper");
        fs::set_permissions(&helper, fs::Permissions::from_mode(0o755)).expect("executable");

        let mut client = Client::new(helper);
        assert_eq!(client.probe(), Availability::Available);
        assert_eq!(
            client.generate("answer", "Ground synthetic text.", "synthetic", 20),
            Ok("ok".to_string())
        );
    }

    #[cfg(unix)]
    #[test]
    fn apple_readiness_uses_probe_and_opt_out_without_a_gguf() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().expect("temp dir");
        let helper = dir.path().join("helper.sh");
        fs::write(
            &helper,
            "#!/bin/sh\nread -r request\nid=$(printf '%s' \"$request\" | sed -n 's/.*\"id\":\\([0-9]*\\).*/\\1/p')\nprintf '{\"id\":%s,\"ok\":true,\"availability\":\"available\"}\\n' \"$id\"\n",
        )
        .expect("write fake helper");
        fs::set_permissions(&helper, fs::Permissions::from_mode(0o755)).expect("executable");

        assert!(status_with_helper(dir.path(), Some(&helper)).enabled);
        write_setting(dir.path(), false).expect("save setting");
        assert!(!status_with_helper(dir.path(), Some(&helper)).enabled);
    }

    #[cfg(unix)]
    #[test]
    fn client_exits_helper_cleanly() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().expect("temp dir");
        let helper = dir.path().join("helper.py");
        let marker = dir.path().join("exited");
        let script = format!(
            "#!/usr/bin/env python3\nimport json, sys\nfor line in sys.stdin:\n    request = json.loads(line)\n    if request['op'] == 'exit':\n        open({:?}, 'w').close()\n        print(json.dumps({{'id': request['id'], 'ok': True}}), flush=True)\n        break\n    print(json.dumps({{'id': request['id'], 'ok': True, 'availability': 'available'}}), flush=True)\n",
            marker.display().to_string(),
        );
        fs::write(&helper, script).expect("write helper");
        fs::set_permissions(&helper, fs::Permissions::from_mode(0o755)).expect("executable");

        let mut client = Client::new(helper);
        assert_eq!(client.probe(), Availability::Available);
        drop(client);
        assert!(
            marker.exists(),
            "helper received exit before client shutdown"
        );
    }
}
