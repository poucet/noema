//! SQLite implementation of DocumentStore

use anyhow::{Context, Result};
use async_trait::async_trait;
use rusqlite::{params, Connection, Row};

use super::SqliteStore;
use crate::storage::helper::unix_timestamp;
use crate::storage::ids::{AssetId, DocumentId, RevisionId, TabId, UserId};
use crate::storage::traits::DocumentStore;
use crate::storage::types::{Document, DocumentRevision, DocumentSource, DocumentTab, Editable, Stored};

/// Parse a document from a database row
fn parse_document(row: &Row<'_>) -> rusqlite::Result<Stored<DocumentId, Editable<Document>>> {
    let source_str: String = row.get(3)?;
    let id: DocumentId = row.get(0)?;
    let created_at: i64 = row.get(5)?;
    let updated_at: i64 = row.get(6)?;

    let doc = Document {
        user_id: row.get(1)?,
        title: row.get(2)?,
        source: source_str.parse().unwrap_or(DocumentSource::UserCreated),
        source_id: row.get(4)?,
    };

    Ok(Stored::new(id, Editable::new(doc, updated_at), created_at))
}

/// Parse referenced assets from JSON
fn parse_assets(assets_json: Option<String>) -> Vec<AssetId> {
    assets_json
        .map(|j| {
            let strings: Vec<String> = serde_json::from_str(&j).unwrap_or_default();
            strings.into_iter().map(AssetId::from_string).collect()
        })
        .unwrap_or_default()
}

/// Parse a document tab from a database row
fn parse_document_tab(row: &Row<'_>) -> rusqlite::Result<Stored<TabId, Editable<DocumentTab>>> {
    let id: TabId = row.get(0)?;
    let created_at: i64 = row.get(10)?;
    let updated_at: i64 = row.get(11)?;
    let assets_json: Option<String> = row.get(7)?;

    let tab = DocumentTab {
        document_id: row.get(1)?,
        parent_tab_id: row.get(2)?,
        tab_index: row.get(3)?,
        title: row.get(4)?,
        icon: row.get(5)?,
        content_markdown: row.get(6)?,
        referenced_assets: parse_assets(assets_json),
        source_tab_id: row.get(8)?,
        current_revision_id: row.get(9)?,
    };

    Ok(Stored::new(id, Editable::new(tab, updated_at), created_at))
}

