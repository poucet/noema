//! Collection types
//!
//! Collections organize any entities into tree structures with optional
//! fields and tags for filtering and display.

use serde::{Deserialize, Serialize};

use crate::storage::ids::{CollectionId, CollectionItemId, EntityId, UserId};

// ============================================================================
// ItemTarget
// ============================================================================

/// What an item in a collection references
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ItemTarget {
    /// Reference to an entity (document, conversation, asset, etc.)
    Entity(EntityId),
    /// Nested collection (collection within collection)
    Collection(CollectionId),
}

impl ItemTarget {
    /// Create a target referencing an entity
    pub fn entity(id: EntityId) -> Self {
        Self::Entity(id)
    }

    /// Create a target referencing a nested collection
    pub fn collection(id: CollectionId) -> Self {
        Self::Collection(id)
    }
}

// ============================================================================
// FieldType
// ============================================================================

/// Type of a field value
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldType {
    /// Plain text
    Text,
    /// Number (stored as f64)
    Number,
    /// Boolean
    Boolean,
    /// Date/time (ISO 8601 string)
    Date,
    /// Single select from options
    Select,
    /// Multi-select from options
    MultiSelect,
    /// URL
    Url,
}

// ============================================================================
// FieldDefinition
// ============================================================================

/// Schema definition for a field
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FieldDefinition {
    /// Field name (used as key)
    pub name: String,
    /// Field type
    pub field_type: FieldType,
    /// Display label (optional, defaults to name)
    pub label: Option<String>,
    /// Options for Select/MultiSelect types
    pub options: Option<Vec<String>>,
    /// Default value (JSON)
    pub default_value: Option<serde_json::Value>,
}

impl FieldDefinition {
    /// Create a text field
    pub fn text(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            field_type: FieldType::Text,
            label: None,
            options: None,
            default_value: None,
        }
    }

    /// Create a number field
    pub fn number(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            field_type: FieldType::Number,
            label: None,
            options: None,
            default_value: None,
        }
    }

    /// Create a boolean field
    pub fn boolean(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            field_type: FieldType::Boolean,
            label: None,
            options: None,
            default_value: None,
        }
    }

    /// Create a date field
    pub fn date(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            field_type: FieldType::Date,
            label: None,
            options: None,
            default_value: None,
        }
    }

    /// Create a select field with options
    pub fn select(name: impl Into<String>, options: Vec<String>) -> Self {
        Self {
            name: name.into(),
            field_type: FieldType::Select,
            label: None,
            options: Some(options),
            default_value: None,
        }
    }

    /// Set display label
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Set default value
    pub fn with_default(mut self, value: serde_json::Value) -> Self {
        self.default_value = Some(value);
        self
    }
}

// ============================================================================
// Collection
// ============================================================================

/// Core collection data
///
/// Collections organize entities into trees with optional schema hints
/// for UI display. Use with `StoredEditable<CollectionId, Collection>`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Collection {
    /// Owning user
    pub user_id: UserId,
    /// Collection name
    pub name: String,
    /// Optional description
    pub description: Option<String>,
    /// Optional icon (emoji or icon name)
    pub icon: Option<String>,
    /// Schema hint: expected fields for items (advisory, not enforced)
    pub schema_hint: Option<Vec<FieldDefinition>>,
}

impl Collection {
    /// Create a new collection
    pub fn new(user_id: UserId, name: impl Into<String>) -> Self {
        Self {
            user_id,
            name: name.into(),
            description: None,
            icon: None,
            schema_hint: None,
        }
    }

    /// Set description
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Set icon
    pub fn with_icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    /// Set schema hint
    pub fn with_schema(mut self, fields: Vec<FieldDefinition>) -> Self {
        self.schema_hint = Some(fields);
        self
    }
}

// ============================================================================
// CollectionItem
// ============================================================================

/// An item in a collection
///
/// Items form a tree structure with parent references and position ordering.
/// Use with `StoredEditable<CollectionItemId, CollectionItem>`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CollectionItem {
    /// Parent collection
    pub collection_id: CollectionId,
    /// What this item references
    pub target: ItemTarget,
    /// Parent item (None for root items)
    pub parent_item_id: Option<CollectionItemId>,
    /// Position within parent (for ordering)
    pub position: i32,
    /// Optional display name override
    pub name_override: Option<String>,
}

impl CollectionItem {
    /// Create a new item referencing an entity
    pub fn entity(collection_id: CollectionId, entity_id: EntityId, position: i32) -> Self {
        Self {
            collection_id,
            target: ItemTarget::entity(entity_id),
            parent_item_id: None,
            position,
            name_override: None,
        }
    }

