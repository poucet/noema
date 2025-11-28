//! Messages panel displaying chat history

use base64::Engine;
use bevy::prelude::*;
use bevy_egui::egui;
use noema_audio::AudioPlayback;

use crate::events::{DisplayContent, DisplayMessage, DisplayToolResultContent, MessageRole};
use crate::state::{AudioPlayState, AudioPlayer, ChatState, ImageCache};

pub fn render(
    ctx: &mut egui::Context,
    chat: &mut ResMut<ChatState>,
    image_cache: &mut ResMut<ImageCache>,
    audio_player: &Res<AudioPlayer>,
    audio_play_state: &mut ResMut<AudioPlayState>,
) {
    egui::CentralPanel::default().show(ctx, |ui| {
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .drag_to_scroll(true)
            .stick_to_bottom(chat.scroll_to_bottom)
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());

                for msg in &chat.messages {
                    render_message(ui, msg, image_cache, audio_player, audio_play_state);
                }

                if chat.is_streaming && !chat.streaming_messages.is_empty() {
                    for msg in &chat.streaming_messages {
                        render_message(ui, msg, image_cache, audio_player, audio_play_state);
                    }
                }

                if chat.scroll_to_bottom {
                    chat.scroll_to_bottom = false;
                }
            });
    });
}

fn render_message(
    ui: &mut egui::Ui,
    msg: &DisplayMessage,
    image_cache: &mut ResMut<ImageCache>,
    audio_player: &Res<AudioPlayer>,
    audio_play_state: &mut ResMut<AudioPlayState>,
) {
    ui.add_space(8.0);

    let (role_label, role_color, bg_color) = match msg.role {
        MessageRole::User => (
            "🧑 You",
            egui::Color32::from_rgb(100, 180, 255),
            egui::Color32::from_gray(50),
        ),
        MessageRole::Assistant => (
            "🤖 Assistant",
            egui::Color32::from_rgb(100, 200, 100),
            egui::Color32::from_gray(40),
        ),
        MessageRole::System => (
            "⚙️ System",
            egui::Color32::from_rgb(255, 200, 100),
            egui::Color32::from_gray(35),
        ),
    };

    ui.horizontal(|ui| {
        ui.colored_label(role_color, role_label);
    });

    egui::Frame::none()
        .fill(bg_color)
        .rounding(4.0)
        .inner_margin(8.0)
        .show(ui, |ui| {
            for content in &msg.content {
                render_content_block(ui, content, image_cache, audio_player, audio_play_state);
            }
        });
}

fn render_content_block(
    ui: &mut egui::Ui,
    content: &DisplayContent,
    image_cache: &mut ResMut<ImageCache>,
    _audio_player: &Res<AudioPlayer>,
    audio_play_state: &mut ResMut<AudioPlayState>,
) {
    match content {
        DisplayContent::Text(text) => {
            ui.label(text);
        }
        DisplayContent::Image { data, mime_type: _ } => {
            render_image(ui, data, image_cache);
        }
        DisplayContent::Audio { data, mime_type } => {
            render_audio_player(ui, data, mime_type, audio_play_state);
        }
        DisplayContent::ToolCall { name, id } => {
            ui.horizontal(|ui| {
                ui.colored_label(
                    egui::Color32::from_rgb(200, 200, 100),
                    format!("Tool: {} ({})", name, &id[..8.min(id.len())]),
                );
            });
        }
        DisplayContent::ToolResult { id, content } => {
            ui.vertical(|ui| {
                ui.colored_label(
                    egui::Color32::from_rgb(150, 200, 150),
                    format!("Result ({})", &id[..8.min(id.len())]),
                );
                for item in content {
                    match item {
                        DisplayToolResultContent::Text(text) => {
                            ui.label(text);
                        }
                        DisplayToolResultContent::Image { data, mime_type: _ } => {
                            render_image(ui, data, image_cache);
                        }
                        DisplayToolResultContent::Audio { data, mime_type } => {
                            render_audio_player(ui, data, mime_type, audio_play_state);
                        }
                    }
                }
            });
        }
    }
}

fn render_image(ui: &mut egui::Ui, data: &str, image_cache: &mut ResMut<ImageCache>) {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    data.hash(&mut hasher);
    let hash = hasher.finish();

    // Check cache first
    if let Some(texture) = image_cache.textures.get(&hash) {
        let size = texture.size_vec2();
        let max_width = ui.available_width().min(400.0);
        let scale = if size.x > max_width {
            max_width / size.x
        } else {
            1.0
        };
        ui.image(egui::ImageSource::Texture(egui::load::SizedTexture::new(
            texture.id(),
            size * scale,
        )));
        return;
    }

    // Decode and cache
    match base64::engine::general_purpose::STANDARD.decode(data) {
        Ok(bytes) => match image::load_from_memory(&bytes) {
            Ok(img) => {
                let rgba = img.to_rgba8();
                let size = [rgba.width() as usize, rgba.height() as usize];
                let pixels = rgba.into_raw();

                let color_image = egui::ColorImage::from_rgba_unmultiplied(size, &pixels);
                let texture = ui.ctx().load_texture(
                    format!("img_{}", hash),
                    color_image,
                    egui::TextureOptions::default(),
                );

                let tex_size = texture.size_vec2();
                let max_width = ui.available_width().min(400.0);
                let scale = if tex_size.x > max_width {
                    max_width / tex_size.x
                } else {
                    1.0
                };

                ui.image(egui::ImageSource::Texture(egui::load::SizedTexture::new(
                    texture.id(),
                    tex_size * scale,
                )));

                image_cache.textures.insert(hash, texture);
            }
            Err(e) => {
                ui.colored_label(egui::Color32::RED, format!("Failed to decode image: {}", e));
            }
        },
        Err(e) => {
            ui.colored_label(egui::Color32::RED, format!("Invalid base64 image: {}", e));
        }
    }
}

fn render_audio_player(
    ui: &mut egui::Ui,
    data: &str,
    mime_type: &str,
    audio_play_state: &mut ResMut<AudioPlayState>,
) {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    data.hash(&mut hasher);
    let hash = hasher.finish();

    let is_playing = audio_play_state.playing.contains(&hash);

    ui.horizontal(|ui| {
        let button_text = if is_playing { "Stop" } else { "Play" };

        if ui.button(button_text).clicked() {
            if is_playing {
                audio_play_state.playing.remove(&hash);
            } else if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(data) {
                audio_play_state.playing.insert(hash);

                // Spawn a thread to play audio (simplified - assumes raw PCM)
                std::thread::spawn(move || {
                    if let Ok(pb) = AudioPlayback::new() {
                        let samples: Vec<f32> = bytes
                            .chunks_exact(2)
                            .map(|chunk| {
                                let sample = i16::from_le_bytes([chunk[0], chunk[1]]);
                                sample as f32 / i16::MAX as f32
                            })
                            .collect();
                        let _ = pb.play_samples(&samples);
                    }
                });
            }
        }

        ui.label(format!("Audio ({})", mime_type));
    });
}
