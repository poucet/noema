//! Application state management

use config::ProviderUrls;
use noema_audio::{BrowserVoiceSession, VoiceCoordinator};
use noema_core::{ChatEngine, SqliteSession, SqliteStore};
use std::collections::HashMap;
use tokio::sync::Mutex;

pub struct AppState {
    pub store: Mutex<Option<SqliteStore>>,
    pub engine: Mutex<Option<ChatEngine<SqliteSession>>>,
    pub current_conversation_id: Mutex<String>,
    pub model_name: Mutex<String>,
    pub provider_urls: Mutex<ProviderUrls>,
    pub voice_coordinator: Mutex<Option<VoiceCoordinator>>,
    pub is_processing: Mutex<bool>,
    /// Maps OAuth state parameter to server ID for pending OAuth flows
    pub pending_oauth_states: Mutex<HashMap<String, String>>,
    /// Browser voice session for WebAudio-based input
    pub browser_voice_session: Mutex<Option<BrowserVoiceSession>>,
}

impl AppState {
    pub fn new() -> Self {
        // Load pending OAuth states from disk
        let pending_states = load_pending_oauth_states().unwrap_or_default();

        Self {
            store: Mutex::new(None),
            engine: Mutex::new(None),
            current_conversation_id: Mutex::new(String::new()),
            model_name: Mutex::new(String::new()),
            provider_urls: Mutex::new(ProviderUrls::default()),
            voice_coordinator: Mutex::new(None),
            is_processing: Mutex::new(false),
            pending_oauth_states: Mutex::new(pending_states),
            browser_voice_session: Mutex::new(None),
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
    dirs::data_dir().map(|d| d.join("noema").join("pending_oauth.json"))
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
