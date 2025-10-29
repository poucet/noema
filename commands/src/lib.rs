pub mod completion;
pub mod command;
pub mod completion_helper;
pub mod command_handler;
pub mod context;
pub mod error;
pub mod registry;
pub mod cache;
pub mod token_stream;

// Re-export main types
pub use completion::{AsyncCompleter, Completable, Completion, filter_completions};
pub use completion_helper::{CompletionHelper, CompletionResult};
pub use command_handler::CommandHandler;
pub use context::{Context, ContextMut};
pub use command::{Command, CommandMetadata, CommandResult};
pub use error::{CommandError, CompletionError, ParseError};
pub use registry::{CommandRegistry, Registrable};
pub use cache::CachedCompleter;
pub use token_stream::TokenStream; // For generated code

// Re-export from macros crate
pub use commands_macros::{commandable, command, completable, completer, register_commands};

// Re-export async_trait for macro-generated code
pub use async_trait;
