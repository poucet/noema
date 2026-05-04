//! Storage trait definitions
//!
//! All storage traits are defined here, with implementations in `implementations/`.

mod asset;
mod blob;
mod entity;
mod text;
mod turn;
mod user;
mod vault;

pub use asset::{AssetStore, StoredAsset};
pub use blob::BlobStore;
pub use entity::{EntityStore, RelationCountMap, StoredEntity};
pub use text::{TextStore, StoredTextBlock};
pub use turn::{TurnStore, StoredTurn, StoredSpan, StoredMessage};
pub use user::{StoredUser, UserStore};
pub use vault::VaultStore;

/// Bundles all storage type associations into a single trait.
///
/// Implement this trait to define a complete storage configuration:
///
/// ```ignore
/// pub struct AppStorage;
///
/// impl StorageTypes for AppStorage {
///     type Blob = FsBlobStore;
///     type Asset = SqliteStore;
///     type Text = SqliteStore;
///     type User = SqliteStore;
///     type Entity = SqliteStore;
///     type Vault = SqliteStore;
/// }
///
/// // Then use as a single type parameter:
/// type AppSession = Session<AppStorage>;
/// type AppCoordinator = StorageCoordinator<AppStorage>;
/// ```
pub trait StorageTypes: Send + Sync + 'static {
    /// Blob storage (filesystem-based binary asset storage)
    type Blob: BlobStore + Send + Sync;
    /// Asset metadata storage
    type Asset: AssetStore + Send + Sync;
    /// Text content storage
    type Text: TextStore + Send + Sync;
    /// Turn/Span/Message storage
    type Turn: TurnStore + Send + Sync;
    /// User storage
    type User: UserStore + Send + Sync;
    /// Entity storage (unified addressable layer for conversations,
    /// documents, notes, tabs, assets). Also carries `entity_relations`
    /// and `entity_assets`.
    type Entity: EntityStore + Send + Sync;
    /// Markdown vault projection and reconciliation conflict storage.
    type Vault: VaultStore + Send + Sync;
}

use std::sync::Arc;

/// Provides access to store instances.
///
/// Implement this trait to define how stores are accessed. The implementation
/// can share underlying storage (e.g., a single SqliteStore) across multiple
/// accessor methods.
///
/// ```ignore
/// pub struct AppStores {
///     sqlite: Arc<SqliteStore>,
///     blob: Arc<FsBlobStore>,
/// }
///
/// impl Stores<AppStorage> for AppStores {
///     fn entity(&self) -> Arc<SqliteStore> { self.sqlite.clone() }
///     fn turn(&self) -> Arc<SqliteStore> { self.sqlite.clone() }
///     fn user(&self) -> Arc<SqliteStore> { self.sqlite.clone() }
///     fn blob(&self) -> Arc<FsBlobStore> { self.blob.clone() }
///     // ... other stores
/// }
/// ```
pub trait Stores<S: StorageTypes>: Send + Sync {
    fn turn(&self) -> Arc<S::Turn>;
    fn user(&self) -> Arc<S::User>;
    fn blob(&self) -> Arc<S::Blob>;
    fn asset(&self) -> Arc<S::Asset>;
    fn text(&self) -> Arc<S::Text>;
    /// Unified entity storage (conversations, documents, entity_relations, entity_assets)
    fn entity(&self) -> Arc<S::Entity>;
    /// Markdown vault projection storage.
    fn vault(&self) -> Arc<S::Vault>;
}
