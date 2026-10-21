use crate::import::*;
use chrono::{NaiveDate, Utc};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Component, Path, PathBuf};

const MAX_JSON_BYTES: u64 = 32 * 1024 * 1024;
const MAX_TEXT_CHARS: usize = 20_000;
const SKIP_DIRS: &[&str] = &["node_modules", "target", "Library", "secrets"];

pub struct SlackSource {
    pub path: PathBuf,
}

struct Entry {
    path: String,
    bytes: Vec<u8>,
}

impl SlackSource {
    fn entries(&self) -> Result<Vec<Entry>, ImportError> {
        if self.path.is_dir() {
            let mut out = Vec::new();
            collect_dir(&self.path, &self.path, &mut out)?;
            return Ok(out);
        }
        let file = File::open(&self.path)?;
        let mut zip =
            zip::ZipArchive::new(file).map_err(|e| ImportError::InvalidData(e.to_string()))?;
        let mut out = Vec::new();
        for i in 0..zip.len() {
            let mut item = zip
                .by_index(i)
                .map_err(|e| ImportError::InvalidData(e.to_string()))?;
            let name = item.name().to_string();
            if item.is_dir() || !safe_json_path(&name) || item.size() > MAX_JSON_BYTES {
                continue;
            }
            let mut bytes = Vec::new();
            (&mut item)
                .take(MAX_JSON_BYTES + 1)
                .read_to_end(&mut bytes)?;
            if bytes.len() as u64 > MAX_JSON_BYTES {
                continue;
            }
            out.push(Entry { path: name, bytes });
        }
        Ok(out)
    }
}

