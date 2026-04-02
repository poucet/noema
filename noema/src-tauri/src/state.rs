//! Application state management

use simply_audio::BrowserAudioController;
use simply_audio::VoiceCoordinator;
use simply_daemon::api::DaemonApi;
use simply_daemon::types::ConversationId;
use simply_daemon::ws::DaemonHandle;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, OnceCell};

pub struct AppState {
    /// The daemon — primary API for everything (embedded or remote)
    pub daemon: OnceCell<Arc<dyn DaemonApi>>,
    /// Keeps the daemon handle alive (owns WS server if we're the host)
    pub _daemon_handle: OnceCell<DaemonHandle>,
    pub voice_coordinator: Mutex<Option<VoiceCoordinator>>,
    pub voice_conversation: Mutex<Option<ConversationId>>,
    pub processing: Mutex<HashMap<ConversationId, bool>>,
    pub forwarders: Mutex<HashMap<String, tokio::task::JoinHandle<()>>>,
    pub browser_audio_controller: Mutex<Option<BrowserAudioController>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            daemon: OnceCell::new(),
            _daemon_handle: OnceCell::new(),
            voice_coordinator: Mutex::new(None),
            voice_conversation: Mutex::new(None),
            processing: Mutex::new(HashMap::new()),
            forwarders: Mutex::new(HashMap::new()),
            browser_audio_controller: Mutex::new(None),
        }
    }

    pub fn get_daemon(&self) -> Result<Arc<dyn DaemonApi>, String> {
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
