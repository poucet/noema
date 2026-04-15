# Embedding & RAG

**Status:** planned
**Version:** 1.0
**Parent:** [ARCHITECTURE.md](ARCHITECTURE.md)
**Related:** [STORAGE.md](STORAGE.md), [UNIFIED_CONTENT_MODEL.md](UNIFIED_CONTENT_MODEL.md)

---

## Problem

The UCM stores all platform knowledge — documents, notes, todos, imported content — but none of it is accessible to agents at chat time. Lumina sessions see only Discord channel history. Noema sessions see only the current conversation. There's no way for an agent to recall "what do I know about X?" across the knowledge base.

RAG is the bridge: embed stored content, retrieve what's relevant, inject it into agent context before generation.

## Goals

- Embed all document content automatically on write
- Retrieval API on the daemon with type-level and frontmatter filtering
- Lumina (and any client) can query for relevant context before sending a message
- Latency-sensitive: embedding on save must not block the user; retrieval at chat time must be fast
- Provider-agnostic: swap between OpenAI, Ollama, local ONNX models
- Storage-agnostic: sqlite-vec by default, but the trait allows alternative backends

## Non-Goals

- Embedding chat messages (content blocks from conversation turns) — too much overhead, low retrieval value
- Real-time streaming embeddings — batch is fine
- Cross-user retrieval — respect document ownership/privacy boundaries
- Re-ranking or hybrid BM25+vector search (future enhancement)

---

## Architecture

```
  Document write (tab create/update)
       │
       ▼
  ┌─────────────────────┐
  │   Embedding Queue    │  (background, async)
  │   chunks tab text    │
  │   calls provider     │
  └──────────┬──────────┘
             │
             ▼
  ┌─────────────────────┐      ┌─────────────────────┐
  │   VectorStore        │      │  EmbeddingProvider   │
  │   (sqlite-vec)       │◄─────│  (OpenAI / Ollama /  │
  │   stores chunks +    │      │   local ONNX)        │
  │   vectors + metadata │      └─────────────────────┘
  └──────────┬──────────┘
             │
             ▼
  ┌─────────────────────┐
  │   SearchApi          │  (daemon REST endpoint)
  │   query + type +     │
  │   frontmatter filter │
  └──────────┬──────────┘
             │
             ▼
  ┌─────────────────────┐
  │  Lumina / Noema      │
  │  inject retrieved    │
  │  chunks as context   │
  └─────────────────────┘
```

---

## Components

### 1. EmbeddingProvider (simply-core)

Trait for generating embedding vectors from text. Follows the voice provider pattern — independent trait, not an enum wrapper.

```rust
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// Embed a batch of texts, returning one vector per input.
    async fn embed(&self, texts: &[&str]) -> Result<Vec<Embedding>>;

    /// The dimensionality of vectors this provider produces.
    fn dimensions(&self) -> usize;

    /// Provider/model identifier (e.g. "openai/text-embedding-3-small").
    fn model_id(&self) -> &str;
}

pub struct Embedding {
    pub vector: Vec<f32>,
}
```

**Implementations:**

| Provider | Model | Dimensions | Notes |
|----------|-------|------------|-------|
| OpenAI | `text-embedding-3-small` | 1536 | API call, ~200ms per batch |
| Claude | `voyage-3-lite` (via Anthropic/Voyage) | 1024 | Anthropic's recommended embedding model |
| Gemini | `text-embedding-004` | 768 | Google AI embedding model |
| Mistral | `mistral-embed` | 1024 | Mistral's embedding endpoint |
| Ollama | `nomic-embed-text` / `all-minilm` | varies | Local, runs on user's Ollama |
| Local ONNX | `bge-small-en-v1.5` | 384 | In-process via ort (ONNX Runtime), no network, ~10-30ms/chunk |

The embedding provider is a **server-side configuration**, set once in `settings.toml`. Switching providers requires re-embedding all stored vectors (different providers produce incompatible dimensions/vector spaces). The daemon stores the active `model_id` alongside vectors and refuses to search if the configured provider doesn't match stored vectors, prompting a re-embed.

