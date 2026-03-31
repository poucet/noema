//! Tauri command modules

pub mod chat;
pub mod files;
pub mod gdocs;
pub mod init;
pub mod mcp;
pub mod settings;
pub mod voice;

// Re-export all commands for convenience
pub use chat::*;
pub use files::*;
pub use gdocs::*;
pub use init::*;
pub use mcp::*;
pub use settings::*;
pub use voice::*;
