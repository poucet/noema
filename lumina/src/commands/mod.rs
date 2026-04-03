//! Slash command registration and dispatch.
//!
//! Use `#[slash_command]` proc macro for stateless commands (auto-registers).
//! Use `register_command!` for stateful commands that need custom constructors.
//! No manual dispatch table — `inventory` collects them at link time.

mod chat;
mod model;
mod ping;
pub mod tool;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use async_trait::async_trait;
use serenity::all::CommandInteraction;
use serenity::builder::CreateCommand;
use serenity::model::id::ChannelId;
use serenity::prelude::*;
use simply_daemon::api::DaemonApi;
use tokio::sync::RwLock;

// ---------------------------------------------------------------------------
// Shared state
// ---------------------------------------------------------------------------

/// Shared mutable state across all handlers.
pub struct SharedState {
    pub paused_channels: RwLock<HashSet<ChannelId>>,
    /// Per-channel model override. Falls back to config default if not set.
    pub channel_models: RwLock<HashMap<ChannelId, String>>,
}

impl SharedState {
    pub fn new() -> Self {
        Self {
            paused_channels: RwLock::new(HashSet::new()),
            channel_models: RwLock::new(HashMap::new()),
        }
    }
}

impl TypeMapKey for SharedState {
    type Value = Arc<SharedState>;
}

// ---------------------------------------------------------------------------
// LuminaContext — passed to every command handler
// ---------------------------------------------------------------------------

/// Rich context for command handlers. Bundles serenity context, daemon, config, and shared state.
pub struct LuminaContext {
    pub ctx: Context,
    pub daemon: Arc<dyn DaemonApi>,
    pub config: config::LuminaConfig,
    pub state: Arc<SharedState>,
}

impl LuminaContext {
    /// Build from serenity Context by extracting TypeMap values.
    pub async fn from_serenity(ctx: &Context) -> Self {
        let data = ctx.data.read().await;
        let daemon = data.get::<crate::DaemonKey>().expect("DaemonKey missing").clone();
        let config = data.get::<crate::ConfigKey>().expect("ConfigKey missing").clone();
        let state = data.get::<SharedState>().expect("SharedState missing").clone();
        Self {
            ctx: ctx.clone(),
            daemon,
            config,
            state,
        }
    }
}

impl std::ops::Deref for LuminaContext {
    type Target = Context;
    fn deref(&self) -> &Self::Target {
        &self.ctx
    }
}

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

/// A slash command that can optionally hold state across invocations.
#[async_trait]
pub trait SlashCommand: Send + Sync {
    /// The command name (must match what `register()` returns).
    fn name(&self) -> &'static str;

    /// Build the Discord command definition.
    fn register(&self) -> CreateCommand;

    /// Handle an incoming invocation.
    async fn run(&self, lx: &LuminaContext, cmd: &CommandInteraction) -> anyhow::Result<()>;

    /// Handle autocomplete for a command option. Default: no-op.
    async fn autocomplete(&self, _lx: &LuminaContext, _ac: &CommandInteraction) -> anyhow::Result<()> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Auto-registration via inventory
// ---------------------------------------------------------------------------

/// Submitted to `inventory` — a factory that produces a boxed command.
pub struct CommandInit {
    pub init: fn() -> Box<dyn SlashCommand>,
}

inventory::collect!(CommandInit);

/// Register a command type for auto-discovery.
///
/// ```ignore
/// // Stateless (Default)
/// register_command!(Ping);
///
/// // Stateful (custom constructor)
/// register_command!(Voice { connections: DashMap::new() });
/// ```
#[macro_export]
macro_rules! register_command {
    ($ty:ident) => {
        ::inventory::submit!($crate::commands::CommandInit {
            init: || Box::new($ty::default()),
        });
    };
    ($ty:ident { $($fields:tt)* }) => {
        ::inventory::submit!($crate::commands::CommandInit {
            init: || Box::new($ty { $($fields)* }),
        });
    };
}

// ---------------------------------------------------------------------------
// Registry — lives in serenity's TypeMap
// ---------------------------------------------------------------------------

/// Holds all instantiated commands, keyed by name.
pub struct CommandRegistry {
    commands: HashMap<&'static str, Box<dyn SlashCommand>>,
}

impl CommandRegistry {
    /// Build the registry from all `inventory`-registered commands.
    pub fn collect() -> Self {
        let commands = inventory::iter::<CommandInit>
            .into_iter()
            .map(|ci| {
                let cmd = (ci.init)();
                (cmd.name(), cmd)
            })
            .collect();
        Self { commands }
    }

    pub fn len(&self) -> usize {
        self.commands.len()
    }

    /// Return Discord command definitions for guild registration.
    pub fn definitions(&self) -> Vec<CreateCommand> {
        self.commands.values().map(|c| c.register()).collect()
    }

    /// Dispatch an incoming command interaction.
    pub async fn dispatch(&self, lx: &LuminaContext, cmd: &CommandInteraction) {
        let name = cmd.data.name.as_str();
        match self.commands.get(name) {
            Some(handler) => {
                if let Err(e) = handler.run(lx, cmd).await {
                    tracing::error!(command = name, error = %e, "command failed");
                }
            }
            None => tracing::warn!(command = name, "unknown command"),
        }
    }

    /// Dispatch an autocomplete interaction.
    pub async fn dispatch_autocomplete(&self, lx: &LuminaContext, ac: &CommandInteraction) {
        let name = ac.data.name.as_str();
        if let Some(handler) = self.commands.get(name) {
            if let Err(e) = handler.autocomplete(lx, ac).await {
                tracing::error!(command = name, error = %e, "autocomplete failed");
            }
        }
    }
}

impl TypeMapKey for CommandRegistry {
    type Value = CommandRegistry;
}
