use fndr_lib::config::DEFAULT_IMAGE_EMBEDDING_DIM;
use fndr_lib::embedding::EMBEDDING_DIM;
use fndr_lib::graph::GraphStore;
use fndr_lib::storage::{
    EdgeType, GraphEdge, GraphNode, MemoryRecord, NodeType, Store, Task, TaskType,
};
use std::collections::HashSet;
use std::sync::Arc;

fn embedding(value: f32) -> Vec<f32> {
    let mut vector = vec![0.0; EMBEDDING_DIM];
    vector[0] = value;
    vector
}

fn record(id: &str, snippet: &str, embedding_value: f32) -> MemoryRecord {
    MemoryRecord {
        id: id.to_string(),
        timestamp: chrono::Utc::now().timestamp_millis(),
        day_bucket: "2026-04-21".to_string(),
        app_name: "Codex".to_string(),
        bundle_id: None,
        window_title: "Regression".to_string(),
        session_id: "session-1".to_string(),
        text: snippet.to_string(),
        clean_text: snippet.to_string(),
        ocr_confidence: 0.99,
        ocr_block_count: 3,
        snippet: snippet.to_string(),
        summary_source: "fallback".to_string(),
        noise_score: 0.0,
        session_key: "codex:regression".to_string(),
        lexical_shadow: snippet.to_string(),
        embedding: embedding(embedding_value),
        image_embedding: vec![0.0; DEFAULT_IMAGE_EMBEDDING_DIM],
        screenshot_path: None,
        url: None,
        snippet_embedding: embedding(embedding_value),
        support_embedding: embedding(embedding_value),
        decay_score: 1.0,
        last_accessed_at: 0,
        // `record_insert_dedup_key` buckets by URL + title + 5m clock; without a unique
        // hash, distinct regression memories in the same bucket collapse to one insert.
        content_hash: format!("store-graph-regression-{id}"),
        ..Default::default()
    }
}

#[test]
fn has_memories_and_targeted_graph_upserts_replace_in_place() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = Store::new(dir.path()).expect("store");
    let rt = tokio::runtime::Runtime::new().expect("runtime");

    rt.block_on(async {
        assert!(!store.has_memories().await.expect("initial has_memories"));

        store
            .add_batch(&[record("mem-1", "Investigated invoice OCR mismatch", 1.0)])
            .await
            .expect("add memory");
        assert!(store
            .has_memories()
            .await
            .expect("has_memories after insert"));

        store
            .upsert_nodes(&[GraphNode {
                id: "task:seed".to_string(),
                node_type: NodeType::Task,
                label: "First".to_string(),
                created_at: 1,
                metadata: serde_json::json!({"version": 1}),
            }])
            .await
            .expect("insert node");
        store
            .upsert_nodes(&[GraphNode {
                id: "task:seed".to_string(),
                node_type: NodeType::Task,
                label: "Second".to_string(),
                created_at: 2,
                metadata: serde_json::json!({"version": 2}),
            }])
            .await
            .expect("replace node");

        let nodes = store.get_all_nodes().await.expect("nodes");
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].label, "Second");
        assert_eq!(nodes[0].metadata["version"], 2);

        store
            .upsert_edges(&[GraphEdge {
                id: "edge-1".to_string(),
                source: "task:seed".to_string(),
                target: "memory:mem-1".to_string(),
                edge_type: EdgeType::ReferenceForTask,
                timestamp: 1,
                metadata: serde_json::json!({"version": 1}),
            }])
            .await
            .expect("insert edge");
        store
            .upsert_edges(&[GraphEdge {
                id: "edge-2".to_string(),
                source: "task:seed".to_string(),
                target: "memory:mem-1".to_string(),
                edge_type: EdgeType::ReferenceForTask,
                timestamp: 2,
                metadata: serde_json::json!({"version": 2}),
            }])
            .await
            .expect("replace edge");

        let edges = store.get_all_edges().await.expect("edges");
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].id, "edge-2");
        assert_eq!(edges[0].metadata["version"], 2);
    });
}

