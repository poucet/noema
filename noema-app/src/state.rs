//! Bevy resources for UI state

use bevy::prelude::*;
use crossbeam_channel::{Receiver, Sender};
use noema_audio::{AudioPlayback, VoiceCoordinator};
use noema_core::ConversationInfo;
use std::collections::HashMap;
use std::path::PathBuf;

use crate::events::{AppCommand, CoreEvent, DisplayMessage, ModelInfo};

/// Holds channels for communicating with the async backend
#[derive(Resource)]
pub struct CoreConnection {
    pub cmd_tx: Sender<AppCommand>,
    pub event_rx: Receiver<CoreEvent>,
}

/// Chat message history and streaming state
#[derive(Resource, Default)]
pub struct ChatState {
    pub messages: Vec<DisplayMessage>,
    pub streaming_messages: Vec<DisplayMessage>,
    pub is_streaming: bool,
    pub scroll_to_bottom: bool,
}

/// Conversations list and side panel state
#[derive(Resource, Default)]
pub struct ConversationsState {
    pub side_panel_open: bool,
    pub conversations: Vec<ConversationInfo>,
    pub current_conversation_id: Option<String>,
    pub renaming_conversation_id: Option<String>,
    pub rename_text: String,
}

/// Model selection state
#[derive(Resource)]
pub struct ModelState {
    pub model_name: String,
    pub available_models: Vec<ModelInfo>,
    pub selected_model_idx: usize,
    pub models_requested: bool,
}

impl Default for ModelState {
    fn default() -> Self {
        Self {
            model_name: "Loading...".to_string(),
            available_models: Vec::new(),
            selected_model_idx: 0,
            models_requested: false,
        }
    }
}

/// Input text and pending attachments
#[derive(Resource, Default)]
pub struct InputState {
    pub input_text: String,
    pub pending_images: Vec<(String, String)>,
}

/// Status message for errors and info
#[derive(Resource, Default)]
pub struct StatusState {
    pub message: Option<String>,
}

/// Scale factor for UI (1.0 for Mac, 2.5 for Android)
#[derive(Resource)]
pub struct UiScale(pub f32);

impl Default for UiScale {
    fn default() -> Self {
        Self(1.0)
    }
}

/// Voice coordinator resource with listening/transcribing state
#[derive(Resource)]
pub struct VoiceState {
    pub coordinator: Option<VoiceCoordinator>,
    pub whisper_model_path: PathBuf,
    pub listening: bool,
    pub transcribing: bool,
}

impl VoiceState {
    /// Voice is enabled when coordinator is present
    pub fn is_enabled(&self) -> bool {
        self.coordinator.is_some()
    }
}

/// Audio playback resource for playing audio content
#[derive(Resource)]
pub struct AudioPlayer {
    pub playback: Option<AudioPlayback>,
}

impl Default for AudioPlayer {
    fn default() -> Self {
        Self {
            playback: AudioPlayback::new().ok(),
        }
    }
}

/// Cache for decoded images (base64 hash -> egui TextureHandle)
#[derive(Resource, Default)]
pub struct ImageCache {
    pub textures: HashMap<u64, bevy_egui::egui::TextureHandle>,
}

/// Track which audio clips are currently playing
#[derive(Resource, Default)]
pub struct AudioPlayState {
    pub playing: std::collections::HashSet<u64>,
}
