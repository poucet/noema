# Unified Content Model

**Status:** Draft
**Created:** 2026-01-10
**Related:** IDEAS #1, #2, #3

---

## Target Use Cases

| # | Use Case | Description |
|---|----------|-------------|
| 1 | Agent calling tool | Agent invokes tool, gets result, continues |
| 2 | Agent → subagent | Parent spawns child with scoped context, gets summary back |
| 3 | Agent ↔ agent (supervised) | Two agents communicate, human approves cross-messages |
| 4 | Parallel models + chaining | Multiple models respond, user selects, chain continues |
| 5 | Fork conversation | Branch from any point, paths diverge |
| 6 | Edit & splice | Edit mid-conversation, optionally keep subsequent messages |
| 7 | Versioned documents | Markdown/typst docs with revision history |
| 8 | Cross-reference | Same content appears in conversation AND as document |
| 9 | Structured data | Ordered lists, trees, tagged items, table views |

---

## Core Principle

**Separate content (heavy, immutable) from structure (lightweight, mutable).**

```
┌─────────────────────────────────────────────────────────┐
│                    CONTENT LAYER                        │
│  (immutable, deduplicated, content-addressed)           │
│                                                         │
│  ContentBlock: id, body, content_type, origin           │
└─────────────────────────────────────────────────────────┘
                           ▲
                           │ references
                           │
┌─────────────────────────────────────────────────────────┐
│                   STRUCTURE LAYER                       │
│  (mutable, cheap to fork, defines paths)                │
│                                                         │
│  Conversations: Thread → SpanSet → Span → Message       │
│  Documents: Document → Revision                         │
└─────────────────────────────────────────────────────────┘
```

---

## Content Layer

### ContentBlock

Universal content primitive. All text, images, documents are content blocks.

```
ContentBlock {
    id: ContentHash,           // SHA-256 of body
    content_type: String,      // "text/markdown", "image/png", etc.
    body: Bytes,
    origin: ContentOrigin,
    created_at: Timestamp,
}

ContentOrigin {
    kind: user | assistant | system | import | tool,
    user_id: Option<UserId>,
    model_id: Option<ModelId>,
    source_id: Option<String>,           // external ID (google doc, url)
    parent_content_id: Option<ContentHash>,  // if edited/derived
}
```

**Benefits:**
- Deduplication (same content = same hash = stored once)
- Cross-referencing (same content in conversation AND document)
- Lineage tracking (who created, from what)
- Unified search/RAG across all content

---

## Structure Layer: Conversations

### Graph Structure

```
Conversation
  └── Thread (a path through span-space)
        └── SpanSet (position in sequence, can have alternatives)
              └── Span (one alternative at this position)
                    └── Message (references ContentBlock)
```

### Key Insight: Spans are Shareable

A Thread doesn't own spans—it **selects** them. Multiple threads can select the same span.

```
Thread A: [span1] → [span2] → [span3] → [span4]
                              ↗
Thread B: [span1] → [span2] → [span5] → [span4]  ← reuses span4!
```