#[test]
fn graph_ingest_and_task_link_are_idempotent() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = Arc::new(Store::new(dir.path()).expect("store"));
    let graph = GraphStore::new(store.clone());
    let rt = tokio::runtime::Runtime::new().expect("runtime");
    let memory = record("mem-1", "Prepared quarterly planning notes", 1.0);

    rt.block_on(async {
        store
            .add_batch(&[memory.clone()])
            .await
            .expect("add memory");
        graph.ingest_memory(&memory).await.expect("ingest memory");
        graph
            .ingest_memory(&memory)
            .await
            .expect("re-ingest memory");

        let task = Task {
            id: "task-1".to_string(),
            title: "Quarterly planning".to_string(),
            description: String::new(),
            source_app: "Codex".to_string(),
            source_memory_id: Some(memory.id.clone()),
            created_at: memory.timestamp,
            due_date: None,
            is_completed: false,
            is_dismissed: false,
            task_type: TaskType::Todo,
            linked_urls: Vec::new(),
            linked_memory_ids: vec![memory.id.clone()],
        };
        graph.link_task(&task).await.expect("link task");
        graph.link_task(&task).await.expect("re-link task");

        let nodes = store.get_all_nodes().await.expect("nodes");
        assert_eq!(
            nodes
                .iter()
                .filter(|node| node.id == "memory:mem-1")
                .count(),
            1
        );
        assert_eq!(
            nodes
                .iter()
                .filter(|node| node.id == "session:session-1")
                .count(),
            1
        );
        assert_eq!(
            nodes.iter().filter(|node| node.id == "task:task-1").count(),
            1
        );

        let edges = store.get_all_edges().await.expect("edges");
        assert_eq!(
            edges
                .iter()
                .filter(|edge| {
                    edge.edge_type == EdgeType::PartOfSession
                        && edge.source == "memory:mem-1"
                        && edge.target == "session:session-1"
                })
                .count(),
            1
        );
        assert_eq!(
            edges
                .iter()
                .filter(|edge| {
                    edge.edge_type == EdgeType::ReferenceForTask
                        && edge.source == "task:task-1"
                        && edge.target == "memory:mem-1"
                })
                .count(),
            1
        );
    });
}

#[test]
fn auto_link_to_task_seeds_cluster_without_duplicates() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = Arc::new(Store::new(dir.path()).expect("store"));
    let graph = GraphStore::new(store.clone());
    let rt = tokio::runtime::Runtime::new().expect("runtime");
    let existing = vec![
        record("mem-1", "Reviewed quarterly planning notes", 1.0),
        record("mem-2", "Updated quarterly planning agenda", 1.0),
    ];
    let incoming = record("mem-3", "Finalized quarterly planning checklist", 1.0);

    rt.block_on(async {
        store.add_batch(&existing).await.expect("add cluster peers");
        for memory in &existing {
            graph.ingest_memory(memory).await.expect("ingest peer");
        }

        graph
            .auto_link_to_task(&incoming)
            .await
            .expect("auto link cluster");
        graph
            .auto_link_to_task(&incoming)
            .await
            .expect("auto link cluster again");

        let nodes = store.get_all_nodes().await.expect("nodes");
        assert_eq!(
            nodes
                .iter()
                .filter(|node| node.node_type == NodeType::Task)
                .count(),
            1
        );

        let edges = store.get_all_edges().await.expect("edges");
        let reference_edges = edges
            .iter()
            .filter(|edge| edge.edge_type == EdgeType::ReferenceForTask)
            .collect::<Vec<_>>();
        assert_eq!(reference_edges.len(), 3);

        let targets = reference_edges
            .iter()
            .map(|edge| edge.target.clone())
            .collect::<HashSet<_>>();
        assert_eq!(
            targets,
            HashSet::from([
                "memory:mem-1".to_string(),
                "memory:mem-2".to_string(),
                "memory:mem-3".to_string(),
            ])
        );
    });
}

