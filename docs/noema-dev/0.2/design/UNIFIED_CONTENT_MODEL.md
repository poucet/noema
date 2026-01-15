# Unified Content Model

**Status:** Draft → Revised
**Created:** 2026-01-10
**Updated:** 2026-01-15
**Related:** IDEAS #1, #2, #3

---

## Three-Layer Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    ADDRESSABLE LAYER                        │
│  Unified identity, naming, and relationships                │
│  - entities (id, type, name, slug, user, privacy, archive) │
│  - entity_relations (from, to, relation, metadata)          │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                    STRUCTURE LAYER                          │
│  Domain-specific internal organization                      │
│  - views → view_selections → turns → spans → messages      │
│  - documents → tabs → tab_content                          │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                     CONTENT LAYER                           │
│  Shared content storage (NOT deduplicated by hash)         │
│  - content_blocks (text with origin tracking)              │
│  - assets + blobs (binary files)                           │
└─────────────────────────────────────────────────────────────┘
```

### Key Insight: Views Are Conversations

**Views ARE the conversation structure.** What we call "conversations" in the UI are really views with metadata. The `Conversation` concept is just organizational metadata attached to a view.

This means:
- **Views are first-class addressable entities** (can be @mentioned)
- **"Conversation" is UI terminology** for a view entity
- **Forking creates a new view entity** with a 'forked_from' relation
- **Views can be promoted** to standalone "conversations" by renaming
- **Deleting a view** doesn't affect its forks (they're independent entities)

---

## Target Use Cases

| # | Use Case | Description |
|---|----------|-------------|
| 1 | Agent → subagent | Parent spawns child with scoped context, gets summary back |
| 2 | Agent ↔ agent (supervised) | Two agents communicate, human approves cross-messages |
| 3 | Parallel models + chaining | Multiple models respond, user selects, chain continues |
| 4 | Fork conversation | Branch from any point, paths diverge |
| 5 | Edit & splice | Edit mid-conversation, optionally keep subsequent messages |
| 6 | Versioned documents | Markdown/typst docs with revision history |
| 7 | Cross-reference | Same content appears in conversation AND as document |
| 8 | Structured data | Ordered lists, trees, tagged items, table views |

---

## Core Principle

**Separate content (heavy, immutable) from structure (lightweight, mutable) from identity (addressable, organizational).**

```
┌─────────────────────────────────────────────────────────────┐
│                   ADDRESSABLE LAYER                         │
│  Unified identity: name, slug (@mention), tags, privacy    │
│  Relationships: forked_from, references, grouped_with      │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                    STRUCTURE LAYER                          │
│                                                             │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐         │
│  │   Views +   │  │  Version    │  │   Tree +    │         │
│  │   Spans     │  │   Chain     │  │  Ordering   │         │
│  │             │  │             │  │             │         │
│  │Conversations│  │  Documents  │  │ Collections │         │
│  └─────────────┘  └─────────────┘  └─────────────┘         │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                     CONTENT LAYER                           │
│  Immutable content with origin tracking                     │
│  - ContentBlocks: text (NOT deduplicated - each has unique │
│    origin metadata even if text is identical)              │
│  - Assets/Blobs: binary (content-addressed, deduplicated)  │
└─────────────────────────────────────────────────────────────┘
```

---

## Content Layer

Two storage types: **text content** (searchable, referenceable) and **binary assets** (opaque blobs).

### ContentBlock (Text)

All textual content: messages, documents, structured text.

```
ContentBlock {
    id: ContentBlockId        // UUID (unique per block)
    content_hash: String      // SHA-256 of text (for integrity, not dedup)
    content_type: String      // "text/plain", "text/markdown", "text/typst"
    text: String              // the actual text content
    origin: ContentOrigin
    created_at: Timestamp
}

