//! SQLite implementation of TemporalStore
//!
//! Provides time-range queries and activity summaries for LLM context.

use std::collections::HashMap;

use anyhow::{Context, Result};
use async_trait::async_trait;
use rusqlite::{params, Connection};

use super::SqliteStore;
use crate::storage::ids::{EntityId, UserId};
use crate::storage::traits::TemporalStore;
use crate::storage::types::{
    ActivitySummary, ContentKind, ContentPreview, EntityType, TemporalEntity, TemporalQuery,
};

/// Initialize temporal indexes for efficient time-range queries
pub(crate) fn init_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        -- Temporal indexes for time-range queries

        -- Content blocks: query by creation time (already created in text.rs, but IF NOT EXISTS is safe)
        CREATE INDEX IF NOT EXISTS idx_content_blocks_created ON content_blocks(created_at);

        -- Messages: query by creation time
        CREATE INDEX IF NOT EXISTS idx_messages_created ON messages(created_at);

        -- Turns: query by creation time
        CREATE INDEX IF NOT EXISTS idx_turns_created ON turns(created_at);

        -- Spans: query by creation time
        CREATE INDEX IF NOT EXISTS idx_spans_created ON spans(created_at);

        -- Document revisions: query by creation time
        CREATE INDEX IF NOT EXISTS idx_document_revisions_created ON document_revisions(created_at);

        -- Entities: query by creation and update time
        CREATE INDEX IF NOT EXISTS idx_entities_created ON entities(created_at);
        CREATE INDEX IF NOT EXISTS idx_entities_updated ON entities(updated_at);

        -- Collections: query by update time (for recent activity)
        CREATE INDEX IF NOT EXISTS idx_collections_updated ON collections(updated_at);

        -- Collection items: query by update time
        CREATE INDEX IF NOT EXISTS idx_collection_items_updated ON collection_items(updated_at);
        "#,
    )
    .context("Failed to initialize temporal indexes")?;
    Ok(())
}

// ============================================================================
// Content Preview Loading
// ============================================================================

/// Load content preview for a conversation entity
///
/// Gets the latest message text from the most recent span in the conversation's main view.
fn load_conversation_preview(conn: &Connection, entity_id: &EntityId) -> Option<ContentPreview> {
    // For conversations, entity_id IS the view_id (views are entities)
    // Get the latest message from the view's selected spans
    let result: rusqlite::Result<String> = conn.query_row(
        r#"
        SELECT cb.text
        FROM view_selections vs
        JOIN spans sp ON sp.id = vs.span_id
        JOIN messages m ON m.span_id = sp.id
        JOIN message_content mc ON mc.message_id = m.id
        JOIN content_blocks cb ON cb.id = mc.content_block_id
        WHERE vs.view_id = ?1
          AND mc.content_type = 'text'
        ORDER BY vs.sequence_number DESC, m.sequence_number DESC, mc.sequence_number DESC
        LIMIT 1
        "#,
        params![entity_id.as_str()],
        |row| row.get(0),
    );

    result.ok().map(ContentPreview::message)
}

/// Load content preview for a document entity
///
/// Gets the latest revision text from the document's first tab.
fn load_document_preview(conn: &Connection, entity_id: &EntityId) -> Option<ContentPreview> {
    // Documents are stored with entity_id matching the document_id
    // Get latest revision from the first tab
    let result: rusqlite::Result<String> = conn.query_row(
        r#"
        SELECT dr.content_markdown
        FROM documents d
        JOIN document_tabs dt ON dt.document_id = d.id
        JOIN document_revisions dr ON dr.tab_id = dt.id
        WHERE d.id = ?1
        ORDER BY dt.tab_index ASC, dr.revision_number DESC
        LIMIT 1
        "#,
        params![entity_id.as_str()],
        |row| row.get(0),
    );

    result.ok().map(ContentPreview::revision)
}