```toml
[embedding]
provider = "local"           # or "mistral", "openai", "claude", "gemini", "ollama"
model = "bge-small-en-v1.5"
chunk_size = 4096            # tokens (~16k chars)
chunk_overlap = 128          # tokens
# api_key pulled from api_keys.{provider}
```

### 2. VectorStore (simply-core)

Storage trait for vector chunks. Each chunk is a piece of a document tab, stored with its embedding vector and metadata for filtered retrieval.

```rust
#[async_trait]
pub trait VectorStore: Send + Sync {
    /// Store a batch of chunks with their embeddings.
    async fn upsert(&self, chunks: &[VectorChunk]) -> Result<()>;

    /// Search for similar chunks, with optional filters.
    async fn search(&self, query: SearchQuery) -> Result<Vec<SearchResult>>;

    /// Delete all chunks for a given document tab.
    async fn delete_by_tab(&self, tab_id: &TabId) -> Result<()>;

    /// Delete all chunks for a given document.
    async fn delete_by_document(&self, document_id: &DocumentId) -> Result<()>;
}

pub struct VectorChunk {
    pub id: ChunkId,
    pub document_id: DocumentId,
    pub tab_id: TabId,
    pub document_type: String,         // first-class column, not just frontmatter
    pub user_id: UserId,
    pub chunk_index: u32,              // position within the tab
    pub text: String,                  // the chunk text
    pub embedding: Vec<f32>,           // the vector
}

pub struct SearchQuery {
    pub vector: Vec<f32>,              // query embedding
    pub top_k: usize,                  // max results
    pub filter: Option<SearchFilter>,
}

pub struct SearchFilter {
    pub document_type: Option<String>,         // fast: indexed column
    pub user_id: Option<UserId>,               // ownership scoping
}

pub struct SearchResult {
    pub chunk: VectorChunk,
    pub score: f32,                    // similarity score (0.0 - 1.0)
}
```

**Default implementation:** sqlite-vec extension. Stores vectors in a virtual table alongside chunk metadata. The `document_type` and `user_id` columns are indexed for fast pre-filtering before the vector search.

### 3. Document Type as First-Class Field

`type` moves from frontmatter-only to a **column on the documents table**, set at creation time. This enables:

- Fast filtered retrieval (indexed column vs. YAML parsing)
- Vector store partitioning by type
- API-level type filtering without touching frontmatter

The `type` field is still written to frontmatter for LLM readability, but the DB column is authoritative for queries.

#### Type Ontology

The DB column is `TEXT` (open for extensibility), but Rust defines well-known types as constants:

```rust
/// Well-known document types.
/// The DB column accepts any string, so new types can be added
/// without schema changes. These constants give compile-time safety
/// for the common cases.
pub mod DocumentType {
    pub const DOCUMENT: &str = "document";       // generic / untyped
    pub const NOTE: &str = "note";               // freeform notes
    pub const TODO: &str = "todo";               // tasks with done/due/assignee
    pub const KNOWLEDGE: &str = "knowledge";     // imported/ingested reference material
    pub const CONTEXT: &str = "context";         // project context, goals, energy
    pub const INTENT: &str = "intent";           // event-triggered automation
    pub const SYSTEM_PROMPT: &str = "system_prompt"; // agent system prompts
    pub const MCP_SERVER: &str = "mcp_server";   // MCP server configuration
    pub const ACCESS_RULE: &str = "access_rule"; // permission rules
}
```

`CreateDocumentRequest` gains a `document_type: Option<String>` field (defaults to `"document"`). The type is set once at creation — changing it would require re-classifying the document (a deliberate action, not an accidental edit).

#### API Surface

`document_type` is a first-class field in all document API responses and supports filtered listing:

- `DocumentInfo` and `DocumentDetail` gain a `document_type: String` field
- `DocumentApi::list_documents()` gains an optional `document_type` filter parameter
- The admin UI can browse content by type (notes, todos, knowledge, etc.) and edit any document regardless of type — the type is visible and filterable, not hidden metadata

