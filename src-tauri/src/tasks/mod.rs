//! Task extraction and management
//!
//! Extracts actionable todos/reminders from captured memories using LLM.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};

const TASKS_FILENAME: &str = "tasks.json";

/// A task extracted from memories
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub description: String,
    pub source_app: String,
    pub source_memory_id: Option<String>,
    pub created_at: i64,
    pub due_date: Option<i64>,
    pub is_completed: bool,
    pub is_dismissed: bool,
    pub task_type: TaskType,
    #[serde(default)]
    pub linked_urls: Vec<String>,
    #[serde(default)]
    pub linked_memory_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskType {
    Todo,
    Reminder,
    Followup,
}

/// Task store for persistence
pub struct TaskStore {
    data_path: PathBuf,
    tasks: Vec<Task>,
}

impl TaskStore {
    pub fn new(data_dir: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let data_path = data_dir.join(TASKS_FILENAME);
        let tasks = if data_path.exists() {
            let file = File::open(&data_path)?;
            let reader = BufReader::new(file);
            serde_json::from_reader(reader).unwrap_or_else(|_| Vec::new())
        } else {
            Vec::new()
        };

        Ok(Self { data_path, tasks })
    }

    /// Save tasks to disk
    fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(parent) = self.data_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = File::create(&self.data_path)?;
        let writer = BufWriter::new(file);
        serde_json::to_writer(writer, &self.tasks)?;
        Ok(())
    }

    /// Get all active (non-dismissed, non-completed) tasks
    pub fn get_active_tasks(&self) -> Vec<&Task> {
        self.tasks
            .iter()
            .filter(|t| !t.is_completed && !t.is_dismissed)
            .collect()
    }

    /// Add a new task
    pub fn add_task(&mut self, task: Task) -> Result<(), Box<dyn std::error::Error>> {
        let normalized_title = normalize_task_title(&task.title);

        if normalized_title.is_empty() {
            return Ok(());
        }

        // Never duplicate still-active tasks with the same normalized title.
        let duplicate_active_title = self.tasks.iter().any(|existing| {
            normalize_task_title(&existing.title) == normalized_title
                && !existing.is_completed
                && !existing.is_dismissed
        });
        if duplicate_active_title {
            return Ok(());
        }

        // Also avoid re-creating the same title from the same memory context,
        // even if the prior task was dismissed/completed.
        let duplicate_origin = self.tasks.iter().any(|existing| {
            normalize_task_title(&existing.title) == normalized_title
                && share_memory_origin(existing, &task)
        });
        if duplicate_origin {
            return Ok(());
        }

        self.tasks.push(task);
        self.save()?;
        Ok(())
    }

    /// Mark task as dismissed
    pub fn dismiss_task(&mut self, task_id: &str) -> Result<bool, Box<dyn std::error::Error>> {
        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == task_id) {
            task.is_dismissed = true;
            self.save()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Mark task as completed
    pub fn complete_task(&mut self, task_id: &str) -> Result<bool, Box<dyn std::error::Error>> {
        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == task_id) {
            task.is_completed = true;
            self.save()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Clear old dismissed/completed tasks (> 7 days)
    pub fn cleanup_old_tasks(&mut self) -> Result<usize, Box<dyn std::error::Error>> {
        let cutoff = chrono::Utc::now().timestamp_millis() - (7 * 24 * 60 * 60 * 1000);
        let initial_len = self.tasks.len();
        self.tasks
            .retain(|t| !(t.is_dismissed || t.is_completed) || t.created_at > cutoff);
        let removed = initial_len - self.tasks.len();
        if removed > 0 {
            self.save()?;
        }
        Ok(removed)
    }

    /// Remove all tasks and persist.
    pub fn clear_all(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.tasks.clear();
        self.save()?;
        Ok(())
    }

    /// Snapshot all tasks (active + dismissed + completed).
    pub fn tasks_snapshot(&self) -> Vec<Task> {
        self.tasks.clone()
    }

    /// Memory ids that have already produced tasks recently.
    pub fn seen_memory_ids(&self) -> HashSet<String> {
        let mut ids = HashSet::new();
        for task in &self.tasks {
            if let Some(id) = task.source_memory_id.as_ref() {
                ids.insert(id.clone());
            }
            for id in &task.linked_memory_ids {
                ids.insert(id.clone());
            }
        }
        ids
    }
}

/// Parse LLM response into tasks
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

fn normalize_task_title(title: &str) -> String {
    title
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn share_memory_origin(left: &Task, right: &Task) -> bool {
    let mut left_ids: HashSet<&str> = HashSet::new();
    let mut right_ids: HashSet<&str> = HashSet::new();

    if let Some(id) = left.source_memory_id.as_deref() {
        left_ids.insert(id);
    }
    if let Some(id) = right.source_memory_id.as_deref() {
        right_ids.insert(id);
    }

    for id in &left.linked_memory_ids {
        left_ids.insert(id.as_str());
    }
    for id in &right.linked_memory_ids {
        right_ids.insert(id.as_str());
    }

    if left_ids.is_empty() || right_ids.is_empty() {
        return false;
    }

    left_ids.into_iter().any(|id| right_ids.contains(id))
}
