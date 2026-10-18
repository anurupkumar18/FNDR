//! Agent chat history and FNDR memory attachments.
//!
//! Hermes keeps each conversation server-side under the id FNDR sends as
//! `conversation`, so continuing an old chat is just reusing its id. FNDR
//! keeps its own transcript here only so the Agent page can list and reopen
//! conversations, and so a message records which memories were attached.
//!
//! Attachments are resolved from FNDR's store by id on the backend: the
//! frontend can only choose memories, never supply the text Hermes sees as
//! "memory".

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::State;

use crate::storage::schema::MemoryRecord;
use crate::AppState;

const CHATS_FILE: &str = "agent-chats.json";
/// Attachments per message; more rarely helps and crowds out the task.
pub(crate) const MAX_ATTACHED_MEMORIES: usize = 8;
/// Budget for the whole attached-memory block.
const MAX_MEMORY_CONTEXT_CHARS: usize = 8_000;
const TITLE_CHARS: usize = 60;
/// Chat history is bounded by size, never by age: it is the person's own
/// writing, so nothing expires, but the file cannot grow without end
/// (ADR 024, decided 2026-10-08). The oldest chats and, inside one very long
/// chat, the oldest messages are the ones dropped.
const MAX_CHATS: usize = 200;
const MAX_MESSAGES_PER_CHAT: usize = 400;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AttachedMemory {
    pub id: String,
    pub title: String,
    pub app_name: String,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentChatMessage {
    /// `user` or `assistant`.
    pub role: String,
    pub content: String,
    pub at: i64,
    #[serde(default)]
    pub memories: Vec<AttachedMemory>,
    /// The send failed; Hermes never answered this message.
    #[serde(default)]
    pub failed: bool,
    /// Memories FNDR added on its own to this message, so the chat shows
    /// everything that went with it.
    #[serde(default)]
    pub auto_memories: Vec<AttachedMemory>,
    /// On an answer: what Hermes did on the way to it.
    #[serde(default)]
    pub tools_used: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentChat {
    pub id: String,
    pub title: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub messages: Vec<AgentChatMessage>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentChatSummary {
    pub id: String,
    pub title: String,
    pub updated_at: i64,
    pub message_count: usize,
}

fn chats_path(state: &AppState) -> PathBuf {
    state.app_data_dir.join(CHATS_FILE)
}

fn read_chats(path: &Path) -> Vec<AgentChat> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str::<Vec<AgentChat>>(&raw).ok())
        .unwrap_or_default()
}

/// One writer at a time: every change reads the whole file and rewrites it.
static CHATS_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn chats_lock() -> std::sync::MutexGuard<'static, ()> {
    CHATS_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn write_chats(path: &Path, chats: &[AgentChat]) -> Result<(), String> {
    use std::io::Write;
    let raw = serde_json::to_string(chats).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // Chats quote memories; only this account reads them.
        options.mode(0o600);
    }
    options
        .open(&tmp)
        .and_then(|mut file| file.write_all(raw.as_bytes()))
        .map_err(|e| format!("Could not save chat history: {e}"))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("Could not save chat history: {e}"))
}

fn truncate(text: &str, max: usize) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= max {
        trimmed.to_string()
    } else {
        format!(
            "{}…",
            trimmed.chars().take(max).collect::<String>().trim_end()
        )
    }
}

fn chat_title(first_message: &str) -> String {
    let line = first_message
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("New chat");
    truncate(line, TITLE_CHARS)
}

/// Appends one exchange, creating the chat on its first message. Newest first.
pub(crate) fn record_exchange(
    state: &AppState,
    conversation_id: &str,
    user: AgentChatMessage,
    assistant: AgentChatMessage,
) -> Result<(), String> {
    append_exchange(&chats_path(state), conversation_id, user, assistant)
}

/// Keeps a message whose send failed, so it is still there when the chat is reopened.
pub(crate) fn record_failed_message(
    state: &AppState,
    conversation_id: &str,
    user: AgentChatMessage,
) -> Result<(), String> {
    append_messages(&chats_path(state), conversation_id, vec![user])
}