This enables **splice** (use case #6): edit position 3, but keep position 4 from original.

### Schema

```sql
-- Threads define paths
CREATE TABLE threads (
    id TEXT PRIMARY KEY,
    conversation_id TEXT REFERENCES conversations(id),
    parent_thread_id TEXT REFERENCES threads(id),  -- for subagents
    fork_span_id TEXT REFERENCES spans(id),        -- where we forked from
    name TEXT,
    created_at INTEGER NOT NULL
);

-- SpanSets are positions that can have alternatives
CREATE TABLE span_sets (
    id TEXT PRIMARY KEY,
    conversation_id TEXT REFERENCES conversations(id),
    sequence_number INTEGER NOT NULL,  -- position in conversation
    span_type TEXT NOT NULL,           -- 'user' or 'assistant'
    created_at INTEGER NOT NULL
);

-- Spans are alternatives at a position
CREATE TABLE spans (
    id TEXT PRIMARY KEY,
    span_set_id TEXT REFERENCES span_sets(id),
    model_id TEXT,                     -- which model (if assistant)
    created_at INTEGER NOT NULL
);

-- Thread selections: which span each thread uses at each position
CREATE TABLE thread_span_selections (
    thread_id TEXT REFERENCES threads(id),
    span_set_id TEXT REFERENCES span_sets(id),
    span_id TEXT REFERENCES spans(id),
    PRIMARY KEY (thread_id, span_set_id)
);

-- Messages reference content
CREATE TABLE messages (
    id TEXT PRIMARY KEY,
    span_id TEXT REFERENCES spans(id),
    sequence_number INTEGER NOT NULL,
    role TEXT NOT NULL,
    content_id TEXT REFERENCES content_blocks(id),
    created_at INTEGER NOT NULL
);
```

---

## Structure Layer: Documents

Documents have revision history, like conversations have span alternatives.

```
Document
  └── Revision (version of document content)
        └── references ContentBlock
```

### Schema

```sql
CREATE TABLE documents (
    id TEXT PRIMARY KEY,
    user_id TEXT REFERENCES users(id),
    title TEXT NOT NULL,
    document_type TEXT NOT NULL,       -- 'markdown', 'typst', etc.
    source TEXT NOT NULL,              -- 'user', 'ai', 'import'
    source_id TEXT,                    -- external ID if imported
    current_revision_id TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE document_revisions (
    id TEXT PRIMARY KEY,
    document_id TEXT REFERENCES documents(id),
    revision_number INTEGER NOT NULL,
    parent_revision_id TEXT REFERENCES document_revisions(id),
    content_id TEXT REFERENCES content_blocks(id),
    created_at INTEGER NOT NULL,
    created_by TEXT NOT NULL           -- user_id or model_id
);
```

---

## Structure Layer: Collections (Structured Data)

Collections provide structure over content: ordered lists, trees, tagged items.

```
Collection
  └── CollectionItem (ordered, can be nested)
        └── references ContentBlock (or another Collection)
```

### Use Cases

| Structure | Example | How it works |
|-----------|---------|--------------|
| Ordered list | Task list, bookmarks | Items with `position` |
| Tree | Folder hierarchy, outline | Items with `parent_item_id` |
| Tagged items | Labeled conversations | Items have tags |
| Table view | Kanban, spreadsheet | Items + columns as tags/fields |

### Schema

```sql
CREATE TABLE collections (
    id TEXT PRIMARY KEY,
    user_id TEXT REFERENCES users(id),
    name TEXT NOT NULL,
    collection_type TEXT NOT NULL,     -- 'list', 'tree', 'tagged', 'table'
    schema_json TEXT,                  -- defines columns/fields for table type
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE collection_items (
    id TEXT PRIMARY KEY,
    collection_id TEXT REFERENCES collections(id),
    parent_item_id TEXT REFERENCES collection_items(id),  -- for trees
    position INTEGER NOT NULL,         -- ordering within parent

    -- What this item points to (one of these)
    content_id TEXT REFERENCES content_blocks(id),
    document_id TEXT REFERENCES documents(id),
    conversation_id TEXT REFERENCES conversations(id),
    subcollection_id TEXT REFERENCES collections(id),

    -- Structured fields (for table views)
    fields_json TEXT,                  -- {"status": "done", "priority": 1}

    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE collection_item_tags (
    item_id TEXT REFERENCES collection_items(id),
    tag TEXT NOT NULL,
    PRIMARY KEY (item_id, tag)
);

CREATE INDEX idx_collection_items_parent ON collection_items(collection_id, parent_item_id, position);
CREATE INDEX idx_collection_item_tags ON collection_item_tags(tag);
```

### Examples

**Ordered task list:**
```
Collection "My Tasks" (type: list)
  ├── Item 1 (position: 0) → ContentBlock "Buy groceries"
  ├── Item 2 (position: 1) → ContentBlock "Review PR"
  └── Item 3 (position: 2) → ContentBlock "Write docs"
```

**Tree (folder hierarchy):**
```
Collection "Projects" (type: tree)
  ├── Item "Work" (parent: null, position: 0)
  │     ├── Item "Noema" (parent: Work, position: 0) → Conversation
  │     └── Item "Other" (parent: Work, position: 1) → Document
  └── Item "Personal" (parent: null, position: 1)
        └── Item "Notes" (parent: Personal, position: 0) → Document
```

**Tagged items:**
```
Collection "Bookmarks" (type: tagged)
  ├── Item → Conversation "AI chat" [tags: ai, research]
  ├── Item → Document "Meeting notes" [tags: work, meetings]
  └── Item → ContentBlock "Quote" [tags: quotes, ai]
```

**Table view (Kanban):**
```
Collection "Sprint Board" (type: table, schema: {status: enum, assignee: text})
  ├── Item → ContentBlock "Feature A" {status: "todo", assignee: "alice"}
  ├── Item → ContentBlock "Bug fix" {status: "in_progress", assignee: "bob"}
  └── Item → ContentBlock "Refactor" {status: "done", assignee: "alice"}
```

### Key Properties

1. **Items reference anything**: content, documents, conversations, or subcollections
2. **Nesting via parent_item_id**: trees of arbitrary depth
3. **Ordering via position**: stable sort within parent
4. **Tags for cross-cutting organization**: filter across collections
5. **Fields for structured data**: table views, kanban boards

---

## Use Case Implementations

### 1. Agent Calling Tool

Normal flow. Span contains:
- Assistant message with `tool_use`
- Tool message with `tool_result`

```
SpanSet[5] → Span[A] → [assistant: tool_use, tool: tool_result]
```

No special structure needed.

### 2. Agent → Subagent

Parent spawns child thread with scoped context.

```
Parent Thread: [1] → [2] → [3] → [4:spawn] → [5:summary]
                              ↓
Child Thread:  [3] → [child work...] → [result]
                     (forked from span 3)
```

```sql
-- Child thread
INSERT INTO threads (parent_thread_id, fork_span_id, ...)
-- Child inherits selections up to fork point
-- Child result summarized back to parent at position 5
```

### 3. Agent ↔ Agent (Supervised)

Two threads, human approves message passing.

```
Agent A Thread: [1] → [2] → [3:msg_to_B] → [6:msg_from_B] → ...
Agent B Thread: [4:msg_from_A] → [5:msg_to_A] → ...
```

Messages cross-reference via shared ContentBlocks. Human approves before insertion.

### 4. Parallel Models + Chaining

Multiple spans at same position.

```
SpanSet[3]:
  ├── Span[A] (claude) ← selected
  ├── Span[B] (gpt-4)
  └── Span[C] (gemini)

Thread selects Span[A], continues from there.
```

```sql
-- User changes selection
UPDATE thread_span_selections
SET span_id = 'span_b'
WHERE thread_id = ? AND span_set_id = ?;
```

### 5. Fork Conversation

New thread from any span.

```
Original: [1] → [2] → [3] → [4] → [5]
                       ↓
Forked:   [1] → [2] → [3] → [6] → [7]  (shares 1,2,3)
```

```sql
INSERT INTO threads (fork_span_id = 'span_3', ...);
-- Copy selections for positions 1-3
INSERT INTO thread_span_selections
SELECT new_thread_id, span_set_id, span_id
FROM thread_span_selections
WHERE thread_id = original AND sequence_number <= 3;
```

### 6. Edit & Splice

Edit position 3, keep positions 4-5 from original.

```
Original: [1] → [2] → [3] → [4] → [5]
                       ↓
Edited:   [1] → [2] → [3'] → [4] → [5]  (3' is new, 4,5 reused)
```

```sql
-- Create new span at position 3
INSERT INTO spans (span_set_id = span_set_3, ...);
-- Create new thread with edited selection
INSERT INTO thread_span_selections VALUES
  (new_thread, span_set_1, span_1),
  (new_thread, span_set_2, span_2),
  (new_thread, span_set_3, span_3_new),  -- edited
  (new_thread, span_set_4, span_4),      -- reused!
  (new_thread, span_set_5, span_5);      -- reused!
```

### 7. Versioned Documents

Document with revision history.

```
Document "notes.md"
  ├── Revision 1 → ContentBlock[abc...]
  ├── Revision 2 → ContentBlock[def...] (parent: rev1)
  └── Revision 3 → ContentBlock[ghi...] (parent: rev2)
```

### 8. Cross-Reference

Same content in conversation AND document.

```
ContentBlock[xyz...] "Meeting summary"
  ↑
  ├── Message in Conversation (role: assistant)
  └── DocumentRevision in "meeting-notes.md"
```

One storage location, multiple usages.

### 9. Structured Data

Organize any content into lists, trees, or tables.

```
Collection "Research" (type: tree)
  ├── Item "Papers" (folder)
  │     ├── Item → Document "Paper A.md"
  │     └── Item → Document "Paper B.md"
  ├── Item "Conversations" (folder)
  │     ├── Item → Conversation "Chat about X" [tags: ml]
  │     └── Item → Conversation "Chat about Y" [tags: nlp]
  └── Item "Notes" (folder)
        └── Item → ContentBlock "Quick thought"
```

```sql
-- Create a project folder structure
INSERT INTO collections (id, name, collection_type) VALUES ('proj1', 'Research', 'tree');

-- Add folders
INSERT INTO collection_items (collection_id, parent_item_id, position)
VALUES ('proj1', NULL, 0);  -- "Papers" folder

-- Add document to folder
INSERT INTO collection_items (collection_id, parent_item_id, position, document_id)
VALUES ('proj1', 'papers_folder', 0, 'doc_paper_a');

-- Tag it
INSERT INTO collection_item_tags (item_id, tag) VALUES ('item1', 'ml');
```

**Queries:**
- "All items tagged 'ml'" → join on `collection_item_tags`
- "Contents of Papers folder" → filter by `parent_item_id`, order by `position`
- "Kanban by status" → group by `fields_json->>'status'`

---

## Summary of Changes from Current Schema

| Current | Proposed | Why |
|---------|----------|-----|
| `span_messages.content` (inline) | `messages.content_id` (reference) | Dedup, cross-ref |
| Thread owns spans | Thread selects spans | Sharing for splice |
| No thread_span_selections | Add `thread_span_selections` | Explicit path definition |
| Documents separate | Documents use `content_blocks` | Unified content |
| No collections | Add `collections`, `collection_items` | Lists, trees, tags, tables |

---

## Open Questions

1. **GC**: When is content orphaned? Reference counting vs. sweep?
2. **Large blobs**: Keep in BlobStore or unify with content_blocks?
3. **Indexing**: Single FTS across all content, or per-type?
4. **Migration**: Inline content → content_blocks extraction strategy?

---

## Migration Path

1. Add `content_blocks` table
2. Add `thread_span_selections` table
3. Migrate inline content to content_blocks
4. Update spans to reference via messages table
5. Migrate documents to use content_blocks
6. Add `collections`, `collection_items`, `collection_item_tags` tables

Each step backward-compatible.
