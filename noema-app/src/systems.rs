//! Bevy systems for event handling, voice, file drops, and egui setup

use base64::Engine;
use bevy::prelude::*;
use bevy_egui::EguiContexts;
use llm::ChatPayload;

use crate::events::{AppCommand, CoreEvent};
use crate::state::{
    ChatState, ConversationsState, CoreConnection, InputState, ModelState, StatusState, UiScale,
    VoiceState,
};

/// System to read events from the Core and update UI state
pub fn event_reader_system(
    connection: Res<CoreConnection>,
    mut chat: ResMut<ChatState>,
    mut convos: ResMut<ConversationsState>,
    mut model: ResMut<ModelState>,
    mut status: ResMut<StatusState>,
) {
    while let Ok(event) = connection.event_rx.try_recv() {
        match event {
            CoreEvent::HistoryLoaded(messages) => {
                chat.messages = messages;
                chat.scroll_to_bottom = true;
            }
            CoreEvent::MessageReceived(msg) => {
                chat.messages.push(msg);
                chat.scroll_to_bottom = true;
            }
            CoreEvent::UserMessageSent(msg) => {
                // Immediately show user message
                chat.messages.push(msg);
                chat.scroll_to_bottom = true;
                chat.is_streaming = true;
            }
            CoreEvent::StreamingMessage(msg) => {
                chat.streaming_messages.push(msg);
                chat.is_streaming = true;
            }
            CoreEvent::MessageComplete => {
                chat.streaming_messages.clear();
                chat.is_streaming = false;
                chat.scroll_to_bottom = true;
            }
            CoreEvent::Error(err) => {
                status.message = Some(format!("Error: {}", err));
                chat.is_streaming = false;
            }
            CoreEvent::ModelChanged(name) => {
                model.model_name = name;
                status.message = None;
            }
            CoreEvent::HistoryCleared => {
                chat.messages.clear();
                chat.streaming_messages.clear();
                status.message = Some("History cleared".to_string());
            }
            CoreEvent::ConversationsList(list) => {
                convos.conversations = list;
            }
            CoreEvent::ConversationSwitched(id) => {
                convos.current_conversation_id = Some(id);
                chat.streaming_messages.clear();
                chat.is_streaming = false;
            }
            CoreEvent::ConversationCreated(id) => {
                convos.current_conversation_id = Some(id);
            }
            CoreEvent::ConversationRenamed => {
                convos.renaming_conversation_id = None;
                convos.rename_text.clear();
            }
            CoreEvent::ModelsList(models) => {
                model.available_models = models;
            }
        }
    }
}

/// System to process voice events
pub fn voice_system(
    mut voice_state: ResMut<VoiceState>,
    mut chat: ResMut<ChatState>,
    mut status: ResMut<StatusState>,
    connection: Res<CoreConnection>,
) {
    if let Some(ref mut coordinator) = voice_state.coordinator {
        // Read voice state flags first
        let listening = coordinator.is_listening();
        let transcribing = coordinator.is_transcribing();

        // Process voice events - buffer if streaming
        let (messages, errors) = coordinator.process(chat.is_streaming);

        // Update voice state flags after processing
        voice_state.listening = listening;
        voice_state.transcribing = transcribing;

        // Send transcribed messages
        for msg in messages {
            if !msg.trim().is_empty() {
                let _ = connection
                    .cmd_tx
                    .send(AppCommand::SendMessage(ChatPayload::text(msg)));
                chat.is_streaming = true;
                chat.scroll_to_bottom = true;
            }
        }

        // Report errors
        for err in errors {
            status.message = Some(format!("Voice error: {}", err));
        }
    }
}

/// One-time setup system for egui styling
pub fn setup_egui(mut contexts: EguiContexts, ui_scale: Res<UiScale>) {
    let ctx = contexts.ctx_mut();
    ctx.set_pixels_per_point(ui_scale.0);
}

/// Handle file drops for image attachments
pub fn file_drop_system(
    mut input: ResMut<InputState>,
    mut status: ResMut<StatusState>,
    mut file_drag_and_drop_events: EventReader<bevy::window::FileDragAndDrop>,
) {
    for event in file_drag_and_drop_events.read() {
        if let bevy::window::FileDragAndDrop::DroppedFile { path_buf, .. } = event {
            let ext = path_buf
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();

            let mime_type = match ext.as_str() {
                "png" => Some("image/png"),
                "jpg" | "jpeg" => Some("image/jpeg"),
                "gif" => Some("image/gif"),
                "webp" => Some("image/webp"),
                _ => None,
            };

            if let Some(mime) = mime_type {
                if let Ok(data) = std::fs::read(path_buf) {
                    let base64_data = base64::engine::general_purpose::STANDARD.encode(&data);
                    input.pending_images.push((base64_data, mime.to_string()));
                    status.message = Some(format!(
                        "Attached: {}",
                        path_buf.file_name().unwrap_or_default().to_string_lossy()
                    ));
                }
            }
        }
    }
}
