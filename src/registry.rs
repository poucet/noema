use std::collections::HashMap;

use crate::command::{Command, CommandResult};
use crate::completion::Completion;
use crate::context::{Context, ContextMut};
use crate::error::{CommandError, CompletionError};

/// Trait for types that can register themselves with a CommandRegistry
pub trait Registrable<R> {
    fn register(registry: &mut R);
}

/// Registry for managing and dispatching commands for type T
pub struct CommandRegistry<T> {
    commands: HashMap<String, Box<dyn Command<T>>>,
}

impl<T> CommandRegistry<T> {
    /// Create a new empty command registry
    pub fn new() -> Self {
        Self {
            commands: HashMap::new(),
        }
    }

    /// Register a command instance
    pub fn register<C>(&mut self, command: C)
    where
        C: Command<T> + 'static,
    {
        let name = command.metadata().name.to_string();
        self.commands.insert(name, Box::new(command));
    }

    /// Execute a command with mutable context
    pub async fn execute<'a>(&self, context: ContextMut<'a, T>) -> Result<CommandResult, CommandError> {
        let cmd_name = context.stream().command_name()
            .ok_or_else(|| CommandError::InvalidArgs("Commands must start with /".to_string()))?;

        let command = self
            .commands
            .get(cmd_name)
            .ok_or_else(|| CommandError::UnknownCommand(cmd_name.to_string()))?;

        command.execute(context).await
    }

    /// Get completions for the current input with access to target
    pub async fn complete<'a>(
        &self, ctx: &Context<'a, T>,
    ) -> Result<Vec<Completion>, CompletionError> {
        // Try to complete command arguments if we have a valid command
        if let Some(cmd_name) = ctx.stream().command_name() {
            if let Some(command) = self.commands.get(cmd_name) {
                return command.complete(ctx).await;
            }
        }

        // Fall through: complete command names
        let partial = ctx.stream().partial().trim_start_matches('/');
        Ok(self
            .commands
            .keys()
            .filter(|name| name.starts_with(partial))
            .map(|name| Completion::simple(name.as_str()))
            .collect())
    }

    /// Get list of all registered command names
    pub fn command_names(&self) -> Vec<&str> {
        self.commands.keys().map(|s| s.as_str()).collect()
    }

    /// Get metadata for a command
    pub fn get_metadata(&self, name: &str) -> Option<&crate::command::CommandMetadata> {
        self.commands.get(name).map(|cmd| cmd.metadata())
    }
}

impl<T> Default for CommandRegistry<T> {
    fn default() -> Self {
        Self::new()
    }
}
