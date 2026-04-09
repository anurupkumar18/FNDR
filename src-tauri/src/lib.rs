//! FNDR Library
//!
//! Core functionality for the FNDR memory search application.

pub mod api;
pub mod capture;
pub mod config;
pub mod demo;
pub mod embed;
pub mod graph;
pub mod inference;
pub mod mcp;
pub mod meeting;
pub mod ocr;
pub mod privacy;
pub mod search;
pub mod store;
pub mod tasks;
pub mod telemetry;

use config::Config;
use graph::GraphStore;
use inference::{InferenceEngine, VlmEngine};
use parking_lot::RwLock;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use store::Store;

/// Application state shared across threads
pub struct AppState {
    pub config: RwLock<Config>,
    pub store: Store,
    pub graph: GraphStore,
    pub is_paused: AtomicBool,
    pub is_incognito: AtomicBool,
    pub frames_captured: AtomicU64,
    pub frames_dropped: AtomicU64,
    pub last_capture_time: AtomicU64,
    pub inference: Arc<InferenceEngine>,
    /// Vision Language Model for intelligent screen analysis (optional)
    pub vlm: Option<Arc<VlmEngine>>,
}

impl AppState {
    pub fn new(
        config: Config,
        store: Store,
        graph: GraphStore,
        inference: InferenceEngine,
        vlm: Option<VlmEngine>,
    ) -> Self {
        Self {
            config: RwLock::new(config),
            store,
            graph,
            is_paused: AtomicBool::new(false),
            is_incognito: AtomicBool::new(false),
            frames_captured: AtomicU64::new(0),
            frames_dropped: AtomicU64::new(0),
            last_capture_time: AtomicU64::new(0),
            inference: Arc::new(inference),
            vlm: vlm.map(Arc::new),
        }
    }

    pub fn pause(&self) {
        self.is_paused.store(true, Ordering::SeqCst);
        tracing::info!("Capture paused");
    }

    pub fn resume(&self) {
        self.is_paused.store(false, Ordering::SeqCst);
        tracing::info!("Capture resumed");
    }

    pub fn is_capturing(&self) -> bool {
        !self.is_paused.load(Ordering::SeqCst) && !self.is_incognito.load(Ordering::SeqCst)
    }
}
