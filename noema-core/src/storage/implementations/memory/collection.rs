//! In-memory CollectionStore implementation (stub)

use anyhow::Result;
use async_trait::async_trait;

use crate::storage::ids::{
    CollectionId, CollectionItemId, CollectionViewId, EntityId, ItemFieldId, UserId,
};
use crate::storage::traits::{
    CollectionStore, ItemField, StoredCollection, StoredCollectionItem, StoredCollectionView,
    StoredItemField,
};
use crate::storage::types::{CollectionView, ItemTarget};

/// In-memory collection store (stub implementation)
pub struct MemoryCollectionStore;

impl MemoryCollectionStore {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MemoryCollectionStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CollectionStore for MemoryCollectionStore {
    async fn create_collection(
        &self,
        _user_id: &UserId,
        _name: &str,
        _description: Option<&str>,
        _icon: Option<&str>,
    ) -> Result<CollectionId> {
        unimplemented!("MemoryCollectionStore::create_collection")
    }

    async fn get_collection(&self, _id: &CollectionId) -> Result<Option<StoredCollection>> {
        unimplemented!("MemoryCollectionStore::get_collection")
    }

    async fn list_collections(&self, _user_id: &UserId) -> Result<Vec<StoredCollection>> {
        unimplemented!("MemoryCollectionStore::list_collections")
    }

    async fn update_collection(
        &self,
        _id: &CollectionId,
        _name: Option<&str>,
        _description: Option<&str>,
        _icon: Option<&str>,
    ) -> Result<bool> {
        unimplemented!("MemoryCollectionStore::update_collection")
    }

    async fn delete_collection(&self, _id: &CollectionId) -> Result<bool> {
        unimplemented!("MemoryCollectionStore::delete_collection")
    }

    async fn add_item(
        &self,
        _collection_id: &CollectionId,
        _target: &ItemTarget,
        _parent_item_id: Option<&CollectionItemId>,
        _position: i32,
        _name_override: Option<&str>,
    ) -> Result<CollectionItemId> {
        unimplemented!("MemoryCollectionStore::add_item")
    }

    async fn get_item(&self, _id: &CollectionItemId) -> Result<Option<StoredCollectionItem>> {
        unimplemented!("MemoryCollectionStore::get_item")
    }

    async fn get_items(&self, _collection_id: &CollectionId) -> Result<Vec<StoredCollectionItem>> {
        unimplemented!("MemoryCollectionStore::get_items")
    }

    async fn get_root_items(
        &self,
        _collection_id: &CollectionId,
    ) -> Result<Vec<StoredCollectionItem>> {
        unimplemented!("MemoryCollectionStore::get_root_items")
    }

    async fn get_children(
        &self,
        _item_id: &CollectionItemId,
    ) -> Result<Vec<StoredCollectionItem>> {
        unimplemented!("MemoryCollectionStore::get_children")
    }

    async fn move_item(
        &self,
        _item_id: &CollectionItemId,
        _new_parent_id: Option<&CollectionItemId>,
        _new_position: i32,
    ) -> Result<bool> {
        unimplemented!("MemoryCollectionStore::move_item")
    }

    async fn remove_item(&self, _id: &CollectionItemId) -> Result<bool> {
        unimplemented!("MemoryCollectionStore::remove_item")
    }

    async fn set_field(
        &self,
        _item_id: &CollectionItemId,
        _field: &ItemField,
    ) -> Result<ItemFieldId> {
        unimplemented!("MemoryCollectionStore::set_field")
    }

    async fn get_fields(&self, _item_id: &CollectionItemId) -> Result<Vec<StoredItemField>> {
        unimplemented!("MemoryCollectionStore::get_fields")
    }

    async fn remove_field(
        &self,
        _item_id: &CollectionItemId,
        _field_name: &str,
    ) -> Result<bool> {
        unimplemented!("MemoryCollectionStore::remove_field")
    }

    async fn add_tag(&self, _item_id: &CollectionItemId, _tag: &str) -> Result<()> {
        unimplemented!("MemoryCollectionStore::add_tag")
    }

    async fn remove_tag(&self, _item_id: &CollectionItemId, _tag: &str) -> Result<bool> {
        unimplemented!("MemoryCollectionStore::remove_tag")
    }

    async fn get_tags(&self, _item_id: &CollectionItemId) -> Result<Vec<String>> {
        unimplemented!("MemoryCollectionStore::get_tags")
    }

    async fn find_by_tag(
        &self,
        _collection_id: &CollectionId,
        _tag: &str,
    ) -> Result<Vec<StoredCollectionItem>> {
        unimplemented!("MemoryCollectionStore::find_by_tag")
    }

    async fn create_view(
        &self,
        _collection_id: &CollectionId,
        _name: &str,
        _view: &CollectionView,
    ) -> Result<CollectionViewId> {
        unimplemented!("MemoryCollectionStore::create_view")
    }

    async fn get_view(&self, _id: &CollectionViewId) -> Result<Option<StoredCollectionView>> {
        unimplemented!("MemoryCollectionStore::get_view")
    }

    async fn list_views(
        &self,
        _collection_id: &CollectionId,
    ) -> Result<Vec<StoredCollectionView>> {
        unimplemented!("MemoryCollectionStore::list_views")
    }

    async fn get_default_view(
        &self,
        _collection_id: &CollectionId,
    ) -> Result<Option<StoredCollectionView>> {
        unimplemented!("MemoryCollectionStore::get_default_view")
    }

    async fn update_view(
        &self,
        _id: &CollectionViewId,
        _view: &CollectionView,
    ) -> Result<bool> {
        unimplemented!("MemoryCollectionStore::update_view")
    }

    async fn delete_view(&self, _id: &CollectionViewId) -> Result<bool> {
        unimplemented!("MemoryCollectionStore::delete_view")
    }

    async fn find_items_by_entity(
        &self,
        _entity_id: &EntityId,
    ) -> Result<Vec<StoredCollectionItem>> {
        unimplemented!("MemoryCollectionStore::find_items_by_entity")
    }
}