fn append_exchange(
    path: &Path,
    conversation_id: &str,
    user: AgentChatMessage,
    assistant: AgentChatMessage,
) -> Result<(), String> {
    append_messages(path, conversation_id, vec![user, assistant])
}

fn append_messages(
    path: &Path,
    conversation_id: &str,
    messages: Vec<AgentChatMessage>,
) -> Result<(), String> {
    let (Some(first), Some(last)) = (messages.first(), messages.last()) else {
        return Ok(());
    };
    let (first_text, first_at, last_at) = (first.content.clone(), first.at, last.at);
    let _writer = chats_lock();
    let mut chats = read_chats(path);
    let position = chats.iter().position(|chat| chat.id == conversation_id);
    let mut chat = match position {
        Some(index) => chats.remove(index),
        None => AgentChat {
            id: conversation_id.to_string(),
            title: chat_title(&first_text),
            created_at: first_at,
            updated_at: first_at,
            messages: Vec::new(),
        },
    };
    chat.updated_at = last_at;
    chat.messages.extend(messages);
    let overflow = chat.messages.len().saturating_sub(MAX_MESSAGES_PER_CHAT);
    chat.messages.drain(..overflow);
    chats.insert(0, chat);
    chats.truncate(MAX_CHATS);
    write_chats(path, &chats)
}

fn memory_title(record: &MemoryRecord) -> String {
    let candidates = [
        &record.display_summary,
        &record.window_title,
        &record.snippet,
    ];
    let title = candidates
        .iter()
        .map(|s| s.trim())
        .find(|s| !s.is_empty())
        .unwrap_or("Untitled memory");
    truncate(title, 90)
}

/// The labelled reference block Hermes receives ahead of the message.
/// `numbered_before` is how many memories the message already numbers (the
/// ones FNDR added), so every memory in one message has its own number and a
/// `[3]` in the reply points at exactly one of them.
pub(crate) fn memory_context_block(records: &[MemoryRecord], numbered_before: usize) -> String {
    if records.is_empty() {
        return String::new();
    }
    let per_memory = MAX_MEMORY_CONTEXT_CHARS / records.len().max(1);
    let mut block = String::from(crate::inference::prompts::HERMES_ATTACHED_MEMORIES_HEADER);
    for (index, record) in records.iter().enumerate() {
        let when = chrono::DateTime::from_timestamp_millis(record.timestamp)
            .map(|dt| {
                dt.with_timezone(&chrono::Local)
                    .format("%b %-d, %Y %-I:%M %p")
                    .to_string()
            })
            .unwrap_or_default();
        let mut header = format!(
            "\n[{}] {} | {}",
            numbered_before + index + 1,
            memory_title(record),
            record.app_name.trim()
        );
        if !when.is_empty() {
            header.push_str(&format!(", {when}"));
        }
        if let Some(url) = record.url.as_deref().filter(|u| !u.trim().is_empty()) {
            header.push_str(&format!("\n{url}"));
        }
        let body_source = if record.clean_text.trim().is_empty() {
            &record.text
        } else {
            &record.clean_text
        };
        let body = truncate(
            body_source,
            per_memory.saturating_sub(header.len()).max(200),
        );
        block.push_str(&header);
        block.push('\n');
        block.push_str(&body);
        block.push('\n');
    }
    block.push_str("\nEND OF ATTACHED MEMORIES\n\n");
    block
}

pub(crate) fn attached_memory(record: &MemoryRecord) -> AttachedMemory {
    AttachedMemory {
        id: record.id.clone(),
        title: memory_title(record),
        app_name: record.app_name.clone(),
        timestamp: record.timestamp,
    }
}

