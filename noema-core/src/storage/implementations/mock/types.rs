//! Mock storage type bundle

use crate::storage::traits::StorageTypes;

use super::{
    MockAssetStore, MockBlobStore, MockConversationStore, MockDocumentStore, MockEntityStore,
    MockTextStore, MockTurnStore, MockUserStore,
};

/// Mock storage types bundled together for coordinator tests
pub struct MockStorage;

impl StorageTypes for MockStorage {
    type Blob = MockBlobStore;
    type Asset = MockAssetStore;
    type Text = MockTextStore;
    type Conversation = MockConversationStore;
    type Turn = MockTurnStore;
    type User = MockUserStore;
    type Document = MockDocumentStore;
    type Entity = MockEntityStore;
}
