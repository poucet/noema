# Phase 3: Unified Content Model - Implementation Plan

## Summary

Implement the full **Unified Content Model** from `design/UNIFIED_CONTENT_MODEL.md`:

1. **Content Layer**: ContentBlock (text, content-addressed) + Assets (binary)
2. **Conversation Structure**: Turn/Span/Message + Views
3. **Document Structure**: Version chain with revisions
4. **Collection Structure**: Tree + ordering + tags
5. **Cross-references**: Links between any entities

**Key decision**: Fresh database schema (no migration from old Thread/SpanSet/Span model).

---

## Type-Safe IDs

All stores use opaque ID types to prevent mixing IDs from different domains:

```rust
// noema-core/src/storage/ids.rs

macro_rules! define_id {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn new() -> Self {
                Self(uuid::Uuid::new_v4().to_string())
            }

            pub fn from_string(s: impl Into<String>) -> Self {
                Self(s.into())
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}", self.0)
            }
        }
    };
}

// Content layer
define_id!(ContentBlockId);
define_id!(AssetId);

// Conversation layer
define_id!(ConversationId);
define_id!(TurnId);
define_id!(SpanId);
define_id!(MessageId);
define_id!(ViewId);

// Document layer
define_id!(DocumentId);
define_id!(TabId);
define_id!(RevisionId);

// Collection layer
define_id!(CollectionId);
define_id!(CollectionItemId);
define_id!(CollectionViewId);

// Reference layer
define_id!(ReferenceId);

// User layer
define_id!(UserId);
```

This prevents compile-time errors like:
```rust
// Won't compile - type mismatch
let doc_id: DocumentId = ...;
store.get_message(doc_id);  // Error: expected MessageId, found DocumentId
```

All trait methods use these typed IDs instead of raw `&str`.

---

## Content Scoping (Tag-Based Overlays)

Tags create isolated "views" for organizational boundaries - projects, contexts, agent workspaces:

```rust
pub struct ContentScope {
    pub include_tags: Vec<String>,    // Must have at least one of these
    pub exclude_tags: Vec<String>,    // Must not have any of these
    pub user_ids: Option<Vec<UserId>>, // Restrict to specific users
    pub time_range: Option<(i64, i64)>, // Temporal bounds
}

pub struct ScopedContentStore<S: ContentBlockStore> {
    inner: S,
    scope: ContentScope,
}
```

**Schema for tags**:
```sql
CREATE TABLE content_block_tags (
    content_id TEXT NOT NULL REFERENCES content_blocks(id) ON DELETE CASCADE,
    tag TEXT NOT NULL,
    PRIMARY KEY (content_id, tag)
);
CREATE INDEX idx_content_block_tags_tag ON content_block_tags(tag);
```

**Use cases**:
1. **Agent isolation**: Side-agent only sees content tagged `agent:research`
2. **Project scoping**: Work context shows `project:noema`, personal shows `project:personal`
3. **Temporal windows**: Agent only sees "last 7 days" of content

### Privacy / Local-Only (Schema-Level)

A simple `is_private` flag that controls whether content can be sent to cloud models:

```sql
-- In content_blocks table
is_private INTEGER NOT NULL DEFAULT 0
```

**Semantics**:
- `is_private = 0`: Can be processed by any model (local or cloud)
- `is_private = 1`: Local models only - never sent to cloud APIs

**Enforcement**: The engine checks `is_private` when building context for LLM requests. If using a cloud model (Anthropic, OpenAI, etc.), private content is excluded. Local models (Ollama, whisper, etc.) can access everything.

```rust
impl ContextBuilder {
    fn build_for_model(&self, model: &ModelInfo, content: Vec<ContentBlockInfo>) -> Vec<ContentBlockInfo> {
        if model.is_cloud() {
            content.into_iter().filter(|c| !c.is_private).collect()
        } else {
            content  // local models see everything
        }
    }
}
```

**Use cases**:
- Sensitive personal notes that shouldn't leave the device
- Proprietary code/data that can't be sent to third parties
- Content that should only be processed by local/on-prem models

**Key distinction**:
- **Soft scoping** (tags): Organizational boundaries
- **Privacy** (`is_private`): Data residency - local vs cloud processing

---

## Implementation Steps

Each step is atomic and end-to-end testable before moving to the next.

### Step 1: Content Layer - ContentBlock Storage

**Goal**: Content-addressed text storage with SHA-256 hashing.

**Files**:
- `noema-core/src/storage/content_block/mod.rs` (new)
- `noema-core/src/storage/content_block/sqlite.rs` (new)

**Schema**:
```sql
CREATE TABLE content_blocks (
    id TEXT PRIMARY KEY,           -- UUID
    content_hash TEXT NOT NULL,    -- SHA-256 of text (for integrity/optional dedup)
    content_type TEXT NOT NULL,    -- text/plain, text/markdown, text/typst
    text TEXT NOT NULL,
    embedding BLOB,                -- Vector embedding (computed async, may be NULL initially)
    embedding_model TEXT,          -- Model used to generate embedding
    is_private INTEGER NOT NULL DEFAULT 0,  -- never exposed to agents
    origin_kind TEXT NOT NULL,     -- user, assistant, system, import
    origin_user_id TEXT,
    origin_model_id TEXT,
    origin_source_id TEXT,
    origin_parent_id TEXT REFERENCES content_blocks(id),
    created_at INTEGER NOT NULL
);
CREATE INDEX idx_content_blocks_hash ON content_blocks(content_hash);
CREATE INDEX idx_content_blocks_origin ON content_blocks(origin_kind);
CREATE INDEX idx_content_blocks_private ON content_blocks(is_private);
CREATE INDEX idx_content_blocks_needs_embedding ON content_blocks(id) WHERE embedding IS NULL;
```

