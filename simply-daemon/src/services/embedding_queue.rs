//! Background embedding queue.
//!
//! Processes entity content writes asynchronously: chunks text, calls the
//! embedding provider, stores vectors keyed on `content_block_id`. Debounces
//! rapid edits to the same block.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;

use tokio::sync::mpsc;
use tokio::time::{Duration, Instant};

use async_trait::async_trait;
use llm::EmbeddingProvider;
use simply_core::embedding::{Chunker, VectorChunk, VectorStore};
use simply_core::storage::ids::{ChunkId, ContentBlockId, EntityId};

/// A job to embed an entity's content.
///
/// Keyed on `content_block_id` — that's the immutable content record that
/// backs the entity. When an entity's text is replaced the content block
/// id changes, so debouncing by `content_block_id` naturally groups
/// updates to the same write, not the same entity.
#[derive(Debug, Clone)]
pub struct EmbedJob {
    pub content_block_id: ContentBlockId,
    pub entity_id: EntityId,
    /// Namespaced entity kind (e.g. `"document::note"`). The only
    /// denormalised field stored on each chunk — needed for filter
    /// predicates at query time. Title / owner / access are all
    /// re-resolved from the live entity when hits come back.
    pub entity_kind: String,
    /// Optional frontmatter prepended to `text` before chunking so that
    /// per-chunk embeddings carry contextual signal (title, kind,
    /// ancestry). Not persisted.
    pub frontmatter: Option<String>,
    pub text: String,
}

pub use simply_daemon_api::EmbeddingQueueStatus;

/// Trait for submitting embedding jobs.
#[async_trait]
pub trait EmbeddingQueue: Send + Sync {
    /// Enqueue an entity's content for embedding (debounced).
    async fn enqueue(&self, job: EmbedJob);

    /// Flush a specific content block — process its pending embedding
    /// immediately, bypassing the debounce. Called on page unload / tab
    /// switch so edits don't linger unindexed.
    async fn flush(&self, content_block_id: &str);

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

    async fn flush(&self, content_block_id: &str) {
        let _ = self.flush_tx.send(content_block_id.to_string());
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

/// Debounce delay — if another edit to the same content block arrives
/// within this window, the previous job is replaced (only the latest
/// content gets embedded). Set high enough that active typing doesn't
/// trigger re-embedding.
const DEBOUNCE_MS: u64 = 15_000; // 15 seconds of idle before embedding

async fn embedding_worker(
    mut rx: mpsc::UnboundedReceiver<EmbedJob>,
    mut flush_rx: mpsc::UnboundedReceiver<String>,
    provider: Arc<dyn EmbeddingProvider>,
    chunker: Arc<dyn Chunker>,
    vector_store: Arc<dyn VectorStore>,
    stats: Arc<QueueStats>,
) {
    // Debounce buffer: content_block_id -> (job, received_at)
    let mut pending: HashMap<String, (EmbedJob, Instant)> = HashMap::new();
    // Content hash cache: content_block_id -> hash of last embedded chunks
    // (skip re-embed if unchanged, even if the block id churned).
    let content_hashes: tokio::sync::Mutex<HashMap<String, u64>> = tokio::sync::Mutex::new(HashMap::new());

    loop {
        let job = if pending.is_empty() {
            tokio::select! {
                job = rx.recv() => job,
                flush_id = flush_rx.recv() => {
                    if let Some(id) = flush_id {
                        tracing::debug!(content_block_id = %id, "flush signal with nothing pending");
                    }
                    continue;
                }
            }
        } else {
            tokio::select! {
                job = rx.recv() => job,
                flush_id = flush_rx.recv() => {
                    if let Some(id) = flush_id {
                        if let Some((job, _)) = pending.remove(&id) {
                            tracing::info!(content_block_id = %id, "flush: processing immediately");
                            stats.pending.fetch_sub(1, Ordering::Relaxed);
                            stats.processing.fetch_add(1, Ordering::Relaxed);
                            if let Err(e) = process_job(&job, &*provider, &*chunker, &*vector_store, &content_hashes).await {
                                stats.failed.fetch_add(1, Ordering::Relaxed);
                                tracing::error!(content_block_id = %id, error = %e, "flush embedding failed");
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
            pending.insert(job.content_block_id.as_str().to_string(), (job, Instant::now()));

            while let Ok(job) = rx.try_recv() {
                pending.insert(job.content_block_id.as_str().to_string(), (job, Instant::now()));
            }
        }

        while let Ok(id) = flush_rx.try_recv() {
            if let Some((job, _)) = pending.remove(&id) {
                stats.pending.fetch_sub(1, Ordering::Relaxed);
                stats.processing.fetch_add(1, Ordering::Relaxed);
                if let Err(_e) = process_job(&job, &*provider, &*chunker, &*vector_store, &content_hashes).await {
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
            pending.remove(job.content_block_id.as_str());
        }

        for job in ready {
            stats.pending.fetch_sub(1, Ordering::Relaxed);
            stats.processing.fetch_add(1, Ordering::Relaxed);

            if let Err(e) = process_job(&job, &*provider, &*chunker, &*vector_store, &content_hashes).await {
                stats.failed.fetch_add(1, Ordering::Relaxed);
                tracing::error!(
                    content_block_id = %job.content_block_id,
                    entity_id = %job.entity_id,
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

/// Compute a fast hash of chunk texts to detect if content meaningfully
/// changed.
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
    // Prepend frontmatter if any — gives each chunk contextual signal
    // (title, kind, ancestry) without storing that signal redundantly.
    let text_for_chunking: String = match job.frontmatter.as_deref() {
        Some(fm) if !fm.is_empty() => format!("{fm}\n\n{}", job.text),
        _ => job.text.clone(),
    };

    let chunks = chunker.chunk(&text_for_chunking).await?;
    if chunks.is_empty() {
        vector_store.delete_by_content_block(&job.content_block_id).await?;
        content_hashes.lock().await.remove(job.content_block_id.as_str());
        return Ok(());
    }

    let new_hash = chunk_content_hash(&chunks);
    {
        let hashes = content_hashes.lock().await;
        if let Some(&old_hash) = hashes.get(job.content_block_id.as_str()) {
            if old_hash == new_hash {
                tracing::debug!(
                    content_block_id = %job.content_block_id,
                    "skipping embedding — content unchanged"
                );
                return Ok(());
            }
        }
    }

    tracing::info!(
        content_block_id = %job.content_block_id,
        entity_id = %job.entity_id,
        entity_kind = %job.entity_kind,
        text_len = job.text.len(),
        chunks = chunks.len(),
        "embedding content"
    );

    // Replace existing chunks for this content block.
    vector_store.delete_by_content_block(&job.content_block_id).await?;

    let texts: Vec<&str> = chunks.iter().map(|c| c.text.as_str()).collect();
    let embeddings = provider.embed(&texts).await?;

    let vector_chunks: Vec<VectorChunk> = chunks
        .iter()
        .zip(embeddings.into_iter())
        .map(|(_, embedding)| VectorChunk {
            id: ChunkId::new(),
            content_block_id: job.content_block_id.clone(),
            entity_id: job.entity_id.clone(),
            entity_kind: job.entity_kind.clone(),
            embedding: embedding.vector,
        })
        .collect();

    let chunks_written = vector_chunks.len();
    vector_store.upsert(&vector_chunks).await?;

    content_hashes.lock().await.insert(
        job.content_block_id.as_str().to_string(),
        new_hash,
    );

    tracing::info!(
        content_block_id = %job.content_block_id,
        chunks = chunks_written,
        "embedding complete"
    );

    Ok(())
}
