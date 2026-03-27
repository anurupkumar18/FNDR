//! FNDR - Privacy-first local memory search
//!
//! Main entry point for the Tauri application.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use fndr_lib::{api, capture, config::Config, graph::GraphStore, store::Store, AppState};
use std::sync::Arc;
use tauri::Manager;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

fn main() {
    // Load environment variables from .env if present
    let _ = dotenvy::dotenv();

    // Initialize logging
    use tracing_subscriber::{fmt, EnvFilter};
    tracing_subscriber::registry()
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "fndr=info,fndr_lib=info".into()),
        )
        .with(fmt::layer())
        .init();

    tracing::info!("Starting FNDR...");

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--hidden"]),
        ))
        .setup(|app| {
            // Load configuration
            let config = Config::load_or_create()?;
            tracing::info!("Configuration loaded");

            // Initialize store
            let data_dir = app.path().app_data_dir()?;
            let store = Store::new(&data_dir)?;
            tracing::info!("Store initialized at {:?}", data_dir);
            let graph = GraphStore::new(&data_dir)?;
            tracing::info!("Graph store initialized at {:?}", data_dir);
            if let Err(err) = fndr_lib::meeting::init(data_dir.clone()) {
                tracing::warn!("Meeting subsystem initialization failed: {}", err);
            }

            // Apply retention: remove records older than config.retention_days (0 = keep forever)
            if config.retention_days > 0 {
                match store.delete_older_than(config.retention_days) {
                    Ok(n) if n > 0 => tracing::info!("Retention: removed {} old records", n),
                    Ok(_) => {}
                    Err(e) => tracing::warn!("Retention cleanup failed: {}", e),
                }
            }

            // Initialize AI Engine (optional, based on model presence)
            let _handle = app.handle().clone();
            let inference = match tauri::async_runtime::block_on(async move {
                fndr_lib::inference::InferenceEngine::new().await
            }) {
                Ok(engine) => {
                    tracing::info!("AI inference engine initialized successfully");
                    Some(Arc::new(engine))
                }
                Err(e) => {
                    tracing::warn!("AI inference initialization failed: {}", e);
                    None
                }
            };

            // Initialize VLM Engine (optional, based on config)
            let vlm = if config.use_vlm {
                tracing::info!(
                    "Initializing VLM engine (Gemma-{})...",
                    config.vlm_model_size
                );
                match tauri::async_runtime::block_on(async {
                    fndr_lib::inference::VlmEngine::new(&config.vlm_model_size).await
                }) {
                    Ok(engine) => {
                        tracing::info!("VLM engine initialized successfully");
                        Some(Arc::new(engine))
                    }
                    Err(e) => {
                        tracing::warn!("VLM initialization failed (will use OCR only): {}", e);
                        None
                    }
                }
            } else {
                tracing::info!("VLM disabled in config");
                None
            };

            // Create app state
            let state = Arc::new(AppState::new(config, store, graph, inference, vlm));

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

            let mcp_state = state.clone();
            app.manage(state);

            if let Err(err) =
                fndr_lib::meeting::bind_runtime(app.handle().clone(), mcp_state.clone())
            {
                tracing::warn!("Meeting auto-monitor initialization failed: {}", err);
            }

            // Start MCP server so FNDR is discoverable by external MCP clients.
            if let Err(err) =
                tauri::async_runtime::block_on(fndr_lib::mcp::start(mcp_state, None, None))
            {
                tracing::warn!("MCP server startup failed: {}", err);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            api::commands::search,
            api::commands::summarize_search,
            api::commands::ask_fndr,
            api::commands::reconstruct_memory,
            api::commands::summarize_memory,
            api::commands::get_status,
            api::commands::get_mcp_server_status,
            api::commands::start_mcp_server,
            api::commands::stop_mcp_server,
            api::commands::get_meeting_status,
            api::commands::start_meeting_recording,
            api::commands::stop_meeting_recording,
            api::commands::list_meetings,
            api::commands::get_meeting_transcript,
            api::commands::search_meeting_transcripts,
            api::commands::pause_capture,
            api::commands::resume_capture,
            api::commands::get_blocklist,
            api::commands::set_blocklist,
            api::commands::delete_all_data,
            api::commands::get_stats,
            api::commands::get_retention_days,
            api::commands::set_retention_days,
            api::commands::delete_older_than,
            api::commands::get_app_names,
            api::commands::get_todos,
            api::commands::dismiss_todo,
            api::commands::execute_todo,
            // Agent SDK commands
            api::commands::start_agent_task,
            api::commands::get_agent_status,
            api::commands::stop_agent,
            // Graph visualization commands
            api::commands::get_graph_data,
            api::commands::search_graph,
            // Onboarding commands
            api::onboarding::get_onboarding_state,
            api::onboarding::save_onboarding_state,
            api::onboarding::request_biometric_auth,
            api::onboarding::check_permissions,
            api::onboarding::open_system_settings,
            api::onboarding::list_available_models,
            api::onboarding::download_model,
            api::onboarding::check_model_exists,
            api::onboarding::delete_ai_model,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
