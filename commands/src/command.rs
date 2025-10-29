use async_trait::async_trait;

use crate::context::ContextMut;
use crate::error::CommandError;

/// Result of executing a command
#[derive(Debug, Clone)]
pub enum CommandResult {
    /// Command succeeded with optional message
    Success(String),

    /// Command requests application exit
    Exit,
}

/// Metadata about a command
#[derive(Debug, Clone)]
pub struct CommandMetadata {
    /// Command name (without leading slash)
    pub name: &'static str,

    /// Help text describing the command
    pub help: &'static str,
}

/// Trait for executable commands that support completion
/// Type parameter T is the target type the command operates on (defaults to () for stateless commands)
#[async_trait]
pub trait Command<T = ()>: crate::completion::AsyncCompleter<T> {
    /// Execute the command with mutable context
    async fn execute<'a>(&self, context: ContextMut<'a, T>) -> Result<CommandResult, CommandError>;

    /// Get command metadata
    fn metadata(&self) -> &CommandMetadata;
}