#### Schema

```sql
-- Migration: add type column to documents table
ALTER TABLE documents ADD COLUMN document_type TEXT NOT NULL DEFAULT 'document';
CREATE INDEX idx_documents_type ON documents(document_type);

-- Vector chunks table
CREATE TABLE vector_chunks (
    id TEXT PRIMARY KEY,
    document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    tab_id TEXT NOT NULL,
    document_type TEXT NOT NULL,
    user_id TEXT NOT NULL,
    chunk_index INTEGER NOT NULL,
    text TEXT NOT NULL,
    model_id TEXT NOT NULL,              -- embedding model that produced the vector
    embedded_at TEXT NOT NULL,            -- ISO 8601
    UNIQUE(tab_id, chunk_index)
);
CREATE INDEX idx_chunks_type ON vector_chunks(document_type);
CREATE INDEX idx_chunks_user ON vector_chunks(user_id);
CREATE INDEX idx_chunks_doc ON vector_chunks(document_id);

-- sqlite-vec virtual table for the actual vectors
-- Joined with vector_chunks by rowid for metadata filtering
CREATE VIRTUAL TABLE vector_chunks_vec USING vec0(
    id TEXT PRIMARY KEY,
    embedding float[{dimensions}]
);
```

**Why vectors are separate from content blocks:** Vector chunks and content blocks operate at different granularity. A single document tab (which stores `content_markdown` directly, not via content blocks) may produce many vector chunks after chunking. Content blocks are the atomic unit for *conversation* text with origin tracking; vector chunks are the atomic unit for *retrieval*. They reference back to `tab_id` / `document_id`, not to `content_block_id`.

### 4. Chunking (simply-core)

Chunking is an async trait to allow swapping strategies in the future (e.g. semantic chunking, LLM-assisted boundary detection).

```rust
#[async_trait]
pub trait Chunker: Send + Sync {
    /// Split text into chunks for embedding.
    async fn chunk(&self, text: &str) -> Result<Vec<Chunk>>;
}

pub struct Chunk {
    pub text: String,
    pub index: u32,
}
```

**Default implementation: `RecursiveCharacterChunker`**

- Splits on `\n\n`, then `\n`, then ` `, then character boundary
- Chunk size and overlap configured via `settings.toml`
- Short content (under chunk size): returned as a single chunk, no splitting

```toml
[embedding]
provider = "mistral"
model = "mistral-embed"
chunk_size = 4096          # tokens (~16k chars)
chunk_overlap = 128        # tokens
```

Chunking happens in the embedding queue, not in the storage layer. The `VectorStore` receives pre-chunked, pre-embedded data.

### 5. Embedding Queue (simply-daemon)

Background service that processes document tab writes asynchronously.

**Flow:**
1. `DocumentStore::create_document_tab()` or `update_document_tab_content()` fires
2. An embed job is enqueued: `(tab_id, document_id, document_type, user_id, text)`
3. Background worker picks up the job:
   - Deletes existing chunks for this tab (re-embed on update)
   - Chunks the text
   - Calls `EmbeddingProvider::embed()` with all chunk texts (single batch)
   - Calls `VectorStore::upsert()` with chunks + vectors
4. `embedded_at` timestamp tracks freshness

**Properties:**
- Non-blocking — document saves return immediately
- Debounced — rapid successive edits to the same tab coalesce (only embed the latest)
- Retry on transient failures (provider API errors)
- Logs warnings if embedding falls behind

**Persistence:** The queue itself is in-memory (tokio channel) — not persisted. Instead, on startup the daemon derives pending work by comparing document state against vector state:

```sql
SELECT d.id, t.id, t.content_markdown
FROM documents d
JOIN document_tabs t ON t.document_id = d.id
LEFT JOIN vector_chunks vc ON vc.tab_id = t.id
WHERE vc.id IS NULL
   OR vc.embedded_at < t.updated_at
   OR vc.model_id != ?current_model_id
```

