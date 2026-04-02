//! Slash command registration and dispatch.
//!
//! Use `#[slash_command]` proc macro for stateless commands (auto-registers).
//! Use `register_command!` for stateful commands that need custom constructors.
//! No manual dispatch table — `inventory` collects them at link time.

mod chat;
mod ping;

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serenity::all::CommandInteraction;
use serenity::builder::CreateCommand;
use serenity::prelude::*;
use simply_daemon::api::DaemonApi;

// ---------------------------------------------------------------------------
// LuminaContext — passed to every command handler
// ---------------------------------------------------------------------------

/// Rich context for command handlers. Bundles serenity context, daemon, and config.
pub struct LuminaContext {
    pub ctx: Context,
    pub daemon: Arc<dyn DaemonApi>,
    pub config: config::LuminaConfig,
}

impl LuminaContext {
    /// Build from serenity Context by extracting TypeMap values.
    pub async fn from_serenity(ctx: &Context) -> Self {
        let data = ctx.data.read().await;
        let daemon = data.get::<crate::DaemonKey>().expect("DaemonKey missing").clone();
        let config = data.get::<crate::ConfigKey>().expect("ConfigKey missing").clone();
        Self {
            ctx: ctx.clone(),
            daemon,
            config,
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

    /// Dispatch an incoming interaction to the matching command.
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
}

impl TypeMapKey for CommandRegistry {
    type Value = CommandRegistry;
}
