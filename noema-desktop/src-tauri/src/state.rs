//! Application state management

use noema_audio::BrowserAudioController;
use noema_audio::VoiceCoordinator;
use noema_core::storage::coordinator::StorageCoordinator;
use noema_core::storage::ids::{ConversationId, UserId};
use noema_core::storage::session::Session;
use noema_core::storage::{FsBlobStore, SqliteStore};
use noema_core::{ChatEngine, McpRegistry};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

// ============================================================================
// App Storage Types - Define once via macro
// ============================================================================

/// Define all storage-parameterized types from a single set of type parameters.
/// Usage: define_storage_types!(B, A, T, C, U, D)
macro_rules! define_storage_types {
    ($B:ty, $A:ty, $T:ty, $C:ty, $U:ty, $D:ty) => {
        pub type AppSession = Session<$B, $A, $T, $C, $U, $D>;
        pub type AppCoordinator = StorageCoordinator<$B, $A, $T, $C, $U, $D>;
        pub type AppEngine = ChatEngine<$B, $A, $T, $C, $U, $D>;
    };
}

// Storage types defined once:
// B = FsBlobStore, A = SqliteStore (asset), T = SqliteStore (text)
// C = SqliteStore (conversation), U = SqliteStore (user), D = SqliteStore (document)
define_storage_types!(FsBlobStore, SqliteStore, SqliteStore, SqliteStore, SqliteStore, SqliteStore);

pub struct AppState {
    pub coordinator: Mutex<Option<Arc<AppCoordinator>>>,
    /// Engines per conversation - enables parallel conversations
    pub engines: Mutex<HashMap<ConversationId, AppEngine>>,
    /// MCP registry (shared across all conversations)
    pub mcp_registry: Mutex<Option<Arc<Mutex<McpRegistry>>>>,
    /// Current user ID (from database)
    pub user_id: Mutex<UserId>,
    /// Full model ID in "provider/model" format
    pub model_id: Mutex<String>,
    /// Display name for the model
    pub model_name: Mutex<String>,
    pub voice_coordinator: Mutex<Option<VoiceCoordinator>>,
    /// Which conversation voice input is currently associated with
    pub voice_conversation: Mutex<Option<ConversationId>>,
    /// Maps conversation ID to processing state
    pub processing: Mutex<HashMap<ConversationId, bool>>,
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
            coordinator: Mutex::new(None),
            engines: Mutex::new(HashMap::new()),
            mcp_registry: Mutex::new(None),
            user_id: Mutex::new(UserId::from_string(String::new())),
            model_id: Mutex::new(String::new()),
            model_name: Mutex::new(String::new()),
            voice_coordinator: Mutex::new(None),
            voice_conversation: Mutex::new(None),
            processing: Mutex::new(HashMap::new()),
            pending_oauth_states: Mutex::new(pending_states),
            browser_audio_controller: Mutex::new(None),
            init_lock: std::sync::Mutex::new(false),
        }
    }

    /// Get the MCP registry, returns error if not initialized
    pub async fn get_mcp_registry(&self) -> Result<Arc<Mutex<McpRegistry>>, String> {
        self.mcp_registry
            .lock()
            .await
            .clone()
            .ok_or_else(|| "MCP registry not initialized".to_string())
    }

    /// Check if a conversation is currently processing
    pub async fn is_processing(&self, conversation_id: &ConversationId) -> bool {
        self.processing
            .lock()
            .await
            .get(conversation_id)
            .copied()
            .unwrap_or(false)
    }

    /// Set processing state for a conversation
    pub async fn set_processing(&self, conversation_id: &ConversationId, processing: bool) {
        self.processing
            .lock()
            .await
            .insert(conversation_id.clone(), processing);
    }

    /// Check if the voice conversation is currently processing (for voice buffering)
    pub async fn is_voice_conversation_processing(&self) -> bool {
        if let Some(conv_id) = self.voice_conversation.lock().await.as_ref() {
            self.processing
                .lock()
                .await
                .get(conv_id)
                .copied()
                .unwrap_or(false)
        } else {
            false
        }
    }

    /// Set which conversation voice input is associated with
    pub async fn set_voice_conversation(&self, conversation_id: Option<ConversationId>) {
        *self.voice_conversation.lock().await = conversation_id;
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
