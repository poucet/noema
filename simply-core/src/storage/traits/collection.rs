//! CollectionStore trait for organizing entities into collections

use anyhow::Result;
use async_trait::async_trait;

use crate::storage::ids::{
    CollectionId, CollectionItemId, CollectionViewId, EntityId, ItemFieldId, UserId,
};
use crate::storage::types::{
    Collection, CollectionItem, CollectionView, FieldType, ItemTarget, StoredEditable,
};

/// Stored representation of a Collection (editable)
pub type StoredCollection = StoredEditable<CollectionId, Collection>;

/// Stored representation of a CollectionItem (editable)
pub type StoredCollectionItem = StoredEditable<CollectionItemId, CollectionItem>;

/// Stored representation of a CollectionView (editable)
pub type StoredCollectionView = StoredEditable<CollectionViewId, CollectionView>;

/// A field value on a collection item
#[derive(Clone, Debug)]
pub struct ItemField {
    /// Field name
    pub field_name: String,
    /// Field type
    pub field_type: FieldType,
    /// Text value (for Text, Date, Url, Select types)
    pub value_text: Option<String>,
    /// Numeric value (for Number type)
    pub value_number: Option<f64>,
    /// Boolean value (for Boolean type)
    pub value_boolean: Option<bool>,
    /// JSON value (for MultiSelect or complex types)
    pub value_json: Option<serde_json::Value>,
}

impl ItemField {
    /// Create a text field value
    pub fn text(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            field_name: name.into(),
            field_type: FieldType::Text,
            value_text: Some(value.into()),
            value_number: None,
            value_boolean: None,
            value_json: None,
        }
    }

    /// Create a number field value
    pub fn number(name: impl Into<String>, value: f64) -> Self {
        Self {
            field_name: name.into(),
            field_type: FieldType::Number,
            value_text: None,
            value_number: Some(value),
            value_boolean: None,
            value_json: None,
        }
    }

    /// Create a boolean field value
    pub fn boolean(name: impl Into<String>, value: bool) -> Self {
        Self {
            field_name: name.into(),
            field_type: FieldType::Boolean,
            value_text: None,
            value_number: None,
            value_boolean: Some(value),
            value_json: None,
        }
    }

    /// Create a select field value
    pub fn select(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            field_name: name.into(),
            field_type: FieldType::Select,
            value_text: Some(value.into()),
            value_number: None,
            value_boolean: None,
            value_json: None,
        }
    }

    /// Create a date field value (ISO 8601 string)
    pub fn date(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            field_name: name.into(),
            field_type: FieldType::Date,
            value_text: Some(value.into()),
            value_number: None,
            value_boolean: None,
            value_json: None,
        }
    }
}

/// Stored representation of an ItemField
pub type StoredItemField = StoredEditable<ItemFieldId, ItemField>;

/// Trait for collection storage operations
#[async_trait]
pub trait CollectionStore: Send + Sync {
    // ========================================================================
    // Collection CRUD
    // ========================================================================

    /// Create a new collection
    async fn create_collection(
        &self,
        user_id: &UserId,
        name: &str,
        description: Option<&str>,
        icon: Option<&str>,
    ) -> Result<CollectionId>;

    /// Get a collection by ID
    async fn get_collection(&self, id: &CollectionId) -> Result<Option<StoredCollection>>;

    /// List all collections for a user
    async fn list_collections(&self, user_id: &UserId) -> Result<Vec<StoredCollection>>;

    /// Update a collection's metadata
    async fn update_collection(
        &self,
        id: &CollectionId,
        name: Option<&str>,
        description: Option<&str>,
        icon: Option<&str>,
    ) -> Result<bool>;

    /// Delete a collection and all its items
    async fn delete_collection(&self, id: &CollectionId) -> Result<bool>;

    // ========================================================================
    // Item Management
    // ========================================================================

    /// Add an item to a collection
    async fn add_item(
        &self,
        collection_id: &CollectionId,
        target: &ItemTarget,
        parent_item_id: Option<&CollectionItemId>,
        position: i32,
        name_override: Option<&str>,
    ) -> Result<CollectionItemId>;

    /// Get an item by ID
    async fn get_item(&self, id: &CollectionItemId) -> Result<Option<StoredCollectionItem>>;

    /// Get all items in a collection (flat list)
    async fn get_items(&self, collection_id: &CollectionId) -> Result<Vec<StoredCollectionItem>>;

    /// Get root items (items with no parent)
    async fn get_root_items(&self, collection_id: &CollectionId) -> Result<Vec<StoredCollectionItem>>;

    /// Get children of an item
    async fn get_children(&self, item_id: &CollectionItemId) -> Result<Vec<StoredCollectionItem>>;

    /// Move an item to a new parent and/or position
    async fn move_item(
        &self,
        item_id: &CollectionItemId,
        new_parent_id: Option<&CollectionItemId>,
        new_position: i32,
    ) -> Result<bool>;

    /// Remove an item from a collection
    async fn remove_item(&self, id: &CollectionItemId) -> Result<bool>;

    // ========================================================================
    // Field Operations
    // ========================================================================

    /// Set a field value on an item (creates or updates)
    async fn set_field(
        &self,
        item_id: &CollectionItemId,
        field: &ItemField,
    ) -> Result<ItemFieldId>;

    /// Get all fields for an item
    async fn get_fields(&self, item_id: &CollectionItemId) -> Result<Vec<StoredItemField>>;

    /// Remove a field from an item
    async fn remove_field(&self, item_id: &CollectionItemId, field_name: &str) -> Result<bool>;

    // ========================================================================
    // Tag Operations
    // ========================================================================

    /// Add a tag to an item
    async fn add_tag(&self, item_id: &CollectionItemId, tag: &str) -> Result<()>;

    /// Remove a tag from an item
    async fn remove_tag(&self, item_id: &CollectionItemId, tag: &str) -> Result<bool>;

    /// Get all tags for an item
    async fn get_tags(&self, item_id: &CollectionItemId) -> Result<Vec<String>>;

    /// Find all items with a specific tag in a collection
    async fn find_by_tag(
        &self,
        collection_id: &CollectionId,
        tag: &str,
    ) -> Result<Vec<StoredCollectionItem>>;

    // ========================================================================
    // View Operations
    // ========================================================================

    /// Create a view for a collection
    async fn create_view(
        &self,
        collection_id: &CollectionId,
        name: &str,
        view: &CollectionView,
    ) -> Result<CollectionViewId>;

    /// Get a view by ID
    async fn get_view(&self, id: &CollectionViewId) -> Result<Option<StoredCollectionView>>;

    /// List all views for a collection
    async fn list_views(&self, collection_id: &CollectionId) -> Result<Vec<StoredCollectionView>>;

    /// Get the default view for a collection
    async fn get_default_view(&self, collection_id: &CollectionId) -> Result<Option<StoredCollectionView>>;

    /// Update a view's configuration
    async fn update_view(&self, id: &CollectionViewId, view: &CollectionView) -> Result<bool>;

    /// Delete a view
    async fn delete_view(&self, id: &CollectionViewId) -> Result<bool>;

    // ========================================================================
    // Query Operations
    // ========================================================================

    /// Find items that reference a specific entity (across all collections)
    async fn find_items_by_entity(&self, entity_id: &EntityId) -> Result<Vec<StoredCollectionItem>>;
}
