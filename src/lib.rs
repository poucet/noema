pub mod completion;
pub mod command;
pub mod error;
pub mod registry;
pub mod cache;

// ParsedArgs is public for generated code but users shouldn't need it
#[doc(hidden)]
pub mod parsed_args;

// Re-export main types
pub use completion::{AsyncCompleter, Completion, CompletionContext};
pub use command::{Command, CommandMetadata, CommandResult};
pub use error::{CommandError, CompletionError, ParseError};
pub use registry::CommandRegistry;
pub use cache::CachedCompleter;
pub use parsed_args::ParsedArgs; // For generated code

// Re-export from macros crate
pub use commands_macros::{commandable, command, completable, completer, register_commands};

// Re-export async_trait for macro-generated code
pub use async_trait;
