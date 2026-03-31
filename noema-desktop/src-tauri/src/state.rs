//! Application state management

use simply_audio::BrowserAudioController;
use simply_audio::VoiceCoordinator;
use simply_daemon::types::{ConversationId, UserId};
use simply_daemon::embedded::EmbeddedDaemon;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, OnceCell};

// Storage type aliases — init.rs constructs these, state just holds the Arc.
// gdocs/mcp commands need direct store access until those move to the daemon API.
pub use simply_core::storage::coordinator::StorageCoordinator;
pub use simply_core::storage::traits::StorageTypes;
pub use simply_core::storage::{FsBlobStore, SqliteStore, Stores};
pub use simply_core::McpRegistry;

pub struct AppStorage;

impl StorageTypes for AppStorage {
    type Blob = FsBlobStore;
    type Asset = SqliteStore;
    type Text = SqliteStore;
    type Turn = SqliteStore;
    type User = SqliteStore;
    type Document = SqliteStore;
    type Entity = SqliteStore;
    type Reference = SqliteStore;
    type Collection = SqliteStore;
}

pub struct AppStores {
    sqlite: Arc<SqliteStore>,
    blob: Arc<FsBlobStore>,
}

impl AppStores {
    pub fn new(sqlite: Arc<SqliteStore>, blob: Arc<FsBlobStore>) -> Self {
        Self { sqlite, blob }
    }
}

impl Stores<AppStorage> for AppStores {
    fn turn(&self) -> Arc<SqliteStore> { self.sqlite.clone() }
    fn user(&self) -> Arc<SqliteStore> { self.sqlite.clone() }
    fn document(&self) -> Arc<SqliteStore> { self.sqlite.clone() }
    fn blob(&self) -> Arc<FsBlobStore> { self.blob.clone() }
    fn asset(&self) -> Arc<SqliteStore> { self.sqlite.clone() }
    fn text(&self) -> Arc<SqliteStore> { self.sqlite.clone() }
    fn entity(&self) -> Arc<SqliteStore> { self.sqlite.clone() }
    fn reference(&self) -> Arc<SqliteStore> { self.sqlite.clone() }
    fn collection(&self) -> Arc<SqliteStore> { self.sqlite.clone() }
}

pub type AppCoordinator = StorageCoordinator<AppStorage>;
pub type AppDaemon = EmbeddedDaemon<AppStorage>;

pub struct AppState {
    /// Storage stores — needed by gdocs/mcp commands that bypass the daemon
    stores: OnceCell<AppStores>,
    /// Storage coordinator — needed by gdocs commands
    pub coordinator: OnceCell<Arc<AppCoordinator>>,
    /// The daemon — primary API for sessions, conversations, models
    pub daemon: OnceCell<Arc<AppDaemon>>,
    /// MCP registry — shared between daemon and MCP config commands
    pub mcp_registry: OnceCell<Arc<Mutex<McpRegistry>>>,
    pub user_id: Mutex<UserId>,
    pub voice_coordinator: Mutex<Option<VoiceCoordinator>>,
    pub voice_conversation: Mutex<Option<ConversationId>>,
    pub processing: Mutex<HashMap<ConversationId, bool>>,
    /// Active event forwarder tasks per session
    pub forwarders: Mutex<HashMap<String, tokio::task::JoinHandle<()>>>,
    pub pending_oauth_states: Mutex<HashMap<String, String>>,
    pub browser_audio_controller: Mutex<Option<BrowserAudioController>>,
    pub init_lock: std::sync::Mutex<bool>,
}

impl AppState {
    pub fn new() -> Self {
        let pending_states = load_pending_oauth_states().unwrap_or_default();

        Self {
            stores: OnceCell::new(),
            coordinator: OnceCell::new(),
            daemon: OnceCell::new(),
            mcp_registry: OnceCell::new(),
            user_id: Mutex::new(UserId::new()),
            voice_coordinator: Mutex::new(None),
            voice_conversation: Mutex::new(None),
            processing: Mutex::new(HashMap::new()),
            forwarders: Mutex::new(HashMap::new()),
            pending_oauth_states: Mutex::new(pending_states),
            browser_audio_controller: Mutex::new(None),
            init_lock: std::sync::Mutex::new(false),
        }
    }

    /// Direct store access (for gdocs/mcp commands that bypass the daemon)
    pub fn get_stores(&self) -> Result<&AppStores, String> {
        self.stores.get().ok_or_else(|| "Storage not initialized".to_string())
    }

    pub fn init_stores(&self, stores: AppStores) -> Result<(), String> {
        self.stores.set(stores).map_err(|_| "Stores already initialized".to_string())
    }

    pub fn get_coordinator(&self) -> Result<Arc<AppCoordinator>, String> {
        self.coordinator.get().cloned().ok_or_else(|| "Storage not initialized".to_string())
    }

    pub fn get_daemon(&self) -> Result<Arc<AppDaemon>, String> {
        self.daemon.get().cloned().ok_or_else(|| "Daemon not initialized".to_string())
    }

    pub fn get_mcp_registry(&self) -> Result<Arc<Mutex<McpRegistry>>, String> {
        self.mcp_registry.get().cloned().ok_or_else(|| "MCP registry not initialized".to_string())
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