impl ImportSource for SlackSource {
    fn kind(&self) -> ImportKind {
        ImportKind::Slack
    }
    fn label(&self) -> &'static str {
        "Slack export"
    }
    fn permissions_needed(&self) -> Vec<Permission> {
        vec![Permission::Files]
    }
    fn scan(
        &self,
        since: Option<Cursor>,
        window_days: u32,
        limit: usize,
    ) -> Result<ScanBatch, ImportError> {
        let entries = self.entries()?;
        let mut users = HashMap::new();
        let mut channels = HashMap::new();
        let mut participants: HashMap<String, Vec<String>> = HashMap::new();
        let mut private = HashSet::new();
        let mut direct = HashSet::new();
        let mut domain = None;
        for entry in &entries {
            let base = entry.path.rsplit('/').next().unwrap_or("");
            let value: Value = match serde_json::from_slice(&entry.bytes) {
                Ok(value) => value,
                Err(_)
                    if matches!(
                        base,
                        "users.json" | "channels.json" | "groups.json" | "dms.json" | "mpims.json"
                    ) =>
                {
                    return Err(ImportError::InvalidData(format!("invalid {base}")))
                }
                Err(_) => continue,
            };
            match base {
                "users.json" => {
                    for user in value.as_array().into_iter().flatten() {
                        if let Some(id) = user["id"].as_str() {
                            let name = user["profile"]["display_name"]
                                .as_str()
                                .filter(|s| !s.is_empty())
                                .or_else(|| user["real_name"].as_str())
                                .or_else(|| user["name"].as_str())
                                .unwrap_or(id);
                            users.insert(id.to_string(), name.to_string());
                        }
                    }
                }
                "channels.json" | "groups.json" | "dms.json" | "mpims.json" => {
                    for channel in value.as_array().into_iter().flatten() {
                        let id = channel["id"].as_str().unwrap_or("");
                        let name = channel["name"].as_str().unwrap_or(id);
                        if !id.is_empty() {
                            channels.insert(id.to_string(), name.to_string());
                        }
                        if let Some(members) = channel["members"].as_array() {
                            participants.insert(
                                name.to_string(),
                                members
                                    .iter()
                                    .filter_map(|m| m.as_str().map(str::to_string))
                                    .collect(),
                            );
                        }
                        if base == "groups.json" || channel["is_private"].as_bool() == Some(true) {
                            private.insert(name.to_string());
                        }
                        if base == "dms.json" || base == "mpims.json" {
                            direct.insert(name.to_string());
                        }
                    }
                }
                "team.json" | "workspace.json" => {
                    domain = value["domain"]
                        .as_str()
                        .filter(|s| !s.is_empty())
                        .map(str::to_string);
                }
                _ => {}
            }
        }
        let mut all = Vec::new();
        for entry in &entries {
            let Some((folder, file)) = entry.path.rsplit_once('/') else {
                continue;
            };
            let Some(day) = file.strip_suffix(".json") else {
                continue;
            };
            let Ok(date) = NaiveDate::parse_from_str(day, "%Y-%m-%d") else {
                continue;
            };
            if (Utc::now().date_naive() - date).num_days() > window_days as i64 {
                continue;
            }
            let messages: Vec<Value> = serde_json::from_slice(&entry.bytes).map_err(|_| {
                ImportError::InvalidData(format!("invalid message file: {}", entry.path))
            })?;
            let channel = folder.rsplit('/').next().unwrap_or(folder);
            let mut grouped: BTreeMap<String, Vec<(String, String, String)>> = BTreeMap::new();
            let mut people = HashSet::new();
            let mut first_ts = None;
            for msg in messages {
                if msg["subtype"].as_str().is_some_and(|s| {
                    matches!(
                        s,
                        "channel_join"
                            | "channel_leave"
                            | "group_join"
                            | "group_leave"
                            | "bot_message"
                            | "message_deleted"
                    )
                }) || msg["bot_id"].is_string()
                {
                    continue;
                }
                let Some(raw) = msg["text"].as_str() else {
                    continue;
                };
                if raw.trim().is_empty() {
                    continue;
                }
                let ts = msg["ts"].as_str().unwrap_or("").to_string();
                if ts.is_empty() {
                    continue;
                }
                let id = msg["user"].as_str().unwrap_or("Unknown");
                let author = users.get(id).map(String::as_str).unwrap_or(id).to_string();
                people.insert(author.clone());
                let text = render_text(raw, &users, &channels);
                let mut text = text;
                if let Some(files) = msg["files"].as_array() {
                    for file in files {
                        if let Some(name) = file["name"].as_str() {
                            text.push_str(" [file: ");
                            text.push_str(name);
                            text.push(']');
                        }
                    }
                }
                let root = msg["thread_ts"].as_str().unwrap_or(&ts).to_string();
                first_ts = Some(first_ts.map_or_else(|| ts.clone(), |v: String| v.min(ts.clone())));
                grouped.entry(root).or_default().push((ts, author, text));
            }
            if grouped.is_empty() {
                continue;
            }
            let mut text = String::new();
            for thread in grouped.values_mut() {
                thread.sort_by(|a, b| a.0.cmp(&b.0));
                for (_, author, message) in thread {
                    text.push_str(author);
                    text.push_str(": ");
                    text.push_str(message);
                    text.push('\n');
                    if text.chars().count() >= MAX_TEXT_CHARS {
                        break;
                    }
                }
                if text.chars().count() >= MAX_TEXT_CHARS {
                    break;
                }
            }
            text = text.chars().take(MAX_TEXT_CHARS).collect();
            if private.contains(channel) {
                people.insert("Private channel".to_string());
            }
            if let Some(members) = participants.get(channel) {
                for id in members {
                    people.insert(users.get(id).cloned().unwrap_or_else(|| id.clone()));
                }
            }
            if direct.contains(channel) || channel.starts_with('D') {
                people.insert("Direct message".to_string());
            }
            let mut people: Vec<_> = people.into_iter().collect();
            people.sort();
            let timestamp_ms = first_ts
                .as_deref()
                .and_then(|s| s.parse::<f64>().ok())
                .map(|s| (s * 1000.0) as i64)
                .unwrap_or_else(|| {
                    date.and_hms_opt(0, 0, 0)
                        .unwrap()
                        .and_utc()
                        .timestamp_millis()
                });
            let target = domain
                .as_ref()
                .map(|d| {
                    format!(
                        "https://{d}.slack.com/archives/{}/p{}",
                        channel_id(channel, &channels),
                        first_ts.as_deref().unwrap_or("").replace('.', "")
                    )
                })
                .unwrap_or_else(|| format!("{}#{}:{}", self.path.display(), channel, day));
            all.push(ImportItem {
                kind: ImportKind::Slack,
                external_id: format!("{}:{}:{}", self.path.display(), channel, day),
                timestamp_ms,
                title: format!("#{channel} on {day}"),
                text,
                reopen_target: Some(target.clone()),
                app_label: "Slack export".to_string(),
                people,
                url: target.starts_with("https://").then_some(target),
            });
        }
        all.sort_by(|a, b| a.external_id.cmp(&b.external_id));
        let offset = since
            .and_then(|c| c.0.parse::<usize>().ok())
            .unwrap_or(0)
            .min(all.len());
        let end = offset.saturating_add(limit).min(all.len());
        let done = end == all.len();
        Ok(ScanBatch {
            items: all.drain(offset..end).collect(),
            next: (!done).then(|| Cursor(end.to_string())),
            done,
        })
    }
}

