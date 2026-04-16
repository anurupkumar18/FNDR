//! Task extraction and management helpers.

pub use crate::store::{Task, TaskType};

/// Parse LLM response into task structs.
pub fn parse_tasks_from_llm_response(response: &str, source_app: &str) -> Vec<Task> {
    let mut tasks = Vec::new();
    let now = chrono::Utc::now().timestamp_millis();

    for line in response.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Parse lines like "TODO: Send email", "REMINDER: ...", "FOLLOW-UP: ..."
        let stripped = line.strip_prefix("- ").unwrap_or(line).trim();
        let (task_type, title) = if let Some((prefix, rest)) = stripped.split_once(':') {
            let normalized_prefix = prefix
                .trim()
                .replace('-', "")
                .replace('_', "")
                .to_ascii_uppercase();
            let parsed_type = match normalized_prefix.as_str() {
                "TODO" => Some(TaskType::Todo),
                "REMINDER" => Some(TaskType::Reminder),
                "FOLLOWUP" => Some(TaskType::Followup),
                _ => None,
            };
            if let Some(parsed_type) = parsed_type {
                (parsed_type, rest.trim())
            } else if line.starts_with("- ") {
                (TaskType::Todo, stripped)
            } else {
                continue;
            }
        } else if line.starts_with("- ") {
            (TaskType::Todo, stripped)
        } else {
            continue;
        };

        if title.len() > 5 {
            tasks.push(Task {
                id: uuid::Uuid::new_v4().to_string(),
                title: title.to_string(),
                description: String::new(),
                source_app: source_app.to_string(),
                source_memory_id: None,
                created_at: now,
                due_date: None,
                is_completed: false,
                is_dismissed: false,
                task_type,
                linked_urls: Vec::new(),
                linked_memory_ids: Vec::new(),
            });
        }
    }

    tasks
}