/// Parse a document revision from a database row
fn parse_document_revision(row: &Row<'_>) -> rusqlite::Result<Stored<RevisionId, DocumentRevision>> {
    let id: RevisionId = row.get(0)?;
    let created_at: i64 = row.get(7)?;
    let assets_json: Option<String> = row.get(6)?;

    let revision = DocumentRevision {
        tab_id: row.get(1)?,
        revision_number: row.get(2)?,
        parent_revision_id: row.get(3)?,
        content_markdown: row.get(4)?,
        content_hash: row.get(5)?,
        referenced_assets: parse_assets(assets_json),
        created_by: row.get(8)?,
    };

    Ok(Stored::new(id, revision, created_at))
}

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
        user_id: &UserId,
        title: &str,
        source: DocumentSource,
        source_id: Option<&str>,
    ) -> Result<DocumentId> {
        let conn = self.conn().lock().unwrap();
        let id = DocumentId::new();
        let now = unix_timestamp();

        conn.execute(
            "INSERT INTO documents (id, user_id, title, source, source_id, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![id.as_str(), user_id.as_str(), title, source.as_str(), source_id, now, now],
        )?;

        Ok(id)
    }

    async fn get_document(&self, id: &DocumentId) -> Result<Option<Stored<DocumentId, Editable<Document>>>> {
        let conn = self.conn().lock().unwrap();
        let doc = conn
            .query_row(
                "SELECT id, user_id, title, source, source_id, created_at, updated_at
                 FROM documents WHERE id = ?1",
                params![id.as_str()],
                parse_document,
            )
            .ok();
        Ok(doc)
    }

    async fn get_document_by_source(
        &self,
        user_id: &UserId,
        source: DocumentSource,
        source_id: &str,
    ) -> Result<Option<Stored<DocumentId, Editable<Document>>>> {
        let conn = self.conn().lock().unwrap();
        let doc = conn
            .query_row(
                "SELECT id, user_id, title, source, source_id, created_at, updated_at
                 FROM documents WHERE user_id = ?1 AND source = ?2 AND source_id = ?3",
                params![user_id.as_str(), source.as_str(), source_id],
                parse_document,
            )
            .ok();
        Ok(doc)
    }

    async fn list_documents(&self, user_id: &UserId) -> Result<Vec<Stored<DocumentId, Editable<Document>>>> {
        let conn = self.conn().lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, user_id, title, source, source_id, created_at, updated_at
             FROM documents WHERE user_id = ?1 ORDER BY updated_at DESC",
        )?;

        let docs = stmt
            .query_map(params![user_id.as_str()], parse_document)?
            .filter_map(|r| r.ok())
            .collect();

        Ok(docs)
    }

    async fn search_documents(
        &self,
        user_id: &UserId,
        query: &str,
        limit: usize,
    ) -> Result<Vec<Stored<DocumentId, Editable<Document>>>> {
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
            .query_map(params![user_id.as_str(), &pattern, limit as i64], parse_document)?
            .filter_map(|r| r.ok())
            .collect();

        Ok(docs)
    }

    async fn update_document_title(&self, id: &DocumentId, title: &str) -> Result<()> {
        let conn = self.conn().lock().unwrap();
        let now = unix_timestamp();
        conn.execute(
            "UPDATE documents SET title = ?1, updated_at = ?2 WHERE id = ?3",
            params![title, now, id.as_str()],
        )?;
        Ok(())
    }

    async fn delete_document(&self, id: &DocumentId) -> Result<bool> {
        let conn = self.conn().lock().unwrap();
        // Revisions and tabs will be cascade deleted
        let rows = conn.execute("DELETE FROM documents WHERE id = ?1", params![id.as_str()])?;
        Ok(rows > 0)
    }

    // ========== Document Tab Methods ==========

    async fn create_document_tab(
        &self,
        document_id: &DocumentId,
        parent_tab_id: Option<&TabId>,
        tab_index: i32,
        title: &str,
        icon: Option<&str>,
        content_markdown: Option<&str>,
        referenced_assets: &[AssetId],
        source_tab_id: Option<&TabId>,
    ) -> Result<TabId> {
        let conn = self.conn().lock().unwrap();
        let id = TabId::new();
        let now = unix_timestamp();
        // Convert AssetId slice to strings for JSON serialization
        let asset_strings: Vec<&str> = referenced_assets.iter().map(|a| a.as_str()).collect();
        let assets_json = serde_json::to_string(&asset_strings)?;

        conn.execute(
            "INSERT INTO document_tabs (id, document_id, parent_tab_id, tab_index, title, icon, content_markdown, referenced_assets, source_tab_id, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                id.as_str(),
                document_id.as_str(),
                parent_tab_id.map(|t| t.as_str()),
                tab_index,
                title,
                icon,
                content_markdown,
                &assets_json,
                source_tab_id.map(|t| t.as_str()),
                now,
                now
            ],
        )?;

        Ok(id)
    }

    async fn get_document_tab(&self, id: &TabId) -> Result<Option<Stored<TabId, Editable<DocumentTab>>>> {
        let conn = self.conn().lock().unwrap();
        let tab = conn
            .query_row(
                "SELECT id, document_id, parent_tab_id, tab_index, title, icon, content_markdown, referenced_assets, source_tab_id, current_revision_id, created_at, updated_at
                 FROM document_tabs WHERE id = ?1",
                params![id.as_str()],
                parse_document_tab,
            )
            .ok();
        Ok(tab)
    }

    async fn list_document_tabs(&self, document_id: &DocumentId) -> Result<Vec<Stored<TabId, Editable<DocumentTab>>>> {
        let conn = self.conn().lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, document_id, parent_tab_id, tab_index, title, icon, content_markdown, referenced_assets, source_tab_id, current_revision_id, created_at, updated_at
             FROM document_tabs WHERE document_id = ?1 ORDER BY tab_index",
        )?;

        let tabs = stmt
            .query_map(params![document_id.as_str()], parse_document_tab)?
            .filter_map(|r| r.ok())
            .collect();

        Ok(tabs)
    }

    async fn update_document_tab_content(
        &self,
        id: &TabId,
        content_markdown: &str,
        referenced_assets: &[AssetId],
    ) -> Result<()> {
        let conn = self.conn().lock().unwrap();
        let now = unix_timestamp();
        let asset_strings: Vec<&str> = referenced_assets.iter().map(|a| a.as_str()).collect();
        let assets_json = serde_json::to_string(&asset_strings)?;

        conn.execute(
            "UPDATE document_tabs SET content_markdown = ?1, referenced_assets = ?2, updated_at = ?3 WHERE id = ?4",
            params![content_markdown, &assets_json, now, id.as_str()],
        )?;

        Ok(())
    }

    async fn update_document_tab_parent(
        &self,
        id: &TabId,
        parent_tab_id: Option<&TabId>,
    ) -> Result<()> {
        let conn = self.conn().lock().unwrap();
        let now = unix_timestamp();
        conn.execute(
            "UPDATE document_tabs SET parent_tab_id = ?1, updated_at = ?2 WHERE id = ?3",
            params![parent_tab_id.map(|t| t.as_str()), now, id.as_str()],
        )?;
        Ok(())
    }

    async fn set_document_tab_revision(&self, tab_id: &TabId, revision_id: &RevisionId) -> Result<()> {
        let conn = self.conn().lock().unwrap();
        let now = unix_timestamp();
        conn.execute(
            "UPDATE document_tabs SET current_revision_id = ?1, updated_at = ?2 WHERE id = ?3",
            params![revision_id.as_str(), now, tab_id.as_str()],
        )?;
        Ok(())
    }

    async fn delete_document_tab(&self, id: &TabId) -> Result<bool> {
        let conn = self.conn().lock().unwrap();
        let rows = conn.execute("DELETE FROM document_tabs WHERE id = ?1", params![id.as_str()])?;
        Ok(rows > 0)
    }

    // ========== Document Revision Methods ==========

    async fn create_document_revision(
        &self,
        tab_id: &TabId,
        content_markdown: &str,
        content_hash: &str,
        referenced_assets: &[AssetId],
        created_by: &UserId,
    ) -> Result<RevisionId> {
        let conn = self.conn().lock().unwrap();
        let id = RevisionId::new();
        let now = unix_timestamp();
        let asset_strings: Vec<&str> = referenced_assets.iter().map(|a| a.as_str()).collect();
        let assets_json = serde_json::to_string(&asset_strings)?;

        // Get next revision number
        let revision_number: i32 = conn
            .query_row(
                "SELECT COALESCE(MAX(revision_number), 0) + 1 FROM document_revisions WHERE tab_id = ?1",
                params![tab_id.as_str()],
                |row| row.get(0),
            )
            .unwrap_or(1);

        // Get current revision as parent
        let parent_revision_id: Option<String> = conn
            .query_row(
                "SELECT current_revision_id FROM document_tabs WHERE id = ?1",
                params![tab_id.as_str()],
                |row| row.get(0),
            )
            .ok()
            .flatten();

        conn.execute(
            "INSERT INTO document_revisions (id, tab_id, revision_number, parent_revision_id, content_markdown, content_hash, referenced_assets, created_at, created_by)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![id.as_str(), tab_id.as_str(), revision_number, &parent_revision_id, content_markdown, content_hash, &assets_json, now, created_by.as_str()],
        )?;

        Ok(id)
    }

    async fn get_document_revision(&self, id: &RevisionId) -> Result<Option<Stored<RevisionId, DocumentRevision>>> {
        let conn = self.conn().lock().unwrap();
        let rev = conn
            .query_row(
                "SELECT id, tab_id, revision_number, parent_revision_id, content_markdown, content_hash, referenced_assets, created_at, created_by
                 FROM document_revisions WHERE id = ?1",
                params![id.as_str()],
                parse_document_revision,
            )
            .ok();
        Ok(rev)
    }

    async fn list_document_revisions(&self, tab_id: &TabId) -> Result<Vec<Stored<RevisionId, DocumentRevision>>> {
        let conn = self.conn().lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, tab_id, revision_number, parent_revision_id, content_markdown, content_hash, referenced_assets, created_at, created_by
             FROM document_revisions WHERE tab_id = ?1 ORDER BY revision_number DESC",
        )?;

        let revs = stmt
            .query_map(params![tab_id.as_str()], parse_document_revision)?
            .filter_map(|r| r.ok())
            .collect();

        Ok(revs)
    }
}