    /// Create a new item referencing a nested collection
    pub fn nested_collection(collection_id: CollectionId, nested_id: CollectionId, position: i32) -> Self {
        Self {
            collection_id,
            target: ItemTarget::collection(nested_id),
            parent_item_id: None,
            position,
            name_override: None,
        }
    }

    /// Set parent item
    pub fn with_parent(mut self, parent_id: CollectionItemId) -> Self {
        self.parent_item_id = Some(parent_id);
        self
    }

    /// Set name override
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name_override = Some(name.into());
        self
    }
}

// ============================================================================
// ViewType
// ============================================================================

/// Type of collection view
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewType {
    /// Simple list
    List,
    /// Table with columns
    Table,
    /// Kanban board grouped by field
    Board,
    /// Calendar view
    Calendar,
    /// Gallery view
    Gallery,
}

impl Default for ViewType {
    fn default() -> Self {
        Self::List
    }
}

// ============================================================================
// ViewConfig
// ============================================================================

/// Configuration for a collection view
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ViewConfig {
    /// View type
    pub view_type: ViewType,
    /// Fields to display (for table view)
    pub visible_fields: Option<Vec<String>>,
    /// Field to sort by
    pub sort_field: Option<String>,
    /// Sort ascending
    pub sort_ascending: bool,
    /// Field to group by (for board view)
    pub group_by_field: Option<String>,
    /// Filter conditions (JSON)
    pub filters: Option<serde_json::Value>,
}

impl ViewConfig {
    /// Create a list view config
    pub fn list() -> Self {
        Self {
            view_type: ViewType::List,
            ..Default::default()
        }
    }

    /// Create a table view config
    pub fn table(fields: Vec<String>) -> Self {
        Self {
            view_type: ViewType::Table,
            visible_fields: Some(fields),
            ..Default::default()
        }
    }

    /// Create a board view config
    pub fn board(group_by: impl Into<String>) -> Self {
        Self {
            view_type: ViewType::Board,
            group_by_field: Some(group_by.into()),
            ..Default::default()
        }
    }

    /// Set sort
    pub fn with_sort(mut self, field: impl Into<String>, ascending: bool) -> Self {
        self.sort_field = Some(field.into());
        self.sort_ascending = ascending;
        self
    }
}

// ============================================================================
// CollectionView
// ============================================================================

/// A saved view configuration for a collection
///
/// Views allow different ways to display the same collection.
/// Use with `StoredEditable<ViewId, CollectionView>`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CollectionView {
    /// Parent collection
    pub collection_id: CollectionId,
    /// View name
    pub name: String,
    /// View configuration
    pub config: ViewConfig,
    /// Whether this is the default view
    pub is_default: bool,
}

impl CollectionView {
    /// Create a new view
    pub fn new(collection_id: CollectionId, name: impl Into<String>, config: ViewConfig) -> Self {
        Self {
            collection_id,
            name: name.into(),
            config,
            is_default: false,
        }
    }

    /// Mark as default view
    pub fn as_default(mut self) -> Self {
        self.is_default = true;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_field_definition() {
        let status = FieldDefinition::select("status", vec!["todo".into(), "done".into()])
            .with_label("Status")
            .with_default(serde_json::json!("todo"));

        assert_eq!(status.name, "status");
        assert_eq!(status.field_type, FieldType::Select);
        assert_eq!(status.label, Some("Status".to_string()));
        assert_eq!(status.options, Some(vec!["todo".to_string(), "done".to_string()]));
    }

    #[test]
    fn test_collection_builder() {
        let user_id = UserId::new();
        let collection = Collection::new(user_id.clone(), "Tasks")
            .with_description("My task list")
            .with_icon("📋")
            .with_schema(vec![
                FieldDefinition::select("status", vec!["todo".into(), "in_progress".into(), "done".into()]),
                FieldDefinition::date("due_date"),
            ]);

        assert_eq!(collection.name, "Tasks");
        assert_eq!(collection.description, Some("My task list".to_string()));
        assert!(collection.schema_hint.is_some());
        assert_eq!(collection.schema_hint.as_ref().unwrap().len(), 2);
    }

    #[test]
    fn test_view_config() {
        let board = ViewConfig::board("status")
            .with_sort("due_date", true);

        assert_eq!(board.view_type, ViewType::Board);
        assert_eq!(board.group_by_field, Some("status".to_string()));
        assert_eq!(board.sort_field, Some("due_date".to_string()));
        assert!(board.sort_ascending);
    }

    #[test]
    fn test_item_target() {
        let entity_id = EntityId::new();
        let target = ItemTarget::entity(entity_id.clone());
        assert_eq!(target, ItemTarget::Entity(entity_id));

        let collection_id = CollectionId::new();
        let nested = ItemTarget::collection(collection_id.clone());
        assert_eq!(nested, ItemTarget::Collection(collection_id));
    }
}
