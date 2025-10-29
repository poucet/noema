use crate::{CommandRegistry, CompletionHelper, CompletionResult, ContextMut, TokenStream, CommandResult, Registrable};
use anyhow::Result;

/// Generic wrapper that manages a CommandRegistry and provides completion functionality
/// for any type T that implements Registrable
pub struct CommandHandler<T: Registrable<CommandRegistry<T>>> {
    target: T,
    registry: CommandRegistry<T>,
}

impl<T: Registrable<CommandRegistry<T>>> CommandHandler<T> {
    /// Create a new CommandHandler with the given target
    /// The registry will be automatically set up by calling T::register
    pub fn new(target: T) -> Self {
        let mut registry = CommandRegistry::new();
        T::register(&mut registry);

        Self {
            target,
            registry,
        }
    }

    /// Get a reference to the wrapped target
    pub fn target(&self) -> &T {
        &self.target
    }

    /// Get a mutable reference to the wrapped target
    pub fn target_mut(&mut self) -> &mut T {
        &mut self.target
    }

    /// Trigger completion for the given input string
    pub async fn trigger_completion(&self, input: &str) -> CompletionResult {
        let helper = CompletionHelper::new(&self.registry);
        helper.trigger_completion(input, &self.target).await
    }

    /// Execute a command with the given input string
    pub async fn execute_command(&mut self, input: &str) -> Result<CommandResult> {
        let tokens = TokenStream::new(input.to_string());
        let ctx = ContextMut::new(tokens, &mut self.target);
        self.registry.execute(ctx).await.map_err(|e| e.into())
    }
}
