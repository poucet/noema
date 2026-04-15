//! SQLite-vec implementation of VectorStore

use anyhow::{Context, Result};
use async_trait::async_trait;
use rusqlite::{params, Connection};

use super::SqliteStore;
use crate::embedding::{SearchFilter, SearchQuery, SearchResult, VectorChunk, VectorStore};
use crate::storage::ids::{ChunkId, DocumentId, TabId, UserId};

/// Register sqlite-vec as an auto-extension. Call once before opening any connections.
pub fn register_sqlite_vec() {
    unsafe {
        rusqlite::ffi::sqlite3_auto_extension(Some(std::mem::transmute(
            sqlite_vec::sqlite3_vec_init as *const (),
        )));
    }
}

pub(crate) fn init_schema(conn: &Connection) -> Result<()> {

    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS vector_chunks (
            id TEXT PRIMARY KEY,
            document_id TEXT NOT NULL,
            tab_id TEXT NOT NULL,
            document_type TEXT NOT NULL,
            user_id TEXT NOT NULL,
            chunk_index INTEGER NOT NULL,
            text TEXT NOT NULL,
            model_id TEXT NOT NULL,
            embedded_at INTEGER NOT NULL,
            UNIQUE(tab_id, chunk_index)
        );

        CREATE INDEX IF NOT EXISTS idx_chunks_type ON vector_chunks(document_type);
        CREATE INDEX IF NOT EXISTS idx_chunks_user ON vector_chunks(user_id);
        CREATE INDEX IF NOT EXISTS idx_chunks_doc ON vector_chunks(document_id);
        CREATE INDEX IF NOT EXISTS idx_chunks_tab ON vector_chunks(tab_id);
        "#,
    )
    .context("Failed to initialize vector_chunks schema")?;

    Ok(())
}

/// Initialize the vec0 virtual table for a given dimension.
/// Called lazily when the first embedding is stored, since dimensions
/// depend on the configured provider.
pub(crate) fn ensure_vec_table(conn: &Connection, dimensions: usize) -> Result<()> {
    let sql = format!(
        "CREATE VIRTUAL TABLE IF NOT EXISTS vector_chunks_vec USING vec0(id TEXT PRIMARY KEY, embedding float[{dimensions}])"
    );
    conn.execute_batch(&sql)
        .context("Failed to create vector_chunks_vec table")?;
    Ok(())
}

fn parse_chunk(row: &rusqlite::Row<'_>) -> rusqlite::Result<VectorChunk> {
    Ok(VectorChunk {
        id: ChunkId::from_string(row.get::<_, String>(0)?),
        document_id: DocumentId::from_string(row.get::<_, String>(1)?),
        tab_id: TabId::from_string(row.get::<_, String>(2)?),
        document_type: row.get(3)?,
        user_id: UserId::from_string(row.get::<_, String>(4)?),
        chunk_index: row.get(5)?,
        text: row.get(6)?,
        embedding: vec![], // not loaded from metadata table
    })
}

#[async_trait]
impl VectorStore for SqliteStore {
    async fn upsert(&self, chunks: &[VectorChunk]) -> Result<()> {
        if chunks.is_empty() {
            return Ok(());
        }

        let conn = self.conn().lock().unwrap();

        // Ensure the vec0 table exists with the right dimensions
        let dimensions = chunks[0].embedding.len();
        ensure_vec_table(&conn, dimensions)?;

        let now = crate::storage::helper::unix_timestamp();

        for chunk in chunks {
            // Insert metadata
            conn.execute(
                "INSERT OR REPLACE INTO vector_chunks (id, document_id, tab_id, document_type, user_id, chunk_index, text, model_id, embedded_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    chunk.id.as_str(),
                    chunk.document_id.as_str(),
                    chunk.tab_id.as_str(),
                    &chunk.document_type,
                    chunk.user_id.as_str(),
                    chunk.chunk_index,
                    &chunk.text,
                    "", // model_id set by caller
                    now,
                ],
            )?;

            // Insert vector
            let embedding_bytes: Vec<u8> = chunk
                .embedding
                .iter()
                .flat_map(|f| f.to_le_bytes())
                .collect();

            conn.execute(
                "INSERT OR REPLACE INTO vector_chunks_vec (id, embedding) VALUES (?1, ?2)",
                params![chunk.id.as_str(), embedding_bytes],
            )?;
        }

