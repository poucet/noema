//! Re-export the daemon API from `simply-daemon-api`.

pub use simply_daemon_api::*;

// Implementation-specific re-exports for this crate's internal use.
pub use simply_core::storage::{
    Entity, EntityType, StoredEditable,
    Stores, UserStore,
    FsBlobStore, SqliteStore,
};
pub use simply_core::storage::coordinator::StorageCoordinator;
pub use simply_core::storage::traits::StorageTypes;
pub use simply_core::mcp::{ServerStatus, spawn_retry_task, start_auto_connect};
