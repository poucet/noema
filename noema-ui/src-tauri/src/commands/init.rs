//! Application initialization command

use config::PathManager;
use llm::create_model;
use noema_core::{ChatEngine, McpRegistry, SqliteSession, SqliteStore};
use tauri::{AppHandle, State};

use crate::commands::chat::start_engine_event_loop;
use crate::state::AppState;

#[tauri::command]
pub async fn init_app(app: AppHandle, state: State<'_, AppState>) -> Result<String, String> {
    init_storage(&state).await?;
    init_config()?;
    let session = init_session(&state).await?;
    let mcp_registry = init_mcp()?;
    let result = init_engine(&state, session, mcp_registry).await?;

    // Start the engine event loop (runs continuously)
    start_engine_event_loop(app);

    Ok(result)
}

async fn init_storage(state: &AppState) -> Result<(), String> {
    let db_path = PathManager::db_path().ok_or("Failed to determine database path")?;

    if let Some(parent) = db_path.parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create database dir: {}", e))?;
        }
    }

    let store =
        SqliteStore::open(&db_path).map_err(|e| format!("Failed to open database: {}", e))?;

    *state.store.lock().await = Some(store);
    Ok(())
}

fn init_config() -> Result<(), String> {
    config::load_env_file();
    Ok(())
}

async fn init_session(state: &AppState) -> Result<SqliteSession, String> {
    let store_guard = state.store.lock().await;
    let store = store_guard.as_ref().ok_or("Storage not initialized")?;

    let session = store
        .create_conversation()
        .map_err(|e| format!("Failed to create session: {}", e))?;

    let conversation_id = session.conversation_id().to_string();
    drop(store_guard); // Release lock before acquiring another
    *state.current_conversation_id.lock().await = conversation_id;

    Ok(session)
}

fn init_mcp() -> Result<McpRegistry, String> {
    Ok(McpRegistry::load().unwrap_or_else(|_| McpRegistry::new(Default::default())))
}

async fn init_engine(
    state: &AppState,
    session: SqliteSession,
    mcp_registry: McpRegistry,
) -> Result<String, String> {
    let default_model_id = "gemini/models/gemini-2.5-flash";
    let model =
        create_model(default_model_id).map_err(|e| format!("Failed to create model: {}", e))?;

    let model_display_name = default_model_id
        .split('/')
        .last()
        .unwrap_or(default_model_id);

    *state.model_id.lock().await = default_model_id.to_string();
    *state.model_name.lock().await = model_display_name.to_string();

    let engine = ChatEngine::new(session, model, mcp_registry);
    *state.engine.lock().await = Some(engine);

    Ok(model_display_name.to_string())
}
