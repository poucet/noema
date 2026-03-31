//! Mock CollectionStore implementation

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

/// Mock collection store (returns unimplemented for all methods)
pub struct MockCollectionStore;

#[async_trait]
impl CollectionStore for MockCollectionStore {
    async fn create_collection(
        &self,
        _user_id: &UserId,
        _name: &str,
        _description: Option<&str>,
        _icon: Option<&str>,
    ) -> Result<CollectionId> {
        unimplemented!()
    }

    async fn get_collection(&self, _id: &CollectionId) -> Result<Option<StoredCollection>> {
        unimplemented!()
    }

    async fn list_collections(&self, _user_id: &UserId) -> Result<Vec<StoredCollection>> {
        unimplemented!()
    }

    async fn update_collection(
        &self,
        _id: &CollectionId,
        _name: Option<&str>,
        _description: Option<&str>,
        _icon: Option<&str>,
    ) -> Result<bool> {
        unimplemented!()
    }

    async fn delete_collection(&self, _id: &CollectionId) -> Result<bool> {
        unimplemented!()
    }

    async fn add_item(
        &self,
        _collection_id: &CollectionId,
        _target: &ItemTarget,
        _parent_item_id: Option<&CollectionItemId>,
        _position: i32,
        _name_override: Option<&str>,
    ) -> Result<CollectionItemId> {
        unimplemented!()
    }

    async fn get_item(&self, _id: &CollectionItemId) -> Result<Option<StoredCollectionItem>> {
        unimplemented!()
    }

    async fn get_items(&self, _collection_id: &CollectionId) -> Result<Vec<StoredCollectionItem>> {
        unimplemented!()
    }

    async fn get_root_items(
        &self,
        _collection_id: &CollectionId,
    ) -> Result<Vec<StoredCollectionItem>> {
        unimplemented!()
    }

    async fn get_children(
        &self,
        _item_id: &CollectionItemId,
    ) -> Result<Vec<StoredCollectionItem>> {
        unimplemented!()
    }

    async fn move_item(
        &self,
        _item_id: &CollectionItemId,
        _new_parent_id: Option<&CollectionItemId>,
        _new_position: i32,
    ) -> Result<bool> {
        unimplemented!()
    }

    async fn remove_item(&self, _id: &CollectionItemId) -> Result<bool> {
        unimplemented!()
    }

    async fn set_field(
        &self,
        _item_id: &CollectionItemId,
        _field: &ItemField,
    ) -> Result<ItemFieldId> {
        unimplemented!()
    }

    async fn get_fields(&self, _item_id: &CollectionItemId) -> Result<Vec<StoredItemField>> {
        unimplemented!()
    }

    async fn remove_field(
        &self,
        _item_id: &CollectionItemId,
        _field_name: &str,
    ) -> Result<bool> {
        unimplemented!()
    }

    async fn add_tag(&self, _item_id: &CollectionItemId, _tag: &str) -> Result<()> {
        unimplemented!()
    }

    async fn remove_tag(&self, _item_id: &CollectionItemId, _tag: &str) -> Result<bool> {
        unimplemented!()
    }

    async fn get_tags(&self, _item_id: &CollectionItemId) -> Result<Vec<String>> {
        unimplemented!()
    }

    async fn find_by_tag(
        &self,
        _collection_id: &CollectionId,
        _tag: &str,
    ) -> Result<Vec<StoredCollectionItem>> {
        unimplemented!()
    }

    async fn create_view(
        &self,
        _collection_id: &CollectionId,
        _name: &str,
        _view: &CollectionView,
    ) -> Result<CollectionViewId> {
        unimplemented!()
    }

    async fn get_view(&self, _id: &CollectionViewId) -> Result<Option<StoredCollectionView>> {
        unimplemented!()
    }

    async fn list_views(
        &self,
        _collection_id: &CollectionId,
    ) -> Result<Vec<StoredCollectionView>> {
        unimplemented!()
    }

    async fn get_default_view(
        &self,
        _collection_id: &CollectionId,
    ) -> Result<Option<StoredCollectionView>> {
        unimplemented!()
    }

    async fn update_view(
        &self,
        _id: &CollectionViewId,
        _view: &CollectionView,
    ) -> Result<bool> {
        unimplemented!()
    }

    async fn delete_view(&self, _id: &CollectionViewId) -> Result<bool> {
        unimplemented!()
    }

    async fn find_items_by_entity(
        &self,
        _entity_id: &EntityId,
    ) -> Result<Vec<StoredCollectionItem>> {
        unimplemented!()
    }
}
