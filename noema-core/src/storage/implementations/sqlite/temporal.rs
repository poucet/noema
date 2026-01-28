//! SQLite implementation of TemporalStore
//!
//! Queries only the `entities` table. Content loading is the caller's
//! responsibility via domain-specific stores.

use std::collections::HashMap;

use anyhow::{Context, Result};
use async_trait::async_trait;
use rusqlite::{params, Connection};

use super::SqliteStore;
use crate::storage::ids::{EntityId, UserId};
use crate::storage::traits::TemporalStore;
use crate::storage::types::{ActivitySummary, EntityType, TemporalEntity, TemporalQuery};

/// Initialize temporal indexes for efficient time-range queries
pub(crate) fn init_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        -- Temporal indexes for time-range queries on entities table
        CREATE INDEX IF NOT EXISTS idx_entities_created ON entities(created_at);
        CREATE INDEX IF NOT EXISTS idx_entities_updated ON entities(updated_at);
        CREATE INDEX IF NOT EXISTS idx_entities_user_updated ON entities(user_id, updated_at);
        "#,
    )
    .context("Failed to initialize temporal indexes")?;
    Ok(())
}

// ============================================================================
// TemporalStore Implementation
// ============================================================================

#[async_trait]
impl TemporalStore for SqliteStore {
    async fn query_entities(
        &self,
        user_id: &UserId,
        query: &TemporalQuery,
    ) -> Result<Vec<TemporalEntity>> {
        let conn = self.conn().lock().unwrap();

        // Build query with optional type filter
        let (sql, type_filter): (String, Option<Vec<String>>) = match &query.entity_types {
            Some(types) if !types.is_empty() => {
                let placeholders: Vec<&str> = types.iter().map(|_| "?").collect();
                let type_list = placeholders.join(", ");
                let sql = format!(
                    r#"
                    SELECT id, entity_type, name, created_at, updated_at
                    FROM entities
                    WHERE user_id = ?1
                      AND updated_at >= ?2
                      AND updated_at <= ?3
                      AND is_archived = 0
                      AND entity_type IN ({})
                    ORDER BY updated_at DESC
                    {}
                    "#,
                    type_list,
                    query
                        .limit
                        .map(|l| format!("LIMIT {}", l))
                        .unwrap_or_default()
                );
                let types_str: Vec<String> =
                    types.iter().map(|t| t.as_str().to_string()).collect();
                (sql, Some(types_str))
            }
            _ => {
                let sql = format!(
                    r#"
                    SELECT id, entity_type, name, created_at, updated_at
                    FROM entities
                    WHERE user_id = ?1
                      AND updated_at >= ?2
                      AND updated_at <= ?3
                      AND is_archived = 0
                    ORDER BY updated_at DESC
                    {}
                    "#,
                    query
                        .limit
                        .map(|l| format!("LIMIT {}", l))
                        .unwrap_or_default()
                );
                (sql, None)
            }
        };

        let mut stmt = conn.prepare(&sql)?;

        let rows: Vec<(String, String, Option<String>, i64, i64)> = match &type_filter {
            Some(types) => {
                // Build params dynamically based on number of types
                let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = vec![
                    Box::new(user_id.as_str().to_string()),
                    Box::new(query.start),
                    Box::new(query.end),
                ];
                for t in types {
                    params_vec.push(Box::new(t.clone()));
                }
                let params_refs: Vec<&dyn rusqlite::ToSql> =
                    params_vec.iter().map(|p| p.as_ref()).collect();

                stmt.query_map(params_refs.as_slice(), |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                })?
                .filter_map(|r| r.ok())
                .collect()
            }
            None => stmt
                .query_map(params![user_id.as_str(), query.start, query.end], |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                })?
                .filter_map(|r| r.ok())
                .collect(),
        };

        let entities = rows
            .into_iter()
            .map(|(id, entity_type, name, created_at, updated_at)| TemporalEntity {
                entity_id: EntityId::from_string(id),
                entity_type: EntityType::new(entity_type),
                name,
                created_at,
                updated_at,
            })
            .collect();

        Ok(entities)
    }

    async fn get_activity_summary(
        &self,
        user_id: &UserId,
        start: i64,
        end: i64,
    ) -> Result<ActivitySummary> {
        let conn = self.conn().lock().unwrap();

        let mut summary = ActivitySummary::new(start, end);

        // Count entities by type
        let mut stmt = conn.prepare(
            r#"
            SELECT entity_type, COUNT(*) as count
            FROM entities
            WHERE user_id = ?1
              AND updated_at >= ?2
              AND updated_at <= ?3
              AND is_archived = 0
            GROUP BY entity_type
            "#,
        )?;

        let counts: HashMap<EntityType, u32> = stmt
            .query_map(params![user_id.as_str(), start, end], |row| {
                let entity_type: String = row.get(0)?;
                let count: i64 = row.get(1)?;
                Ok((EntityType::new(entity_type), count as u32))
            })?
            .filter_map(|r| r.ok())
            .collect();

        summary.total_entities = counts.values().sum();
        summary.entity_counts = counts;

        Ok(summary)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::helper::unix_timestamp;
    use crate::storage::traits::EntityStore;

    async fn setup_store_with_user() -> (SqliteStore, UserId) {
        let store = SqliteStore::in_memory().unwrap();
        let user_id = UserId::new();

        // Create test user
        {
            let conn = store.conn().lock().unwrap();
            conn.execute(
                "INSERT INTO users (id, email, created_at) VALUES (?1, ?2, ?3)",
                params![user_id.as_str(), "test@example.com", unix_timestamp()],
            )
            .unwrap();
        }

        (store, user_id)
    }

    #[tokio::test]
    async fn test_query_entities_empty() {
        let (store, user_id) = setup_store_with_user().await;

        let query = TemporalQuery::new(0, i64::MAX);
        let entities = store.query_entities(&user_id, &query).await.unwrap();

        assert!(entities.is_empty());
    }

    #[tokio::test]
    async fn test_query_entities_returns_results() {
        let (store, user_id) = setup_store_with_user().await;

        // Create some entities
        let conv_id = store
            .create_entity(EntityType::conversation(), Some(&user_id))
            .await
            .unwrap();
        let doc_id = store
            .create_entity(EntityType::document(), Some(&user_id))
            .await
            .unwrap();

        let query = TemporalQuery::new(0, i64::MAX);
        let entities = store.query_entities(&user_id, &query).await.unwrap();

        assert_eq!(entities.len(), 2);
        let ids: Vec<_> = entities.iter().map(|e| &e.entity_id).collect();
        assert!(ids.contains(&&conv_id));
        assert!(ids.contains(&&doc_id));
    }

    #[tokio::test]
    async fn test_query_entities_filter_by_type() {
        let (store, user_id) = setup_store_with_user().await;

        // Create entities of different types
        store
            .create_entity(EntityType::conversation(), Some(&user_id))
            .await
            .unwrap();
        store
            .create_entity(EntityType::conversation(), Some(&user_id))
            .await
            .unwrap();
        store
            .create_entity(EntityType::document(), Some(&user_id))
            .await
            .unwrap();

        // Query only conversations
        let query =
            TemporalQuery::new(0, i64::MAX).with_types(vec![EntityType::conversation()]);
        let entities = store.query_entities(&user_id, &query).await.unwrap();

        assert_eq!(entities.len(), 2);
        assert!(entities
            .iter()
            .all(|e| e.entity_type.as_str() == "conversation"));
    }

    #[tokio::test]
    async fn test_query_entities_time_range() {
        let (store, user_id) = setup_store_with_user().await;

        // Create entity
        store
            .create_entity(EntityType::conversation(), Some(&user_id))
            .await
            .unwrap();

        let now = unix_timestamp();

        // Query with time range that includes the entity
        let query = TemporalQuery::new(now - 1000, now + 1000);
        let entities = store.query_entities(&user_id, &query).await.unwrap();
        assert_eq!(entities.len(), 1);

        // Query with time range in the past (before entity was created)
        let query = TemporalQuery::new(0, now - 1000);
        let entities = store.query_entities(&user_id, &query).await.unwrap();
        assert!(entities.is_empty());
    }

    #[tokio::test]
    async fn test_query_entities_limit() {
        let (store, user_id) = setup_store_with_user().await;

        // Create 5 entities
        for _ in 0..5 {
            store
                .create_entity(EntityType::conversation(), Some(&user_id))
                .await
                .unwrap();
        }

        // Query with limit
        let query = TemporalQuery::new(0, i64::MAX).with_limit(3);
        let entities = store.query_entities(&user_id, &query).await.unwrap();

        assert_eq!(entities.len(), 3);
    }

    #[tokio::test]
    async fn test_query_entities_excludes_archived() {
        let (store, user_id) = setup_store_with_user().await;

        // Create and archive an entity
        let entity_id = store
            .create_entity(EntityType::conversation(), Some(&user_id))
            .await
            .unwrap();
        store.archive_entity(&entity_id).await.unwrap();

        // Create a non-archived entity
        store
            .create_entity(EntityType::document(), Some(&user_id))
            .await
            .unwrap();

        let query = TemporalQuery::new(0, i64::MAX);
        let entities = store.query_entities(&user_id, &query).await.unwrap();

        // Only the non-archived entity should be returned
        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0].entity_type.as_str(), "document");
    }

    #[tokio::test]
    async fn test_query_entities_ordered_by_updated_at_desc() {
        let (store, user_id) = setup_store_with_user().await;

        // Create entities (most recent last)
        let id1 = store
            .create_entity(EntityType::conversation(), Some(&user_id))
            .await
            .unwrap();

        // Small delay to ensure different timestamps
        std::thread::sleep(std::time::Duration::from_millis(10));

        let id2 = store
            .create_entity(EntityType::conversation(), Some(&user_id))
            .await
            .unwrap();

        let query = TemporalQuery::new(0, i64::MAX);
        let entities = store.query_entities(&user_id, &query).await.unwrap();

        // Most recently updated should be first
        assert_eq!(entities[0].entity_id, id2);
        assert_eq!(entities[1].entity_id, id1);
    }

    #[tokio::test]
    async fn test_get_activity_summary_empty() {
        let (store, user_id) = setup_store_with_user().await;

        let summary = store
            .get_activity_summary(&user_id, 0, i64::MAX)
            .await
            .unwrap();

        assert!(summary.entity_counts.is_empty());
        assert_eq!(summary.total_entities, 0);
    }

    #[tokio::test]
    async fn test_get_activity_summary_counts_by_type() {
        let (store, user_id) = setup_store_with_user().await;

        // Create entities of different types
        store
            .create_entity(EntityType::conversation(), Some(&user_id))
            .await
            .unwrap();
        store
            .create_entity(EntityType::conversation(), Some(&user_id))
            .await
            .unwrap();
        store
            .create_entity(EntityType::document(), Some(&user_id))
            .await
            .unwrap();

        let summary = store
            .get_activity_summary(&user_id, 0, i64::MAX)
            .await
            .unwrap();

        assert_eq!(summary.total_entities, 3);
        assert_eq!(
            summary.entity_counts.get(&EntityType::conversation()),
            Some(&2)
        );
        assert_eq!(
            summary.entity_counts.get(&EntityType::document()),
            Some(&1)
        );
    }

    #[tokio::test]
    async fn test_get_activity_summary_excludes_archived() {
        let (store, user_id) = setup_store_with_user().await;

        // Create and archive an entity
        let entity_id = store
            .create_entity(EntityType::conversation(), Some(&user_id))
            .await
            .unwrap();
        store.archive_entity(&entity_id).await.unwrap();

        // Create a non-archived entity
        store
            .create_entity(EntityType::document(), Some(&user_id))
            .await
            .unwrap();

        let summary = store
            .get_activity_summary(&user_id, 0, i64::MAX)
            .await
            .unwrap();

        assert_eq!(summary.total_entities, 1);
        assert_eq!(
            summary.entity_counts.get(&EntityType::conversation()),
            None
        );
        assert_eq!(
            summary.entity_counts.get(&EntityType::document()),
            Some(&1)
        );
    }
}