**Trait** (using type-safe IDs):
```rust
#[derive(Clone)]
pub struct ContentOrigin {
    pub kind: OriginKind,  // User, Assistant, System, Import
    pub user_id: Option<UserId>,
    pub model_id: Option<String>,
    pub source_id: Option<String>,
    pub parent_id: Option<ContentBlockId>,
    pub is_private: bool,  // default false
}

#[async_trait]
pub trait ContentBlockStore {
    async fn store(&self, text: &str, content_type: &str, origin: ContentOrigin) -> Result<ContentBlockId>;
    async fn get(&self, id: &ContentBlockId) -> Result<Option<ContentBlockInfo>>;
    async fn get_text(&self, id: &ContentBlockId) -> Result<Option<String>>;
    async fn exists(&self, id: &ContentBlockId) -> Result<bool>;
    async fn find_by_hash(&self, hash: &str) -> Result<Vec<ContentBlockInfo>>; // for dedup lookup

    // Embedding support (async computation)
    async fn set_embedding(&self, id: &ContentBlockId, embedding: &[f32], model: &str) -> Result<()>;
    async fn get_pending_embeddings(&self, limit: usize) -> Result<Vec<ContentBlockId>>;
    async fn search_similar(&self, embedding: &[f32], limit: usize) -> Result<Vec<(ContentBlockId, f32)>>;
}
```

**Temporal metadata**: All content blocks carry `created_at` timestamp. This enables:
- "Summarize everything from the last month"
- "What topics have I been working on this week?"
- "Show me how my thinking evolved on X over time"

**Test**: Store text → get UUID back → retrieve text → verify hash matches.

---

### Step 1b: Asset Layer

**Goal**: Binary asset storage (images, audio, PDFs). Assets are content-addressed and referenced inline from content.

**Key insight**: Assets are referenced **inline within content** as `AssetRef { asset_id, mime_type }` or `DocumentRef { id, title }`. There's no separate join table - the reference is embedded in the message/document content JSON.

**Schema**:
```sql
-- Assets (content-addressed binary blobs)
CREATE TABLE assets (
    id TEXT PRIMARY KEY,           -- SHA-256 hash of content
    mime_type TEXT NOT NULL,
    original_filename TEXT,
    file_size_bytes INTEGER,
    is_private INTEGER NOT NULL DEFAULT 0,  -- local-only (never sent to cloud models)
    metadata_json TEXT,            -- dimensions, duration, codec, etc.
    local_path TEXT,               -- relative path in blob_storage/
    created_at INTEGER NOT NULL
);
```

**Inline references** (from existing `StoredContent` enum):
```rust
enum StoredContent {
    Text { text: String },
    Image { data: String, mime_type: String },      // inline base64
    Audio { data: String, mime_type: String },      // inline base64
    AssetRef { asset_id: String, mime_type: String, filename: Option<String> },  // CAS reference
    DocumentRef { id: String, title: String },      // RAG injection
    ToolCall(ToolCall),
    ToolResult(ToolResult),
}
```

**Trait**:
```rust
#[async_trait]
pub trait AssetStore {
    async fn store(&self, data: &[u8], mime_type: &str, filename: Option<&str>, is_private: bool) -> Result<AssetId>;
    async fn get(&self, id: &AssetId) -> Result<Option<AssetInfo>>;
    async fn get_data(&self, id: &AssetId) -> Result<Vec<u8>>;
    async fn exists(&self, id: &AssetId) -> Result<bool>;
}
```

**Resolution flow**: When sending to LLM, `StoredPayload::resolve()` fetches asset data and converts `AssetRef` to inline `Image`/`Audio`.

**Benefits**:
- Deduplication (same blob → same hash → single file)
- References embedded in content (no join table complexity)
- Separate caching/eviction from text content

**Test**: Store image → create message with `AssetRef` → resolve payload → verify inline image.

---

### Step 2: Conversation Structure - Turns, Spans, Messages

**Goal**: Replace SpanSet/Span/SpanMessage with Turn/Span/Message.

**Key concepts**:
- **Turn**: A position in the conversation sequence. Each turn has one or more alternative spans.
- **Span**: A sequence of messages representing one autonomous flow. The span has an overall role (user/assistant) but messages within can have different roles (assistant, tool, system) for multi-step execution.
- **Message**: Individual content within a span. Role on message allows: `assistant → tool_call → tool_result → assistant`.

**Why role on span AND message?**
- **Span role**: Identifies who "owns" this turn alternative (user editing, or assistant responding)
- **Message role**: Supports multi-step flows within a single span (tool calls, thinking, etc.)

```
Turn 3 (position in sequence):
  ├── Span A (role: assistant, model: claude):
  │     └── Message 1 (role: assistant, thinking)
  │     └── Message 2 (role: assistant, tool_call)
  │     └── Message 3 (role: tool, tool_result)
  │     └── Message 4 (role: assistant, response)
  ├── Span B (role: assistant, model: gpt-4):
  │     └── Message 1 (role: assistant, response)
  └── Span C (role: user, edit):  ← user can also have spans (edits)
        └── Message 1 (role: user, edited question)
```

This enables:
1. **Parallel model responses**: Multiple assistant spans at same turn
2. **Tool interactions**: Single span contains full assistant→tool→assistant flow
3. **User edits as alternatives**: Edit creates new user span at same turn
3. **Side-agent execution**: Spawn sub-conversations that can be spliced back
4. **Flexible forking**: Fork and splice at any granularity

```
Turn 3:
  ├── Span A (assistant, claude):  [thinking] → [tool_call] → [tool_result] → [response]
  ├── Span B (assistant, gpt-4):   [tool_call] → [tool_result] → [response]
  └── Span C (assistant, gemini):  [response]

Turn 4:
  ├── Span A (user):  [edited question]
  └── Span B (user):  [original question]
```

Role is on the **span**, not the turn. This enables:
- User edits creating alternative user spans at same turn
- Side-agent results injected as alternative spans
- Mixed-role spans (future: multi-agent conversations)

