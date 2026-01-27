//! FNDR - Privacy-first local memory search
//!
//! Main entry point for the Tauri application.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use fndr_lib::{api, capture, config::Config, store::Store, AppState};
use std::sync::Arc;
use tauri::Manager;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

fn main() {
    // Initialize logging
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "fndr=info,fndr_lib=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!("Starting FNDR...");

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            // Load configuration
            let config = Config::load_or_create()?;
            tracing::info!("Configuration loaded");

            // Initialize store
            let data_dir = app.path().app_data_dir()?;
            let store = Store::new(&data_dir)?;
            tracing::info!("Store initialized at {:?}", data_dir);

            // Initialize AI Engine (blocking for start)
            let handle = app.handle().clone();
            let inference = tauri::async_runtime::block_on(async move {
                fndr_lib::inference::InferenceEngine::new().await
            }).map_err(|e| format!("Failed to init AI engine: {}", e))?;

            // Create app state
            let state = Arc::new(AppState::new(config, store, inference));

            // Start capture pipeline
            let capture_state = state.clone();
            std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async {
                    if let Err(e) = capture::run_capture_loop(capture_state).await {
                        tracing::error!("Capture loop error: {}", e);
                    }
                });
            });

            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            api::commands::search,
            api::commands::ask_fndr,
            api::commands::get_status,
            api::commands::pause_capture,
            api::commands::resume_capture,
            api::commands::get_blocklist,
            api::commands::set_blocklist,
            api::commands::delete_all_data,
            api::commands::get_stats,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
