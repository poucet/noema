//! SQLite implementation of CollectionStore

use anyhow::{Context, Result};
use async_trait::async_trait;
use rusqlite::{params, Connection};

use super::SqliteStore;
use crate::storage::helper::unix_timestamp;
use crate::storage::ids::{
    CollectionId, CollectionItemId, CollectionViewId, EntityId, ItemFieldId, UserId,
};
use crate::storage::traits::{
    CollectionStore, ItemField, StoredCollection, StoredCollectionItem, StoredCollectionView,
    StoredItemField,
};
use crate::storage::types::{
    stored_editable, Collection, CollectionItem, CollectionView, FieldType, ItemTarget, ViewConfig,
};

/// Initialize collections schema
pub(crate) fn init_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        -- Collections (tree organization of entities)
        CREATE TABLE IF NOT EXISTS collections (
            id TEXT PRIMARY KEY,
            user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            name TEXT NOT NULL,
            description TEXT,
            icon TEXT,
            schema_hint TEXT,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );

        -- Index for user's collections
        CREATE INDEX IF NOT EXISTS idx_collections_user ON collections(user_id);

        -- Collection items (entities organized in tree structure)
        CREATE TABLE IF NOT EXISTS collection_items (
            id TEXT PRIMARY KEY,
            collection_id TEXT NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
            target_type TEXT NOT NULL,
            target_id TEXT NOT NULL,
            parent_item_id TEXT REFERENCES collection_items(id) ON DELETE CASCADE,
            position INTEGER NOT NULL,
            name_override TEXT,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );

        -- Indexes for collection items
        CREATE INDEX IF NOT EXISTS idx_collection_items_collection ON collection_items(collection_id);
        CREATE INDEX IF NOT EXISTS idx_collection_items_parent ON collection_items(parent_item_id);
        CREATE INDEX IF NOT EXISTS idx_collection_items_target ON collection_items(target_type, target_id);

        -- Item fields (typed metadata on collection items)
        CREATE TABLE IF NOT EXISTS item_fields (
            id TEXT PRIMARY KEY,
            item_id TEXT NOT NULL REFERENCES collection_items(id) ON DELETE CASCADE,
            field_name TEXT NOT NULL,
            field_type TEXT NOT NULL,
            value_text TEXT,
            value_number REAL,
            value_boolean INTEGER,
            value_json TEXT,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            UNIQUE(item_id, field_name)
        );

        -- Indexes for item fields
        CREATE INDEX IF NOT EXISTS idx_item_fields_item ON item_fields(item_id);
        CREATE INDEX IF NOT EXISTS idx_item_fields_name ON item_fields(field_name);

        -- Item tags (cross-cutting organization)
        CREATE TABLE IF NOT EXISTS item_tags (
            item_id TEXT NOT NULL REFERENCES collection_items(id) ON DELETE CASCADE,
            tag TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            PRIMARY KEY(item_id, tag)
        );

        -- Index for finding items by tag
        CREATE INDEX IF NOT EXISTS idx_item_tags_tag ON item_tags(tag);

        -- Collection views (saved view configurations)
        CREATE TABLE IF NOT EXISTS collection_views (
            id TEXT PRIMARY KEY,
            collection_id TEXT NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
            name TEXT NOT NULL,
            view_type TEXT NOT NULL,
            config TEXT NOT NULL,
            is_default INTEGER NOT NULL DEFAULT 0,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );

        -- Index for collection's views
        CREATE INDEX IF NOT EXISTS idx_collection_views_collection ON collection_views(collection_id);
        "#,
    )
    .context("Failed to initialize collections schema")?;
    Ok(())
}

// ============================================================================
// Helper Functions
// ============================================================================