fn channel_id(name: &str, channels: &HashMap<String, String>) -> String {
    channels
        .iter()
        .find(|(_, n)| n.as_str() == name)
        .map(|(id, _)| id.clone())
        .unwrap_or_else(|| name.to_string())
}

fn safe_json_path(path: &str) -> bool {
    let p = Path::new(path);
    p.extension().and_then(|s| s.to_str()) == Some("json")
        && !p.components().any(|c| !matches!(c, Component::Normal(_)))
        && !path
            .split('/')
            .any(|c| c.starts_with('.') || SKIP_DIRS.contains(&c))
}

fn collect_dir(root: &Path, current: &Path, out: &mut Vec<Entry>) -> Result<(), ImportError> {
    for item in fs::read_dir(current)? {
        let item = item?;
        let path = item.path();
        let relative = path
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        if !safe_json_path(&relative) && !path.is_dir() {
            continue;
        }
        if item.file_type()?.is_symlink() {
            continue;
        }
        if path.is_dir() {
            if !item.file_name().to_string_lossy().starts_with('.')
                && !relative.split('/').any(|part| SKIP_DIRS.contains(&part))
            {
                collect_dir(root, &path, out)?;
            }
        } else if item.metadata()?.len() <= MAX_JSON_BYTES && safe_json_path(&relative) {
            out.push(Entry {
                path: relative,
                bytes: fs::read(path)?,
            });
        }
    }
    Ok(())
}

