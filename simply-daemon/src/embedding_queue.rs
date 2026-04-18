//! Background embedding queue.
//!
//! Processes document tab writes asynchronously: chunks text, calls the
//! embedding provider, stores vectors. Debounces rapid edits to the same tab.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
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

/// Status of the embedding queue.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[ts(export, export_to = "admin/src/lib/generated/types/")]
pub struct EmbeddingQueueStatus {
    /// Number of jobs waiting to be processed.
    pub pending: usize,
    /// Number of jobs currently being processed.
    pub processing: usize,
    /// Total jobs completed since startup.
    pub completed: u64,
    /// Total jobs failed since startup.
    pub failed: u64,
}

/// Trait for submitting embedding jobs.
///
/// The default implementation uses an in-memory channel. Future implementations
/// could back this with persistent storage (e.g. a SQLite queue table).
#[async_trait]
pub trait EmbeddingQueue: Send + Sync {
    /// Enqueue a tab for embedding (debounced).
    async fn enqueue(&self, job: EmbedJob);

    /// Flush a specific tab — process its pending embedding immediately,
    /// bypassing the debounce. Called on tab switch / page unload.
    async fn flush(&self, tab_id: &str);

    /// Get the current queue status.
    async fn status(&self) -> EmbeddingQueueStatus;
}

/// Shared counters between the queue handle and the background worker.
struct QueueStats {
    pending: AtomicUsize,
    processing: AtomicUsize,
    completed: AtomicU64,
    failed: AtomicU64,
}

/// In-memory embedding queue backed by a tokio channel.
/// Spawns a background worker on creation.
#[derive(Clone)]
pub struct ChannelEmbeddingQueue {
    tx: mpsc::UnboundedSender<EmbedJob>,
    flush_tx: mpsc::UnboundedSender<String>,
    stats: Arc<QueueStats>,
}

impl ChannelEmbeddingQueue {
    /// Create a new embedding queue. Spawns the background worker immediately.
    pub fn new(
        provider: Arc<dyn EmbeddingProvider>,
        chunker: Arc<dyn Chunker>,
        vector_store: Arc<dyn VectorStore>,
    ) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let (flush_tx, flush_rx) = mpsc::unbounded_channel();
        let stats = Arc::new(QueueStats {
            pending: AtomicUsize::new(0),
            processing: AtomicUsize::new(0),
            completed: AtomicU64::new(0),
            failed: AtomicU64::new(0),
        });
        tokio::spawn(embedding_worker(rx, flush_rx, provider, chunker, vector_store, Arc::clone(&stats)));
        Self { tx, flush_tx, stats }
    }
}

#[async_trait]
impl EmbeddingQueue for ChannelEmbeddingQueue {
    async fn enqueue(&self, job: EmbedJob) {
        self.stats.pending.fetch_add(1, Ordering::Relaxed);
        if let Err(e) = self.tx.send(job) {
            self.stats.pending.fetch_sub(1, Ordering::Relaxed);
            tracing::warn!(error = %e, "embedding queue closed, job dropped");
        }
    }

    async fn flush(&self, tab_id: &str) {
        let _ = self.flush_tx.send(tab_id.to_string());
    }

    async fn status(&self) -> EmbeddingQueueStatus {
        EmbeddingQueueStatus {
            pending: self.stats.pending.load(Ordering::Relaxed),
            processing: self.stats.processing.load(Ordering::Relaxed),
            completed: self.stats.completed.load(Ordering::Relaxed),
            failed: self.stats.failed.load(Ordering::Relaxed),
        }
    }
}

/// Debounce delay — if another edit to the same tab arrives within this window,
/// the previous job is replaced (only the latest content gets embedded).
/// Set high enough that active typing doesn't trigger re-embedding.
const DEBOUNCE_MS: u64 = 15_000; // 15 seconds of idle before embedding

