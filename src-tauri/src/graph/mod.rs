//! Local-first temporal memory graph inspired by Graphiti.

use crate::embed::Embedder;
use crate::search::HybridSearcher;
use crate::store::{MemoryRecord, Store};
use crate::tasks::Task;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};

const GRAPH_FILENAME: &str = "memory_graph.json";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum NodeType {
    MemoryChunk,
    Entity,
    Task,
    Url,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum EdgeType {
    #[serde(rename = "PART_OF_SESSION")]
    PartOfSession,
    #[serde(rename = "REFERENCE_FOR_TASK")]
    ReferenceForTask,
    #[serde(rename = "OCCURRED_AT")]
    OccurredAt,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: String,
    pub node_type: NodeType,
    pub label: String,
    pub created_at: i64,
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    pub id: String,
    pub source: String,
    pub target: String,
    pub edge_type: EdgeType,
    pub timestamp: i64,
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct GraphSnapshot {
    nodes: Vec<GraphNode>,
    edges: Vec<GraphEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryCard {
    pub id: String,
    pub timestamp: i64,
    pub app_name: String,
    pub window_title: String,
    pub snippet: String,
    pub url: Option<String>,
    pub screenshot_path: Option<String>,
    pub score: f32,
    pub related_tasks: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryReconstruction {
    pub answer: String,
    pub cards: Vec<MemoryCard>,
    pub structural_context: Vec<String>,
}

/// Persisted graph store.
pub struct GraphStore {
    data_path: PathBuf,
    snapshot: RwLock<GraphSnapshot>,
}

impl GraphStore {
    pub fn new(data_dir: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let data_path = data_dir.join(GRAPH_FILENAME);
        let snapshot = if data_path.exists() {
            let file = File::open(&data_path)?;
            let reader = BufReader::new(file);
            serde_json::from_reader(reader).unwrap_or_default()
        } else {
            GraphSnapshot::default()
        };

        Ok(Self {
            data_path,
            snapshot: RwLock::new(snapshot),
        })
    }

    pub fn ingest_memory(&self, record: &MemoryRecord) -> Result<(), Box<dyn std::error::Error>> {
        let mut snapshot = self.snapshot.write();
        let now = chrono::Utc::now().timestamp_millis();

        let memory_node_id = memory_node_id(&record.id);
        upsert_node(
            &mut snapshot.nodes,
            GraphNode {
                id: memory_node_id.clone(),
                node_type: NodeType::MemoryChunk,
                label: record.snippet.clone(),
                created_at: record.timestamp,
                metadata: json!({
                    "app_name": record.app_name,
                    "bundle_id": record.bundle_id,
                    "window_title": record.window_title,
                    "day_bucket": record.day_bucket,
                    "session_id": record.session_id,
                    "url": record.url,
                }),
            },
        );

        let session_node_id = session_node_id(&record.session_id);
        upsert_node(
            &mut snapshot.nodes,
            GraphNode {
                id: session_node_id.clone(),
                node_type: NodeType::Entity,
                label: format!("Session {}", record.day_bucket),
                created_at: record.timestamp,
                metadata: json!({
                    "entity_type": "session",
                    "session_id": record.session_id,
                    "day_bucket": record.day_bucket,
                }),
            },
        );
        upsert_edge(
            &mut snapshot.edges,
            &memory_node_id,
            &session_node_id,
            EdgeType::PartOfSession,
            now,
            json!({}),
        );

        if let Some(url) = record.url.as_ref() {
            let url_node_id = url_node_id(url);
            upsert_node(
                &mut snapshot.nodes,
                GraphNode {
                    id: url_node_id.clone(),
                    node_type: NodeType::Url,
                    label: url.clone(),
                    created_at: record.timestamp,
                    metadata: json!({ "host": host_from_url(url) }),
                },
            );
            upsert_edge(
                &mut snapshot.edges,
                &memory_node_id,
                &url_node_id,
                EdgeType::OccurredAt,
                now,
                json!({}),
            );
        }

        self.save(&snapshot)?;
        Ok(())
    }

    pub fn link_task(&self, task: &Task) -> Result<(), Box<dyn std::error::Error>> {
        let mut snapshot = self.snapshot.write();
        let now = chrono::Utc::now().timestamp_millis();
        let task_node_id = task_node_id(&task.id);

        upsert_node(
            &mut snapshot.nodes,
            GraphNode {
                id: task_node_id.clone(),
                node_type: NodeType::Task,
                label: task.title.clone(),
                created_at: task.created_at,
                metadata: json!({
                    "task_type": format!("{:?}", task.task_type),
                    "source_app": task.source_app,
                    "is_completed": task.is_completed,
                }),
            },
        );

        if let Some(source_memory_id) = task.source_memory_id.as_ref() {
            upsert_edge(
                &mut snapshot.edges,
                &task_node_id,
                &memory_node_id(source_memory_id),
                EdgeType::ReferenceForTask,
                now,
                json!({"reason": "source_memory"}),
            );
        }

        for memory_id in &task.linked_memory_ids {
            upsert_edge(
                &mut snapshot.edges,
                &task_node_id,
                &memory_node_id(memory_id),
                EdgeType::ReferenceForTask,
                now,
                json!({"reason": "linked_memory"}),
            );
        }

        for url in &task.linked_urls {
            let url_id = url_node_id(url);
            upsert_node(
                &mut snapshot.nodes,
                GraphNode {
                    id: url_id.clone(),
                    node_type: NodeType::Url,
                    label: url.clone(),
                    created_at: task.created_at,
                    metadata: json!({ "host": host_from_url(url) }),
                },
            );
            upsert_edge(
                &mut snapshot.edges,
                &task_node_id,
                &url_id,
                EdgeType::ReferenceForTask,
                now,
                json!({"reason": "linked_url"}),
            );
        }

        self.save(&snapshot)?;
        Ok(())
    }

    pub fn related_urls_for_task(&self, task_id: &str) -> Vec<String> {
        let snapshot = self.snapshot.read();
        related_urls_for_task_from_snapshot(&snapshot, task_id)
    }

    pub async fn reconstruct(
        &self,
        store: &Store,
        embedder: &Embedder,
        query: &str,
        limit: usize,
    ) -> Result<MemoryReconstruction, Box<dyn std::error::Error>> {
        let results = HybridSearcher::search(store, embedder, query, limit, None, None).await?;
        let cards = self.map_cards(results);
        let structural_context = self.structural_context_for_query(query);

        Ok(MemoryReconstruction {
            answer: String::new(),
            cards,
            structural_context,
        })
    }

    fn structural_context_for_query(&self, query: &str) -> Vec<String> {
        let snapshot = self.snapshot.read();
        let normalized = query.to_lowercase();
        let include_tasks = normalized.contains("task")
            || normalized.contains("todo")
            || normalized.contains("follow up")
            || normalized.contains("reminder")
            || normalized.contains("url");

        if !include_tasks {
            return Vec::new();
        }

        let mut task_nodes: Vec<&GraphNode> = snapshot
            .nodes
            .iter()
            .filter(|node| node.node_type == NodeType::Task)
            .collect();
        task_nodes.sort_by_key(|node| std::cmp::Reverse(node.created_at));

        let mut notes = Vec::new();
        for task in task_nodes.into_iter().take(3) {
            let id = task.id.trim_start_matches("task:");
            let urls = related_urls_for_task_from_snapshot(&snapshot, id);
            if urls.is_empty() {
                notes.push(format!("Task '{}': no linked URL context", task.label));
            } else {
                notes.push(format!(
                    "Task '{}': linked URLs {}",
                    task.label,
                    urls.join(", ")
                ));
            }
        }
        notes
    }

    fn map_cards(&self, results: Vec<crate::store::SearchResult>) -> Vec<MemoryCard> {
        let snapshot = self.snapshot.read();
        let memory_to_tasks = task_edges_by_memory(&snapshot.nodes, &snapshot.edges);

        results
            .into_iter()
            .map(|result| {
                let task_titles = memory_to_tasks
                    .get(&memory_node_id(&result.id))
                    .cloned()
                    .unwrap_or_default();
                MemoryCard {
                    id: result.id,
                    timestamp: result.timestamp,
                    app_name: result.app_name,
                    window_title: result.window_title,
                    snippet: result.snippet,
                    url: result.url,
                    screenshot_path: result.screenshot_path,
                    score: result.score,
                    related_tasks: task_titles,
                }
            })
            .collect()
    }

    /// Export all nodes and edges for frontend visualization.
    pub fn export_for_visualization(&self) -> (Vec<GraphNode>, Vec<GraphEdge>) {
        let snapshot = self.snapshot.read();
        (snapshot.nodes.clone(), snapshot.edges.clone())
    }

    fn save(&self, snapshot: &GraphSnapshot) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(parent) = self.data_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = File::create(&self.data_path)?;
        let writer = BufWriter::new(file);
        serde_json::to_writer(writer, snapshot)?;
        Ok(())
    }

    /// Clear all graph data (nodes + edges) and persist the empty state.
    pub fn clear_all(&self) -> Result<(), Box<dyn std::error::Error>> {
        let empty = GraphSnapshot::default();
        {
            let mut snap = self.snapshot.write();
            *snap = empty.clone();
        }
        self.save(&empty)?;
        Ok(())
    }
}

fn upsert_node(nodes: &mut Vec<GraphNode>, node: GraphNode) {
    if let Some(existing) = nodes.iter_mut().find(|n| n.id == node.id) {
        existing.label = node.label;
        existing.metadata = node.metadata;
        return;
    }
    nodes.push(node);
}

fn upsert_edge(
    edges: &mut Vec<GraphEdge>,
    source: &str,
    target: &str,
    edge_type: EdgeType,
    timestamp: i64,
    metadata: Value,
) {
    if edges
        .iter()
        .any(|edge| edge.source == source && edge.target == target && edge.edge_type == edge_type)
    {
        return;
    }

    edges.push(GraphEdge {
        id: uuid::Uuid::new_v4().to_string(),
        source: source.to_string(),
        target: target.to_string(),
        edge_type,
        timestamp,
        metadata,
    });
}

fn memory_node_id(memory_id: &str) -> String {
    format!("memory:{memory_id}")
}

fn session_node_id(session_id: &str) -> String {
    format!("session:{session_id}")
}

fn task_node_id(task_id: &str) -> String {
    format!("task:{task_id}")
}

fn url_node_id(url: &str) -> String {
    format!("url:{}", url.to_lowercase())
}

fn host_from_url(url: &str) -> String {
    let trimmed = url
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    trimmed.split('/').next().unwrap_or(trimmed).to_string()
}

fn task_edges_by_memory(nodes: &[GraphNode], edges: &[GraphEdge]) -> HashMap<String, Vec<String>> {
    let node_map: HashMap<&str, &GraphNode> = nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    let mut mapping: HashMap<String, Vec<String>> = HashMap::new();

    for edge in edges {
        if edge.edge_type != EdgeType::ReferenceForTask {
            continue;
        }
        let Some(source) = node_map.get(edge.source.as_str()) else {
            continue;
        };
        let Some(target) = node_map.get(edge.target.as_str()) else {
            continue;
        };
        if source.node_type != NodeType::Task || target.node_type != NodeType::MemoryChunk {
            continue;
        }

        mapping
            .entry(target.id.clone())
            .or_default()
            .push(source.label.clone());
    }

    for titles in mapping.values_mut() {
        *titles = unique_keep_order(std::mem::take(titles));
    }

    mapping
}

fn related_urls_for_task_from_snapshot(snapshot: &GraphSnapshot, task_id: &str) -> Vec<String> {
    let task_node = task_node_id(task_id);
    let node_map: HashMap<&str, &GraphNode> = snapshot
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect();

    let mut memory_targets = Vec::new();
    let mut urls = Vec::new();

    for edge in &snapshot.edges {
        if edge.source != task_node || edge.edge_type != EdgeType::ReferenceForTask {
            continue;
        }
        if let Some(target) = node_map.get(edge.target.as_str()) {
            match target.node_type {
                NodeType::Url => urls.push(target.label.clone()),
                NodeType::MemoryChunk => memory_targets.push(target.id.clone()),
                _ => {}
            }
        }
    }

    for memory_id in memory_targets {
        for edge in &snapshot.edges {
            if edge.source == memory_id && edge.edge_type == EdgeType::OccurredAt {
                if let Some(target) = node_map.get(edge.target.as_str()) {
                    if target.node_type == NodeType::Url {
                        urls.push(target.label.clone());
                    }
                }
            }
        }
    }

    unique_keep_order(urls)
}

fn unique_keep_order(values: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut output = Vec::new();
    for value in values {
        if seen.insert(value.clone()) {
            output.push(value);
        }
    }
    output
}
