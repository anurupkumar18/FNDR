use crate::embed::Embedder;
use crate::search::HybridSearcher;
use crate::store::{EdgeType, GraphEdge, GraphNode, MemoryRecord, NodeType, Store};
use crate::tasks::Task;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

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
    store: Arc<Store>,
}

impl GraphStore {
    pub fn new(store: Arc<Store>) -> Self {
        Self { store }
    }

    pub async fn ingest_memory(
        &self,
        record: &MemoryRecord,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let now = chrono::Utc::now().timestamp_millis();

        let memory_node_id = memory_node_id(&record.id);
        let node = GraphNode {
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
                "session_key": record.session_key,
                "summary_source": record.summary_source,
                "memory_type": classify_memory_type(
                    &record.app_name,
                    record.url.as_deref(),
                    &record.summary_source,
                ),
                "url": record.url,
            }),
        };

        let session_node_id = session_node_id(&record.session_id);
        let s_node = GraphNode {
            id: session_node_id.clone(),
            node_type: NodeType::Entity,
            label: format!("Session {}", record.day_bucket),
            created_at: record.timestamp,
            metadata: json!({
                "entity_type": "session",
                "session_id": record.session_id,
                "day_bucket": record.day_bucket,
            }),
        };

        // We use a set-based approach for nodes to mimic upsert behavior
        // In a real DB we'd check if they exist, but here we just send them to the store's upsert
        self.store.upsert_nodes(&[node, s_node]).await?;

        let edge = GraphEdge {
            id: uuid::Uuid::new_v4().to_string(),
            source: memory_node_id.clone(),
            target: session_node_id.clone(),
            edge_type: EdgeType::PartOfSession,
            timestamp: now,
            metadata: json!({}),
        };
        self.store.upsert_edges(&[edge]).await?;

        if let Some(url) = record.url.as_ref() {
            let url_node_id = url_node_id(url);
            let u_node = GraphNode {
                id: url_node_id.clone(),
                node_type: NodeType::Url,
                label: url.clone(),
                created_at: record.timestamp,
                metadata: json!({ "host": host_from_url(url) }),
            };
            self.store.upsert_nodes(&[u_node]).await?;

            let u_edge = GraphEdge {
                id: uuid::Uuid::new_v4().to_string(),
                source: memory_node_id,
                target: url_node_id,
                edge_type: EdgeType::OccurredAt,
                timestamp: now,
                metadata: json!({}),
            };
            self.store.upsert_edges(&[u_edge]).await?;
        }

        Ok(())
    }

    pub async fn link_task(&self, task: &Task) -> Result<(), Box<dyn std::error::Error>> {
        let now = chrono::Utc::now().timestamp_millis();
        let task_node_id = task_node_id(&task.id);

        let t_node = GraphNode {
            id: task_node_id.clone(),
            node_type: NodeType::Task,
            label: task.title.clone(),
            created_at: task.created_at,
            metadata: json!({
                "task_type": format!("{:?}", task.task_type),
                "source_app": task.source_app,
                "is_completed": task.is_completed,
            }),
        };
        self.store.upsert_nodes(&[t_node]).await?;

        let mut edges = Vec::new();
        if let Some(source_memory_id) = task.source_memory_id.as_ref() {
            edges.push(GraphEdge {
                id: uuid::Uuid::new_v4().to_string(),
                source: task_node_id.clone(),
                target: memory_node_id(source_memory_id),
                edge_type: EdgeType::ReferenceForTask,
                timestamp: now,
                metadata: json!({"reason": "source_memory"}),
            });
        }

        for memory_id in &task.linked_memory_ids {
            edges.push(GraphEdge {
                id: uuid::Uuid::new_v4().to_string(),
                source: task_node_id.clone(),
                target: memory_node_id(memory_id),
                edge_type: EdgeType::ReferenceForTask,
                timestamp: now,
                metadata: json!({"reason": "linked_memory"}),
            });
        }

        for url in &task.linked_urls {
            let url_id = url_node_id(url);
            let u_node = GraphNode {
                id: url_id.clone(),
                node_type: NodeType::Url,
                label: url.clone(),
                created_at: task.created_at,
                metadata: json!({ "host": host_from_url(url) }),
            };
            self.store.upsert_nodes(&[u_node]).await?;

            edges.push(GraphEdge {
                id: uuid::Uuid::new_v4().to_string(),
                source: task_node_id.clone(),
                target: url_id,
                edge_type: EdgeType::ReferenceForTask,
                timestamp: now,
                metadata: json!({"reason": "linked_url"}),
            });
        }

        if !edges.is_empty() {
            self.store.upsert_edges(&edges).await?;
        }
        Ok(())
    }

    pub async fn related_urls_for_task(&self, task_id: &str) -> Vec<String> {
        let nodes = match self.store.get_all_nodes().await {
            Ok(nodes) => nodes,
            Err(_) => return Vec::new(),
        };
        let edges = match self.store.get_all_edges().await {
            Ok(edges) => edges,
            Err(_) => return Vec::new(),
        };
        related_urls_for_task_from_snapshot(&nodes, &edges, task_id)
    }

    pub async fn reconstruct(
        &self,
        store: &Store,
        embedder: &Embedder,
        query: &str,
        limit: usize,
    ) -> Result<MemoryReconstruction, Box<dyn std::error::Error>> {
        let results = HybridSearcher::search(store, embedder, query, limit, None, None).await?;
        let nodes = self.store.get_all_nodes().await?;
        let edges = self.store.get_all_edges().await?;

        let cards = self.map_cards(results, &nodes, &edges);
        let structural_context = self.structural_context_for_query(query, &nodes, &edges);

        Ok(MemoryReconstruction {
            answer: String::new(),
            cards,
            structural_context,
        })
    }

    fn structural_context_for_query(
        &self,
        query: &str,
        nodes: &[GraphNode],
        edges: &[GraphEdge],
    ) -> Vec<String> {
        let normalized = query.to_lowercase();
        let include_tasks = normalized.contains("task")
            || normalized.contains("todo")
            || normalized.contains("follow up")
            || normalized.contains("reminder")
            || normalized.contains("url");

        if !include_tasks {
            return Vec::new();
        }

        let mut task_nodes: Vec<&GraphNode> = nodes
            .iter()
            .filter(|node| node.node_type == NodeType::Task)
            .collect();
        task_nodes.sort_by_key(|node| std::cmp::Reverse(node.created_at));

        let mut notes = Vec::new();
        for task in task_nodes.into_iter().take(3) {
            let id = task.id.trim_start_matches("task:");
            let urls = related_urls_for_task_from_snapshot(nodes, edges, id);
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

    fn map_cards(
        &self,
        results: Vec<crate::store::SearchResult>,
        nodes: &[GraphNode],
        edges: &[GraphEdge],
    ) -> Vec<MemoryCard> {
        let memory_to_tasks = task_edges_by_memory(nodes, edges);

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
    pub async fn export_for_visualization(&self) -> (Vec<GraphNode>, Vec<GraphEdge>) {
        let nodes = match self.store.get_all_nodes().await {
            Ok(nodes) => nodes,
            Err(_) => return (Vec::new(), Vec::new()),
        };
        let edges = match self.store.get_all_edges().await {
            Ok(edges) => edges,
            Err(_) => return (Vec::new(), Vec::new()),
        };
        (nodes, edges)
    }

    /// Clear all graph data is managed via store reset/deletion in this version.
    pub async fn clear_all(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Implementation depends on Store's ability to truncate specific tables.
        // For now, we omit individual clear in favor of consolidated store management.
        Ok(())
    }
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

fn classify_memory_type(app_name: &str, url: Option<&str>, summary_source: &str) -> &'static str {
    let app = app_name.to_lowercase();
    if url.is_some() || is_browser_app(&app) {
        return "web";
    }

    if app.contains("meeting") || app.contains("zoom") || app.contains("teams") {
        return "meeting";
    }

    if app.contains("code")
        || app.contains("terminal")
        || app.contains("xcode")
        || app.contains("iterm")
    {
        return "development";
    }

    if app.contains("mail")
        || app.contains("slack")
        || app.contains("messages")
        || app.contains("discord")
    {
        return "communication";
    }

    if app.contains("docs")
        || app.contains("notion")
        || app.contains("word")
        || app.contains("pages")
        || app.contains("preview")
        || app.contains("pdf")
    {
        return "documents";
    }

    if summary_source.eq_ignore_ascii_case("vlm") {
        return "visual";
    }

    "general"
}

fn is_browser_app(app_name: &str) -> bool {
    app_name.contains("safari")
        || app_name.contains("chrome")
        || app_name.contains("arc")
        || app_name.contains("brave")
        || app_name.contains("edge")
        || app_name.contains("firefox")
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

fn related_urls_for_task_from_snapshot(
    nodes: &[GraphNode],
    edges: &[GraphEdge],
    task_id: &str,
) -> Vec<String> {
    let task_node = task_node_id(task_id);
    let node_map: HashMap<&str, &GraphNode> =
        nodes.iter().map(|node| (node.id.as_str(), node)).collect();

    let mut memory_targets = Vec::new();
    let mut urls = Vec::new();

    for edge in edges {
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
        for edge in edges {
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
