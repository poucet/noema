//! Application state management

use noema_audio::BrowserAudioController;
use noema_audio::VoiceCoordinator;
use noema_core::storage::coordinator::StorageCoordinator;
use noema_core::storage::ids::{ConversationId, UserId};
use noema_core::storage::traits::StorageTypes;
use noema_core::storage::{FsBlobStore, SqliteStore, Stores};
use noema_core::{ConversationManager, ManagerEvent, McpRegistry};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex, OnceCell};

// ============================================================================
// App Storage Types - Define once via StorageTypes
// ============================================================================

/// Application storage configuration.
///
/// Defines all storage type associations in one place.
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

/// Holds all store instances for the application.
///
/// Uses a single SqliteStore for all SQL-based stores, sharing the connection pool.
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
    fn turn(&self) -> Arc<SqliteStore> {
        self.sqlite.clone()
    }
    fn user(&self) -> Arc<SqliteStore> {
        self.sqlite.clone()
    }
    fn document(&self) -> Arc<SqliteStore> {
        self.sqlite.clone()
    }
    fn blob(&self) -> Arc<FsBlobStore> {
        self.blob.clone()
    }
    fn asset(&self) -> Arc<SqliteStore> {
        self.sqlite.clone()
    }
    fn text(&self) -> Arc<SqliteStore> {
        self.sqlite.clone()
    }
    fn entity(&self) -> Arc<SqliteStore> {
        self.sqlite.clone()
    }
    fn reference(&self) -> Arc<SqliteStore> {
        self.sqlite.clone()
    }
    fn collection(&self) -> Arc<SqliteStore> {
        self.sqlite.clone()
    }
}

pub type AppCoordinator = StorageCoordinator<AppStorage>;
pub type AppManager = ConversationManager<AppStorage>;

/// Tagged event for routing to UI - includes conversation ID for dispatch
pub type TaggedEvent = (ConversationId, ManagerEvent);
pub type EventSender = mpsc::UnboundedSender<TaggedEvent>;
pub type EventReceiver = mpsc::UnboundedReceiver<TaggedEvent>;

pub struct AppState {
    /// All stores - initialized once at startup
    stores: OnceCell<AppStores>,
    /// Storage coordinator for multi-store operations - initialized once at startup
    pub coordinator: OnceCell<Arc<AppCoordinator>>,
    /// Managers per conversation - enables parallel conversations
    pub managers: Mutex<HashMap<ConversationId, AppManager>>,
    /// MCP registry (shared across all conversations) - initialized once at startup
    pub mcp_registry: OnceCell<Arc<Mutex<McpRegistry>>>,
    /// Shared event sender - managers send events here tagged with conversation ID
    pub event_tx: EventSender,
    /// Shared event receiver - single consumer dispatches to UI
    pub event_rx: Mutex<Option<EventReceiver>>,
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

        // Create shared event channel
        let (event_tx, event_rx) = mpsc::unbounded_channel();

        Self {
            stores: OnceCell::new(),
            coordinator: OnceCell::new(),
            managers: Mutex::new(HashMap::new()),
            mcp_registry: OnceCell::new(),
            event_tx,
            event_rx: Mutex::new(Some(event_rx)),
            user_id: Mutex::new(UserId::new()),
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

    /// Take the event receiver (can only be called once)
    pub async fn take_event_receiver(&self) -> Option<EventReceiver> {
        self.event_rx.lock().await.take()
    }

    /// Get a clone of the event sender for passing to managers
    pub fn event_sender(&self) -> EventSender {
        self.event_tx.clone()
    }

    /// Get stores, returns error if not initialized
    pub fn get_stores(&self) -> Result<&AppStores, String> {
        self.stores
            .get()
            .ok_or_else(|| "Storage not initialized".to_string())
    }

    /// Initialize stores (called once at startup)
    pub fn init_stores(&self, stores: AppStores) -> Result<(), String> {
        self.stores
            .set(stores)
            .map_err(|_| "Stores already initialized".to_string())
    }

    /// Get the coordinator, returns error if not initialized
    pub fn get_coordinator(&self) -> Result<Arc<AppCoordinator>, String> {
        self.coordinator
            .get()
            .cloned()
            .ok_or_else(|| "Storage not initialized".to_string())
    }

    /// Get the MCP registry, returns error if not initialized
    pub fn get_mcp_registry(&self) -> Result<Arc<Mutex<McpRegistry>>, String> {
        self.mcp_registry
            .get()
            .cloned()
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
