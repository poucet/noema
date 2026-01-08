//! SQLite implementation of DocumentStore

use anyhow::{Context, Result};
use async_trait::async_trait;
use rusqlite::{params, Connection};
use uuid::Uuid;

use super::{DocumentInfo, DocumentRevisionInfo, DocumentSource, DocumentStore, DocumentTabInfo};
use crate::storage::session::SqliteStore;
use crate::storage::helper::unix_timestamp;

pub (crate) fn init_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        -- Documents (Episteme-compatible)
        CREATE TABLE IF NOT EXISTS documents (
            id TEXT PRIMARY KEY,
            user_id TEXT REFERENCES users(id) ON DELETE CASCADE,
            title TEXT NOT NULL,
            source TEXT NOT NULL,
            source_id TEXT,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );

        -- Document Tabs (hierarchical structure within documents)
        CREATE TABLE IF NOT EXISTS document_tabs (
            id TEXT PRIMARY KEY,
            document_id TEXT REFERENCES documents(id) ON DELETE CASCADE,
            parent_tab_id TEXT REFERENCES document_tabs(id) ON DELETE CASCADE,
            tab_index INTEGER NOT NULL,
            title TEXT NOT NULL,
            icon TEXT,
            content_markdown TEXT,
            referenced_assets TEXT,
            source_tab_id TEXT,
            current_revision_id TEXT,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );

        -- Document Revisions (version history for tabs)
        CREATE TABLE IF NOT EXISTS document_revisions (
            id TEXT PRIMARY KEY,
            tab_id TEXT REFERENCES document_tabs(id) ON DELETE CASCADE,
            revision_number INTEGER NOT NULL,
            parent_revision_id TEXT REFERENCES document_revisions(id) ON DELETE SET NULL,
            content_markdown TEXT NOT NULL,
            content_hash TEXT NOT NULL,
            referenced_assets TEXT,
            created_at INTEGER NOT NULL,
            created_by TEXT NOT NULL DEFAULT 'import'
        );

        -- Add foreign key for current_revision_id after document_revisions exists
        -- (SQLite doesn't support ALTER TABLE ADD CONSTRAINT, so we handle this at app level)

        -- Indexes for documents
        CREATE INDEX IF NOT EXISTS idx_documents_user ON documents(user_id);
        CREATE INDEX IF NOT EXISTS idx_documents_source ON documents(source);
        CREATE INDEX IF NOT EXISTS idx_documents_user_source_id ON documents(user_id, source, source_id);
        CREATE INDEX IF NOT EXISTS idx_document_tabs_document ON document_tabs(document_id);
        CREATE INDEX IF NOT EXISTS idx_document_tabs_parent ON document_tabs(parent_tab_id);
        CREATE INDEX IF NOT EXISTS idx_document_revisions_tab ON document_revisions(tab_id);
        "#,
    )
    .context("Failed to initialize document schema")?;
    Ok(())
}

#[async_trait]
impl DocumentStore for SqliteStore {
    // ========== Document Methods ==========

    async fn create_document(
        &self,
        user_id: &str,
        title: &str,
        source: DocumentSource,
        source_id: Option<&str>,
    ) -> Result<String> {
        let conn = self.conn().lock().unwrap();
        let id = Uuid::new_v4().to_string();
        let now = unix_timestamp();

        conn.execute(
            "INSERT INTO documents (id, user_id, title, source, source_id, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![&id, user_id, title, source.to_string(), source_id, now, now],
        )?;

        Ok(id)
    }

    async fn get_document(&self, id: &str) -> Result<Option<DocumentInfo>> {
        let conn = self.conn().lock().unwrap();
        let doc = conn
            .query_row(
                "SELECT id, user_id, title, source, source_id, created_at, updated_at
                 FROM documents WHERE id = ?1",
                params![id],
                |row| {
                    let source_str: String = row.get(3)?;
                    Ok(DocumentInfo {
                        id: row.get(0)?,
                        user_id: row.get(1)?,
                        title: row.get(2)?,
                        source: source_str
                            .parse::<DocumentSource>()
                            .unwrap_or(DocumentSource::UserCreated),
                        source_id: row.get(4)?,
                        created_at: row.get(5)?,
                        updated_at: row.get(6)?,
                    })
                },
            )
            .ok();
        Ok(doc)
    }