async fn embedding_worker(
    mut rx: mpsc::UnboundedReceiver<EmbedJob>,
    mut flush_rx: mpsc::UnboundedReceiver<String>,
    provider: Arc<dyn EmbeddingProvider>,
    chunker: Arc<dyn Chunker>,
    vector_store: Arc<dyn VectorStore>,
    stats: Arc<QueueStats>,
) {
    // Debounce buffer: tab_id -> (job, received_at)
    let mut pending: HashMap<String, (EmbedJob, Instant)> = HashMap::new();
    // Content hash cache: tab_id -> hash of last embedded chunks (skip if unchanged)
    let content_hashes: tokio::sync::Mutex<HashMap<String, u64>> = tokio::sync::Mutex::new(HashMap::new());

    loop {
        // Drain all available jobs and flush signals
        let job = if pending.is_empty() {
            // Nothing pending — block until a job arrives
            tokio::select! {
                job = rx.recv() => job,
                flush_id = flush_rx.recv() => {
                    // Flush with nothing pending — ignore
                    if let Some(id) = flush_id {
                        tracing::debug!(tab_id = %id, "flush signal with nothing pending");
                    }
                    continue;
                }
            }
        } else {
            // Jobs pending — check for new ones or flush signals
            tokio::select! {
                job = rx.recv() => job,
                flush_id = flush_rx.recv() => {
                    // Flush: immediately process the named tab if pending
                    if let Some(id) = flush_id {
                        if let Some((job, _)) = pending.remove(&id) {
                            tracing::info!(tab_id = %id, "flush: processing immediately");
                            stats.pending.fetch_sub(1, Ordering::Relaxed);
                            stats.processing.fetch_add(1, Ordering::Relaxed);
                            if let Err(e) = process_job(&job, &*provider, &*chunker, &*vector_store, &content_hashes).await {
                                stats.failed.fetch_add(1, Ordering::Relaxed);
                                tracing::error!(tab_id = %id, error = %e, "flush embedding failed");
                            } else {
                                stats.completed.fetch_add(1, Ordering::Relaxed);
                            }
                            stats.processing.fetch_sub(1, Ordering::Relaxed);
                        }
                    }
                    continue;
                }
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

        // Also drain any flush signals
        while let Ok(id) = flush_rx.try_recv() {
            if let Some((job, _)) = pending.remove(&id) {
                stats.pending.fetch_sub(1, Ordering::Relaxed);
                stats.processing.fetch_add(1, Ordering::Relaxed);
                if let Err(e) = process_job(&job, &*provider, &*chunker, &*vector_store, &content_hashes).await {
                    stats.failed.fetch_add(1, Ordering::Relaxed);
                } else {
                    stats.completed.fetch_add(1, Ordering::Relaxed);
                }
                stats.processing.fetch_sub(1, Ordering::Relaxed);
            }
        }

        // Process jobs that have been stable long enough (debounce expired)
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
            stats.pending.fetch_sub(1, Ordering::Relaxed);
            stats.processing.fetch_add(1, Ordering::Relaxed);

            if let Err(e) = process_job(&job, &*provider, &*chunker, &*vector_store, &content_hashes).await {
                stats.failed.fetch_add(1, Ordering::Relaxed);
                tracing::error!(
                    tab_id = %job.tab_id,
                    document_id = %job.document_id,
                    error = %e,
                    "embedding failed"
                );
            } else {
                stats.completed.fetch_add(1, Ordering::Relaxed);
            }

            stats.processing.fetch_sub(1, Ordering::Relaxed);
        }
    }
}

/// Compute a fast hash of chunk texts to detect if content meaningfully changed.
fn chunk_content_hash(chunks: &[simply_core::embedding::Chunk]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for chunk in chunks {
        chunk.text.hash(&mut hasher);
    }
    hasher.finish()
}

async fn process_job(
    job: &EmbedJob,
    provider: &dyn EmbeddingProvider,
    chunker: &dyn Chunker,
    vector_store: &dyn VectorStore,
    content_hashes: &tokio::sync::Mutex<HashMap<String, u64>>,
) -> anyhow::Result<()> {
    // 1. Chunk the text
    let chunks = chunker.chunk(&job.text).await?;
    if chunks.is_empty() {
        // Content is empty — delete existing chunks
        vector_store.delete_by_tab(&job.tab_id).await?;
        content_hashes.lock().await.remove(job.tab_id.as_str());
        return Ok(());
    }

    // 2. Check if chunks changed since last embedding
    let new_hash = chunk_content_hash(&chunks);
    {
        let hashes = content_hashes.lock().await;
        if let Some(&old_hash) = hashes.get(job.tab_id.as_str()) {
            if old_hash == new_hash {
                tracing::debug!(
                    tab_id = %job.tab_id,
                    "skipping embedding — content unchanged"
                );
                return Ok(());
            }
        }
    }

    tracing::info!(
        tab_id = %job.tab_id,
        document_id = %job.document_id,
        text_len = job.text.len(),
        chunks = chunks.len(),
        "embedding tab"
    );

    // 3. Delete existing chunks for this tab
    vector_store.delete_by_tab(&job.tab_id).await?;

    // 4. Embed all chunks in one batch
    let texts: Vec<&str> = chunks.iter().map(|c| c.text.as_str()).collect();
    let embeddings = provider.embed(&texts).await?;

    // 5. Build VectorChunks and store
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

    // 6. Remember the hash for next time
    content_hashes.lock().await.insert(
        job.tab_id.as_str().to_string(),
        new_hash,
    );

    tracing::info!(
        tab_id = %job.tab_id,
        chunks = vector_chunks.len(),
        "embedding complete"
    );

    Ok(())
}
