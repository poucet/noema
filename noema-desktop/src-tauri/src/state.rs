//! Application state management

use noema_audio::BrowserAudioController;
use noema_audio::VoiceCoordinator;
use noema_core::storage::coordinator::StorageCoordinator;
use noema_core::storage::ids::UserId;
use noema_core::storage::{FsBlobStore, SqliteStore};
use noema_core::ChatEngine;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Type alias for our concrete coordinator type
pub type AppCoordinator = StorageCoordinator<FsBlobStore, SqliteStore, SqliteStore, SqliteStore>;
/// Type alias for our concrete engine type
pub type AppEngine = ChatEngine<SqliteStore, SqliteStore, FsBlobStore, SqliteStore>;

pub struct AppState {
    pub store: Mutex<Option<Arc<SqliteStore>>>,
    pub coordinator: Mutex<Option<Arc<AppCoordinator>>>,
    pub engine: Mutex<Option<AppEngine>>,
    pub current_conversation_id: Mutex<String>,
    /// Current thread ID within the conversation (None = main thread)
    pub current_thread_id: Mutex<Option<String>>,
    /// Current user ID (from database)
    pub user_id: Mutex<UserId>,
    /// Full model ID in "provider/model" format
    pub model_id: Mutex<String>,
    /// Display name for the model
    pub model_name: Mutex<String>,
    pub voice_coordinator: Mutex<Option<VoiceCoordinator>>,
    pub is_processing: Mutex<bool>,
    /// Maps OAuth state parameter to server ID for pending OAuth flows
    pub pending_oauth_states: Mutex<HashMap<String, String>>,
    /// Browser voice controller for WebAudio-based input
    pub browser_audio_controller: Mutex<Option<BrowserAudioController>>,
    /// Lock to prevent concurrent initialization (React StrictMode calls init twice)
    pub init_lock: std::sync::Mutex<bool>,
}

impl AppState {
    pub fn new() -> Self {
        // Load pending OAuth states from disk
        let pending_states = load_pending_oauth_states().unwrap_or_default();

        Self {
            store: Mutex::new(None),
            coordinator: Mutex::new(None),
            engine: Mutex::new(None),
            current_conversation_id: Mutex::new(String::new()),
            current_thread_id: Mutex::new(None),
            user_id: Mutex::new(UserId::from_string(String::new())),
            model_id: Mutex::new(String::new()),
            model_name: Mutex::new(String::new()),
            voice_coordinator: Mutex::new(None),
            is_processing: Mutex::new(false),
            pending_oauth_states: Mutex::new(pending_states),
            browser_audio_controller: Mutex::new(None),
            init_lock: std::sync::Mutex::new(false),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

/// Get the path to the pending OAuth states file
pub fn get_oauth_states_path() -> Option<std::path::PathBuf> {
    use config::PathManager;
    PathManager::data_dir().map(|d| d.join("pending_oauth.json"))
}

/// Load pending OAuth states from disk
pub fn load_pending_oauth_states() -> Option<HashMap<String, String>> {
    let path = get_oauth_states_path()?;
    let content = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&content).ok()
}

/// Save pending OAuth states to disk
pub fn save_pending_oauth_states(states: &HashMap<String, String>) -> Result<(), String> {
    let path = get_oauth_states_path().ok_or("Could not determine data directory")?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let content = serde_json::to_string(states).map_err(|e| e.to_string())?;
    std::fs::write(&path, content).map_err(|e| e.to_string())
}