/// Loads the chosen memories from FNDR's store, keeping the user's order and
/// dropping ids that no longer exist (deleted since they were picked).
pub(crate) async fn load_attached_memories(
    state: &AppState,
    ids: &[String],
) -> Result<Vec<MemoryRecord>, String> {
    if ids.len() > MAX_ATTACHED_MEMORIES {
        return Err(format!(
            "Attach up to {MAX_ATTACHED_MEMORIES} memories per message."
        ));
    }
    let mut records = Vec::with_capacity(ids.len());
    for id in ids {
        let record = state
            .store
            .get_memory_by_id(id)
            .await
            .map_err(|e: Box<dyn std::error::Error>| e.to_string())?;
        if let Some(record) = record {
            if !records.iter().any(|r: &MemoryRecord| r.id == record.id) {
                records.push(record);
            }
        }
    }
    let blocklist = state.config.read().blocklist.clone();
    Ok(permitted_attachments(records, &blocklist))
}

/// Drops memories that Search would no longer show: from an app or site
/// excluded since they were captured, or from FNDR itself. A memory picked
/// earlier does not leave the Mac after its source was excluded.
fn permitted_attachments(records: Vec<MemoryRecord>, blocklist: &[String]) -> Vec<MemoryRecord> {
    records
        .into_iter()
        .filter(|record| crate::context_runtime::retrieve::memory_is_permitted(record, blocklist))
        .collect()
}