**Files**:
- `noema-core/src/storage/conversation/mod.rs` (replace)
- `noema-core/src/storage/conversation/sqlite.rs` (replace)
- `noema-core/src/storage/conversation/types.rs` (new)

**Schema**:
```sql
CREATE TABLE conversations (
    id TEXT PRIMARY KEY,
    user_id TEXT REFERENCES users(id) ON DELETE CASCADE,
    title TEXT,
    system_prompt TEXT,
    is_private INTEGER NOT NULL DEFAULT 0,
    parent_span_id TEXT REFERENCES spans(id),  -- for step-into sub-conversations
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE INDEX idx_conversations_parent ON conversations(parent_span_id);

CREATE TABLE turns (
    id TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    sequence_number INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    UNIQUE (conversation_id, sequence_number)
);
CREATE INDEX idx_turns_conversation ON turns(conversation_id, sequence_number);

CREATE TABLE spans (
    id TEXT PRIMARY KEY,
    turn_id TEXT NOT NULL REFERENCES turns(id) ON DELETE CASCADE,
    role TEXT NOT NULL CHECK(role IN ('user', 'assistant')),
    model_id TEXT,                 -- NULL for user spans
    parent_span_id TEXT REFERENCES spans(id),  -- for side-agent/sub-conversation linking
    created_at INTEGER NOT NULL
);
CREATE INDEX idx_spans_turn ON spans(turn_id);
CREATE INDEX idx_spans_parent ON spans(parent_span_id);

CREATE TABLE messages (
    id TEXT PRIMARY KEY,
    span_id TEXT NOT NULL REFERENCES spans(id) ON DELETE CASCADE,
    sequence_number INTEGER NOT NULL,
    role TEXT NOT NULL CHECK(role IN ('user', 'assistant', 'system', 'tool')),
    content_id TEXT REFERENCES content_blocks(id),
    tool_calls TEXT,           -- JSON array
    tool_results TEXT,         -- JSON array
    created_at INTEGER NOT NULL
);
CREATE INDEX idx_messages_span ON messages(span_id, sequence_number);

-- Note: Asset attachments use polymorphic asset_refs table (Step 1b)
-- Query: SELECT * FROM asset_refs WHERE from_type = 'message' AND from_id = ?
```

**Test**: Create conversation → add turn → add span → add message with content_id.

---

### Step 3: Views and View Selections

**Goal**: Named paths through conversation spans (replaces threads).

**Schema**:
```sql
CREATE TABLE views (
    id TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    name TEXT,
    is_main INTEGER NOT NULL DEFAULT 0,
    forked_from_view_id TEXT REFERENCES views(id),
    forked_at_turn_id TEXT REFERENCES turns(id),
    created_at INTEGER NOT NULL
);
CREATE INDEX idx_views_conversation ON views(conversation_id);

CREATE TABLE view_selections (
    view_id TEXT NOT NULL REFERENCES views(id) ON DELETE CASCADE,
    turn_id TEXT NOT NULL REFERENCES turns(id) ON DELETE CASCADE,
    span_id TEXT NOT NULL REFERENCES spans(id),
    PRIMARY KEY (view_id, turn_id)
);
```

**Trait** (using type-safe IDs):
```rust
#[async_trait]
pub trait ConversationStore {
    // Turns
    async fn add_turn(&self, conversation_id: &ConversationId) -> Result<TurnInfo>;
    async fn get_turns(&self, conversation_id: &ConversationId) -> Result<Vec<TurnInfo>>;

    // Spans (role is here, not on turn)
    async fn add_span(&self, turn_id: &TurnId, role: SpanRole, model_id: Option<&str>) -> Result<SpanInfo>;
    async fn add_child_span(&self, parent_span_id: &SpanId, role: SpanRole, model_id: Option<&str>) -> Result<SpanInfo>;
    async fn get_spans(&self, turn_id: &TurnId) -> Result<Vec<SpanInfo>>;
    async fn get_child_spans(&self, span_id: &SpanId) -> Result<Vec<SpanInfo>>;

    // Messages
    async fn add_message(&self, span_id: &SpanId, msg: NewMessage) -> Result<MessageInfo>;
    async fn get_messages(&self, span_id: &SpanId) -> Result<Vec<MessageInfo>>;

    // Views
    async fn create_view(&self, conversation_id: &ConversationId, name: Option<&str>, is_main: bool) -> Result<ViewInfo>;
    async fn fork_view(&self, view_id: &ViewId, at_turn_id: &TurnId) -> Result<ViewInfo>;
    async fn select_span(&self, view_id: &ViewId, turn_id: &TurnId, span_id: &SpanId) -> Result<()>;
    async fn get_view_path(&self, view_id: &ViewId) -> Result<Vec<ViewPathEntry>>;

    // Edit & Splice (IDEAS #2)
    async fn edit_turn(&self, view_id: &ViewId, turn_id: &TurnId, new_content: &str) -> Result<SpanInfo>;
    async fn fork_view_with_selections(&self, view_id: &ViewId, selections: &[(TurnId, SpanId)]) -> Result<ViewInfo>;
    async fn get_view_context_at(&self, view_id: &ViewId, turn_id: &TurnId) -> Result<Vec<MessageInfo>>;

    // Side-agent / sub-conversation (for agentic workflows)
    async fn inject_span(&self, turn_id: &TurnId, span: SpanInfo, messages: Vec<MessageInfo>) -> Result<SpanInfo>;

    // Message-to-document promotion
    async fn promote_message_to_document(&self, message_id: &MessageId, title: &str) -> Result<DocumentInfo>;
}

pub enum SpanRole {
    User,
    Assistant,
}
```