fn parse_collection(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredCollection> {
    let id: CollectionId = row.get(0)?;
    let user_id: UserId = row.get(1)?;
    let name: String = row.get(2)?;
    let description: Option<String> = row.get(3)?;
    let icon: Option<String> = row.get(4)?;
    let schema_hint_json: Option<String> = row.get(5)?;
    let created_at: i64 = row.get(6)?;
    let updated_at: i64 = row.get(7)?;

    let schema_hint = schema_hint_json.and_then(|j| serde_json::from_str(&j).ok());

    let collection = Collection {
        user_id,
        name,
        description,
        icon,
        schema_hint,
    };

    Ok(stored_editable(id, collection, created_at, updated_at))
}

fn parse_collection_item(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredCollectionItem> {
    let id: CollectionItemId = row.get(0)?;
    let collection_id: CollectionId = row.get(1)?;
    let target_type: String = row.get(2)?;
    let target_id: String = row.get(3)?;
    let parent_item_id: Option<CollectionItemId> = row.get(4)?;
    let position: i32 = row.get(5)?;
    let name_override: Option<String> = row.get(6)?;
    let created_at: i64 = row.get(7)?;
    let updated_at: i64 = row.get(8)?;

    let target = match target_type.as_str() {
        "entity" => ItemTarget::Entity(EntityId::from_string(target_id)),
        "collection" => ItemTarget::Collection(CollectionId::from_string(target_id)),
        _ => ItemTarget::Entity(EntityId::from_string(target_id)),
    };

    let item = CollectionItem {
        collection_id,
        target,
        parent_item_id,
        position,
        name_override,
    };

    Ok(stored_editable(id, item, created_at, updated_at))
}

fn parse_collection_view(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredCollectionView> {
    let id: CollectionViewId = row.get(0)?;
    let collection_id: CollectionId = row.get(1)?;
    let name: String = row.get(2)?;
    let _view_type: String = row.get(3)?;
    let config_json: String = row.get(4)?;
    let is_default: bool = row.get(5)?;
    let created_at: i64 = row.get(6)?;
    let updated_at: i64 = row.get(7)?;

    let config: ViewConfig = serde_json::from_str(&config_json).unwrap_or_default();

    let view = CollectionView {
        collection_id,
        name,
        config,
        is_default,
    };

    Ok(stored_editable(id, view, created_at, updated_at))
}

fn parse_item_field(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredItemField> {
    let id: ItemFieldId = row.get(0)?;
    let _item_id: CollectionItemId = row.get(1)?;
    let field_name: String = row.get(2)?;
    let field_type_str: String = row.get(3)?;
    let value_text: Option<String> = row.get(4)?;
    let value_number: Option<f64> = row.get(5)?;
    let value_boolean: Option<bool> = row.get(6)?;
    let value_json_str: Option<String> = row.get(7)?;
    let created_at: i64 = row.get(8)?;
    let updated_at: i64 = row.get(9)?;

    let field_type = match field_type_str.as_str() {
        "text" => FieldType::Text,
        "number" => FieldType::Number,
        "boolean" => FieldType::Boolean,
        "date" => FieldType::Date,
        "select" => FieldType::Select,
        "multi_select" => FieldType::MultiSelect,
        "url" => FieldType::Url,
        _ => FieldType::Text,
    };

    let value_json = value_json_str.and_then(|j| serde_json::from_str(&j).ok());

    let field = ItemField {
        field_name,
        field_type,
        value_text,
        value_number,
        value_boolean,
        value_json,
    };

    Ok(stored_editable(id, field, created_at, updated_at))
}

fn field_type_to_str(ft: &FieldType) -> &'static str {
    match ft {
        FieldType::Text => "text",
        FieldType::Number => "number",
        FieldType::Boolean => "boolean",
        FieldType::Date => "date",
        FieldType::Select => "select",
        FieldType::MultiSelect => "multi_select",
        FieldType::Url => "url",
    }
}

// ============================================================================
// CollectionStore Implementation
// ============================================================================

#[async_trait]
impl CollectionStore for SqliteStore {
    // ========================================================================
    // Collection CRUD
    // ========================================================================

    async fn create_collection(
        &self,
        user_id: &UserId,
        name: &str,
        description: Option<&str>,
        icon: Option<&str>,
    ) -> Result<CollectionId> {
        let conn = self.conn().lock().unwrap();
        let id = CollectionId::new();
        let now = unix_timestamp();

        conn.execute(
            "INSERT INTO collections (id, user_id, name, description, icon, schema_hint, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?6, ?7)",
            params![
                id.as_str(),
                user_id.as_str(),
                name,
                description,
                icon,
                now,
                now
            ],
        )?;

        Ok(id)
    }

    async fn get_collection(&self, id: &CollectionId) -> Result<Option<StoredCollection>> {
        let conn = self.conn().lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, user_id, name, description, icon, schema_hint, created_at, updated_at
             FROM collections WHERE id = ?1",
        )?;

        let result = stmt
            .query_row(params![id.as_str()], parse_collection)
            .ok();

        Ok(result)
    }

    async fn list_collections(&self, user_id: &UserId) -> Result<Vec<StoredCollection>> {
        let conn = self.conn().lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, user_id, name, description, icon, schema_hint, created_at, updated_at
             FROM collections WHERE user_id = ?1
             ORDER BY name",
        )?;

        let collections = stmt
            .query_map(params![user_id.as_str()], parse_collection)?
            .filter_map(|r| r.ok())
            .collect();

        Ok(collections)
    }

    async fn update_collection(
        &self,
        id: &CollectionId,
        name: Option<&str>,
        description: Option<&str>,
        icon: Option<&str>,
    ) -> Result<bool> {
        let conn = self.conn().lock().unwrap();
        let now = unix_timestamp();

        // Build dynamic update
        let mut updates = vec!["updated_at = ?1"];
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(now)];

        if let Some(n) = name {
            updates.push("name = ?");
            params_vec.push(Box::new(n.to_string()));
        }
        if let Some(d) = description {
            updates.push("description = ?");
            params_vec.push(Box::new(d.to_string()));
        }
        if let Some(i) = icon {
            updates.push("icon = ?");
            params_vec.push(Box::new(i.to_string()));
        }

        // Renumber placeholders
        let mut sql = String::from("UPDATE collections SET ");
        for (i, update) in updates.iter().enumerate() {
            if i > 0 {
                sql.push_str(", ");
            }
            sql.push_str(&update.replace("?", &format!("?{}", i + 1)));
        }
        sql.push_str(&format!(" WHERE id = ?{}", params_vec.len() + 1));
        params_vec.push(Box::new(id.as_str().to_string()));

        let rows = conn.execute(&sql, rusqlite::params_from_iter(params_vec.iter().map(|p| p.as_ref())))?;
        Ok(rows > 0)
    }

    async fn delete_collection(&self, id: &CollectionId) -> Result<bool> {
        let conn = self.conn().lock().unwrap();
        let rows = conn.execute(
            "DELETE FROM collections WHERE id = ?1",
            params![id.as_str()],
        )?;
        Ok(rows > 0)
    }

    // ========================================================================
    // Item Management
    // ========================================================================

    async fn add_item(
        &self,
        collection_id: &CollectionId,
        target: &ItemTarget,
        parent_item_id: Option<&CollectionItemId>,
        position: i32,
        name_override: Option<&str>,
    ) -> Result<CollectionItemId> {
        let conn = self.conn().lock().unwrap();
        let id = CollectionItemId::new();
        let now = unix_timestamp();

        let (target_type, target_id) = match target {
            ItemTarget::Entity(eid) => ("entity", eid.as_str()),
            ItemTarget::Collection(cid) => ("collection", cid.as_str()),
        };

        conn.execute(
            "INSERT INTO collection_items (id, collection_id, target_type, target_id, parent_item_id, position, name_override, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                id.as_str(),
                collection_id.as_str(),
                target_type,
                target_id,
                parent_item_id.map(|p| p.as_str()),
                position,
                name_override,
                now,
                now
            ],
        )?;

        Ok(id)
    }

    async fn get_item(&self, id: &CollectionItemId) -> Result<Option<StoredCollectionItem>> {
        let conn = self.conn().lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, collection_id, target_type, target_id, parent_item_id, position, name_override, created_at, updated_at
             FROM collection_items WHERE id = ?1",
        )?;

        let result = stmt
            .query_row(params![id.as_str()], parse_collection_item)
            .ok();

        Ok(result)
    }

    async fn get_items(&self, collection_id: &CollectionId) -> Result<Vec<StoredCollectionItem>> {
        let conn = self.conn().lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, collection_id, target_type, target_id, parent_item_id, position, name_override, created_at, updated_at
             FROM collection_items WHERE collection_id = ?1
             ORDER BY position",
        )?;

        let items = stmt
            .query_map(params![collection_id.as_str()], parse_collection_item)?
            .filter_map(|r| r.ok())
            .collect();

        Ok(items)
    }

    async fn get_root_items(&self, collection_id: &CollectionId) -> Result<Vec<StoredCollectionItem>> {
        let conn = self.conn().lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, collection_id, target_type, target_id, parent_item_id, position, name_override, created_at, updated_at
             FROM collection_items WHERE collection_id = ?1 AND parent_item_id IS NULL
             ORDER BY position",
        )?;

        let items = stmt
            .query_map(params![collection_id.as_str()], parse_collection_item)?
            .filter_map(|r| r.ok())
            .collect();

        Ok(items)
    }

    async fn get_children(&self, item_id: &CollectionItemId) -> Result<Vec<StoredCollectionItem>> {
        let conn = self.conn().lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, collection_id, target_type, target_id, parent_item_id, position, name_override, created_at, updated_at
             FROM collection_items WHERE parent_item_id = ?1
             ORDER BY position",
        )?;

        let items = stmt
            .query_map(params![item_id.as_str()], parse_collection_item)?
            .filter_map(|r| r.ok())
            .collect();

        Ok(items)
    }

    async fn move_item(
        &self,
        item_id: &CollectionItemId,
        new_parent_id: Option<&CollectionItemId>,
        new_position: i32,
    ) -> Result<bool> {
        let conn = self.conn().lock().unwrap();
        let now = unix_timestamp();

        let rows = conn.execute(
            "UPDATE collection_items SET parent_item_id = ?1, position = ?2, updated_at = ?3 WHERE id = ?4",
            params![
                new_parent_id.map(|p| p.as_str()),
                new_position,
                now,
                item_id.as_str()
            ],
        )?;

        Ok(rows > 0)
    }

    async fn remove_item(&self, id: &CollectionItemId) -> Result<bool> {
        let conn = self.conn().lock().unwrap();
        let rows = conn.execute(
            "DELETE FROM collection_items WHERE id = ?1",
            params![id.as_str()],
        )?;
        Ok(rows > 0)
    }

    // ========================================================================
    // Field Operations
    // ========================================================================

    async fn set_field(
        &self,
        item_id: &CollectionItemId,
        field: &ItemField,
    ) -> Result<ItemFieldId> {
        let conn = self.conn().lock().unwrap();
        let now = unix_timestamp();
        let field_type_str = field_type_to_str(&field.field_type);
        let value_json = field.value_json.as_ref().map(|v| serde_json::to_string(v).unwrap_or_default());

        // Try to update existing field first
        let rows = conn.execute(
            "UPDATE item_fields SET field_type = ?1, value_text = ?2, value_number = ?3, value_boolean = ?4, value_json = ?5, updated_at = ?6
             WHERE item_id = ?7 AND field_name = ?8",
            params![
                field_type_str,
                field.value_text,
                field.value_number,
                field.value_boolean,
                value_json,
                now,
                item_id.as_str(),
                field.field_name
            ],
        )?;

        if rows > 0 {
            // Return existing field ID
            let id: String = conn.query_row(
                "SELECT id FROM item_fields WHERE item_id = ?1 AND field_name = ?2",
                params![item_id.as_str(), field.field_name],
                |row| row.get(0),
            )?;
            return Ok(ItemFieldId::from_string(id));
        }

        // Insert new field
        let id = ItemFieldId::new();
        conn.execute(
            "INSERT INTO item_fields (id, item_id, field_name, field_type, value_text, value_number, value_boolean, value_json, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                id.as_str(),
                item_id.as_str(),
                field.field_name,
                field_type_str,
                field.value_text,
                field.value_number,
                field.value_boolean,
                value_json,
                now,
                now
            ],
        )?;

        Ok(id)
    }

    async fn get_fields(&self, item_id: &CollectionItemId) -> Result<Vec<StoredItemField>> {
        let conn = self.conn().lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, item_id, field_name, field_type, value_text, value_number, value_boolean, value_json, created_at, updated_at
             FROM item_fields WHERE item_id = ?1",
        )?;

        let fields = stmt
            .query_map(params![item_id.as_str()], parse_item_field)?
            .filter_map(|r| r.ok())
            .collect();

        Ok(fields)
    }

    async fn remove_field(&self, item_id: &CollectionItemId, field_name: &str) -> Result<bool> {
        let conn = self.conn().lock().unwrap();
        let rows = conn.execute(
            "DELETE FROM item_fields WHERE item_id = ?1 AND field_name = ?2",
            params![item_id.as_str(), field_name],
        )?;
        Ok(rows > 0)
    }

    // ========================================================================
    // Tag Operations
    // ========================================================================

    async fn add_tag(&self, item_id: &CollectionItemId, tag: &str) -> Result<()> {
        let conn = self.conn().lock().unwrap();
        let now = unix_timestamp();

        conn.execute(
            "INSERT OR IGNORE INTO item_tags (item_id, tag, created_at) VALUES (?1, ?2, ?3)",
            params![item_id.as_str(), tag, now],
        )?;

        Ok(())
    }

    async fn remove_tag(&self, item_id: &CollectionItemId, tag: &str) -> Result<bool> {
        let conn = self.conn().lock().unwrap();
        let rows = conn.execute(
            "DELETE FROM item_tags WHERE item_id = ?1 AND tag = ?2",
            params![item_id.as_str(), tag],
        )?;
        Ok(rows > 0)
    }

    async fn get_tags(&self, item_id: &CollectionItemId) -> Result<Vec<String>> {
        let conn = self.conn().lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT tag FROM item_tags WHERE item_id = ?1 ORDER BY tag",
        )?;

        let tags = stmt
            .query_map(params![item_id.as_str()], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(tags)
    }

    async fn find_by_tag(
        &self,
        collection_id: &CollectionId,
        tag: &str,
    ) -> Result<Vec<StoredCollectionItem>> {
        let conn = self.conn().lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT ci.id, ci.collection_id, ci.target_type, ci.target_id, ci.parent_item_id, ci.position, ci.name_override, ci.created_at, ci.updated_at
             FROM collection_items ci
             JOIN item_tags it ON ci.id = it.item_id
             WHERE ci.collection_id = ?1 AND it.tag = ?2
             ORDER BY ci.position",
        )?;

        let items = stmt
            .query_map(params![collection_id.as_str(), tag], parse_collection_item)?
            .filter_map(|r| r.ok())
            .collect();

        Ok(items)
    }

    // ========================================================================
    // View Operations
    // ========================================================================

    async fn create_view(
        &self,
        collection_id: &CollectionId,
        name: &str,
        view: &CollectionView,
    ) -> Result<CollectionViewId> {
        let conn = self.conn().lock().unwrap();
        let id = CollectionViewId::new();
        let now = unix_timestamp();
        let config_json = serde_json::to_string(&view.config)?;
        let view_type = match view.config.view_type {
            crate::storage::types::ViewType::List => "list",
            crate::storage::types::ViewType::Table => "table",
            crate::storage::types::ViewType::Board => "board",
            crate::storage::types::ViewType::Calendar => "calendar",
            crate::storage::types::ViewType::Gallery => "gallery",
        };

        conn.execute(
            "INSERT INTO collection_views (id, collection_id, name, view_type, config, is_default, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                id.as_str(),
                collection_id.as_str(),
                name,
                view_type,
                config_json,
                view.is_default,
                now,
                now
            ],
        )?;

        Ok(id)
    }

    async fn get_view(&self, id: &CollectionViewId) -> Result<Option<StoredCollectionView>> {
        let conn = self.conn().lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, collection_id, name, view_type, config, is_default, created_at, updated_at
             FROM collection_views WHERE id = ?1",
        )?;

        let result = stmt
            .query_row(params![id.as_str()], parse_collection_view)
            .ok();

        Ok(result)
    }

    async fn list_views(&self, collection_id: &CollectionId) -> Result<Vec<StoredCollectionView>> {
        let conn = self.conn().lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, collection_id, name, view_type, config, is_default, created_at, updated_at
             FROM collection_views WHERE collection_id = ?1
             ORDER BY name",
        )?;

        let views = stmt
            .query_map(params![collection_id.as_str()], parse_collection_view)?
            .filter_map(|r| r.ok())
            .collect();

        Ok(views)
    }

    async fn get_default_view(&self, collection_id: &CollectionId) -> Result<Option<StoredCollectionView>> {
        let conn = self.conn().lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, collection_id, name, view_type, config, is_default, created_at, updated_at
             FROM collection_views WHERE collection_id = ?1 AND is_default = 1",
        )?;

        let result = stmt
            .query_row(params![collection_id.as_str()], parse_collection_view)
            .ok();

        Ok(result)
    }

    async fn update_view(&self, id: &CollectionViewId, view: &CollectionView) -> Result<bool> {
        let conn = self.conn().lock().unwrap();
        let now = unix_timestamp();
        let config_json = serde_json::to_string(&view.config)?;
        let view_type = match view.config.view_type {
            crate::storage::types::ViewType::List => "list",
            crate::storage::types::ViewType::Table => "table",
            crate::storage::types::ViewType::Board => "board",
            crate::storage::types::ViewType::Calendar => "calendar",
            crate::storage::types::ViewType::Gallery => "gallery",
        };

        let rows = conn.execute(
            "UPDATE collection_views SET name = ?1, view_type = ?2, config = ?3, is_default = ?4, updated_at = ?5 WHERE id = ?6",
            params![
                view.name,
                view_type,
                config_json,
                view.is_default,
                now,
                id.as_str()
            ],
        )?;

        Ok(rows > 0)
    }

    async fn delete_view(&self, id: &CollectionViewId) -> Result<bool> {
        let conn = self.conn().lock().unwrap();
        let rows = conn.execute(
            "DELETE FROM collection_views WHERE id = ?1",
            params![id.as_str()],
        )?;
        Ok(rows > 0)
    }

    // ========================================================================
    // Query Operations
    // ========================================================================

    async fn find_items_by_entity(&self, entity_id: &EntityId) -> Result<Vec<StoredCollectionItem>> {
        let conn = self.conn().lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, collection_id, target_type, target_id, parent_item_id, position, name_override, created_at, updated_at
             FROM collection_items WHERE target_type = 'entity' AND target_id = ?1",
        )?;

        let items = stmt
            .query_map(params![entity_id.as_str()], parse_collection_item)?
            .filter_map(|r| r.ok())
            .collect();

        Ok(items)
    }
}
