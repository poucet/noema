//! Top panel with title, model dropdown, and status

use bevy::prelude::*;
use bevy_egui::egui;

use crate::events::AppCommand;
use crate::state::{
    ChatState, ConversationsState, CoreConnection, ModelState, StatusState, VoiceState,
};

pub fn render(
    ctx: &mut egui::Context,
    convos: &mut ResMut<ConversationsState>,
    model: &mut ResMut<ModelState>,
    chat: &ResMut<ChatState>,
    voice_state: &ResMut<VoiceState>,
    status: &ResMut<StatusState>,
    connection: &Res<CoreConnection>,
) {
    egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
        ui.horizontal(|ui| {
            render_panel_toggle(ui, convos, connection);
            ui.heading("Noema");
            ui.separator();
            render_model_dropdown(ui, model, connection);
            render_status_indicators(ui, chat, voice_state);
            render_clear_button(ui, connection);
        });

        if let Some(ref msg) = status.message {
            ui.colored_label(egui::Color32::YELLOW, msg);
        }
    });
}

fn render_panel_toggle(
    ui: &mut egui::Ui,
    convos: &mut ResMut<ConversationsState>,
    connection: &Res<CoreConnection>,
) {
    let panel_icon = if convos.side_panel_open { "<" } else { ">" };
    if ui.button(panel_icon).clicked() {
        convos.side_panel_open = !convos.side_panel_open;
        if convos.side_panel_open {
            let _ = connection.cmd_tx.send(AppCommand::ListConversations);
        }
    }
}

fn render_model_dropdown(
    ui: &mut egui::Ui,
    model: &mut ResMut<ModelState>,
    connection: &Res<CoreConnection>,
) {
    let combo = egui::ComboBox::from_label("")
        .selected_text(format!("Model: {}", model.model_name))
        .width(300.0);

    let models_empty = model.available_models.is_empty();
    let models_requested = model.models_requested;
    let models: Vec<_> = model.available_models.clone();
    let selected_idx = model.selected_model_idx;
    let mut new_selection: Option<(usize, String, String)> = None;
    let mut should_request_models = false;

    combo.show_ui(ui, |ui| {
        ui.set_min_width(280.0);
        egui::ScrollArea::vertical()
            .max_height(500.0)
            .show(ui, |ui| {
                if models_empty && !models_requested {
                    should_request_models = true;
                    ui.label("Loading models...");
                } else if models_empty {
                    ui.label("Loading models...");
                } else {
                    let mut current_provider = String::new();
                    for (idx, m) in models.iter().enumerate() {
                        if m.provider != current_provider {
                            if !current_provider.is_empty() {
                                ui.separator();
                            }
                            ui.label(
                                egui::RichText::new(m.provider.to_uppercase())
                                    .small()
                                    .color(egui::Color32::GRAY),
                            );
                            current_provider = m.provider.clone();
                        }

                        let is_selected = idx == selected_idx;
                        if ui.selectable_label(is_selected, &m.display_name).clicked() {
                            new_selection = Some((idx, m.id.clone(), m.provider.clone()));
                        }
                    }
                }
            });
    });

    if should_request_models {
        model.models_requested = true;
        let _ = connection.cmd_tx.send(AppCommand::ListModels);
    }

    if let Some((idx, model_id, provider)) = new_selection {
        model.selected_model_idx = idx;
        let _ = connection.cmd_tx.send(AppCommand::SetModel { model_id, provider });
    }
}

fn render_status_indicators(
    ui: &mut egui::Ui,
    chat: &ResMut<ChatState>,
    voice_state: &ResMut<VoiceState>,
) {
    if chat.is_streaming {
        ui.spinner();
        ui.label("Thinking...");
    }

    if voice_state.is_enabled() {
        ui.separator();
        if voice_state.transcribing {
            ui.spinner();
            ui.label("Transcribing...");
        } else if voice_state.listening {
            ui.colored_label(egui::Color32::from_rgb(255, 100, 100), "Listening...");
        } else {
            ui.label("Voice On");
        }
    }
}

fn render_clear_button(ui: &mut egui::Ui, connection: &Res<CoreConnection>) {
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        if ui.button("Clear").clicked() {
            let _ = connection.cmd_tx.send(AppCommand::ClearHistory);
        }
    });
}
