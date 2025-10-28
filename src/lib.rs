pub mod completion;
pub mod command;
pub mod error;
pub mod registry;
pub(crate) mod parsed_args;
pub mod cache;

// Re-export main types
pub use completion::{AsyncCompleter, Completion, CompletionContext};
pub use command::{Command, CommandMetadata, CommandResult};
pub use error::{CommandError, CompletionError, ParseError};
pub use registry::CommandRegistry;
pub use cache::CachedCompleter;

// Re-export from macros crate (will be added later)
pub use commands_macros::{command, completable, completer};