fn render_text(
    text: &str,
    users: &HashMap<String, String>,
    channels: &HashMap<String, String>,
) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find('<') {
        out.push_str(&rest[..start]);
        let Some(end) = rest[start..].find('>') else {
            rest = &rest[start..];
            break;
        };
        let token = &rest[start + 1..start + end];
        if let Some(id) = token.strip_prefix('@') {
            out.push('@');
            out.push_str(users.get(id).map(String::as_str).unwrap_or(id));
        } else if let Some(value) = token.strip_prefix('#') {
            let (id, label) = value.split_once('|').unwrap_or((value, ""));
            out.push('#');
            out.push_str(if label.is_empty() {
                channels.get(id).map(String::as_str).unwrap_or(id)
            } else {
                label
            });
        } else if token.starts_with("http://") || token.starts_with("https://") {
            let (url, label) = token.split_once('|').unwrap_or((token, token));
            out.push_str(label);
            if label != url {
                out.push_str(" (");
                out.push_str(url);
                out.push(')');
            }
        } else {
            out.push('<');
            out.push_str(token);
            out.push('>');
        }
        rest = &rest[start + end + 1..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn groups_day_threads_and_resolves_names() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("users.json"),
            r#"[{"id":"U1","name":"old","profile":{"display_name":"Zoë"}}]"#,
        )
        .unwrap();
        fs::write(
            dir.path().join("channels.json"),
            r#"[{"id":"C1","name":"project","is_private":true}]"#,
        )
        .unwrap();
        fs::create_dir(dir.path().join("project")).unwrap();
        fs::write(dir.path().join(format!("project/{}.json", Utc::now().date_naive())), r#"[{"ts":"1791936002.000001","user":"U2","thread_ts":"1791936001.000001","text":"reply"},{"ts":"1791936001.000001","user":"U1","text":"Hi <@U2> see <#C1|project> <https://example.com|design>"},{"ts":"1791936003.000001","subtype":"channel_join","text":"joined"}]"#).unwrap();
        let batch = SlackSource {
            path: dir.path().to_owned(),
        }
        .scan(None, 30, 10)
        .unwrap();
        assert_eq!(batch.items.len(), 1);
        let item = &batch.items[0];
        assert!(item
            .text
            .contains("Zoë: Hi @U2 see #project design (https://example.com)"));
        assert!(item.text.contains("U2: reply"));
        assert!(item.text.find("Zoë: Hi").unwrap() < item.text.find("U2: reply").unwrap());
        assert!(!item.text.contains("joined"));
        assert!(item.people.iter().any(|p| p == "Private channel"));
    }

    #[test]
    fn zip_caps_text_and_omits_file_contents() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("slack.zip");
        let file = File::create(&path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        zip.start_file("team.json", options).unwrap();
        zip.write_all(br#"{"domain":"example"}"#).unwrap();
        zip.start_file("channels.json", options).unwrap();
        zip.write_all(br#"[{"id":"C1","name":"project"}]"#).unwrap();
        zip.start_file(format!("project/{}.json", Utc::now().date_naive()), options)
            .unwrap();
        let message = serde_json::json!([{"ts":"1791936001.000001","user":"U1","text":"x".repeat(25_000),"files":[{"name":"report.txt","content":"private content"}]}]);
        zip.write_all(message.to_string().as_bytes()).unwrap();
        zip.finish().unwrap();
        let item = SlackSource { path }
            .scan(None, 30, 10)
            .unwrap()
            .items
            .remove(0);
        assert_eq!(item.text.chars().count(), 20_000);
        assert!(!item.text.contains("private content"));
        assert_eq!(
            item.url
                .as_deref()
                .unwrap()
                .starts_with("https://example.slack.com/archives/C1/"),
            true
        );
    }

    #[test]
    fn empty_and_corrupt_export() {
        let dir = tempfile::tempdir().unwrap();
        assert!(SlackSource {
            path: dir.path().to_owned()
        }
        .scan(None, 30, 10)
        .unwrap()
        .items
        .is_empty());
        fs::write(dir.path().join("users.json"), "not json").unwrap();
        assert!(matches!(
            SlackSource {
                path: dir.path().to_owned()
            }
            .scan(None, 30, 10),
            Err(ImportError::InvalidData(_))
        ));
    }

    #[test]
    fn direct_message_keeps_file_name_and_skips_missing_days() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("users.json"),
            r#"[{"id":"U2","profile":{"display_name":"Ari"}}]"#,
        )
        .unwrap();
        fs::write(
            dir.path().join("dms.json"),
            r#"[{"id":"D1","name":"conversation","members":["U1","U2"]}]"#,
        )
        .unwrap();
        fs::create_dir(dir.path().join("conversation")).unwrap();
        let day = Utc::now().date_naive();
        fs::write(dir.path().join(format!("conversation/{day}.json")), r#"[{"ts":"1791936001.000001","user":"U1","text":"sent the file","files":[{"name":"résumé.pdf","content":"hidden body"}]}]"#).unwrap();
        fs::write(
            dir.path().join("conversation/2020-01-01.json"),
            r#"[{"ts":"1577836801.000001","text":"old day"}]"#,
        )
        .unwrap();
        let mut batch = SlackSource {
            path: dir.path().to_owned(),
        }
        .scan(None, 30, 10)
        .unwrap();
        assert_eq!(batch.items.len(), 1);
        let item = batch.items.remove(0);
        assert!(item.text.contains("résumé.pdf"));
        assert!(!item.text.contains("hidden body"));
        assert!(item.people.iter().any(|p| p == "Direct message"));
        assert!(item.people.iter().any(|p| p == "Ari"));
        assert!(item.reopen_target.unwrap().contains("conversation"));
    }

    #[test]
    fn unreadable_export_returns_io_error() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("blocked");
        fs::create_dir(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o000)).unwrap();
        let result = SlackSource { path: path.clone() }.scan(None, 30, 10);
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        assert!(matches!(
            result,
            Err(ImportError::PermissionDenied(Permission::Files))
        ));
    }
}