**Edit & Splice flow** (from IDEAS #2):

Three modes when editing a past message at turn N:

**Mode 1: Edit + re-execute all** (regenerate everything after edit)
```
Original:  T1 → T2 → T3 → T4 → T5
                 ↓ (edit T2, re-execute all)
Edited:    T1 → T2' → T3' → T4' → T5'  (new view, all regenerated)
```

**Mode 2: Edit + keep some + re-execute rest** (splice)
```
Original:  T1 → T2 → T3 → T4 → T5
                 ↓ (edit T2, keep T3-T4, re-execute from T5)
Spliced:   T1 → T2' → T3 → T4 → T5'  (T3-T4 reused from original)
```

**Mode 3: Edit only** (just replace, keep everything after)
```
Original:  T1 → T2 → T3 → T4 → T5
                 ↓ (edit T2, keep all)
Edited:    T1 → T2' → T3 → T4 → T5  (T3-T5 reused, may not make sense contextually)
```

Key insight: **Spans are shared across views**. When splicing, the new view can select the *same* spans at T3-T4 that the original view used, while having a different span at T2.

Key operations:
- `edit_turn(view_id, turn_id, new_content)` → creates new span at turn
- `fork_view_with_selections(view_id, selections)` → new view with custom span selections per turn
- `get_view_context_at(view_id, turn_id)` → messages up to turn (for re-execute)

**Side-agent execution** (agentic workflows):
```
Main conversation:           Side-agent:
T1 (user) → T2 (assistant)
                ↘ spawns sub-conversation
                              T1' (system) → T2' (assistant) → T3' (tool) → T4' (assistant)
                ↙ inject result as span
T3 (assistant, from side-agent)
```

The `parent_span_id` links the injected span back to its source, preserving provenance.

**Step-into / deep-dive** (user-initiated sub-conversation):

The inverse pattern: user wants to explore a topic in depth, then summarize back to main conversation.

```
Main conversation:           Sub-conversation (step-into):
T1 (user) → T2 (assistant)
     ↓ "Let me explore this deeper..."
     ├──────────────────────→ T1' (context from T2) → T2' (user) → T3' (assistant) → ... → TN' (done)
     │                                                                                        │
     │                                                                        summarize ←─────┘
     ↓
T3 (assistant, summary of sub-conversation)
```

Key operations:
- `step_into(span_id)` → creates sub-conversation with context from span
- `summarize_and_return(sub_conversation_id)` → generates summary, injects as span in parent

The sub-conversation is a full conversation (can be forked, edited, etc.) but linked via `parent_span_id` back to where it was spawned. When done, the summary becomes a new span in the parent conversation.

Trait additions:
```rust
async fn step_into(&self, span_id: &str, system_prompt: Option<&str>) -> Result<ConversationInfo>;  // create sub-conversation
async fn summarize_sub_conversation(&self, sub_conversation_id: &str, parent_turn_id: &str) -> Result<SpanInfo>;  // summarize and inject
async fn get_sub_conversations(&self, span_id: &str) -> Result<Vec<ConversationInfo>>;  // list step-into conversations
```

**Message-to-document promotion**:

A single assistant message can be "promoted" to a standalone document for further editing:

```
Message (content_id: abc123)
    ↓ promote_to_document()
Document (revision 1 → content_id: abc123)
    ↓ user edits
Document (revision 2 → content_id: xyz789)
```

The original `content_id` is reused as the first revision, establishing lineage. The message retains its `content_id` reference, but now that content block is also the basis of a document.

Trait addition:
```rust
async fn promote_message_to_document(&self, message_id: &str, title: &str) -> Result<DocumentInfo>;
```

This enables:
- Refining AI responses into polished documents
- Building artifacts from conversation outputs
- Maintaining provenance (document traces back to original message/conversation)

**Test**: Edit turn 2 → keep turns 3-4 → re-execute turn 5 → verify spliced history with mixed old/new spans.

---

### Step 4: Document Structure (Version Chain + Tabs)

**Goal**: Documents with revision history and structural tabs, all referencing content_blocks.

**Files**:
- `noema-core/src/storage/document/mod.rs` (replace)
- `noema-core/src/storage/document/sqlite.rs` (replace)

**Schema**:
```sql
CREATE TABLE documents (
    id TEXT PRIMARY KEY,
    user_id TEXT REFERENCES users(id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    source TEXT NOT NULL,          -- user_created, ai_generated, google_drive, import, promoted
    source_id TEXT,                -- external ID, or message_id for promoted docs
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE INDEX idx_documents_user ON documents(user_id);
CREATE INDEX idx_documents_source ON documents(source, source_id);

-- Tabs are structural pointers to content blocks (not separate content)
CREATE TABLE document_tabs (
    id TEXT PRIMARY KEY,
    document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    parent_tab_id TEXT REFERENCES document_tabs(id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    icon TEXT,
    position INTEGER NOT NULL,
    current_revision_id TEXT,      -- points to latest revision for this tab
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE INDEX idx_document_tabs_document ON document_tabs(document_id);
CREATE INDEX idx_document_tabs_parent ON document_tabs(parent_tab_id);

-- Revisions track content history per tab
CREATE TABLE revisions (
    id TEXT PRIMARY KEY,
    tab_id TEXT NOT NULL REFERENCES document_tabs(id) ON DELETE CASCADE,
    content_id TEXT NOT NULL REFERENCES content_blocks(id),
    parent_revision_id TEXT REFERENCES revisions(id),
    revision_number INTEGER NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE INDEX idx_revisions_tab ON revisions(tab_id);

-- Note: Asset attachments use polymorphic asset_refs table (Step 1b)
```

**Structure visualization**:
```
Document "API Design"
  └── Tab "Overview" (current_revision → rev3 → content_block:abc)
  │     └── rev1 → content_block:xyz (original)
  │     └── rev2 → content_block:def (edit)
  │     └── rev3 → content_block:abc (current)
  └── Tab "Endpoints"
        └── Sub-tab "Auth" → content_block:auth1
        └── Sub-tab "Users" → content_block:users1
```

**Trait**:
```rust
#[async_trait]
pub trait DocumentStore {
    // Document CRUD
    async fn create(&self, user_id: &UserId, title: &str, source: DocumentSource) -> Result<DocumentInfo>;
    async fn get(&self, id: &DocumentId) -> Result<Option<DocumentInfo>>;
    async fn find_by_source(&self, source: DocumentSource, source_id: &str) -> Result<Option<DocumentInfo>>;

    // Tab management (structural)
    async fn add_tab(&self, document_id: &DocumentId, parent_tab_id: Option<&TabId>, title: &str, content: &str) -> Result<TabInfo>;
    async fn add_tab_from_content(&self, document_id: &DocumentId, parent_tab_id: Option<&TabId>, title: &str, content_id: &ContentBlockId) -> Result<TabInfo>;
    async fn get_tabs(&self, document_id: &DocumentId) -> Result<Vec<TabInfo>>;
    async fn move_tab(&self, tab_id: &TabId, new_parent: Option<&TabId>, position: i32) -> Result<()>;

    // Revisions (per tab)
    async fn commit(&self, tab_id: &TabId, content: &str) -> Result<RevisionInfo>;
    async fn branch(&self, tab_id: &TabId, from_revision_id: &RevisionId, content: &str) -> Result<RevisionInfo>;
    async fn checkout(&self, tab_id: &TabId, revision_id: &RevisionId) -> Result<()>;
    async fn get_revisions(&self, tab_id: &TabId) -> Result<Vec<RevisionInfo>>;
    async fn get_content(&self, revision_id: &RevisionId) -> Result<String>;

    // Convenience: promote message to document
    async fn promote_from_message(&self, user_id: &UserId, title: &str, message_id: &MessageId, content_id: &ContentBlockId) -> Result<DocumentInfo>;
}

pub enum DocumentSource {
    UserCreated,
    AiGenerated,
    GoogleDrive,
    Import,
    Promoted { message_id: String },
}
```

**Key points**:
- Tabs are **structural** - they point to content blocks, not contain content
- Each tab has its own revision history
- `add_tab_from_content` reuses existing content_id (for message promotion)
- Sub-tabs via `parent_tab_id` for hierarchy

**Test**: Create doc → add tabs with hierarchy → commit revisions → branch from rev 2 → checkout branch → verify structure preserved.

---

### Step 5: Collections (Meta-Structure over Entities)

**Goal**: Collections as a structural layer organizing any entity type (documents, conversations, content blocks, other collections), with schema hints for UI and cached field indexes for queries.

**Core principle: Flexible references, documents as primary**

| Layer | Purpose | Source of Truth |
|-------|---------|-----------------|
| Collection | Structural grouping | `collection_items` → any entity |
| Schema hint | UI guidance (expected fields) | `collections.schema_hint` |
| Field index | Fast queries | `item_fields` (cached) |

**For document items**: Frontmatter in document content is the source of truth for fields. `item_fields` is a **cached index** regenerated on content change.

**For other items** (conversations, content blocks): Fields stored directly in `item_fields` since they don't have frontmatter.

```markdown
---
priority: P1
status: in_progress
phase: 3
tags: [ui, theme]
---
# Dark Mode Feature

Full description that the LLM can read and edit naturally...
```

**Data flow**:
- **LLM edits document** → frontmatter changes → reindex `item_fields`
- **UI edits field** → update document frontmatter → new content block revision → reindex
- **Query** → use `item_fields` index for fast filter/sort

**Files**:
- `noema-core/src/storage/collection/mod.rs` (new)
- `noema-core/src/storage/collection/sqlite.rs` (new)

**Schema**:
```sql
-- Collection = structural grouping with advisory schema
CREATE TABLE collections (
    id TEXT PRIMARY KEY,
    user_id TEXT REFERENCES users(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    description TEXT,
    schema_hint TEXT,              -- JSON: advisory field definitions for UI
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

-- Items link any entity into collections (structural relationship)
CREATE TABLE collection_items (
    id TEXT PRIMARY KEY,
    collection_id TEXT NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
    target_type TEXT NOT NULL,     -- document, conversation, content_block, collection
    target_id TEXT NOT NULL,
    parent_item_id TEXT REFERENCES collection_items(id) ON DELETE CASCADE,
    position INTEGER NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE INDEX idx_collection_items_collection ON collection_items(collection_id);
CREATE INDEX idx_collection_items_target ON collection_items(target_type, target_id);
CREATE INDEX idx_collection_items_parent ON collection_items(parent_item_id);

-- Field index (CACHED - derived from document frontmatter, not source of truth)
CREATE TABLE item_fields (
    item_id TEXT NOT NULL REFERENCES collection_items(id) ON DELETE CASCADE,
    field_name TEXT NOT NULL,
    field_value TEXT NOT NULL,     -- JSON value
    PRIMARY KEY (item_id, field_name)
);
CREATE INDEX idx_item_fields_field ON item_fields(field_name, field_value);

-- Tags (cross-cutting)
CREATE TABLE item_tags (
    item_id TEXT NOT NULL REFERENCES collection_items(id) ON DELETE CASCADE,
    tag TEXT NOT NULL,
    PRIMARY KEY (item_id, tag)
);
CREATE INDEX idx_item_tags_tag ON item_tags(tag);

-- Saved views (table, list, board, calendar, etc.)
CREATE TABLE collection_views (
    id TEXT PRIMARY KEY,
    collection_id TEXT NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    view_type TEXT NOT NULL,       -- table, list, board, calendar, gallery
    config TEXT NOT NULL,          -- JSON: {sort: [...], filter: {...}, columns: [...], group_by: "status"}
    is_default INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL
);
CREATE INDEX idx_collection_views ON collection_views(collection_id);

-- Embed views in documents (like Notion inline databases)
CREATE TABLE document_embeds (
    id TEXT PRIMARY KEY,
    document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    view_id TEXT NOT NULL REFERENCES collection_views(id) ON DELETE CASCADE,
    position INTEGER NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE INDEX idx_document_embeds ON document_embeds(document_id);
```

**Schema hint format** (advisory, not enforced):
```json
{
  "fields": [
    {"name": "priority", "type": "select", "options": ["P0", "P1", "P2"]},
    {"name": "status", "type": "select", "options": ["todo", "in_progress", "done"]},
    {"name": "phase", "type": "number"},
    {"name": "assignee", "type": "relation", "target": "users"}
  ]
}
```

UI uses schema_hint to:
- Show appropriate field editors (dropdowns, date pickers, etc.)
- Suggest fields when creating new items
- Validate input (soft validation, not enforced at DB level)

**Trait**:
```rust
#[async_trait]
pub trait CollectionStore {
    // Collection CRUD
    async fn create(&self, user_id: &str, name: &str) -> Result<CollectionInfo>;
    async fn get(&self, id: &str) -> Result<Option<CollectionInfo>>;

    // Schema management
    async fn add_field(&self, collection_id: &str, field: FieldDefinition) -> Result<()>;
    async fn update_field(&self, collection_id: &str, field_name: &str, field: FieldDefinition) -> Result<()>;
    async fn remove_field(&self, collection_id: &str, field_name: &str) -> Result<()>;
    async fn get_schema(&self, collection_id: &str) -> Result<Vec<FieldDefinition>>;

    // Items (reference any entity type)
    async fn add_item(&self, collection_id: &str, target: ItemTarget, parent_id: Option<&str>, position: i32) -> Result<ItemInfo>;
    async fn update_item_fields(&self, item_id: &str, fields: HashMap<String, Value>) -> Result<()>;
    async fn move_item(&self, item_id: &str, new_parent_id: Option<&str>, new_position: i32) -> Result<()>;
    async fn remove_item(&self, item_id: &str) -> Result<()>;

    // Tags
    async fn tag(&self, item_id: &str, tags: &[&str]) -> Result<()>;
    async fn untag(&self, item_id: &str, tags: &[&str]) -> Result<()>;

    // Views
    async fn create_view(&self, collection_id: &str, name: &str, view_type: ViewType, config: ViewConfig) -> Result<ViewInfo>;
    async fn query_view(&self, view_id: &str) -> Result<Vec<ItemWithFields>>;  // applies sort/filter from view config

    // Embeds
    async fn embed_view(&self, document_id: &str, view_id: &str, position: i32) -> Result<String>;
    async fn get_document_embeds(&self, document_id: &str) -> Result<Vec<EmbedInfo>>;
}

pub enum ItemTarget {
    Document(DocumentId),
    Conversation(ConversationId),
    ContentBlock(ContentBlockId),
    Collection(CollectionId),
}

pub struct FieldDefinition {
    pub name: String,
    pub field_type: FieldType,  // Text, Number, Select, MultiSelect, Date, Checkbox, Relation, Formula
    pub config: Option<FieldConfig>,  // options for select, formula expression, relation target, etc.
}

pub struct ViewConfig {
    pub sort: Vec<SortSpec>,           // [{field: "priority", dir: "asc"}]
    pub filter: Option<FilterSpec>,    // {field: "status", op: "in", values: ["todo", "in_progress"]}
    pub columns: Vec<String>,          // which fields to show
    pub group_by: Option<String>,      // for board view
}
```

**Example usage**:
```
Collection "Features" (schema: priority:select, status:select, phase:number, assignee:relation)
    └── Item → Document "Dark Mode" (priority: P1, status: in_progress, phase: 3)
    └── Item → Document "RAG Search" (priority: P0, status: todo, phase: 5)
    └── Item → Document "Audio Models" (priority: P1, status: todo, phase: 7)

View "Phase 3 Table" (type: table, filter: {phase: 3}, sort: [{priority: asc}], columns: [title, priority, status])

Document "Roadmap.md" embeds View "Phase 3 Table" at position 5
    → renders as live table showing filtered/sorted features
```

**LLM-writable collection syntax**: LLMs can create/modify collections by writing marked-up tables.

**Markdown format**:
```markdown
<!-- collection:features -->
<!-- schema: priority(select:P0,P1,P2), status(select:todo,in_progress,done), phase(number) -->

| title | priority | status | phase |
|-------|----------|--------|-------|
| Dark Mode | P1 | in_progress | 3 |
| RAG Search | P0 | todo | 5 |
| Audio Models | P1 | todo | 7 |

<!-- /collection -->
```

**Typst format**:
```typst
#collection("features", schema: (
  priority: select("P0", "P1", "P2"),
  status: select("todo", "in_progress", "done"),
  phase: number,
))[
  #item[Dark Mode][priority: P1, status: in_progress, phase: 3]
  #item[RAG Search][priority: P0, status: todo, phase: 5]
  #item[Audio Models][priority: P1, status: todo, phase: 7]
]
```

**On parse**:
1. Create/update collection with schema
2. Create document for each row (title becomes doc title + content)
3. Add items with field values from frontmatter
4. Index fields in `item_fields` table

**On render** (for LLM context): Collections render back to this format so LLM can read and modify.

**Test**: Create collection with schema → add items with fields → create table view with filter/sort → embed in document → query returns correct sorted/filtered items → LLM context shows markdown table.

---

### Step 6: Cross-References

**Goal**: Links between any entity types with backlinks.

**Files**:
- `noema-core/src/storage/reference/mod.rs` (new)
- `noema-core/src/storage/reference/sqlite.rs` (new)

**Schema**:
```sql
CREATE TABLE references (
    id TEXT PRIMARY KEY,
    from_type TEXT NOT NULL,       -- message, document, collection_item
    from_id TEXT NOT NULL,
    to_type TEXT NOT NULL,         -- content_block, document, conversation, revision
    to_id TEXT NOT NULL,
    relation_type TEXT,            -- NULL, 'derived_from', 'cites', etc.
    created_at INTEGER NOT NULL,
    UNIQUE (from_type, from_id, to_type, to_id, relation_type)
);
CREATE INDEX idx_references_from ON references(from_type, from_id);
CREATE INDEX idx_references_to ON references(to_type, to_id);
```

**Trait**:
```rust
#[async_trait]
pub trait ReferenceStore {
    async fn create(&self, from: EntityRef, to: EntityRef, relation: Option<&str>) -> Result<String>;
    async fn get_outgoing(&self, from: EntityRef) -> Result<Vec<ReferenceInfo>>;
    async fn get_backlinks(&self, to: EntityRef) -> Result<Vec<ReferenceInfo>>;
    async fn delete(&self, id: &str) -> Result<()>;
}
```

**Test**: Create ref from message to document → query outgoing → query backlinks.

---

### Step 7: Temporal Queries & LLM Context

**Goal**: Enable temporal reasoning over all content - the LLM should be able to query, summarize, and organize content by time.

**Design principle**: All entities carry `created_at` timestamps. The LLM receives this temporal information when building context, enabling queries like:
- "What did we discuss last week?"
- "Summarize my activity this month"
- "How has my thinking on X evolved?"
- "Organize these topics by when they first appeared"

**Unified query trait**:
```rust
#[async_trait]
pub trait TemporalStore {
    /// Query all content (messages, documents, revisions) in a time range
    async fn query_by_time_range(
        &self,
        user_id: &str,
        start: i64,      // Unix timestamp
        end: i64,
        content_types: &[ContentType],  // Filter by type
        limit: usize,
    ) -> Result<Vec<TemporalContent>>;

    /// Get activity summary for a period
    async fn get_activity_summary(
        &self,
        user_id: &str,
        start: i64,
        end: i64,
    ) -> Result<ActivitySummary>;

    /// Timeline of all content, paginated
    async fn get_timeline(
        &self,
        user_id: &str,
        before: Option<i64>,  // cursor
        limit: usize,
    ) -> Result<Vec<TemporalContent>>;
}

pub struct TemporalContent {
    pub id: String,
    pub content_type: ContentType,  // Message, Document, Revision
    pub created_at: i64,
    pub title: Option<String>,      // Document title, conversation title, etc.
    pub preview: String,            // First N chars of content
    pub context: ContentContext,    // Parent conversation/document info
}

pub struct ActivitySummary {
    pub message_count: usize,
    pub document_count: usize,
    pub revision_count: usize,
    pub conversations_active: usize,
    pub top_topics: Vec<String>,    // Extracted from embeddings/content
}

pub enum ContentType {
    Message,
    Document,
    Revision,
    CollectionItem,
}
```

**LLM context rendering**: When the LLM needs temporal context, we render it as markdown with timestamps:

```markdown
## Activity: 2026-01-03 to 2026-01-10

### Conversations
- **Jan 10, 14:32** - "Unified Content Model planning" (12 turns)
- **Jan 8, 09:15** - "Bug fix in auth module" (5 turns)
- **Jan 5, 16:45** - "Feature brainstorm: dark mode" (8 turns)

### Documents Modified
- **Jan 9, 11:20** - "Phase 3 Plan" (rev 4)
- **Jan 7, 14:00** - "API Design Notes" (rev 2)

### Topics (by embedding clusters)
- Storage architecture (4 conversations, 2 docs)
- Authentication (1 conversation)
- UI/UX (2 conversations, 1 doc)
```

This format is both human-readable and LLM-parseable. The LLM can:
1. Summarize what happened in a period
2. Find related content across time
3. Track evolution of topics
4. Organize/categorize historical content

**Indexes for temporal queries**:
```sql
CREATE INDEX idx_content_blocks_created ON content_blocks(created_at);
CREATE INDEX idx_messages_created ON messages(created_at);
CREATE INDEX idx_revisions_created ON revisions(created_at);
CREATE INDEX idx_conversations_updated ON conversations(updated_at);
```

**Test**: Create content over simulated time range → query by date range → verify correct filtering → render as LLM context.

---

### Step 8: Session Adapter Integration

**Goal**: Connect `SqliteSession` to use new Turn/Span/Message model.

**Files**:
- `noema-core/src/storage/session/sqlite.rs` (modify)

**Changes**:
1. `commit()` creates Turn + Span + Message(s)
2. Message text stored via `ContentBlockStore::store()` → `content_id`
3. `commit_parallel_responses()` creates one Turn with multiple Spans
4. `open_conversation()` uses `get_view_path()` for main view

**Test**: Full session flow with new schema in app.

---

### Step 9: Remove Legacy Tables

**Goal**: Clean up old Thread/SpanSet/Span/SpanMessage tables.

**Files**:
- `noema-core/src/storage/conversation/sqlite.rs`
- `noema-core/src/storage/session/sqlite.rs`

**Drop**:
```sql
DROP TABLE IF EXISTS span_messages;
DROP TABLE IF EXISTS spans;
DROP TABLE IF EXISTS span_sets;
DROP TABLE IF EXISTS threads;
```

Also drop old document tables:
```sql
DROP TABLE IF EXISTS document_tabs;
DROP TABLE IF EXISTS document_revisions;
```

**Test**: Fresh app start with new schema only.

---

## Verification Plan

| Step | Build | Unit Test | E2E Test |
|------|-------|-----------|----------|
| 1 | `cargo build` | content_block tests | - |
| 1b | `cargo build` | asset + refs tests | Store, attach, backlinks |
| 2 | `cargo build` | turn/span/msg tests | - |
| 3 | `cargo build` | view + splice tests | Create conv, fork view, splice |
| 4 | `cargo build` | document + tabs tests | Create, tabs, revisions, promote |
| 5 | `cargo build` | collection tests | Schema hint, views, reindex |
| 6 | `cargo build` | reference tests | Create, backlinks |
| 7 | `cargo build` | temporal query tests | Time range, activity summary |
| 8 | `cargo build` | session tests | Full conversation |
| 9 | `cargo build` | all tests | Fresh DB, full app |

---

## Schema Relationship Diagram

```
                 content_blocks                    assets (binary blobs)
                 (text, immutable)                 (SHA-256 keyed)
                        ↑                                ↑
         ┌──────────────┼──────────────┐                 │
         │              │              │      (inline AssetRef in content)
         │              │              │
      messages      revisions     item_fields
         │              │          (cached)
         │              │              │
      spans ←───── document_tabs      │
         │  ↖           │             │
         │   parent     │             │
       turns        documents         │
         │              │             │
   conversations        │        collections
         │              │             │
       views            │      collection_items ──→ (any entity)
         │              │             │
   view_selections      │        item_tags
                        │
                  document_embeds ← collection_views


                      references (any → any)

@-mentions: ContextRef resolves any node to content_blocks
```

---

## File Summary

| Module | Purpose |
|--------|---------|
| `storage/ids.rs` | Type-safe opaque IDs |
| `storage/content_block/` | Content-addressed text storage with embeddings |
| `storage/asset/` | Binary blob storage (CAS) |
| `storage/conversation/` | Turn/Span/Message/View + splice + step-into |
| `storage/document/` | Documents with tabs, revisions, promotion |
| `storage/collection/` | Meta-structure over any entity, schema hints, cached fields |
| `storage/reference/` | Cross-references and backlinks |
| `storage/temporal/` | Time-based queries and LLM context building |
| `storage/context/` | `ContextRef` resolution for @-mentions |
| `storage/session/sqlite.rs` | Session adapter for engine |

---

## Design Clarifications

### Document Tabs as Structure

Tabs are **structural relationships**, not separate content. A tab is a pointer to a content block with position/hierarchy:

```
Document
  └── Tab "Overview" → content_block_id: abc123
  └── Tab "Details"
        └── Sub-tab "API" → content_block_id: def456
        └── Sub-tab "Schema" → content_block_id: ghi789
```

### Context Injection (@mentions)

`@`-referencing works at **any granularity** in the hierarchy, from entire structures down to individual content blocks:

```rust
enum ContextRef {
    // Content layer (leaf)
    ContentBlock(ContentBlockId),
    Asset(AssetId),

    // Document hierarchy
    Document(DocumentId),                    // entire doc with all tabs
    DocumentTab(TabId),                      // single tab
    DocumentRevision(RevisionId),            // specific revision

    // Conversation hierarchy
    Conversation(ConversationId),            // entire conversation
    ConversationView(ViewId),                // specific view/branch
    ConversationTurn(TurnId),                // single turn (all spans)
    ConversationSpan(SpanId),                // specific span
    ConversationMessage(MessageId),          // single message
    ConversationRange {                      // range of turns
        view_id: ViewId,
        from_turn: u32,
        to_turn: u32,
    },

    // Collection hierarchy
    Collection(CollectionId),                // all items
    CollectionView(CollectionViewId),        // filtered/sorted view
    CollectionItem(CollectionItemId),        // single item (→ its document)
}
```

**Resolution**: Each ref type knows how to resolve to ordered content blocks:

| Ref Level | Resolves To |
|-----------|-------------|
| `@document` | All tabs' current revisions, with hierarchy markers |
| `@document/tab` | Single tab's current revision |
| `@conversation` | Main view's selected spans |
| `@conversation/view` | That view's selected spans |
| `@conversation/turn:3` | All spans at turn 3 |
| `@conversation/span:xyz` | Messages in that span |
| `@collection` | All items' documents |
| `@collection/view` | Filtered/sorted subset |

**Hierarchy markers** in LLM context:
```markdown
## Document: API Design

### Tab: Overview
[content block abc123]

### Tab: Endpoints
#### Sub-tab: Auth
[content block auth1]
```

This enables precise `@`-referencing: "look at @api-design/endpoints/auth" or "based on @conversation/turn:5".

### Assets as Separate Layer

Assets (binary blobs) have different semantics than content blocks:
- **Immutable & content-addressed** - SHA-256 hash as ID
- **Shared across contexts** - Same image in multiple messages/documents
- **Different caching** - Lazy-loaded, thumbnailed, eviction policies
- **Different privacy model** - Asset `is_private` separate from referencing entity

### Collections as Meta-Structure

Collections are a **structural layer over any entity** (documents, conversations, content blocks, other collections):
- `schema_hint` is advisory (tells UI what fields to show/suggest)
- For document items: frontmatter is source of truth, `item_fields` is cached index
- For other items: `item_fields` stores fields directly

**Query flow** (for document items):
1. Parse document content block for frontmatter
2. Extract fields matching schema_hint
3. Index in `item_fields` (computed, regenerated on content change)

**Edit flow** (for document items):
- LLM edits document text → frontmatter changes → index updates
- UI edits field → updates document frontmatter → new content block revision

---

## Future Extension Points

These extension points from [UNIFIED_CONTENT_MODEL.md](../design/UNIFIED_CONTENT_MODEL.md) and [HOOK_SYSTEM.md](../design/HOOK_SYSTEM.md) are designed for but not implemented in this phase:

### EP-1: Event Emission
All store mutations emit events (`entity.created.*`, `entity.updated.*`, etc.) logged as ContentBlocks. Enables hook system.

### EP-2: Temporal Indexing
Timestamps on all entities enable time-range queries. Indexes: `idx_content_blocks_created`, `idx_messages_created`.

### EP-3: Hook Registry
Hooks bind event patterns to actions. Both patterns and actions stored as ContentBlocks.

### EP-4: Dynamic Content Flag
ContentBlocks can be marked `is_dynamic` for Typst evaluation before render.

### EP-5: Context Strategy
Views can reference a context strategy for compressing/summarizing history before LLM injection.

### EP-6: Temporal Triggers
Schedule-based, idle-based, and timeout triggers that emit events for proactive behaviors.

**Implementation approach**: Add schema for events/hooks/triggers when needed. Current focus is UCM foundation.
