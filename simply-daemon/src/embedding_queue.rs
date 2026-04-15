//! Background embedding queue.
//!
//! Processes document tab writes asynchronously: chunks text, calls the
//! embedding provider, stores vectors. Debounces rapid edits to the same tab.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::mpsc;
use tokio::time::{Duration, Instant};

use async_trait::async_trait;
use llm::EmbeddingProvider;
use simply_core::embedding::{Chunker, VectorChunk, VectorStore};
use simply_core::storage::ids::{ChunkId, DocumentId, TabId, UserId};

/// A job to embed a document tab's content.
#[derive(Debug, Clone)]
pub struct EmbedJob {
    pub tab_id: TabId,
    pub document_id: DocumentId,
    pub document_type: String,
    pub user_id: UserId,
    pub text: String,
}

/// Trait for submitting embedding jobs.
///
/// The default implementation uses an in-memory channel. Future implementations
/// could back this with persistent storage (e.g. a SQLite queue table).
#[async_trait]
pub trait EmbeddingQueue: Send + Sync {
    /// Enqueue a tab for embedding.
    async fn enqueue(&self, job: EmbedJob);
}

/// In-memory embedding queue backed by a tokio channel.
#[derive(Clone)]
pub struct ChannelEmbeddingQueue {
    tx: mpsc::UnboundedSender<EmbedJob>,
}

#[async_trait]
impl EmbeddingQueue for ChannelEmbeddingQueue {
    async fn enqueue(&self, job: EmbedJob) {
        if let Err(e) = self.tx.send(job) {
            tracing::warn!(error = %e, "embedding queue closed, job dropped");
        }
    }
}

/// Debounce delay — if another edit to the same tab arrives within this window,
/// the previous job is replaced (only the latest content gets embedded).
const DEBOUNCE_MS: u64 = 500;

/// Start the background embedding queue. Returns a handle for submitting jobs.
pub fn spawn_embedding_queue(
    provider: Arc<dyn EmbeddingProvider>,
    chunker: Arc<dyn Chunker>,
    vector_store: Arc<dyn VectorStore>,
) -> ChannelEmbeddingQueue {
    let (tx, rx) = mpsc::unbounded_channel();

    tokio::spawn(embedding_worker(rx, provider, chunker, vector_store));

    ChannelEmbeddingQueue { tx }
}

async fn embedding_worker(
    mut rx: mpsc::UnboundedReceiver<EmbedJob>,
    provider: Arc<dyn EmbeddingProvider>,
    chunker: Arc<dyn Chunker>,
    vector_store: Arc<dyn VectorStore>,
) {
    // Debounce buffer: tab_id -> (job, received_at)
    let mut pending: HashMap<String, (EmbedJob, Instant)> = HashMap::new();

    loop {
        // Drain all available jobs into the debounce buffer
        let job = if pending.is_empty() {
            // Nothing pending — block until a job arrives
            match rx.recv().await {
                Some(job) => Some(job),
                None => break, // channel closed
            }
        } else {
            // Jobs pending — check for new ones but don't block long
            tokio::select! {
                job = rx.recv() => job,
                _ = tokio::time::sleep(Duration::from_millis(DEBOUNCE_MS)) => None,
            }
        };

        if let Some(job) = job {
            // Insert/replace in debounce buffer
            pending.insert(job.tab_id.as_str().to_string(), (job, Instant::now()));

            // Keep draining any immediately available jobs
            while let Ok(job) = rx.try_recv() {
                pending.insert(job.tab_id.as_str().to_string(), (job, Instant::now()));
            }
        }

        // Process jobs that have been stable long enough
        let cutoff = Instant::now() - Duration::from_millis(DEBOUNCE_MS);
        let ready: Vec<EmbedJob> = pending
            .iter()
            .filter(|(_, (_, received_at))| *received_at <= cutoff)
            .map(|(_, (job, _))| job.clone())
            .collect();

        for job in &ready {
            pending.remove(job.tab_id.as_str());
        }

        for job in ready {
            if let Err(e) = process_job(&job, &*provider, &*chunker, &*vector_store).await {
                tracing::error!(
                    tab_id = %job.tab_id,
                    document_id = %job.document_id,
                    error = %e,
                    "embedding failed"
                );
            }
        }
    }
}

async fn process_job(
    job: &EmbedJob,
    provider: &dyn EmbeddingProvider,
    chunker: &dyn Chunker,
    vector_store: &dyn VectorStore,
) -> anyhow::Result<()> {
    tracing::info!(
        tab_id = %job.tab_id,
        document_id = %job.document_id,
        text_len = job.text.len(),
        "embedding tab"
    );

    // 1. Delete existing chunks for this tab
    vector_store.delete_by_tab(&job.tab_id).await?;

    // 2. Chunk the text
    let chunks = chunker.chunk(&job.text).await?;
    if chunks.is_empty() {
        return Ok(());
    }

    // 3. Embed all chunks in one batch
    let texts: Vec<&str> = chunks.iter().map(|c| c.text.as_str()).collect();
    let embeddings = provider.embed(&texts).await?;

    // 4. Build VectorChunks and store
    let vector_chunks: Vec<VectorChunk> = chunks
        .iter()
        .zip(embeddings.into_iter())
        .map(|(chunk, embedding)| VectorChunk {
            id: ChunkId::new(),
            document_id: job.document_id.clone(),
            tab_id: job.tab_id.clone(),
            document_type: job.document_type.clone(),
            user_id: job.user_id.clone(),
            chunk_index: chunk.index,
            text: chunk.text.clone(),
            embedding: embedding.vector,
        })
        .collect();

    vector_store.upsert(&vector_chunks).await?;

    tracing::info!(
        tab_id = %job.tab_id,
        chunks = vector_chunks.len(),
        "embedding complete"
    );

    Ok(())
}