    async fn get_document_by_source(
        &self,
        user_id: &str,
        source: DocumentSource,
        source_id: &str,
    ) -> Result<Option<DocumentInfo>> {
        let conn = self.conn().lock().unwrap();
        let doc = conn
            .query_row(
                "SELECT id, user_id, title, source, source_id, created_at, updated_at
                 FROM documents WHERE user_id = ?1 AND source = ?2 AND source_id = ?3",
                params![user_id, source.to_string(), source_id],
                |row| {
                    let source_str: String = row.get(3)?;
                    Ok(DocumentInfo {
                        id: row.get(0)?,
                        user_id: row.get(1)?,
                        title: row.get(2)?,
                        source: source_str
                            .parse::<DocumentSource>()
                            .unwrap_or(DocumentSource::UserCreated),
                        source_id: row.get(4)?,
                        created_at: row.get(5)?,
                        updated_at: row.get(6)?,
                    })
                },
            )
            .ok();
        Ok(doc)
    }

    async fn list_documents(&self, user_id: &str) -> Result<Vec<DocumentInfo>> {
        let conn = self.conn().lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, user_id, title, source, source_id, created_at, updated_at
             FROM documents WHERE user_id = ?1 ORDER BY updated_at DESC",
        )?;

        let docs = stmt
            .query_map(params![user_id], |row| {
                let source_str: String = row.get(3)?;
                Ok(DocumentInfo {
                    id: row.get(0)?,
                    user_id: row.get(1)?,
                    title: row.get(2)?,
                    source: source_str
                        .parse::<DocumentSource>()
                        .unwrap_or(DocumentSource::UserCreated),
                    source_id: row.get(4)?,
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(docs)
    }

    async fn search_documents(
        &self,
        user_id: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<DocumentInfo>> {
        let conn = self.conn().lock().unwrap();
        let pattern = format!("%{}%", query);
        let mut stmt = conn.prepare(
            "SELECT id, user_id, title, source, source_id, created_at, updated_at
             FROM documents
             WHERE user_id = ?1 AND title LIKE ?2 COLLATE NOCASE
             ORDER BY updated_at DESC
             LIMIT ?3",
        )?;

        let docs = stmt
            .query_map(params![user_id, &pattern, limit as i64], |row| {
                let source_str: String = row.get(3)?;
                Ok(DocumentInfo {
                    id: row.get(0)?,
                    user_id: row.get(1)?,
                    title: row.get(2)?,
                    source: source_str
                        .parse::<DocumentSource>()
                        .unwrap_or(DocumentSource::UserCreated),
                    source_id: row.get(4)?,
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(docs)
    }

    async fn update_document_title(&self, id: &str, title: &str) -> Result<()> {
        let conn = self.conn().lock().unwrap();
        let now = unix_timestamp();
        conn.execute(
            "UPDATE documents SET title = ?1, updated_at = ?2 WHERE id = ?3",
            params![title, now, id],
        )?;
        Ok(())
    }

    async fn delete_document(&self, id: &str) -> Result<bool> {
        let conn = self.conn().lock().unwrap();
        // Revisions and tabs will be cascade deleted
        let rows = conn.execute("DELETE FROM documents WHERE id = ?1", params![id])?;
        Ok(rows > 0)
    }

    // ========== Document Tab Methods ==========

    async fn create_document_tab(
        &self,
        document_id: &str,
        parent_tab_id: Option<&str>,
        tab_index: i32,
        title: &str,
        icon: Option<&str>,
        content_markdown: Option<&str>,
        referenced_assets: &[String],
        source_tab_id: Option<&str>,
    ) -> Result<String> {
        let conn = self.conn().lock().unwrap();
        let id = Uuid::new_v4().to_string();
        let now = unix_timestamp();
        let assets_json = serde_json::to_string(referenced_assets)?;

        conn.execute(
            "INSERT INTO document_tabs (id, document_id, parent_tab_id, tab_index, title, icon, content_markdown, referenced_assets, source_tab_id, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![&id, document_id, parent_tab_id, tab_index, title, icon, content_markdown, &assets_json, source_tab_id, now, now],
        )?;

        Ok(id)
    }

    async fn get_document_tab(&self, id: &str) -> Result<Option<DocumentTabInfo>> {
        let conn = self.conn().lock().unwrap();
        let tab = conn
            .query_row(
                "SELECT id, document_id, parent_tab_id, tab_index, title, icon, content_markdown, referenced_assets, source_tab_id, current_revision_id, created_at, updated_at
                 FROM document_tabs WHERE id = ?1",
                params![id],
                |row| {
                    let assets_json: Option<String> = row.get(7)?;
                    let referenced_assets: Vec<String> = assets_json
                        .map(|j| serde_json::from_str(&j).unwrap_or_default())
                        .unwrap_or_default();
                    Ok(DocumentTabInfo {
                        id: row.get(0)?,
                        document_id: row.get(1)?,
                        parent_tab_id: row.get(2)?,
                        tab_index: row.get(3)?,
                        title: row.get(4)?,
                        icon: row.get(5)?,
                        content_markdown: row.get(6)?,
                        referenced_assets,
                        source_tab_id: row.get(8)?,
                        current_revision_id: row.get(9)?,
                        created_at: row.get(10)?,
                        updated_at: row.get(11)?,
                    })
                },
            )
            .ok();
        Ok(tab)
    }

    async fn list_document_tabs(&self, document_id: &str) -> Result<Vec<DocumentTabInfo>> {
        let conn = self.conn().lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, document_id, parent_tab_id, tab_index, title, icon, content_markdown, referenced_assets, source_tab_id, current_revision_id, created_at, updated_at
             FROM document_tabs WHERE document_id = ?1 ORDER BY tab_index",
        )?;

        let tabs = stmt
            .query_map(params![document_id], |row| {
                let assets_json: Option<String> = row.get(7)?;
                let referenced_assets: Vec<String> = assets_json
                    .map(|j| serde_json::from_str(&j).unwrap_or_default())
                    .unwrap_or_default();
                Ok(DocumentTabInfo {
                    id: row.get(0)?,
                    document_id: row.get(1)?,
                    parent_tab_id: row.get(2)?,
                    tab_index: row.get(3)?,
                    title: row.get(4)?,
                    icon: row.get(5)?,
                    content_markdown: row.get(6)?,
                    referenced_assets,
                    source_tab_id: row.get(8)?,
                    current_revision_id: row.get(9)?,
                    created_at: row.get(10)?,
                    updated_at: row.get(11)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(tabs)
    }

    async fn update_document_tab_content(
        &self,
        id: &str,
        content_markdown: &str,
        referenced_assets: &[String],
    ) -> Result<()> {
        let conn = self.conn().lock().unwrap();
        let now = unix_timestamp();
        let assets_json = serde_json::to_string(referenced_assets)?;

        conn.execute(
            "UPDATE document_tabs SET content_markdown = ?1, referenced_assets = ?2, updated_at = ?3 WHERE id = ?4",
            params![content_markdown, &assets_json, now, id],
        )?;

        Ok(())
    }

    async fn update_document_tab_parent(
        &self,
        id: &str,
        parent_tab_id: Option<&str>,
    ) -> Result<()> {
        let conn = self.conn().lock().unwrap();
        let now = unix_timestamp();
        conn.execute(
            "UPDATE document_tabs SET parent_tab_id = ?1, updated_at = ?2 WHERE id = ?3",
            params![parent_tab_id, now, id],
        )?;
        Ok(())
    }

    async fn set_document_tab_revision(&self, tab_id: &str, revision_id: &str) -> Result<()> {
        let conn = self.conn().lock().unwrap();
        let now = unix_timestamp();
        conn.execute(
            "UPDATE document_tabs SET current_revision_id = ?1, updated_at = ?2 WHERE id = ?3",
            params![revision_id, now, tab_id],
        )?;
        Ok(())
    }

    async fn delete_document_tab(&self, id: &str) -> Result<bool> {
        let conn = self.conn().lock().unwrap();
        let rows = conn.execute("DELETE FROM document_tabs WHERE id = ?1", params![id])?;
        Ok(rows > 0)
    }

    // ========== Document Revision Methods ==========

    async fn create_document_revision(
        &self,
        tab_id: &str,
        content_markdown: &str,
        content_hash: &str,
        referenced_assets: &[String],
        created_by: &str,
    ) -> Result<String> {
        let conn = self.conn().lock().unwrap();
        let id = Uuid::new_v4().to_string();
        let now = unix_timestamp();
        let assets_json = serde_json::to_string(referenced_assets)?;

        // Get next revision number
        let revision_number: i32 = conn
            .query_row(
                "SELECT COALESCE(MAX(revision_number), 0) + 1 FROM document_revisions WHERE tab_id = ?1",
                params![tab_id],
                |row| row.get(0),
            )
            .unwrap_or(1);

        // Get current revision as parent
        let parent_revision_id: Option<String> = conn
            .query_row(
                "SELECT current_revision_id FROM document_tabs WHERE id = ?1",
                params![tab_id],
                |row| row.get(0),
            )
            .ok()
            .flatten();

        conn.execute(
            "INSERT INTO document_revisions (id, tab_id, revision_number, parent_revision_id, content_markdown, content_hash, referenced_assets, created_at, created_by)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![&id, tab_id, revision_number, &parent_revision_id, content_markdown, content_hash, &assets_json, now, created_by],
        )?;

        Ok(id)
    }

    async fn get_document_revision(&self, id: &str) -> Result<Option<DocumentRevisionInfo>> {
        let conn = self.conn().lock().unwrap();
        let rev = conn
            .query_row(
                "SELECT id, tab_id, revision_number, parent_revision_id, content_markdown, content_hash, referenced_assets, created_at, created_by
                 FROM document_revisions WHERE id = ?1",
                params![id],
                |row| {
                    let assets_json: Option<String> = row.get(6)?;
                    let referenced_assets: Vec<String> = assets_json
                        .map(|j| serde_json::from_str(&j).unwrap_or_default())
                        .unwrap_or_default();
                    Ok(DocumentRevisionInfo {
                        id: row.get(0)?,
                        tab_id: row.get(1)?,
                        revision_number: row.get(2)?,
                        parent_revision_id: row.get(3)?,
                        content_markdown: row.get(4)?,
                        content_hash: row.get(5)?,
                        referenced_assets,
                        created_at: row.get(7)?,
                        created_by: row.get(8)?,
                    })
                },
            )
            .ok();
        Ok(rev)
    }

    async fn list_document_revisions(&self, tab_id: &str) -> Result<Vec<DocumentRevisionInfo>> {
        let conn = self.conn().lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, tab_id, revision_number, parent_revision_id, content_markdown, content_hash, referenced_assets, created_at, created_by
             FROM document_revisions WHERE tab_id = ?1 ORDER BY revision_number DESC",
        )?;

        let revs = stmt
            .query_map(params![tab_id], |row| {
                let assets_json: Option<String> = row.get(6)?;
                let referenced_assets: Vec<String> = assets_json
                    .map(|j| serde_json::from_str(&j).unwrap_or_default())
                    .unwrap_or_default();
                Ok(DocumentRevisionInfo {
                    id: row.get(0)?,
                    tab_id: row.get(1)?,
                    revision_number: row.get(2)?,
                    parent_revision_id: row.get(3)?,
                    content_markdown: row.get(4)?,
                    content_hash: row.get(5)?,
                    referenced_assets,
                    created_at: row.get(7)?,
                    created_by: row.get(8)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(revs)
    }
}
