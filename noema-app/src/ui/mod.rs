//! UI module - composable egui panels

mod input_panel;
mod messages_panel;
mod side_panel;
mod top_panel;

use bevy::prelude::*;
use bevy_egui::EguiContexts;

use crate::state::{
    AudioPlayState, AudioPlayer, ChatState, ConversationsState, CoreConnection, ImageCache,
    InputState, ModelState, StatusState, VoiceState,
};

/// Main UI system that composes all panels
pub fn ui_system(
    mut contexts: EguiContexts,
    mut chat: ResMut<ChatState>,
    mut convos: ResMut<ConversationsState>,
    mut model: ResMut<ModelState>,
    mut input: ResMut<InputState>,
    mut status: ResMut<StatusState>,
    connection: Res<CoreConnection>,
    mut voice_state: ResMut<VoiceState>,
    mut image_cache: ResMut<ImageCache>,
    audio_player: Res<AudioPlayer>,
    mut audio_play_state: ResMut<AudioPlayState>,
) {
    let ctx = contexts.ctx_mut();

    side_panel::render(ctx, &mut convos, &connection);

    top_panel::render(
        ctx,
        &mut convos,
        &mut model,
        &chat,
        &voice_state,
        &status,
        &connection,
    );

    input_panel::render(
        ctx,
        &mut input,
        &mut chat,
        &mut status,
        &mut voice_state,
        &connection,
    );

    messages_panel::render(
        ctx,
        &mut chat,
        &mut image_cache,
        &audio_player,
        &mut audio_play_state,
    );
}
