//! Side panel for conversation list

use bevy::prelude::*;
use bevy_egui::egui;

use crate::events::AppCommand;
use crate::state::{ConversationsState, CoreConnection};

pub fn render(
    ctx: &mut egui::Context,
    convos: &mut ResMut<ConversationsState>,
    connection: &Res<CoreConnection>,
) {
    egui::SidePanel::left("conversations_panel")
        .resizable(true)
        .default_width(220.0)
        .show_animated(ctx, convos.side_panel_open, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Conversations");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("+").clicked() {
                        let _ = connection.cmd_tx.send(AppCommand::NewConversation);
                    }
                });
            });
            ui.separator();

            egui::ScrollArea::vertical().show(ui, |ui| {
                let current_id = convos.current_conversation_id.clone();
                let renaming_id = convos.renaming_conversation_id.clone();
                let conversations: Vec<_> = convos.conversations.clone();
                let mut switch_to = None;
                let mut delete_id = None;
                let mut start_rename: Option<(String, Option<String>)> = None;
                let mut finish_rename = None;
                let mut cancel_rename = false;

                for convo in &conversations {
                    let is_current = current_id.as_ref() == Some(&convo.id);
                    let is_renaming = renaming_id.as_ref() == Some(&convo.id);

                    let name = convo.name.clone().unwrap_or_else(|| {
                        let now = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_secs() as i64)
                            .unwrap_or(0);
                        let age_secs = now - convo.updated_at;
                        let age_str = if age_secs < 60 {
                            "just now".to_string()
                        } else if age_secs < 3600 {
                            format!("{}m ago", age_secs / 60)
                        } else if age_secs < 86400 {
                            format!("{}h ago", age_secs / 3600)
                        } else {
                            format!("{}d ago", age_secs / 86400)
                        };
                        format!("Chat ({}) - {}", convo.message_count, age_str)
                    });

                    if is_renaming {
                        let response = ui.text_edit_singleline(&mut convos.rename_text);
                        if response.lost_focus() {
                            if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                                finish_rename =
                                    Some((convo.id.clone(), convos.rename_text.clone()));
                            } else {
                                cancel_rename = true;
                            }
                        }
                        response.request_focus();
                    } else {
                        ui.horizontal(|ui| {
                            let label = if is_current {
                                egui::RichText::new(&name).strong()
                            } else {
                                egui::RichText::new(&name)
                            };

                            let response = ui.selectable_label(is_current, label);
                            if response.clicked() && !is_current {
                                switch_to = Some(convo.id.clone());
                            }

                            response.context_menu(|ui| {
                                if ui.button("Rename").clicked() {
                                    start_rename = Some((convo.id.clone(), convo.name.clone()));
                                    ui.close_menu();
                                }
                                if !is_current && ui.button("Delete").clicked() {
                                    delete_id = Some(convo.id.clone());
                                    ui.close_menu();
                                }
                            });

                            if !is_current {
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui.small_button("x").clicked() {
                                            delete_id = Some(convo.id.clone());
                                        }
                                    },
                                );
                            }
                        });
                    }
                }

                if let Some(id) = switch_to {
                    let _ = connection.cmd_tx.send(AppCommand::SwitchConversation(id));
                }
                if let Some(id) = delete_id {
                    let _ = connection.cmd_tx.send(AppCommand::DeleteConversation(id));
                }
                if let Some((id, current_name)) = start_rename {
                    convos.rename_text = current_name.unwrap_or_default();
                    convos.renaming_conversation_id = Some(id);
                }
                if cancel_rename {
                    convos.renaming_conversation_id = None;
                    convos.rename_text.clear();
                }
                if let Some((id, name)) = finish_rename {
                    let _ = connection.cmd_tx.send(AppCommand::RenameConversation { id, name });
                }
            });
        });
}
