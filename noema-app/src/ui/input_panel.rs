//! Input panel with text input, send button, and voice toggle

use bevy::prelude::*;
use bevy_egui::egui;
use llm::{ChatPayload, ContentBlock};
use noema_audio::{VoiceAgent, VoiceCoordinator};

use crate::events::AppCommand;
use crate::state::{ChatState, CoreConnection, InputState, StatusState, VoiceState};

pub fn render(
    ctx: &mut egui::Context,
    input: &mut ResMut<InputState>,
    chat: &mut ResMut<ChatState>,
    status: &mut ResMut<StatusState>,
    voice_state: &mut ResMut<VoiceState>,
    connection: &Res<CoreConnection>,
) {
    let bottom_height = if input.pending_images.is_empty() {
        56.0
    } else {
        90.0
    };

    egui::TopBottomPanel::bottom("input_panel")
        .exact_height(bottom_height)
        .show(ctx, |ui| {
            render_attachments(ui, input);
            ui.add_space(8.0);
            render_input_row(ui, input, chat, status, voice_state, connection);
        });
}

fn render_attachments(ui: &mut egui::Ui, input: &mut ResMut<InputState>) {
    if input.pending_images.is_empty() {
        return;
    }

    ui.horizontal(|ui| {
        ui.label("Attachments:");
        let mut to_remove = Vec::new();
        for (i, (_, mime)) in input.pending_images.iter().enumerate() {
            let label = format!("{} x", mime.split('/').last().unwrap_or("image"));
            if ui.button(&label).clicked() {
                to_remove.push(i);
            }
        }
        for i in to_remove.into_iter().rev() {
            input.pending_images.remove(i);
        }
    });
    ui.add_space(4.0);
}

fn render_input_row(
    ui: &mut egui::Ui,
    input: &mut ResMut<InputState>,
    chat: &mut ResMut<ChatState>,
    status: &mut ResMut<StatusState>,
    voice_state: &mut ResMut<VoiceState>,
    connection: &Res<CoreConnection>,
) {
    ui.horizontal(|ui| {
        let available = ui.available_width();
        let button_width = 120.0;
        let input_width = (available - button_width).max(100.0);

        let input_response = ui.add_sized(
            [input_width, 32.0],
            egui::TextEdit::singleline(&mut input.input_text)
                .hint_text("Type a message... (or drop images)"),
        );

        let enter_pressed =
            input_response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        let cmd_enter = ui.input(|i| i.key_pressed(egui::Key::Enter) && i.modifiers.command);
        let send_clicked = ui.add_sized([60.0, 32.0], egui::Button::new("Send")).clicked();

        render_mic_button(ui, status, voice_state);

        if (send_clicked || enter_pressed || cmd_enter)
            && (!input.input_text.trim().is_empty() || !input.pending_images.is_empty())
            && !chat.is_streaming
        {
            send_message(input, chat, status, connection);
        }

        if !chat.is_streaming {
            input_response.request_focus();
        }
    });
}

fn render_mic_button(
    ui: &mut egui::Ui,
    status: &mut ResMut<StatusState>,
    voice_state: &mut ResMut<VoiceState>,
) {
    let mic_icon = if voice_state.is_enabled() { "Mic" } else { "Mute" };
    let mic_button = ui.add_sized([40.0, 32.0], egui::Button::new(mic_icon));

    if mic_button.clicked() {
        if voice_state.is_enabled() {
            voice_state.coordinator = None;
            voice_state.listening = false;
            voice_state.transcribing = false;
            status.message = Some("Voice disabled".to_string());
        } else {
            match VoiceAgent::new(&voice_state.whisper_model_path) {
                Ok(agent) => {
                    voice_state.coordinator = Some(VoiceCoordinator::new(agent));
                    status.message = Some("Voice enabled - speak to send".to_string());
                }
                Err(e) => {
                    status.message = Some(format!("Voice init failed: {}", e));
                }
            }
        }
    }

    mic_button.on_hover_text(if voice_state.is_enabled() {
        "Disable voice input"
    } else {
        "Enable voice input"
    });
}

fn send_message(
    input: &mut ResMut<InputState>,
    chat: &mut ResMut<ChatState>,
    status: &mut ResMut<StatusState>,
    connection: &Res<CoreConnection>,
) {
    let text = std::mem::take(&mut input.input_text);
    let images = std::mem::take(&mut input.pending_images);

    let mut content_blocks = Vec::new();
    if !text.trim().is_empty() {
        content_blocks.push(ContentBlock::Text { text });
    }
    for (data, mime_type) in images {
        content_blocks.push(ContentBlock::Image { data, mime_type });
    }

    let payload = ChatPayload::new(content_blocks);
    let _ = connection.cmd_tx.send(AppCommand::SendMessage(payload));
    chat.scroll_to_bottom = true;
    status.message = None;
}
