//! FNDR Library
//!
//! Core functionality for the FNDR memory search application.

pub mod api;
pub mod capture;
pub mod config;
pub mod embed;
pub mod graph;
pub mod inference;
pub mod mcp;
pub mod meeting;
pub mod models;
pub mod ocr;
pub mod privacy;
pub mod search;
pub mod speech;
pub mod store;
pub mod tasks;
pub mod telemetry;

use config::Config;
use graph::GraphStore;
use inference::{InferenceEngine, VlmEngine};
use parking_lot::RwLock;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use store::Store;
use tokio::sync::Mutex as AsyncMutex;

pub struct LoadedAiEngines {
    pub inference: Option<Arc<InferenceEngine>>,
    pub vlm: Option<Arc<VlmEngine>>,
}

/// Application state shared across threads
pub struct AppState {
    pub app_data_dir: PathBuf,
    pub config: RwLock<Config>,
    pub store: Store,
    pub graph: GraphStore,
    pub is_paused: AtomicBool,
    pub is_incognito: AtomicBool,
    pub frames_captured: AtomicU64,
    pub frames_dropped: AtomicU64,
    pub last_capture_time: AtomicU64,
    pub inference: RwLock<Option<Arc<InferenceEngine>>>,
    /// Vision Language Model for intelligent screen analysis (optional)
    pub vlm: RwLock<Option<Arc<VlmEngine>>>,
    inference_init: AsyncMutex<()>,
}

impl AppState {
    pub fn new(
        app_data_dir: PathBuf,
        config: Config,
        store: Store,
        graph: GraphStore,
        inference: Option<Arc<InferenceEngine>>,
        vlm: Option<Arc<VlmEngine>>,
    ) -> Self {
        Self {
            app_data_dir,
            config: RwLock::new(config),
            store,
            graph,
            is_paused: AtomicBool::new(false),
            is_incognito: AtomicBool::new(false),
            frames_captured: AtomicU64::new(0),
            frames_dropped: AtomicU64::new(0),
            last_capture_time: AtomicU64::new(0),
            inference: RwLock::new(inference),
            vlm: RwLock::new(vlm),
            inference_init: AsyncMutex::new(()),
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

    pub fn inference_engine(&self) -> Option<Arc<InferenceEngine>> {
        self.inference.read().clone()
    }

    pub fn vlm_engine(&self) -> Option<Arc<VlmEngine>> {
        self.vlm.read().clone()
    }

    pub fn ai_model_loaded(&self) -> bool {
        self.inference.read().is_some()
    }

    pub fn ai_model_available(&self) -> bool {
        let preferred_model_id = self.preferred_model_id();
        models::resolve_model(
            preferred_model_id.as_deref(),
            Some(self.app_data_dir.as_path()),
        )
        .is_some()
    }

    pub fn preferred_model_id(&self) -> Option<String> {
        models::preferred_model_id_from_onboarding(self.app_data_dir.as_path())
    }

    pub fn loaded_model_id(&self) -> Option<String> {
        self.inference
            .read()
            .as_ref()
            .map(|engine| engine.model_id().to_string())
    }

    pub fn replace_ai_engines(
        &self,
        inference: Option<Arc<InferenceEngine>>,
        vlm: Option<Arc<VlmEngine>>,
    ) {
        *self.inference.write() = inference;
        *self.vlm.write() = vlm;
    }

    pub async fn ensure_inference_engine(&self) -> Result<Option<Arc<InferenceEngine>>, String> {
        if let Some(engine) = self.inference_engine() {
            return Ok(Some(engine));
        }

        let preferred_model_id = self.preferred_model_id();
        if models::resolve_model(
            preferred_model_id.as_deref(),
            Some(self.app_data_dir.as_path()),
        )
        .is_none()
        {
            return Ok(None);
        }

        let _guard = self.inference_init.lock().await;
        if let Some(engine) = self.inference_engine() {
            return Ok(Some(engine));
        }

        let engine = InferenceEngine::new(Some(self.app_data_dir.clone()), preferred_model_id)
            .await
            .map_err(|err| err.to_string())?;
        let engine = Arc::new(engine);
        *self.inference.write() = Some(engine.clone());
        Ok(Some(engine))
    }
}

pub async fn load_ai_engines(
    app_data_dir: &Path,
    _config: &Config,
    preferred_model_id: Option<&str>,
) -> LoadedAiEngines {
    let inference = match InferenceEngine::new(
        Some(app_data_dir.to_path_buf()),
        preferred_model_id.map(str::to_owned),
    )
    .await
    {
        Ok(engine) => {
            tracing::info!(
                "AI inference engine initialized successfully with {}",
                engine.model_id()
            );
            Some(Arc::new(engine))
        }
        Err(err) => {
            tracing::warn!("AI inference initialization failed: {}", err);
            None
        }
    };

    tracing::info!(
        "Skipping eager VLM warm-up; Gemma 4 core and optional accelerators load on demand."
    );
    let vlm = None;

    LoadedAiEngines { inference, vlm }
}
