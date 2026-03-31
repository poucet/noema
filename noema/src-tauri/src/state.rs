//! Application state management

use simply_audio::BrowserAudioController;
use simply_audio::VoiceCoordinator;
use simply_daemon::embedded::EmbeddedDaemon;
use simply_daemon::storage::SqliteStorage;
use simply_daemon::types::ConversationId;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, OnceCell};

pub type AppDaemon = EmbeddedDaemon<SqliteStorage>;

pub struct AppState {
    /// The daemon — primary API for everything
    pub daemon: OnceCell<Arc<AppDaemon>>,
    pub voice_coordinator: Mutex<Option<VoiceCoordinator>>,
    pub voice_conversation: Mutex<Option<ConversationId>>,
    pub processing: Mutex<HashMap<ConversationId, bool>>,
    pub forwarders: Mutex<HashMap<String, tokio::task::JoinHandle<()>>>,
    pub pending_oauth_states: Mutex<HashMap<String, String>>,
    pub browser_audio_controller: Mutex<Option<BrowserAudioController>>,
}

impl AppState {
    pub fn new() -> Self {
        let pending_states = load_pending_oauth_states().unwrap_or_default();

        Self {
            daemon: OnceCell::new(),
            voice_coordinator: Mutex::new(None),
            voice_conversation: Mutex::new(None),
            processing: Mutex::new(HashMap::new()),
            forwarders: Mutex::new(HashMap::new()),
            pending_oauth_states: Mutex::new(pending_states),
            browser_audio_controller: Mutex::new(None),
        }
    }

    pub fn get_daemon(&self) -> Result<Arc<AppDaemon>, String> {
        self.daemon.get().cloned().ok_or_else(|| "Daemon not initialized".to_string())
    }

    pub fn is_initialized(&self) -> bool {
        self.daemon.get().is_some()
    }

    pub async fn is_processing(&self, conversation_id: &ConversationId) -> bool {
        self.processing.lock().await.get(conversation_id).copied().unwrap_or(false)
    }

    pub async fn set_processing(&self, conversation_id: &ConversationId, processing: bool) {
        self.processing.lock().await.insert(conversation_id.clone(), processing);
    }

    pub async fn is_voice_conversation_processing(&self) -> bool {
        if let Some(conv_id) = self.voice_conversation.lock().await.as_ref() {
            self.processing.lock().await.get(conv_id).copied().unwrap_or(false)
        } else {
            false
        }
    }

    pub async fn set_voice_conversation(&self, conversation_id: Option<ConversationId>) {
        *self.voice_conversation.lock().await = conversation_id;
    }
}

impl Default for AppState {
    fn default() -> Self { Self::new() }
}

pub fn get_oauth_states_path() -> Option<std::path::PathBuf> {
    use config::PathManager;
    PathManager::data_dir().map(|d| d.join("pending_oauth.json"))
}

pub fn load_pending_oauth_states() -> Option<HashMap<String, String>> {
    let path = get_oauth_states_path()?;
    let content = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&content).ok()
}

pub fn save_pending_oauth_states(states: &HashMap<String, String>) -> Result<(), String> {
    let path = get_oauth_states_path().ok_or("Could not determine data directory")?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let content = serde_json::to_string(states).map_err(|e| e.to_string())?;
    std::fs::write(&path, content).map_err(|e| e.to_string())
}
