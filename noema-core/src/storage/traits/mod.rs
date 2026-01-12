//! Storage trait definitions
//!
//! All storage traits are defined here, with implementations in `implementations/`.

mod asset;
mod blob;
mod conversation;
mod document;
mod text;
mod turn;
mod user;

pub use asset::AssetStore;
pub use blob::BlobStore;
pub use conversation::ConversationStore;
pub use document::DocumentStore;
pub use text::TextStore;
pub use turn::TurnStore;
pub use user::UserStore;

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
///     type Conversation = SqliteStore;
///     type User = SqliteStore;
///     type Document = SqliteStore;
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
    /// Conversation storage (includes TurnStore via supertrait)
    type Conversation: ConversationStore + Send + Sync;
    /// User storage
    type User: UserStore + Send + Sync;
    /// Document storage
    type Document: DocumentStore + Send + Sync;
}