/// Load content preview for an asset entity
///
/// Returns metadata only (size, mime_type), never the blob data.
fn load_asset_preview(conn: &Connection, entity_id: &EntityId) -> Option<ContentPreview> {
    let result: rusqlite::Result<(i64, String)> = conn.query_row(
        "SELECT size, mime_type FROM assets WHERE id = ?1",
        params![entity_id.as_str()],
        |row| Ok((row.get(0)?, row.get(1)?)),
    );

    result
        .ok()
        .map(|(size, mime_type)| ContentPreview::asset(size as u64, mime_type))
}

/// Load content preview based on entity type
fn load_content_preview(
    conn: &Connection,
    entity_id: &EntityId,
    entity_type: &EntityType,
) -> Option<ContentPreview> {
    match entity_type.as_str() {
        "conversation" => load_conversation_preview(conn, entity_id),
        "document" => load_document_preview(conn, entity_id),
        "asset" => load_asset_preview(conn, entity_id),
        _ => None,
    }
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

        // Build base query with optional type filter
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
                    query.limit.map(|l| format!("LIMIT {}", l)).unwrap_or_default()
                );
                let types_str: Vec<String> = types.iter().map(|t| t.as_str().to_string()).collect();
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
                    query.limit.map(|l| format!("LIMIT {}", l)).unwrap_or_default()
                );
                (sql, None)
            }
        };

        // Execute query and collect base entities
        let mut entities: Vec<TemporalEntity> = {
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
                        Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?))
                    })?
                    .filter_map(|r| r.ok())
                    .collect()
                }
                None => {
                    stmt.query_map(params![user_id.as_str(), query.start, query.end], |row| {
                        Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?))
                    })?
                    .filter_map(|r| r.ok())
                    .collect()
                }
            };

            rows.into_iter()
                .map(|(id, entity_type, name, created_at, updated_at)| TemporalEntity {
                    entity_id: EntityId::from_string(id),
                    entity_type: EntityType::new(entity_type),
                    name,
                    created_at,
                    updated_at,
                    content_preview: None,
                })
                .collect()
        };

        // Load content previews if requested
        if query.include_content {
            for entity in &mut entities {
                entity.content_preview =
                    load_content_preview(&conn, &entity.entity_id, &entity.entity_type);
            }
        }

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

        summary.entity_counts = counts;

        // Count messages created in range (across all user's conversations)
        let message_count: i64 = conn
            .query_row(
                r#"
                SELECT COUNT(*)
                FROM messages m
                JOIN spans sp ON sp.id = m.span_id
                JOIN turns t ON t.id = sp.turn_id
                JOIN view_selections vs ON vs.turn_id = t.id
                JOIN entities e ON e.id = vs.view_id
                WHERE e.user_id = ?1
                  AND m.created_at >= ?2
                  AND m.created_at <= ?3
                "#,
                params![user_id.as_str(), start, end],
                |row| row.get(0),
            )
            .unwrap_or(0);
        summary.total_messages = message_count as u32;

        // Count document revisions created in range
        let revision_count: i64 = conn
            .query_row(
                r#"
                SELECT COUNT(*)
                FROM document_revisions dr
                JOIN document_tabs dt ON dt.id = dr.tab_id
                JOIN documents d ON d.id = dt.document_id
                WHERE d.user_id = ?1
                  AND dr.created_at >= ?2
                  AND dr.created_at <= ?3
                "#,
                params![user_id.as_str(), start, end],
                |row| row.get(0),
            )
            .unwrap_or(0);
        summary.total_revisions = revision_count as u32;

        Ok(summary)
    }

    async fn render_activity_context(
        &self,
        user_id: &UserId,
        query: &TemporalQuery,
        max_chars: Option<u32>,
    ) -> Result<String> {
        // Always load content for rendering
        let mut content_query = query.clone();
        content_query.include_content = true;

        let entities = self.query_entities(user_id, &content_query).await?;

        if entities.is_empty() {
            return Ok(String::new());
        }

        // Group entities by type
        let mut by_type: HashMap<String, Vec<&TemporalEntity>> = HashMap::new();
        for entity in &entities {
            by_type
                .entry(entity.entity_type.as_str().to_string())
                .or_default()
                .push(entity);
        }

        // Format time range for header
        let start_date = format_timestamp(query.start);
        let end_date = format_timestamp(query.end);

        let mut output = format!("## Recent Activity ({} - {})\n\n", start_date, end_date);

        // Render each type section
        for (type_name, type_entities) in &by_type {
            let section_title = pluralize_type(type_name);
            output.push_str(&format!("### {}\n", section_title));

            for entity in type_entities {
                let name = entity.name.as_deref().unwrap_or("Untitled");
                let age = format_relative_time(entity.updated_at);

                match &entity.content_preview {
                    Some(preview) => {
                        let preview_text = match preview.kind {
                            ContentKind::Message | ContentKind::Revision => {
                                preview.text.as_deref().map(truncate_preview).unwrap_or("")
                            }
                            ContentKind::Asset => {
                                let size = preview.byte_size.unwrap_or(0);
                                let mime = preview.mime_type.as_deref().unwrap_or("unknown");
                                &format!("[{}, {}]", mime, format_bytes(size))
                            }
                        };
                        output.push_str(&format!(
                            "- **{}** (updated {}): \"{}\"\n",
                            name, age, preview_text
                        ));
                    }
                    None => {
                        output.push_str(&format!("- **{}** (updated {})\n", name, age));
                    }
                }
            }
            output.push('\n');
        }

        // Truncate if needed
        if let Some(max) = max_chars {
            if output.len() > max as usize {
                output.truncate(max as usize - 3);
                output.push_str("...");
            }
        }

        Ok(output)
    }
}

