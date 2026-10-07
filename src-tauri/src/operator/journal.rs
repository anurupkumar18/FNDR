//! Local, append-only record of every Notch Do action, for auditing a run.
//! Typed text and values are stored as a hash and a length, never verbatim.

use serde::Serialize;
use serde_json::Value;
use std::path::PathBuf;

use super::policy::Risk;

#[derive(Debug, Clone, Serialize)]
pub struct JournalEntry {
    pub at: String,
    pub run_id: String,
    pub step: Option<usize>,
    pub tool: String,
    pub args: Value,
    pub risk: Option<Risk>,
    /// `ran`, `approved`, `declined`, `blocked`, `ok` or `failed`.
    pub outcome: String,
    pub detail: Option<String>,
}

pub struct Journal {
    path: PathBuf,
}

/// Arguments with sensitive values replaced by `sha256:<prefix> (<n> chars)`.
pub fn redact(args: &Value) -> Value {
    let Some(object) = args.as_object() else {
        return args.clone();
    };
    let redacted = object
        .iter()
        .map(|(key, value)| {
            let replaced = match (key.as_str(), value.as_str()) {
                ("text" | "value", Some(text)) => Value::String(fingerprint(text)),
                ("url", Some(url)) => {
                    let origin_end = url
                        .find("://")
                        .map(|scheme| scheme + 3)
                        .and_then(|start| url[start..].find(['/', '?']).map(|end| start + end))
                        .unwrap_or(url.len());
                    let rest = &url[origin_end..];
                    if rest.is_empty() || rest == "/" {
                        value.clone()
                    } else {
                        Value::String(format!("{}/ {}", &url[..origin_end], fingerprint(rest)))
                    }
                }
                _ => value.clone(),
            };
            (key.clone(), replaced)
        })
        .collect();
    Value::Object(redacted)
}

fn fingerprint(text: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(text.as_bytes());
    let hex: String = digest
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect();
    format!("sha256:{hex} ({} chars)", text.chars().count())
}

impl Journal {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn append(&self, entry: &JournalEntry) -> Result<(), String> {
        use std::io::Write;
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Could not prepare the action log: {e}"))?;
        }
        let line = serde_json::to_string(entry).map_err(|e| e.to_string())?;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|e| format!("Could not open the action log: {e}"))?;
        writeln!(file, "{line}").map_err(|e| format!("Could not write the action log: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn typed_text_is_hashed_and_structure_kept() {
        let redacted =
            redact(&json!({"app": "Spotify", "text": "Blinding Lights", "element_index": "4"}));
        assert_eq!(redacted["app"], "Spotify");
        assert_eq!(redacted["element_index"], "4");
        let text = redacted["text"].as_str().unwrap();
        assert!(
            text.starts_with("sha256:") && text.ends_with("(15 chars)"),
            "{text}"
        );
        assert!(!text.contains("Blinding"));
    }

    #[test]
    fn urls_keep_only_their_origin() {
        let redacted = redact(&json!({"url": "https://www.google.com/search?q=private+thing"}));
        let url = redacted["url"].as_str().unwrap();
        assert!(url.starts_with("https://www.google.com/ sha256:"), "{url}");
        assert!(!url.contains("private"));
    }

    #[test]
    fn appends_one_json_line_per_action() {
        let dir = tempfile::tempdir().unwrap();
        let journal = Journal::new(dir.path().join("nested").join("journal.jsonl"));
        for outcome in ["ran", "blocked"] {
            journal
                .append(&JournalEntry {
                    at: "2026-10-06T00:00:00Z".into(),
                    run_id: "r1".into(),
                    step: Some(1),
                    tool: "type_text".into(),
                    args: redact(&json!({"text": "secret"})),
                    risk: Some(Risk::Confirm),
                    outcome: outcome.into(),
                    detail: None,
                })
                .unwrap();
        }
        let written =
            std::fs::read_to_string(dir.path().join("nested").join("journal.jsonl")).unwrap();
        let lines: Vec<Value> = written
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[1]["outcome"], "blocked");
        assert_eq!(lines[0]["risk"], "confirm");
        assert!(!written.contains("secret"));
    }
}
