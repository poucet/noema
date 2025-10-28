use async_trait::async_trait;

use crate::completion::AsyncCompleter;
use crate::error::CommandError;
use crate::parsed_args::ParsedArgs;

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
#[async_trait]
pub trait Command: AsyncCompleter {
    /// Execute the command with parsed arguments
    async fn execute(&mut self, args: ParsedArgs) -> Result<CommandResult, CommandError>;

    /// Get command metadata
    fn metadata(&self) -> &CommandMetadata;
}
