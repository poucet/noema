use crate::{CommandRegistry, Completion, Context, TokenStream, ContextMut, CommandResult, CommandError};

/// Result of triggering completion
pub enum CompletionResult {
    /// No completions available
    NoCompletions,
    /// Single completion that should be auto-accepted
    Single(String),
    /// Common prefix was auto-filled, input should be updated to this value, with remaining completions
    AutoFilledPrefix { new_input: String, completions: Vec<Completion> },
    /// Multiple completions available (no common prefix)
    Multiple(Vec<Completion>),
}

/// Helper class that wraps CommandRegistry and provides completion logic
pub struct CompletionHelper<T> {
    registry: CommandRegistry<T>,
}

impl<T> CompletionHelper<T> {
    /// Create a new completion helper with the given registry
    pub fn new(registry: CommandRegistry<T>) -> Self {
        Self { registry }
    }

    /// Trigger completion for the current input and return result
    pub async fn trigger_completion(&self, input: &str, target: &T) -> CompletionResult {
        if !input.starts_with('/') {
            return CompletionResult::NoCompletions;
        }

        let ctx = Context::new(input, target);
        match self.registry.complete(&ctx).await {
            Ok(completions) => {
                if completions.is_empty() {
                    CompletionResult::NoCompletions
                } else if completions.len() == 1 {
                    // Only one completion - return for auto-accept
                    CompletionResult::Single(completions[0].value.clone())
                } else {
                    // Multiple completions - check for common prefix
                    if let Some(prefix) = Self::find_common_prefix(&completions) {
                        // Build the new input with the common prefix
                        let new_input = if let Some(last_space) = input.rfind(char::is_whitespace) {
                            format!("{} {}", &input[..=last_space].trim(), prefix)
                        } else {
                            format!("/{}", prefix)
                        };

                        // We already have the completions - they're all valid for this prefix
                        // Just check if they all match the prefix exactly (single option)
                        if completions.len() == 1 || completions.iter().all(|c| c.value == prefix) {
                            CompletionResult::Single(prefix)
                        } else {
                            // Still multiple options - return prefix and completions
                            CompletionResult::AutoFilledPrefix {
                                new_input,
                                completions,
                            }
                        }
                    } else {
                        // No common prefix - return completions
                        CompletionResult::Multiple(completions)
                    }
                }
            }
            Err(_) => CompletionResult::NoCompletions,
        }
    }

    /// Find common prefix among all completions
    fn find_common_prefix(completions: &[Completion]) -> Option<String> {
        if completions.is_empty() {
            return None;
        }

        let first = &completions[0].value;
        let mut common_prefix = first.clone();

        for completion in &completions[1..] {
            let value = &completion.value;
            let mut prefix_len = 0;

            for (c1, c2) in common_prefix.chars().zip(value.chars()) {
                if c1 == c2 {
                    prefix_len += c1.len_utf8();
                } else {
                    break;
                }
            }

            if prefix_len == 0 {
                return None;
            }

            common_prefix.truncate(prefix_len);
        }

        if &common_prefix == first {
            // All completions are the same - just return None
            None
        } else {
            Some(common_prefix)
        }
    }

    /// Execute a command
    pub async fn execute(
        &self,
        input: &str,
        target: &mut T,
    ) -> Result<CommandResult, CommandError> {
        let tokens = TokenStream::new(input.to_string());
        let ctx = ContextMut::new(tokens, target);
        self.registry.execute(ctx).await
    }
}