        Ok(())
    }

    async fn search(&self, query: SearchQuery) -> Result<Vec<SearchResult>> {
        let conn = self.conn().lock().unwrap();

        let query_bytes: Vec<u8> = query
            .vector
            .iter()
            .flat_map(|f| f.to_le_bytes())
            .collect();

        // KNN search via vec0
        let mut stmt = conn.prepare(
            "SELECT v.id, v.distance
             FROM vector_chunks_vec v
             WHERE v.embedding MATCH ?1
             ORDER BY v.distance
             LIMIT ?2",
        )?;

        let candidates: Vec<(String, f64)> = stmt
            .query_map(params![query_bytes, query.top_k as i64], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
            })?
            .filter_map(|r| r.ok())
            .collect();

        // Now fetch metadata and apply filters
        let mut results = Vec::new();
        for (chunk_id, distance) in candidates {
            let chunk = conn.query_row(
                "SELECT id, document_id, tab_id, document_type, user_id, chunk_index, text
                 FROM vector_chunks WHERE id = ?1",
                params![&chunk_id],
                parse_chunk,
            );

            let chunk = match chunk {
                Ok(c) => c,
                Err(_) => continue,
            };

            // Apply filters
            if let Some(ref filter) = query.filter {
                if let Some(ref doc_type) = filter.document_type {
                    if &chunk.document_type != doc_type {
                        continue;
                    }
                }
                if let Some(ref user_id) = filter.user_id {
                    if &chunk.user_id != user_id {
                        continue;
                    }
                }
            }

            // Convert L2 distance to similarity score (0-1)
            let score = 1.0 / (1.0 + distance as f32);
            results.push(SearchResult { chunk, score });
        }

        Ok(results)
    }

    async fn delete_by_tab(&self, tab_id: &TabId) -> Result<()> {
        let conn = self.conn().lock().unwrap();

        // Get chunk IDs first for vec table cleanup
        let ids: Vec<String> = {
            let mut stmt = conn.prepare(
                "SELECT id FROM vector_chunks WHERE tab_id = ?1",
            )?;
            stmt.query_map(params![tab_id.as_str()], |row| row.get(0))?
                .filter_map(|r| r.ok())
                .collect()
        };

        for id in &ids {
            let _ = conn.execute(
                "DELETE FROM vector_chunks_vec WHERE id = ?1",
                params![id],
            );
        }

        conn.execute(
            "DELETE FROM vector_chunks WHERE tab_id = ?1",
            params![tab_id.as_str()],
        )?;
        Ok(())
    }

    async fn delete_by_document(&self, document_id: &DocumentId) -> Result<()> {
        let conn = self.conn().lock().unwrap();

        let ids: Vec<String> = {
            let mut stmt = conn.prepare(
                "SELECT id FROM vector_chunks WHERE document_id = ?1",
            )?;
            stmt.query_map(params![document_id.as_str()], |row| row.get(0))?
                .filter_map(|r| r.ok())
                .collect()
        };

        for id in &ids {
            let _ = conn.execute(
                "DELETE FROM vector_chunks_vec WHERE id = ?1",
                params![id],
            );
        }

        conn.execute(
            "DELETE FROM vector_chunks WHERE document_id = ?1",
            params![document_id.as_str()],
        )?;
        Ok(())
    }

    async fn delete_all(&self) -> Result<()> {
        let conn = self.conn().lock().unwrap();
        conn.execute("DELETE FROM vector_chunks_vec", [])?;
        conn.execute("DELETE FROM vector_chunks", [])?;
        Ok(())
    }
}