// ============================================================================
// Formatting Helpers
// ============================================================================

/// Format a unix timestamp (ms) as a short date
fn format_timestamp(ts: i64) -> String {
    use std::time::{Duration, UNIX_EPOCH};
    let datetime = UNIX_EPOCH + Duration::from_millis(ts as u64);
    let secs = datetime
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // Simple date formatting (day/month)
    let days_since_epoch = secs / 86400;
    let year = 1970 + (days_since_epoch / 365);
    let day_of_year = days_since_epoch % 365;
    let month = (day_of_year / 30).min(11) + 1;
    let day = (day_of_year % 30) + 1;

    let month_name = match month {
        1 => "Jan",
        2 => "Feb",
        3 => "Mar",
        4 => "Apr",
        5 => "May",
        6 => "Jun",
        7 => "Jul",
        8 => "Aug",
        9 => "Sep",
        10 => "Oct",
        11 => "Nov",
        _ => "Dec",
    };

    format!("{} {}, {}", month_name, day, year)
}

/// Format a timestamp as relative time (e.g., "2h ago", "1d ago")
fn format_relative_time(ts: i64) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;

    let diff_ms = now - ts;
    let diff_secs = diff_ms / 1000;
    let diff_mins = diff_secs / 60;
    let diff_hours = diff_mins / 60;
    let diff_days = diff_hours / 24;

    if diff_days > 0 {
        format!("{}d ago", diff_days)
    } else if diff_hours > 0 {
        format!("{}h ago", diff_hours)
    } else if diff_mins > 0 {
        format!("{}m ago", diff_mins)
    } else {
        "just now".to_string()
    }
}

/// Truncate preview text for display
fn truncate_preview(text: &str) -> &str {
    let max_len = 80;
    if text.len() <= max_len {
        text
    } else {
        // Find a word boundary
        let truncated = &text[..max_len];
        truncated
            .rfind(' ')
            .map(|i| &truncated[..i])
            .unwrap_or(truncated)
    }
}

/// Format byte size for display
fn format_bytes(bytes: u64) -> String {
    if bytes >= 1_000_000 {
        format!("{:.1}MB", bytes as f64 / 1_000_000.0)
    } else if bytes >= 1_000 {
        format!("{:.1}KB", bytes as f64 / 1_000.0)
    } else {
        format!("{}B", bytes)
    }
}

/// Pluralize entity type for section headers
fn pluralize_type(type_name: &str) -> String {
    match type_name {
        "conversation" => "Conversations".to_string(),
        "document" => "Documents".to_string(),
        "asset" => "Assets".to_string(),
        "collection" => "Collections".to_string(),
        _ => format!("{}s", type_name),
    }
}