#[test]
fn auto_link_to_task_joins_existing_task_without_duplicates() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = Arc::new(Store::new(dir.path()).expect("store"));
    let graph = GraphStore::new(store.clone());
    let rt = tokio::runtime::Runtime::new().expect("runtime");
    let existing = record("mem-1", "Debugged quarterly planning timeline", 1.0);
    let incoming = record("mem-2", "Refined quarterly planning timeline", 1.0);

    rt.block_on(async {
        store
            .add_batch(&[existing.clone()])
            .await
            .expect("add memory");
        graph.ingest_memory(&existing).await.expect("ingest memory");
        graph
            .link_task(&Task {
                id: "task-join".to_string(),
                title: "Quarterly planning".to_string(),
                description: String::new(),
                source_app: "Codex".to_string(),
                source_memory_id: Some(existing.id.clone()),
                created_at: existing.timestamp,
                due_date: None,
                is_completed: false,
                is_dismissed: false,
                task_type: TaskType::Todo,
                linked_urls: Vec::new(),
                linked_memory_ids: Vec::new(),
            })
            .await
            .expect("link task");

        graph
            .auto_link_to_task(&incoming)
            .await
            .expect("auto link join");
        graph
            .auto_link_to_task(&incoming)
            .await
            .expect("auto link join again");

        let edges = store.get_all_edges().await.expect("edges");
        let reference_edges = edges
            .iter()
            .filter(|edge| edge.edge_type == EdgeType::ReferenceForTask)
            .collect::<Vec<_>>();
        assert_eq!(reference_edges.len(), 2);
        assert!(reference_edges
            .iter()
            .any(|edge| edge.target == "memory:mem-1"));
        assert!(reference_edges
            .iter()
            .any(|edge| edge.target == "memory:mem-2"));
    });
}