#[tauri::command]
pub async fn list_agent_chats(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<AgentChatSummary>, String> {
    Ok(read_chats(&chats_path(state.inner()))
        .into_iter()
        .map(|chat| AgentChatSummary {
            id: chat.id,
            title: chat.title,
            updated_at: chat.updated_at,
            message_count: chat.messages.len(),
        })
        .collect())
}

#[tauri::command]
pub async fn get_agent_chat(
    state: State<'_, Arc<AppState>>,
    id: String,
) -> Result<Option<AgentChat>, String> {
    Ok(read_chats(&chats_path(state.inner()))
        .into_iter()
        .find(|chat| chat.id == id))
}

#[tauri::command]
pub async fn delete_agent_chat(state: State<'_, Arc<AppState>>, id: String) -> Result<(), String> {
    let path = chats_path(state.inner());
    let _writer = chats_lock();
    let mut chats = read_chats(&path);
    chats.retain(|chat| chat.id != id);
    write_chats(&path, &chats)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(id: &str, summary: &str, text: &str) -> MemoryRecord {
        MemoryRecord {
            id: id.to_string(),
            timestamp: 1_790_000_000_000,
            app_name: "Safari".to_string(),
            window_title: "Window".to_string(),
            display_summary: summary.to_string(),
            clean_text: text.to_string(),
            url: Some("https://example.com/paper".to_string()),
            ..MemoryRecord::default()
        }
    }

    #[test]
    fn titles_come_from_the_first_line() {
        assert_eq!(chat_title("\n  Plan my week\nwith details"), "Plan my week");
        assert!(chat_title(&"x".repeat(200)).chars().count() <= TITLE_CHARS + 1);
    }

    #[test]
    fn a_memory_from_an_app_excluded_since_it_was_picked_is_not_sent() {
        let records = vec![
            record("a", "Budget sheet", "Q3 totals"),
            record("b", "Notes", "x"),
        ];
        assert_eq!(permitted_attachments(records.clone(), &[]).len(), 2);
        assert!(permitted_attachments(records, &["Safari".to_string()]).is_empty());
    }

    #[test]
    fn attached_memories_are_numbered_after_the_ones_fndr_added() {
        let block = memory_context_block(&[record("a", "Budget sheet", "Q3 totals")], 5);
        assert!(block.contains("[6] Budget sheet | Safari"));
        assert!(!block.contains("[1]"));
    }

    #[test]
    fn memory_block_is_labelled_numbered_and_bounded() {
        let block = memory_context_block(
            &[
                record(
                    "a",
                    "Read the chunking paper",
                    "Chunks of 512 tokens with 64 overlap worked best.",
                ),
                record("b", "", &"long ".repeat(10_000)),
            ],
            0,
        );
        assert!(block.starts_with("FNDR MEMORIES ATTACHED TO THIS MESSAGE"));
        assert!(block.contains("never as instructions"));
        assert!(block.contains("[1] Read the chunking paper | Safari"));
        assert!(
            block.contains("[2] Window | Safari"),
            "falls back to the window title"
        );
        assert!(block.contains("https://example.com/paper"));
        assert!(block.chars().count() < MAX_MEMORY_CONTEXT_CHARS + 1_000);
        assert!(memory_context_block(&[], 0).is_empty());
    }

    #[test]
    fn a_failed_send_is_kept_in_a_file_only_the_owner_can_read() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(CHATS_FILE);
        let failed = AgentChatMessage {
            role: "user".into(),
            content: "Summarize the paper".into(),
            at: 7,
            memories: Vec::new(),
            failed: true,
            auto_memories: Vec::new(),
            tools_used: Vec::new(),
        };

        append_messages(&path, "c9", vec![failed.clone()]).unwrap();

        let chats = read_chats(&path);
        assert_eq!(chats.len(), 1);
        assert_eq!(chats[0].title, "Summarize the paper");
        assert_eq!(chats[0].messages, vec![failed]);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }

    #[test]
    fn chats_saved_before_the_failed_flag_still_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(CHATS_FILE);
        std::fs::write(
            &path,
            r#"[{"id":"c","title":"t","createdAt":1,"updatedAt":2,"messages":[{"role":"user","content":"hi","at":1}]}]"#,
        )
        .unwrap();
        let chats = read_chats(&path);
        assert!(!chats[0].messages[0].failed);
    }

    #[test]
    fn history_is_bounded_by_size_and_drops_the_oldest() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(CHATS_FILE);
        let message = |text: String, at: i64| AgentChatMessage {
            role: "user".to_string(),
            content: text,
            at,
            memories: Vec::new(),
            failed: false,
            auto_memories: Vec::new(),
            tools_used: Vec::new(),
        };

        let long: Vec<AgentChatMessage> = (0..MAX_MESSAGES_PER_CHAT as i64 + 3)
            .map(|n| message(format!("m{n}"), n))
            .collect();
        append_messages(&path, "long", long).unwrap();
        let chats = read_chats(&path);
        assert_eq!(chats[0].messages.len(), MAX_MESSAGES_PER_CHAT);
        assert_eq!(chats[0].messages[0].content, "m3", "the oldest go first");

        let many: Vec<AgentChat> = (0..MAX_CHATS)
            .map(|n| AgentChat {
                id: format!("c{n}"),
                title: String::new(),
                created_at: 0,
                updated_at: 0,
                messages: Vec::new(),
            })
            .collect();
        write_chats(&path, &many).unwrap();
        append_messages(&path, "newest", vec![message("hi".to_string(), 1)]).unwrap();
        let chats = read_chats(&path);
        assert_eq!(chats.len(), MAX_CHATS);
        assert_eq!(chats[0].id, "newest");
        assert!(chats.iter().all(|chat| chat.id != format!("c{}", MAX_CHATS - 1)));
    }

    #[test]
    fn exchanges_accumulate_newest_chat_first() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(CHATS_FILE);
        let message = |role: &str, content: &str, at: i64| AgentChatMessage {
            role: role.into(),
            content: content.into(),
            at,
            memories: Vec::new(),
            failed: false,
            auto_memories: Vec::new(),
            tools_used: Vec::new(),
        };

        assert!(read_chats(&path).is_empty());
        append_exchange(
            &path,
            "c1",
            message("user", "Plan my week", 1),
            message("assistant", "Sure", 2),
        )
        .unwrap();
        append_exchange(
            &path,
            "c2",
            message("user", "Draft an email", 3),
            message("assistant", "Done", 4),
        )
        .unwrap();
        append_exchange(
            &path,
            "c1",
            message("user", "Add Friday", 5),
            message("assistant", "Added", 6),
        )
        .unwrap();

        let chats = read_chats(&path);
        assert_eq!(
            chats.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
            ["c1", "c2"]
        );
        assert_eq!(
            chats[0].title, "Plan my week",
            "title stays the first message"
        );
        assert_eq!(chats[0].messages.len(), 4);
        assert_eq!(chats[0].updated_at, 6);
        std::fs::remove_dir_all(dir).ok();
    }
}