ContentOrigin {
    kind: user | assistant | system | import
    user_id: Option<UserId>                    // which user (multi-user)
    model_id: Option<ModelId>                  // which model (if AI)
    source_id: Option<String>                  // external ID (google doc, url)
    parent_content_id: Option<ContentBlockId>  // if edited/derived
}
```

**Important:** ContentBlocks are NOT deduplicated by hash. Each block gets a unique ID even if the text is identical, because:
- Different origin metadata (user vs assistant, different models)
- Different timestamps
- Different privacy settings
- Need to track provenance separately

The hash is computed and stored for integrity checking, not for deduplication.

**What goes in ContentBlock:**
- User messages (text)
- Assistant responses (text)
- Document content (markdown, typst)
- Imported documents (converted to text)

**ContentBlock enables:**
- Full-text search across all text
- RAG (retrieve relevant content for context)
- Cross-referencing ("as I said in message X")
- Summarization (summarize any content block)

### Asset (Binary)

Binary content: images, audio, PDF, video. Stored in BlobStore (CAS).

```
Asset {
    id: SHA256Hash            // content-addressed
    mime_type: String         // "image/png", "audio/mp3", etc.
    filename: Option<String>
    size_bytes: u64
}
```

**What goes in BlobStore:**
- Images (png, jpg, webp)
- Audio (mp3, wav)
- PDF, video, other binary

### Tool Interactions

Tool calls and results stay **inline in messages** (not ContentBlock):
- May contain binary references (AssetRef)
- Ephemeral to conversation flow
- Not independently searchable/referenceable

```
Message {
    role: user | assistant
    content: ContentBlockRef          // text → searchable
    asset_refs: [AssetRef]            // binary attachments
    tool_calls: [ToolCall]            // inline
    tool_results: [ToolResult]        // inline
}
```

---

## Addressable Layer

The addressable layer provides unified identity, naming, and relationships for all entity types.

### Entity Table

Every addressable thing (view, document, asset) is an entity:

```sql
CREATE TABLE entities (
    id TEXT PRIMARY KEY,
    entity_type TEXT NOT NULL,  -- 'view', 'document', 'asset'
    user_id TEXT REFERENCES users(id),
    name TEXT,                  -- display name
    slug TEXT UNIQUE,           -- for @mentions (user-assigned)
    is_private INTEGER DEFAULT 0,
    is_archived INTEGER DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE INDEX idx_entities_user ON entities(user_id);
CREATE INDEX idx_entities_type ON entities(entity_type, user_id);
CREATE INDEX idx_entities_slug ON entities(slug) WHERE slug IS NOT NULL;
```

### Entity Relations

Relationships between entities (fork ancestry, references, grouping):

```sql
CREATE TABLE entity_relations (
    from_id TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    to_id TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    relation TEXT NOT NULL,     -- 'forked_from', 'references', 'grouped_with'
    metadata TEXT,              -- JSON (e.g., {at_turn_id: "..."} for forks)
    created_at INTEGER NOT NULL,
    PRIMARY KEY (from_id, to_id, relation)
);

CREATE INDEX idx_entity_relations_to ON entity_relations(to_id, relation);
```

### Relation Types

| Relation | From | To | Metadata | Use Case |
|----------|------|-----|----------|----------|
| `forked_from` | View | View | `{at_turn_id}` | Fork ancestry |
| `references` | Any | Any | `{context}` | Cross-references, citations |
| `derived_from` | Document | Document | `{revision_id}` | Document lineage |
| `grouped_with` | Any | Any | - | Manual grouping |

### Key Benefits

1. **Unified @mentions**: All entities addressable by slug
2. **Decoupled relationships**: Fork ancestry stored in relations, not on views
3. **Independent lifecycle**: Deleting one view doesn't affect its forks
4. **Flexible organization**: Tags, grouping without rigid hierarchy
5. **Consistent metadata**: name, privacy, archive across all types

### Views Replace Conversations

The old `conversations` table is eliminated. Its responsibilities move to:

| Old (conversations) | New Location |
|---------------------|--------------|
| `id` | `entities.id` (view id IS entity id) |
| `name` | `entities.name` |
| `user_id` | `entities.user_id` |
| `is_private` | `entities.is_private` |
| `main_view_id` | N/A - view IS the entity |
| Fork tracking | `entity_relations` with `relation='forked_from'` |

---

## Structure Layer

### Three Structure Types

| Type | Abstraction | Used for |
|------|-------------|----------|
| **Sequence + Spans** | Ordered positions, each with spans; views select path | Conversations |
| **Version Chain** | Revisions with parent links, linear with branches | Documents |
| **Tree + Ordering** | Nested items with position | Collections |

All three reference the same content layer. Cross-references link between any entities.

---

## Structure Type 1: Turn Sequences (Views/Conversations)

### Model

A **view** is a sequence of **turns**. Each turn has a role (user/assistant) and one or more **spans**. A span is a sequence of messages (not a single message).

```
View (addressable entity - what UI calls a "conversation")
  └── view_selections (ordered list of turn+span pairs)
        └── Turn (user or assistant turn, shared across views)
              └── Span (one possible response - a sequence of messages)
                    └── [Message, Message, ...] → each Message has ContentBlockRef
```

**Key:** Views are entities. The view's ID is its entity ID. Turns and spans are shared structural components that views reference.

### Why Spans Contain Multiple Messages

Different models (or regenerations) produce different numbers of messages for the same turn:

```
Turn 3 (assistant):
  ├── Span A (claude):  [thinking] → [tool_call] → [tool_result] → [response]  (4 messages)
  ├── Span B (gpt-4):   [tool_call] → [tool_result] → [response]               (3 messages)
  └── Span C (gemini):  [response]                                              (1 message)
```

All three are valid spans for the same assistant turn, despite having different lengths.

### Key Insight: Spans are Shared

Views don't own spans—they **select** them. Multiple views can select the same span, or different spans at the same turn.

```
View A: [turn1:span1] → [turn2:span1] → [turn3:span1] → [turn4:span1]
                                             ↗
View B: [turn1:span1] → [turn2:span1] → [turn3:span2] → [turn4:span1]  ← reuses turn4:span1!
```

This enables splice: edit turn 3, but keep turn 4 from original.

### Operations

| Operation | Description |
|-----------|-------------|
| `add_turn(role)` | Append new turn to conversation |
| `add_span(turn, model)` | Generate span at turn |
| `select(view, turn, span)` | View selects which span |
| `fork(view, turn)` | New view sharing selections up to turn |
| `spawn_child(view, turn)` | New conversation inheriting context |

---

## Structure Type 2: Version Chain (Documents)

### Model

```
Document
  └── Revision → ContentBlock
        └── parent_revision (forms DAG)
```

Linear history with optional branching. Current revision pointer.

### Operations

| Operation | Description |
|-----------|-------------|
| `commit(content)` | New revision, parent is current |
| `branch(revision)` | New revision with different parent |
| `checkout(revision)` | Move current pointer |

---

## Structure Type 3: Tree + Ordering (Collections)

### Model

```
Collection
  └── Item (has parent, position)
        └── references: ContentBlock | Document | Conversation | Collection
        └── tags: [String]
        └── fields: {key: value}  // for table views
```

### Variants

| Variant | Structure | Use case |
|---------|-----------|----------|
| List | Flat, ordered | Task list, bookmarks |
| Tree | Nested, ordered | Folders, outlines |
| Tagged | Flat + tags | Cross-cutting organization |
| Table | Flat + fields | Kanban, spreadsheet |

### Operations

| Operation | Description |
|-----------|-------------|
| `add_item(parent, position, target)` | Add item to tree |
| `move(item, new_parent, new_position)` | Reorder |
| `tag(item, tags)` | Add tags |
| `set_fields(item, fields)` | Set structured data |

---

## Use Case Analysis

### 1. Agent → Subagent

Parent spawns child conversation. Child works with scoped context. Result summarized back.

```
Parent:  T1 → T2 → T3 ─────────────────────────────→ T4(with summary)
                    │                                      ▲
                    └─ Span A contains child messages:     │
                         [spawn] → [child work...] → [summary]
                                        │
                                        ▼
Child:                                C1 → C2 → C3
                                   (inherits T1-T2 context)
```

**Key insight:** The subagent call is part of the parent's turn span. The child conversation is a separate entity, but its summary becomes part of the parent's span.

**Structure needed:**
- Parent-child relationship between conversations
- Child inherits context (turns/spans) up to spawn point
- Child messages embedded within parent's span
- Summary content flows back as message in parent's span

**Operations:**
- `spawn_child(parent_view, turn)` → new conversation
- Child sees parent's context as read-only prefix
- Child messages form nested sequence within parent's span
- `summarize()` → ContentBlock added to parent's current span

---

### 2. Agent ↔ Agent (Supervised)

Two independent conversations. Human mediates message passing.

```
Agent A: A1 → A2 → A3 ──[propose to B]──→ A4(from B) → A5
                              │                 ▲
                              ▼                 │
Agent B:              B1 → B2(from A) → B3 ──[propose to A]

Human approves: A3→B2, B3→A4
```

**Structure needed:**
- Two independent conversations
- Proposed links (pending cross-references)
- Approval state on links
- Shared content (same ContentBlock in both conversations)

**Operations:**
- `propose_message(from_conv, to_conv, content)` → pending link
- `approve(link)` → content added to target conversation
- Both reference same ContentBlock (dedup)

---

### 3. Parallel Models + Chaining

Multiple spans at a turn. User selects. Chain continues from selection.

```
Turn 3 (assistant):
  ├── Span A (claude) ← selected
  │     └── [thinking] → [tool_call] → [result] → [response]
  ├── Span B (gpt-4)
  │     └── [response]
  └── Span C (gemini)
        └── [tool_call] → [result] → [response]

Turn 4 continues from Span A's context
```

**Structure:** Multiple spans at turn. Each span contains a sequence of messages. View selection determines path.

**Operations:**
- `add_span(turn, model)` → generate with model
- `select(view, turn, span)` → choose winner
- Selection change = context change for subsequent turns

**UI consideration:**
- Short spans → tabs inline
- Many/long spans → dropdown or separate view

---

### 4. Fork Conversation

Branch from any point. Paths diverge independently.

```
Original: T1 → T2 → T3 → T4 → T5
                    │
                    ▼
Forked:   T1 → T2 → T3 → F4 → F5
          (shared)    (new turns)
```

**Structure:** New view sharing turns up to fork point. New turns after.

**Operations:**
- `fork(view, turn)` → new view
- Turns 1-3 shared (same spans selected)
- Turn 4+ are new turns in conversation

**UI consideration:**
- Show fork relationship in conversation list
- Breadcrumb: "Forked from [Original] at message 3"
- Lineage view: tree of related conversations

---

### 5. Edit & Splice

Edit a turn. Optionally keep subsequent turns from original.

```
Original: T1 → T2 → T3 → T4 → T5
                    │
                    ▼
Edited:   T1 → T2 → T3' → T4 → T5
               (new span)  (reused!)
```

**Key insight:** This is NOT a fork. It's:
1. New span at turn 3
2. New view selecting: [span1, span1, span_new, span1, span1]

The original T4, T5 are reused because spans are shared across views.

**Operations:**
- `add_span(turn_3, edited_content)`
- `create_view(selections)` with mix of original and new spans

**Constraint:** Reusing T4/T5 only makes sense if they don't depend on T3's specific content. May need to regenerate.

---

### 6. Versioned Documents

Linear revision history with optional branching.

```
Doc: v1 → v2 → v3 (current)
           │
           └→ v2a → v2b (branch)
```

**Structure:** Version chain. Each revision → ContentBlock.

**Operations:**
- `commit(content)` → new revision
- `branch(revision)` → new revision from old point
- `checkout(revision)` → move current pointer

---

### 7. Cross-Reference

Any entity can reference any other entity. References are first-class.

```
Referenceable entities:
  - ContentBlock
  - Document (or specific Revision)
  - Conversation (or specific Turn/Span)
  - Collection (or specific Item)

Examples:
  - Message references Document → RAG grounding
  - Message references another Conversation → "as discussed in..."
  - Document references Conversation → "generated from chat"
  - Collection item references anything → organization
  - ContentBlock used in multiple places → deduplication
```

**Reference types:**

| From | To | Use case |
|------|-----|----------|
| Message | Document | RAG, "summarize this doc" |
| Message | Conversation | "as we discussed in [chat]" |
| Message | ContentBlock | Inline content, images |
| Document | Conversation | "source: generated from [chat]" |
| Document | Document | "see also", linked docs |
| Collection Item | Any | Organization, bookmarks |

**Operations:**
- `reference(from, to)` → create link
- `backlinks(entity)` → all entities referencing this one
- References resolve at render time (get current content)

**UI:**
- "Used in: [Conversation X], [Document Y], [Collection Z]"
- Hover to preview referenced content
- Click to navigate

---

### 8. Structured Data

Organize entities into trees/lists with metadata.

```
Collection "Research" (tree)
  ├── Folder "Papers"
  │     ├── Document "Paper A" [tags: ml, transformers]
  │     └── Document "Paper B" [tags: ml, rl]
  ├── Folder "Chats"
  │     └── Conversation "Discussion" [tags: ml]
  └── ContentBlock "Quick note"
```

**Structure:** Tree with ordering. Items reference any entity type.

**Operations:**
- `add_item(parent, position, target)`
- `move(item, new_parent, new_position)`
- `tag(item, tags)` → cross-cutting queries
- `set_fields(item, fields)` → table/kanban views

**Queries:**
- "All items tagged 'ml'" → across collections
- "Contents of Papers folder" → tree traversal
- "Kanban by status" → group by field

---

## UI Considerations

### Same Data, Different Views

| Context | Appropriate View |
|---------|------------------|
| Few short spans | Tabs inline |
| Many/long spans | List with previews |
| Forked conversations | Tree showing lineage |
| Subagent work | Collapsed summary, expandable |
| Edit history at position | "Edited" badge, hover for original |

### Navigation Needs

- **Conversation list:** Show fork relationships, group by lineage
- **Conversation detail:** Linear view with span indicators
- **Lineage view:** Tree of related conversations
- **Search:** Across all content, grouped by structure type

---

## Open Questions

1. **Regeneration on splice:** If T4 depends on T3, does editing T3 invalidate T4?
2. **Context inheritance:** How much parent context does subagent see?
3. **Approval workflow:** How does supervised agent communication flow?
4. **GC:** When is content orphaned?
5. **Many spans:** When do tabs become unwieldy?
6. **Span boundaries:** When does a new message start vs continue same span?

---

## Summary

| Structure | Core abstraction | Key operation |
|-----------|------------------|---------------|
| Conversation | Turns + spans + views | View selects path through spans |
| Document | Revision chain | Commit creates version |
| Collection | Tree + ordering + tags | Items reference anything |
| Content | Immutable blocks | Shared across structures |
| Links | Cross-references | Connect any entities |

### Spans Contain Multiple Messages

The key insight for conversations: a **span is a sequence of messages**, not a single message. This handles:
- Tool call iterations (model does N tool calls before responding)
- Subagent work (spawn → child messages → summary)
- Thinking/reasoning chains (thinking → response)

```
Span {
    id: SpanId
    turn_id: TurnId
    model_id: Option<ModelId>
    messages: [Message]           // ordered sequence of messages
    child_conversations: [ConversationRef]  // if spawned subagents
}
```

---

## Feature Requirements

Detailed implementation requirements derived from use cases and ROADMAP features.

---

### FR-0: Addressable Layer (Entity System)

**Use Cases:** All - provides unified identity and relationships

| ID | Requirement | Priority |
|----|-------------|----------|
| FR-0.1 | All addressable things (views, documents, assets) are entities | P0 |
| FR-0.2 | Entities have: id, type, name, slug, user, privacy, archive status | P0 |
| FR-0.3 | Slug enables @mentions (`@project-planning`) | P1 |
| FR-0.4 | Entity relations track: forked_from, references, derived_from | P0 |
| FR-0.5 | Deleting entity doesn't cascade to related entities | P0 |
| FR-0.6 | Views replace conversations - view IS the conversation | P0 |

**Schema:**

```sql
CREATE TABLE entities (
    id TEXT PRIMARY KEY,
    entity_type TEXT NOT NULL,     -- 'view', 'document', 'asset'
    user_id TEXT REFERENCES users(id),
    name TEXT,
    slug TEXT UNIQUE,              -- for @mentions
    is_private INTEGER DEFAULT 0,
    is_archived INTEGER DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE INDEX idx_entities_user ON entities(user_id);
CREATE INDEX idx_entities_type ON entities(entity_type, user_id);
CREATE INDEX idx_entities_slug ON entities(slug) WHERE slug IS NOT NULL;

CREATE TABLE entity_relations (
    from_id TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    to_id TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    relation TEXT NOT NULL,        -- 'forked_from', 'references', 'derived_from'
    metadata TEXT,                 -- JSON (e.g., {at_turn_id: "..."})
    created_at INTEGER NOT NULL,
    PRIMARY KEY (from_id, to_id, relation)
);

CREATE INDEX idx_entity_relations_to ON entity_relations(to_id, relation);
```

**Operations:**

```rust
trait EntityStore {
    // CRUD
    async fn create_entity(&self, entity_type: &str, user_id: &UserId) -> Result<EntityId>;
    async fn get_entity(&self, id: &EntityId) -> Result<Option<StoredEntity>>;
    async fn list_entities(&self, user_id: &UserId, entity_type: Option<&str>) -> Result<Vec<StoredEntity>>;
    async fn update_entity(&self, id: &EntityId, updates: EntityUpdate) -> Result<()>;
    async fn archive_entity(&self, id: &EntityId) -> Result<()>;
    async fn delete_entity(&self, id: &EntityId) -> Result<()>;

    // Relations
    async fn add_relation(&self, from: &EntityId, to: &EntityId, relation: &str, metadata: Option<Value>) -> Result<()>;
    async fn get_relations_from(&self, id: &EntityId, relation: Option<&str>) -> Result<Vec<EntityRelation>>;
    async fn get_relations_to(&self, id: &EntityId, relation: Option<&str>) -> Result<Vec<EntityRelation>>;
    async fn remove_relation(&self, from: &EntityId, to: &EntityId, relation: &str) -> Result<()>;
}
```

**Acceptance Criteria:**
- [ ] Create entity, get back ID
- [ ] Set slug, lookup by @slug
- [ ] Add forked_from relation between views
- [ ] Delete view without affecting forked views
- [ ] List all views for user (replaces list_conversations)

---

### FR-1: Content Storage

**Use Cases:** All

| ID | Requirement | Priority |
|----|-------------|----------|
| FR-1.1 | ContentBlocks have unique IDs (UUID), hash stored for integrity | P0 |
| FR-1.2 | Store content_type, text, origin metadata | P0 |
| FR-1.3 | Origin tracks: kind, user_id, model_id, source_id, parent_content_id | P0 |
| FR-1.4 | ContentBlocks NOT deduplicated - each block unique for provenance | P0 |
| FR-1.5 | Assets stored separately in BlobStore (content-addressed, deduplicated) | P0 |
| FR-1.6 | Full-text search across ContentBlocks | P1 |

**Note:** ContentBlocks are NOT deduplicated because identical text may have different origins, timestamps, and privacy settings. The hash is computed for integrity checking only.

**Schema:**

```sql
CREATE TABLE content_blocks (
    id TEXT PRIMARY KEY,           -- UUID (unique per block)
    content_hash TEXT NOT NULL,    -- SHA-256 of text (for integrity)
    content_type TEXT NOT NULL,    -- text/plain, text/markdown, text/typst
    text TEXT NOT NULL,
    origin_kind TEXT NOT NULL,     -- user, assistant, system, import
    origin_user_id TEXT,
    origin_model_id TEXT,
    origin_source_id TEXT,
    origin_parent_id TEXT,
    is_private INTEGER DEFAULT 0,
    created_at INTEGER NOT NULL
);

CREATE TABLE assets (
    id TEXT PRIMARY KEY,           -- UUID
    blob_hash TEXT NOT NULL,       -- SHA-256 of bytes (for dedup in blob store)
    mime_type TEXT NOT NULL,
    filename TEXT,
    size_bytes INTEGER NOT NULL,
    created_at INTEGER NOT NULL
);
-- Actual bytes stored in BlobStore (content-addressed)
```

**Acceptance Criteria:**
- Create ContentBlock, get back unique ID
- Hash computed and stored for integrity
- Store and retrieve assets
- Full-text search returns matching ContentBlocks

---

### FR-2: View/Conversation Structure

**Use Cases:** 1, 2, 3, 4, 5 (subagent, agent↔agent, parallel, fork, splice)

| ID | Requirement | Priority |
|----|-------------|----------|
| FR-2.1 | Views are entities (addressable, named) | P0 |
| FR-2.2 | Views contain ordered turns via view_selections | P0 |
| FR-2.3 | Each turn has role (user/assistant) and one or more spans | P0 |
| FR-2.4 | Each span contains a sequence of messages | P0 |
| FR-2.5 | Messages reference ContentBlock for text | P0 |
| FR-2.6 | Views select one span per turn | P0 |
| FR-2.7 | Spans are shared across views | P0 |
| FR-2.8 | Fork creates new view entity with 'forked_from' relation | P0 |
| FR-2.9 | Views replace conversations - no separate conversations table | P0 |
| FR-2.10 | Spawn child creates new view inheriting parent context | P1 |

**Schema:**

```sql
-- Addressable layer (see FR-0)
-- entities table provides: id, name, user_id, slug, is_private, is_archived
-- entity_relations provides: forked_from relationships

-- Views reference entities (view.id = entity.id)
CREATE TABLE views (
    id TEXT PRIMARY KEY REFERENCES entities(id) ON DELETE CASCADE
    -- All metadata (name, user, privacy) lives in entities table
);

-- Turns are standalone (not owned by conversations)
CREATE TABLE turns (
    id TEXT PRIMARY KEY,
    role TEXT NOT NULL,            -- user, assistant
    created_at INTEGER NOT NULL
);

CREATE TABLE spans (
    id TEXT PRIMARY KEY,
    turn_id TEXT NOT NULL REFERENCES turns(id),
    model_id TEXT,
    created_at INTEGER NOT NULL
);

CREATE TABLE messages (
    id TEXT PRIMARY KEY,
    span_id TEXT NOT NULL REFERENCES spans(id),
    sequence_number INTEGER NOT NULL,
    role TEXT NOT NULL,            -- user, assistant, system, tool
    created_at INTEGER NOT NULL
);

CREATE TABLE message_content (
    id TEXT PRIMARY KEY,
    message_id TEXT NOT NULL REFERENCES messages(id),
    sequence_number INTEGER NOT NULL,
    content_type TEXT NOT NULL,    -- text, asset_ref, document_ref, tool_call, tool_result
    content_block_id TEXT,         -- for text
    asset_id TEXT,                 -- for asset_ref
    document_id TEXT,              -- for document_ref
    tool_data TEXT                 -- JSON for tool_call/tool_result
);

CREATE TABLE view_selections (
    view_id TEXT NOT NULL REFERENCES views(id) ON DELETE CASCADE,
    turn_id TEXT NOT NULL REFERENCES turns(id),
    span_id TEXT NOT NULL REFERENCES spans(id),
    sequence_number INTEGER NOT NULL,  -- order of turns in this view
    PRIMARY KEY (view_id, turn_id)
);

-- Subagent spawning (future)
CREATE TABLE view_children (
    parent_span_id TEXT NOT NULL REFERENCES spans(id),
    child_view_id TEXT NOT NULL REFERENCES views(id),
    spawn_position INTEGER NOT NULL,
    PRIMARY KEY (parent_span_id, child_view_id)
);
```

**Operations:**

```rust
trait ConversationStore {
    // Turn management
    fn add_turn(&self, conversation_id: &str, role: Role) -> Result<Turn>;
    fn get_turns(&self, conversation_id: &str) -> Result<Vec<Turn>>;

    // Span management
    fn add_span(&self, turn_id: &str, model_id: Option<&str>) -> Result<Span>;
    fn add_message(&self, span_id: &str, message: NewMessage) -> Result<Message>;
    fn get_messages(&self, span_id: &str) -> Result<Vec<Message>>;

    // View management
    fn create_view(&self, conversation_id: &str, name: Option<&str>) -> Result<View>;
    fn fork_view(&self, view_id: &str, at_turn_id: &str) -> Result<View>;
    fn select_span(&self, view_id: &str, turn_id: &str, span_id: &str) -> Result<()>;
    fn get_view_path(&self, view_id: &str) -> Result<Vec<(Turn, Span, Vec<Message>)>>;

    // Subagent
    fn spawn_child(&self, parent_span_id: &str, position: i32) -> Result<Conversation>;
    fn get_inherited_context(&self, child_id: &str) -> Result<Vec<Message>>;
}
```

**Acceptance Criteria:**
- [ ] Create conversation with turns and spans
- [ ] Span contains multiple messages
- [ ] Different spans at same turn have different message counts
- [ ] Views select path through spans
- [ ] Fork shares prior selections, diverges after
- [ ] Spawn child inherits parent context

---

### FR-3: Document Structure

**Use Cases:** 6, 7 (versioned documents, cross-reference)

| ID | Requirement | Priority |
|----|-------------|----------|
| FR-3.1 | Documents have ordered revisions (DAG) | P0 |
| FR-3.2 | Each revision references a ContentBlock | P0 |
| FR-3.3 | Current revision pointer tracks head | P0 |
| FR-3.4 | Branch creates revision with different parent | P1 |
| FR-3.5 | Documents referenceable from conversations/collections | P0 |

**Schema:**

```sql
CREATE TABLE documents (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    current_revision_id TEXT,
    source TEXT NOT NULL,          -- user_created, ai_generated, google_drive, import
    source_id TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE revisions (
    id TEXT PRIMARY KEY,
    document_id TEXT NOT NULL,
    content_id TEXT NOT NULL,
    parent_revision_id TEXT,
    revision_number INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    FOREIGN KEY (document_id) REFERENCES documents(id),
    FOREIGN KEY (content_id) REFERENCES content_blocks(id)
);
```

**Acceptance Criteria:**
- [ ] Create document with initial content
- [ ] Commit creates new revision
- [ ] Branch from non-head revision
- [ ] Checkout moves current pointer
- [ ] Diff between revisions

---

### FR-4: Collection Structure

**Use Cases:** 8 (structured data)

| ID | Requirement | Priority |
|----|-------------|----------|
| FR-4.1 | Collections contain items in tree structure | P0 |
| FR-4.2 | Items reference any entity type | P0 |
| FR-4.3 | Items have position within parent | P0 |
| FR-4.4 | Items can have tags | P1 |
| FR-4.5 | Items can have typed fields | P1 |
| FR-4.6 | Schema defines field types for database collections | P2 |

**Schema:**

```sql
CREATE TABLE collections (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    schema_id TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE collection_items (
    id TEXT PRIMARY KEY,
    collection_id TEXT NOT NULL,
    parent_item_id TEXT,
    position INTEGER NOT NULL,
    target_type TEXT NOT NULL,     -- content, document, conversation, collection
    target_id TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE TABLE item_tags (
    item_id TEXT NOT NULL,
    tag TEXT NOT NULL,
    PRIMARY KEY (item_id, tag)
);

CREATE TABLE item_fields (
    item_id TEXT NOT NULL,
    field_name TEXT NOT NULL,
    field_value TEXT NOT NULL,     -- JSON
    PRIMARY KEY (item_id, field_name)
);
```

**Acceptance Criteria:**
- [ ] Create collection with tree of items
- [ ] Items reference various entity types
- [ ] Move items, reorder
- [ ] Tag and query by tag
- [ ] Set/get typed fields

---

### FR-5: Cross-References

**Use Cases:** 7 (cross-reference), all

| ID | Requirement | Priority |
|----|-------------|----------|
| FR-5.1 | Any entity can reference any other entity | P0 |
| FR-5.2 | References have optional relation type | P1 |
| FR-5.3 | Backlinks auto-computed | P1 |
| FR-5.4 | Inline `[[type:id]]` syntax parsed | P2 |

**Schema:**

```sql
CREATE TABLE references (
    id TEXT PRIMARY KEY,
    from_type TEXT NOT NULL,
    from_id TEXT NOT NULL,
    to_type TEXT NOT NULL,
    to_id TEXT NOT NULL,
    relation_type TEXT,
    created_at INTEGER NOT NULL,
    UNIQUE (from_type, from_id, to_type, to_id, relation_type)
);

CREATE INDEX idx_references_from ON references(from_type, from_id);
CREATE INDEX idx_references_to ON references(to_type, to_id);
```

**Acceptance Criteria:**
- [ ] Create reference between entities
- [ ] Query outgoing references
- [ ] Query incoming references (backlinks)
- [ ] Parse inline reference syntax

---

### FR-6: Views and Queries

**Use Cases:** 8, navigation

| ID | Requirement | Priority |
|----|-------------|----------|
| FR-6.1 | List view: flat, sortable, filterable | P0 |
| FR-6.2 | Tree view: hierarchical navigation | P0 |
| FR-6.3 | Table view: columns from fields | P1 |
| FR-6.4 | Board view: grouped by field (kanban) | P2 |
| FR-6.5 | Query by type, tag, field, date | P1 |

**Acceptance Criteria:**
- [ ] List view with sort/filter
- [ ] Tree view for hierarchy
- [ ] Basic query parsing
- [ ] Filter by type, tag, field

---

### FR-7: Agent Context

**Use Cases:** 1, 2 (subagent, agent↔agent)

| ID | Requirement | Priority |
|----|-------------|----------|
| FR-7.1 | Agent templates define system prompt, context sources | P1 |
| FR-7.2 | Context from static nodes or queries | P1 |
| FR-7.3 | Template variables expanded at runtime | P2 |
| FR-7.4 | Sub-agents inherit scoped parent context | P2 |

**Schema:**

```sql
CREATE TABLE agent_templates (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    system_prompt TEXT NOT NULL,
    context_spec TEXT NOT NULL,    -- JSON
    tools TEXT,                    -- JSON
    created_at INTEGER NOT NULL
);
```

**Acceptance Criteria:**
- [ ] Define agent with system prompt and context
- [ ] Expand template variables
- [ ] Context injection from nodes/queries

---

### FR-8: Import/Export

**Use Cases:** Data portability

| ID | Requirement | Priority |
|----|-------------|----------|
| FR-8.1 | Export entity to JSON | P1 |
| FR-8.2 | Export document to Markdown | P1 |
| FR-8.3 | Export conversation to Markdown | P1 |
| FR-8.4 | Import from JSON | P1 |
| FR-8.5 | Import markdown files | P2 |

**Acceptance Criteria:**
- [ ] Export entity with all metadata
- [ ] Markdown export for documents/conversations
- [ ] Import restores entities

---

## Extension Points

UCM provides hooks for future systems (temporality, dynamic content, automation) without coupling to specific implementations. See [HOOK_SYSTEM.md](HOOK_SYSTEM.md) for the full design.

### EP-1: Event Emission

UCM emits events after entity lifecycle operations. Events are logged as ContentBlocks.

| Operation | Event Type |
|-----------|------------|
| Create entity | `entity.created.{type}` |
| Update entity | `entity.updated.{type}` |
| Delete entity | `entity.deleted.{type}` |

**Schema addition:**

```sql
CREATE TABLE events (
    id TEXT PRIMARY KEY,
    event_type TEXT NOT NULL,           -- Extensible string
    payload_content_id TEXT,            -- ContentBlock: event details
    source_entity_type TEXT,
    source_entity_id TEXT,
    timestamp INTEGER NOT NULL,
    FOREIGN KEY (payload_content_id) REFERENCES content_blocks(id)
);

CREATE INDEX idx_events_type_time ON events(event_type, timestamp);
```

**Integration:** Every `Store` trait method that mutates data calls `emit_event()` after success.

### EP-2: Temporal Indexing

All entities have `created_at` and `updated_at` timestamps. Indexes support time-range queries.

```sql
CREATE INDEX idx_messages_created ON messages(created_at);
CREATE INDEX idx_content_blocks_created ON content_blocks(created_at);
CREATE INDEX idx_messages_conv_created ON messages(conversation_id, created_at);
```

**Integration:** Query methods accept optional `TemporalQuery { after, before, limit }`.

### EP-3: Hook Registry

Hooks bind event patterns to actions. Both are ContentBlocks.

```sql
CREATE TABLE hooks (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    pattern_content_id TEXT NOT NULL,   -- ContentBlock: event pattern
    action_content_id TEXT NOT NULL,    -- ContentBlock: action spec
    priority INTEGER DEFAULT 0,
    enabled BOOLEAN DEFAULT TRUE,
    created_at INTEGER NOT NULL,
    FOREIGN KEY (pattern_content_id) REFERENCES content_blocks(id),
    FOREIGN KEY (action_content_id) REFERENCES content_blocks(id)
);
```

**Integration:** Hook engine queries registry on each event, matches patterns, executes actions.

### EP-4: Dynamic Content Flag

ContentBlocks can be marked as containing evaluatable expressions.

```sql
ALTER TABLE content_blocks ADD COLUMN is_dynamic BOOLEAN DEFAULT FALSE;
```

**Integration:** Render pipeline checks `is_dynamic` and invokes evaluator before display/LLM injection.

### EP-5: Context Strategy

Views can reference a context strategy for building LLM context from history.

```sql
ALTER TABLE views ADD COLUMN context_strategy_id TEXT;
```

**Integration:** `get_view_context(view_id, budget)` applies strategy to compress/summarize history.

---

## Concept Summary

| Structure | Core Abstraction | Key Operation |
|-----------|------------------|---------------|
| Entity | Addressable identity with relations | @mention, fork ancestry |
| View | Path through turns/spans | Select span at turn |
| Turn | Position in conversation | Add span alternatives |
| Span | One response (sequence of messages) | Compare, select |
| Document | Revision chain | Commit, checkout |
| Collection | Tree of references | Organize anything |
| ContentBlock | Immutable text with origin | Store once, reference many |
| Asset | Binary blob (deduplicated) | Content-addressed storage |