#[test]
fn insight_graph_context_requires_authorized_backing_and_project_scoped_edges() {
    use fndr_lib::graph::graph_store::GraphStore as InsightStore;
    use fndr_lib::graph::schema::{
        GraphEdge as InsightEdge, GraphEdgeType, GraphNode as InsightNode, GraphNodeType,
    };
    use fndr_lib::storage::StateStore;
    use fndr_lib::AppState;
    use serde_json::json;
    use uuid::Uuid;

    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::new(dir.path()).unwrap());
    let state_store = Arc::new(StateStore::new(dir.path()).unwrap());
    let state = AppState::new(
        dir.path().to_path_buf(),
        fndr_lib::config::Config::default(),
        store.clone(),
        state_store,
        GraphStore::new(store.clone()),
        None,
        None,
    );
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let text = "Reviewed release verification evidence and discussed a conditional rollout.";
        let visible = record("visible", text, 1.0);
        let other = record("other", text, 1.0);
        let mut blocked = record("blocked", text, 1.0);
        blocked.app_name = "PrivateWorkspace".into();
        let mut deleted = record("deleted", text, 1.0);
        deleted.is_soft_deleted = true;
        let mut note = record("note", text, 1.0);
        note.source_type = "agent".into();
        store
            .add_batch_preserving_ids(&[visible, other, blocked, deleted, note])
            .await
            .unwrap();
        state.config.write().blocklist = vec!["privateworkspace".into()];
        let graph = InsightStore::new(store.clone());
        let node = |id: u128, label: &str, node_type: GraphNodeType, ids: &[&str]| InsightNode {
            id: Uuid::from_u128(id),
            node_type,
            label: label.into(),
            confidence: 0.9,
            source_memory_ids: ids.iter().map(|id| (*id).into()).collect(),
            embedding: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            stale: false,
            metadata: json!({}),
        };
        for n in [
            node(1, "Visible project", GraphNodeType::Project, &["visible"]),
            node(2, "Other project", GraphNodeType::Project, &["other"]),
            node(3, "Visible evidence", GraphNodeType::Concept, &["visible"]),
            node(4, "Other evidence", GraphNodeType::Concept, &["other"]),
            node(
                5,
                "PRIVATE_MIXED_PROJECT",
                GraphNodeType::Project,
                &["visible", "blocked"],
            ),
            node(6, "PRIVATE_UNBACKED_PROJECT", GraphNodeType::Project, &[]),
            node(
                7,
                "PRIVATE_MISSING_PROJECT",
                GraphNodeType::Project,
                &["missing"],
            ),
            node(8, "PRIVATE_NOTE_PROJECT", GraphNodeType::Project, &["note"]),
            node(
                9,
                "PRIVATE_DELETED_PROJECT",
                GraphNodeType::Project,
                &["deleted"],
            ),
        ] {
            graph.upsert_node(&n).await.unwrap();
        }
        let edge = |id: u128,
                    source: u128,
                    target: u128,
                    edge_type: GraphEdgeType,
                    confidence: f32,
                    metadata| InsightEdge {
            id: Uuid::from_u128(id),
            source_id: Uuid::from_u128(source),
            target_id: Uuid::from_u128(target),
            edge_type,
            confidence,
            conflict_flag: edge_type == GraphEdgeType::Contradicts,
            created_at: chrono::Utc::now(),
            metadata,
        };
        for e in [
            edge(101, 1, 3, GraphEdgeType::Supports, 0.5, json!({})),
            edge(102, 2, 4, GraphEdgeType::Contradicts, 0.99, json!({})),
            edge(103, 1, 5, GraphEdgeType::Contradicts, 0.98, json!({})),
            edge(
                104,
                1,
                3,
                GraphEdgeType::Refines,
                0.97,
                json!({"memory_id":"blocked"}),
            ),
            edge(
                105,
                1,
                3,
                GraphEdgeType::Questions,
                0.96,
                json!({"source_memory_ids":["visible", "missing"]}),
            ),
            edge(
                106,
                1,
                3,
                GraphEdgeType::DependsOn,
                0.95,
                json!({"memory_id":42}),
            ),
        ] {
            graph.upsert_edge(&e).await.unwrap();
        }

        let all = fndr_lib::context_runtime::insight_graph_context_mcp(&state, None)
            .await
            .unwrap();
        assert!(
            !all.to_string().contains("PRIVATE_"),
            "excluded graph labels leaked: {all}"
        );
        assert_eq!(all["top_project_nodes"].as_array().unwrap().len(), 2);
        assert_eq!(
            all["top_edges"].as_array().unwrap().len(),
            2,
            "only endpoint- and provenance-authorized edges"
        );
        assert_eq!(all["conflicts"].as_array().unwrap().len(), 1);

        let scoped =
            fndr_lib::context_runtime::insight_graph_context_mcp(&state, Some("Visible project"))
                .await
                .unwrap();
        assert_eq!(scoped["top_project_nodes"].as_array().unwrap().len(), 1);
        assert_eq!(
            scoped["top_edges"].as_array().unwrap().len(),
            1,
            "project view must not emit unrelated global edges"
        );
        assert_eq!(
            scoped["top_edges"][0]["source"],
            Uuid::from_u128(1).to_string()
        );
        assert_eq!(
            scoped["top_edges"][0]["target"],
            Uuid::from_u128(3).to_string()
        );
        assert!(scoped["conflicts"].as_array().unwrap().is_empty());
        let absent =
            fndr_lib::context_runtime::insight_graph_context_mcp(&state, Some("Absent project"))
                .await
                .unwrap();
        assert!(absent["top_project_nodes"].as_array().unwrap().is_empty());
        assert!(absent["top_edges"].as_array().unwrap().is_empty());
        assert!(absent["conflicts"].as_array().unwrap().is_empty());
    });
}