This catches: tabs never embedded, tabs updated since last embed, and model mismatches after provider switch. The `reindex` endpoint is just the forced version of this same scan. No queue state to corrupt or lose on crash.

### 6. SearchApi (simply-daemon)

New daemon API trait for retrieval.

```rust
#[rpc_service]
pub trait SearchApi {
    /// Semantic search over embedded documents.
    /// Flat parameters for MCP tool compatibility — LLMs can call this directly.
    #[rest(POST, "/search")]
    async fn search(&self, query: &str, document_type: Option<&str>, top_k: Option<usize>) -> Result<Vec<SearchHit>>;

    /// Re-embed all documents. Deletes existing vectors and re-indexes everything.
    /// Runs in the background; returns immediately.
    #[rest(POST, "/search/reindex")]
    async fn reindex(&self) -> Result<ReindexStatus>;
}

pub struct SearchHit {
    pub document_id: String,
    pub document_title: String,
    pub tab_id: String,
    pub chunk_text: String,
    pub chunk_index: u32,
    pub score: f32,
    pub document_type: String,
}
```

The `SearchApi` implementation:
1. Embeds the query text via `EmbeddingProvider`
2. Calls `VectorStore::search()` with the query vector + filters
3. Returns hits with document metadata for context injection

The flat parameter style means this also works as an MCP tool out of the box:
```json
{"name": "search", "parameters": {"query": "voice architecture", "document_type": "knowledge"}}
```

### 7. Lumina Integration (Auto-RAG)

When a Discord message arrives, before creating the LLM session:

1. Take the **last N messages** from channel history as the query text (concatenated, no extra LLM call). N is a configurable constant (default: 5 user messages).
2. Call `SearchApi::search()` with that text, scoped to the user (or public docs)
3. If results found, prepend them to the system prompt as a "Relevant knowledge" section:

```
## Relevant knowledge
The following documents may be relevant to this conversation:

### [Document Title] (type: knowledge)
> chunk text here...

### [Another Doc] (type: note)
> chunk text here...
```

4. The agent sees this context and can reference it naturally

**Latency budget:** embedding the query (~50-200ms) + vector search (~10-50ms) = 60-250ms added to chat response time. Acceptable given LLM generation takes 1-5s.

### 8. Noema Integration (Search + Document Refs)

Noema exposes RAG through two mechanisms:

1. **Search panel** — user can search for documents by query, type filter, etc. Results appear as a list. Selecting a document inserts it into the chat as a **document reference** (rendered as a compact icon/card in the UI, not inline text). The agent sees the full document content when it expands the reference.

2. **Auto-inject** — same as Lumina: relevant documents injected into system prompt based on recent conversation context. Optional, can be toggled per conversation.

Document references use the existing `StoredContent` reference system — an `AssetRef` or `DocumentRef` that renders as a compact UI element but resolves to full text for the agent.

---

## Decisions

1. **Embedding provider is server-side config** — set once in `settings.toml`, not per-user. Switching providers requires re-embedding all vectors. The daemon tracks `model_id` on each chunk and refuses to search against mismatched vectors.

2. **Query message count** — configurable constant, default 5 user messages. Easily tunable without code changes.

3. **Cross-document deduplication** — handled at import time. If the same source is imported again, the existing document is updated (matched by `source_id`), not duplicated. The embedding queue re-embeds on update, replacing old chunks.

## Resolved

1. **Re-embed** — exposed as a `SearchApi` endpoint (`POST /search/reindex`) triggerable from the admin UI. Deletes all existing chunks and re-embeds every document tab. Also triggered automatically if the daemon detects a `model_id` mismatch on startup (stored model != configured model).

2. **Chunk size** — default 4096 tokens (~16k chars) with 128 token overlap. Most retrieved chunks will be injected into models with large context windows, so preserving document coherence matters more than keeping chunks tiny. Short documents (under chunk size) are embedded whole. Configurable constant.
