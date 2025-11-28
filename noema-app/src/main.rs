//! Noema GUI - Native Bevy/egui wrapper for the Noema LLM client
//!
//! Architecture:
//! - Main thread (Bevy): Windowing, input, rendering, egui layout, voice input
//! - Background thread (Tokio): HTTP, MCP, database I/O, OAuth
//!
//! Communication via crossbeam channels (AppCommand -> Core, CoreEvent -> UI)

mod backend;
mod events;
mod state;
mod systems;
mod ui;

use bevy::prelude::*;
use bevy_egui::EguiPlugin;
use crossbeam_channel::unbounded;

use config::{load_env_file, ProviderUrls};

use backend::{get_whisper_model_path, spawn_async_backend};
use events::{AppCommand, CoreEvent};
use state::{
    AudioPlayState, AudioPlayer, ChatState, ConversationsState, CoreConnection, ImageCache,
    InputState, ModelState, StatusState, UiScale, VoiceState,
};
use systems::{event_reader_system, file_drop_system, setup_egui, voice_system};
use ui::ui_system;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    tracing::info!("Starting Noema GUI");

    load_env_file();
    let provider_urls = ProviderUrls::from_env();

    let (cmd_tx, cmd_rx) = unbounded::<AppCommand>();
    let (event_tx, event_rx) = unbounded::<CoreEvent>();

    spawn_async_backend(cmd_rx, event_tx, provider_urls);

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Noema".to_string(),
                resolution: (1000.0, 800.0).into(),
                prevent_default_event_handling: false,
                ..default()
            }),
            ..default()
        }))
        .add_plugins(EguiPlugin)
        .insert_resource(CoreConnection { cmd_tx, event_rx })
        // UI state split into focused resources
        .insert_resource(ChatState::default())
        .insert_resource(ConversationsState::default())
        .insert_resource(ModelState::default())
        .insert_resource(InputState::default())
        .insert_resource(StatusState::default())
        .insert_resource(UiScale::default())
        .insert_resource(VoiceState {
            coordinator: None,
            whisper_model_path: get_whisper_model_path(),
            listening: false,
            transcribing: false,
        })
        .insert_resource(AudioPlayer::default())
        .insert_resource(ImageCache::default())
        .insert_resource(AudioPlayState::default())
        .add_systems(Startup, setup_egui)
        .add_systems(
            Update,
            (
                event_reader_system,
                voice_system,
                file_drop_system,
                ui_system,
            )
                .chain(),
        )
        .run();
}
