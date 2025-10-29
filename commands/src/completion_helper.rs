use crate::{CommandRegistry, Completion, Context};

/// Result of triggering completion
pub enum CompletionResult {
    /// Completions available - may be empty, single, or multiple
    Completions(Vec<Completion>),
    /// Common prefix was auto-filled, input should be updated to this value, with remaining completions
    AutoFilledPrefix { new_input: String, completions: Vec<Completion> },
}

/// Helper class that wraps CommandRegistry and provides completion logic
pub struct CompletionHelper<'a, T> {
    registry: &'a CommandRegistry<T>,
}

impl<'a, T> CompletionHelper<'a, T> {
    /// Create a new completion helper with the given registry
    pub fn new(registry: &'a CommandRegistry<T>) -> Self {
        Self { registry }
    }

    /// Trigger completion for the current input and return result
    pub async fn trigger_completion(&self, input: &str, target: &T) -> CompletionResult {
        if !input.starts_with('/') {
            return CompletionResult::Completions(vec![]);
        }

        let ctx = Context::new(input, target);
        match self.registry.complete(&ctx).await {
            Ok(completions) => {
                // Check for common prefix to auto-fill (only if multiple completions)
                if completions.len() > 1 {
                    if let Some(prefix) = Self::find_common_prefix(&completions) {
                        // Build the new input with the common prefix
                        let new_input = if let Some(last_space) = input.rfind(char::is_whitespace) {
                            format!("{} {}", &input[..=last_space].trim(), prefix)
                        } else {
                            format!("/{}", prefix)
                        };

                        // Check if all completions match the prefix exactly (would be single after fill)
                        if completions.iter().all(|c| c.value == prefix) {
                            // All match the prefix exactly - just return as single completion
                            CompletionResult::Completions(completions)
                        } else {
                            // Still multiple options after prefix fill
                            CompletionResult::AutoFilledPrefix {
                                new_input,
                                completions,
                            }
                        }
                    } else {
                        // No common prefix - return completions as-is
                        CompletionResult::Completions(completions)
                    }
                } else {
                    // 0 or 1 completions - return as-is
                    CompletionResult::Completions(completions)
                }
            }
            Err(_) => CompletionResult::Completions(vec![]),
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
}
