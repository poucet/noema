use std::collections::HashMap;

use crate::command::{Command, CommandResult};
use crate::completion::{Completion, CompletionContext};
use crate::error::{CommandError, CompletionError};
use crate::token_stream::TokenStream;

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

    /// Execute a command from input string on the target
    pub async fn execute(&self, target: &mut T, input: &str) -> Result<CommandResult, CommandError> {
        let (cmd_name, args_str) = parse_command_input(input)?;

        let command = self
            .commands
            .get(cmd_name)
            .ok_or_else(|| CommandError::UnknownCommand(cmd_name.to_string()))?;

        let args = TokenStream::from_quoted(args_str);
        command.execute(target, args).await
    }

    /// Get completions for the current input with access to target
    pub async fn complete(
        &self,
        target: &T,
        input: &str,
        cursor: usize,
    ) -> Result<Vec<Completion>, CompletionError> {
        let ctx = CompletionContext::new(input.to_string(), cursor, target);

        // Try to complete command arguments if we have a valid command
        if let Ok((cmd_name, _args_str)) = parse_command_input(input) {
            if let Some(command) = self.commands.get(cmd_name) {
                return command.complete(&ctx).await;
            }
        }

        // Fall through: complete command names
        let partial = input.trim_start_matches('/');
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

/// Parse command input into (command_name, arguments)
fn parse_command_input(input: &str) -> Result<(&str, &str), CommandError> {
    let input = input.trim();

    if !input.starts_with('/') {
        return Err(CommandError::InvalidArgs(
            "Commands must start with /".to_string(),
        ));
    }

    let without_slash = &input[1..];

    if let Some(space_pos) = without_slash.find(char::is_whitespace) {
        let cmd_name = &without_slash[..space_pos];
        let args = without_slash[space_pos..].trim_start();
        Ok((cmd_name, args))
    } else {
        // No args
        Ok((without_slash, ""))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_command_input() {
        assert_eq!(
            parse_command_input("/help").unwrap(),
            ("help", "")
        );

        assert_eq!(
            parse_command_input("/model gemini").unwrap(),
            ("model", "gemini")
        );

        assert_eq!(
            parse_command_input("/model gemini gemini-2.0-flash").unwrap(),
            ("model", "gemini gemini-2.0-flash")
        );

        assert!(parse_command_input("not a command").is_err());
    }
}
