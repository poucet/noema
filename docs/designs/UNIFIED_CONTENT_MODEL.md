# Unified Content Model

**Status:** Revised — entity-first
**Created:** 2026-01-10
**Updated:** 2026-04-21
**Related:** IDEAS #1, #2, #3

---

## Three-Layer Architecture

```
┌───────────────────────────────────────────────────────────────────┐
│                      ADDRESSABLE LAYER                            │
│  Every addressable thing is an entity. Hierarchy, ordering, and   │
│  cross-references live in entity_relations.                       │
│  - entities (id, type, name, slug, user, privacy, archive,        │
│              content_block_id, origin, metadata)                  │
│  - entity_relations (from, to, relation, position, metadata)      │
│  - entity_assets (entity_id, asset_id)  — image/asset GC joins    │
└───────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌───────────────────────────────────────────────────────────────────┐
│                      STRUCTURE LAYER                              │
│  There is no dedicated document/tab schema. Structure is an       │
│  interpretation of entities + their relations:                    │
│  - Conversations: views + view_selections + turns + spans         │
│  - Documents-with-tabs: entity composition —                      │
│      document::tabbed ── structure::contained_in ──> document::tab│
│  - Flat notes/todos/prompts: one entity, one content_block        │
│  - Directories: entity composition with `structure::contained_in` │
└───────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌───────────────────────────────────────────────────────────────────┐
│                       CONTENT LAYER                               │
│  One substrate for all text and binary payloads.                  │
│  - content_blocks (text with origin tracking; at most one per     │
│    entity, referenced via entities.content_block_id)              │
│  - assets + blobs (binary files; joined to entities via           │
│    entity_assets)                                                 │
└───────────────────────────────────────────────────────────────────┘
```

**One design principle.** The only part of the system that knows "a Google Doc becomes a `document::tabbed` entity with child `document::tab` entities linked by `structure::contained_in`" is the import skill. Everything else — daemon APIs, admin UI, Noema UI, RAG — deals in entities, relations, and content blocks. UI renders per entity capability (has content? has `structure::contained_in` children?), not per hardcoded type.

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
| 6 | Cross-reference | Same content appears in conversation AND as document |
| 7 | Structured data | Ordered lists, trees, tagged items, table views |

---

## Core Principle

**Separate identity (addressable, organizational) from content (the text/binary payload).** Structure is not a separate layer — it is the shape that emerges when you interpret an entity's relations.

An entity carries:
- **Identity**: id, type, name, slug, privacy, archive, user, timestamps.
- **Content**: at most one `content_block_id` (its markdown text, if any) plus its `entity_assets` (images it references).
- **Provenance**: a single `origin` string like `"google_drive:<gdoc_id>"`.

Everything else — tabs inside a document, files in a folder, citations across a knowledge graph — is expressed as `entity_relations` rows.

---

## Content Layer

Two storage types: **text content** (searchable, referenceable) and **binary assets** (opaque blobs). Every entity that carries text points at one `content_blocks` row via its own `content_block_id` column.

### ContentBlock (Text)

All textual content: messages, documents, tabs, notes, todos, prompts, structured text — everything flows through `content_blocks`. No entity stores its text inline; it references a block.

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

**At most one block per entity.** An entity holds a single `content_block_id` at a time. Updating the content creates a new block and swaps the pointer; the old block is orphaned and becomes a candidate for the orchestrator's GC.

**What goes in ContentBlock:**
- User messages (text)
- Assistant responses (text)
- Document content (markdown, typst)
- Imported documents (converted to text)
- Flat notes, todos, prompts, knowledge, system prompts, ...

**ContentBlock enables:**
- Full-text search across all text
- RAG (retrieve relevant content for context)
- Cross-referencing ("as I said in message X")
- Summarization (summarize any content block)

### Asset (Binary)

Binary content: images, audio, PDF, video. Stored in BlobStore (CAS) and tracked in the `assets` table. Entities that reference an asset (e.g. a tab's markdown includes an imported image) record the linkage in the `entity_assets` mapping:

```
entity_assets {
    entity_id: EntityId  → entities(id)
    asset_id:  AssetId   → assets(id)
    PRIMARY KEY (entity_id, asset_id)
}
```

This enables blob GC via a direct join: an asset with no `entity_assets` row is eligible for deletion.

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

Every addressable thing is an entity — conversations, documents, tabs, notes, todos, prompts, directories, labels, … The `entity_type` column is a namespaced string; new kinds are added without schema changes.

```sql
CREATE TABLE entities (
    id                TEXT PRIMARY KEY,
    entity_type       TEXT NOT NULL,   -- 'conversation', 'document::tabbed', 'document::note',
                                       -- 'document::tab', 'system::directory', 'system::label', ...
    user_id           TEXT REFERENCES users(id),
    name              TEXT,            -- display name / title
    is_private        INTEGER DEFAULT 0,
    content_block_id  TEXT REFERENCES content_blocks(id),  -- at most one block per entity
    origin            TEXT,            -- "<scheme>:<id>" e.g. "google_drive:abc123"; NULL for local
    metadata          TEXT,            -- JSON bag for type-specific extras (icon, etc.)
    created_at        INTEGER NOT NULL,
    updated_at        INTEGER NOT NULL
);

CREATE INDEX idx_entities_user         ON entities(user_id);
CREATE INDEX idx_entities_type         ON entities(entity_type, user_id);
CREATE INDEX idx_entities_origin       ON entities(user_id, origin) WHERE origin IS NOT NULL;
CREATE INDEX idx_entities_has_content  ON entities(content_block_id) WHERE content_block_id IS NOT NULL;
```

**Origin column.** `origin` is a single URI-like string that combines the source system and its id (`"google_drive:gdoc-abc123"`, `"ai_generated:msg-xyz"`). Well-known schemes live in a small `origin_scheme` constants module; new schemes are added without code changes. Lookups use either exact match (`WHERE origin = 'google_drive:gdoc-abc123'`) or prefix (`WHERE origin LIKE 'google_drive:%'`).

### Entity Relations

Relationships between entities — hierarchy, cross-references, grouping, tags, … all live in the same table. The `position` column gives ordered children first-class support (tab order, collection item order).

```sql
CREATE TABLE entity_relations (
    from_id      TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    to_id        TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    relation     TEXT NOT NULL,   -- 'structure::contained_in', 'reference::to', ...
    position     INTEGER,         -- optional; first-class order across siblings (e.g. tab_index)
    metadata     TEXT,            -- optional JSON for relation-specific extras (fork-point, citation context)
    PRIMARY KEY (from_id, to_id, relation)
);

CREATE INDEX idx_entity_relations_to            ON entity_relations(to_id, relation);
CREATE INDEX idx_entity_relations_ordered       ON entity_relations(to_id, relation, position);
```

The primary key is `(from_id, to_id, relation)`, so the same pair can coexist under multiple relation types (entity A can be both `structure::contained_in` B and `references` B — two rows, different semantics).

### Relation Types

Relation names are namespaced with `::` — same convention as entity types. The namespace groups relations by family (`structure::`, `reference::`, `label::`, `conversation::`, `collection::`), making it easy to add new relations within a family and grep for all uses of a family.

| Relation                          | Semantic                         | Cardinality (convention)  | Uses `position`        | Orchestrator cascades delete?  | UI |
|-----------------------------------|----------------------------------|---------------------------|------------------------|--------------------------------|-------------|
| `structure::contained_in`         | Child lives inside parent        | singular parent           | yes (sibling order)    | yes (folder → contents)        | tree nav |
| `reference::to`                   | A has a cross-reference to B     | many-to-many              | no                     | no                             | backlinks / graph |
| `label::tagged_with`              | Entity is tagged with a label    | many-to-many              | no                     | no                             | filter facets |
| `conversation::forked_from`       | View forked from another view    | many-to-many              | no                     | no                             | lineage |
| `conversation::spawned_from`      | Subconversation spawned from     | many-to-many              | no                     | no                             | parent nav |
| `collection::grouped_with`        | Manual grouping                  | many-to-many              | no                     | no                             | collections |

All of these live in the same `entity_relations` table. Structure emerges from how code and UI interpret them.

### Key Benefits

1. **Unified @mentions**: All entities addressable by slug
2. **Decoupled relationships**: Fork ancestry stored in relations, not on views
3. **Independent lifecycle**: Deleting one view doesn't affect its forks
4. **Flexible organization**: Tags, grouping without rigid hierarchy
5. **Consistent metadata**: name, privacy, archive across all types

### Namespaced entity types

Well-known types used by the system. New types can be added without schema changes — `entity_type` is a free-form string.

| Type | Shape | Notes |
|---|---|---|
| `conversation` | view + turns + spans + messages | Existing conversation substructure. |
| `document::tabbed` | entity with no content; children via `structure::contained_in` | Imported Google Docs, multi-section documents. |
| `document::note` | flat content | Free-form note. |
| `document::todo` | flat content | TODO list. |
| `document::prompt` | flat content | Reusable prompt / intent template. |
| `document::knowledge` | flat content | Knowledge-base entry. |
| `document::context` | flat content | Context document injected by agents. |
| `document::intent` | flat content | Event-compiled intent definition. |
| `document::system_prompt` | flat content | System prompt for a model. |
| `document::access_rule` | flat content | Access control rule document. |
| `document::tab` | entity with content; always a child via `structure::contained_in` of a `document::tabbed` or another `document::tab` | The only kind that has tabs. |
| `system::directory` | entity, no content; children via `structure::contained_in` | Filing/folder structure. |
| `system::label` | entity, no content; subjects point at it via `label::tagged_with` | Cross-cutting tags. |

Assets are **not** entities. They live in the `assets` table (with mime_type, blob_hash, size_bytes) and are referenced from entities via the `entity_assets` mapping. Making them entities would require a side-table FK with no benefit — the `assets` row already carries everything an asset needs.

Filter queries:
- "All my documents (any kind)" → `WHERE entity_type LIKE 'document::%'`.
- "All my notes specifically" → `WHERE entity_type = 'document::note'`.
- "Everything with text content" → `WHERE content_block_id IS NOT NULL`.
- "Everything imported from Google" → `WHERE origin LIKE 'google_drive:%'`.

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

Structure is emergent, not schematic — every shape the system supports is expressed as entities plus their `entity_relations`. There are no `documents` / `document_tabs` tables; there is no dedicated collection schema beyond entity relations. Keeping structure derived from the same two tables means directories, knowledge-graph edges, labels, tabs, and any future composition need no further migrations.

### How common shapes map to the model

| Shape | Entity types involved | Relations used |
|---|---|---|
| Conversation | `conversation` (view) + turns + spans + messages | `view_selections` + `forked_from` / `spawned_from` |
| Tabbed document | `document::tabbed` + `document::tab` | `structure::contained_in` (position = tab_index), nested tabs use the same relation |
| Flat note / todo / prompt | `document::note` / `::todo` / `::prompt` | content lives directly on the entity |
| Directory / folder | `system::directory` | `structure::contained_in` to a parent directory |
| Labels | `system::label` | `label::tagged_with` from the labelled entity |
| Knowledge graph | any kinds | `references` (many-to-many, may cycle) |
| Collection item | any kinds | `structure::contained_in` to a collection entity; `position` for order |

Conversations keep their own substructure (turns/spans/messages) because multi-model responses and view-selection semantics don't map cleanly onto flat relation rows. Everything else collapses to entities + relations.

---

## Worked examples

Concrete tables showing how the model expresses different shapes. Ids shortened (`e1`, `cb-1`, …) for readability.

### Example 1 — Tabbed document with nested tabs

```
entities
┌─────┬──────────────────┬────────────────────┬──────────────────┬────────────────────────────┐
│ id  │ entity_type      │ name               │ content_block_id │ origin                     │
├─────┼──────────────────┼────────────────────┼──────────────────┼────────────────────────────┤
│ e1  │ document::tabbed │ Project Plan       │ —                │ google_drive:gdoc-abc123   │
│ e2  │ document::tab     │ Overview           │ cb-1             │ —                          │
│ e3  │ document::tab     │ Timeline           │ cb-2             │ —                          │
│ e4  │ document::tab     │ Q1 Milestones      │ cb-3             │ —                          │
│ e5  │ document::tab     │ Q2 Milestones      │ cb-4             │ —                          │
└─────┴──────────────────┴────────────────────┴──────────────────┴────────────────────────────┘

entity_relations
┌────────┬─────────┬──────────────┬──────────┐
│ from   │ to      │ relation     │ position │
├────────┼─────────┼──────────────┼──────────┤
│ e2     │ e1      │ contained_in │ 0        │   Overview is tab #0 of Project Plan
│ e3     │ e1      │ contained_in │ 1        │   Timeline is tab #1
│ e4     │ e3      │ contained_in │ 0        │   Q1 Milestones nested under Timeline
│ e5     │ e3      │ contained_in │ 1        │   Q2 Milestones likewise
└────────┴─────────┴──────────────┴──────────┘
```

Tree:
```
Project Plan (document::tabbed)      — no content itself; content lives in tabs
├── Overview                         — cb-1 (markdown)
└── Timeline                         — cb-2
    ├── Q1 Milestones                — cb-3
    └── Q2 Milestones                — cb-4
```

### Example 2 — Directory structure with mixed contents

```
entities
┌──────┬──────────────────┬──────────────────────┬──────────────────┐
│ id   │ entity_type      │ name                 │ content_block_id │
├──────┼──────────────────┼──────────────────────┼──────────────────┤
│ d1   │ system::directory│ Research             │ —                │
│ d2   │ system::directory│ Papers               │ —                │
│ d3   │ system::directory│ Meetings             │ —                │
│ n1   │ document::note   │ ICML paper           │ cb-5             │
│ n2   │ document::note   │ NeurIPS paper        │ cb-6             │
│ n3   │ document::note   │ 2026-01-15 meeting   │ cb-7             │
│ doc1 │ document::tabbed │ Transformer Survey   │ —                │
└──────┴──────────────────┴──────────────────────┴──────────────────┘

entity_relations
┌────────┬─────────┬──────────────┬──────────┐
│ from   │ to      │ relation     │ position │
├────────┼─────────┼──────────────┼──────────┤
│ d2     │ d1      │ contained_in │ 0        │
│ d3     │ d1      │ contained_in │ 1        │
│ n1     │ d2      │ contained_in │ 0        │
│ n2     │ d2      │ contained_in │ 1        │
│ n3     │ d3      │ contained_in │ 0        │
│ doc1   │ d2      │ contained_in │ 2        │
└────────┴─────────┴──────────────┴──────────┘
```

```
Research
├── Papers
│   ├── ICML paper
│   ├── NeurIPS paper
│   └── Transformer Survey       (tabbed doc — its tabs expand with another contained_in query)
└── Meetings
    └── 2026-01-15 meeting
```

Directories and tabs reuse the same relation. A folder view walks `structure::contained_in` uniformly; when it reaches a `document::tabbed` it keeps drilling with the same query.

### Example 3 — Labels on entities

Labels are entities of type `system::label`. Tagging uses `label::tagged_with`.

```
entities
┌──────┬──────────────────┬───────────────┐
│ id   │ entity_type      │ name          │
├──────┼──────────────────┼───────────────┤
│ n1   │ document::note   │ ICML paper    │
│ n2   │ document::note   │ NeurIPS paper │
│ l1   │ system::label    │ ml            │
│ l2   │ system::label    │ transformers  │
│ l3   │ system::label    │ rl            │
└──────┴──────────────────┴───────────────┘

entity_relations
┌──────┬─────┬──────────────┐
│ from │ to  │ relation     │
├──────┼─────┼──────────────┤
│ n1   │ l1  │ tagged_with  │
│ n1   │ l2  │ tagged_with  │
│ n2   │ l1  │ tagged_with  │
│ n2   │ l3  │ tagged_with  │
└──────┴─────┴──────────────┘
```

"All my `ml`-labelled entities" is `get_relations_to(l1, "tagged_with")` → `[n1, n2]`. Labels are themselves entities, so they can nest, cross-reference, or carry content for free.

### Example 4 — Knowledge graph (cross-references)

Notes citing each other, tab cited from a note, bidirectional links.

```
entities
┌──────┬──────────────────┬────────────────────┐
│ id   │ entity_type      │ name               │
├──────┼──────────────────┼────────────────────┤
│ k1   │ document::note   │ Transformers       │
│ k2   │ document::note   │ Attention          │
│ k3   │ document::note   │ Backpropagation    │
│ k4   │ document::note   │ RLHF               │
│ e3   │ document::tab     │ Timeline           │   (from Example 1)
└──────┴──────────────────┴────────────────────┘

entity_relations
┌──────┬─────┬────────────┬──────────────────────────────────┐
│ from │ to  │ relation   │ metadata                         │
├──────┼─────┼────────────┼──────────────────────────────────┤
│ k1   │ k2  │ references │ {"context": "see: attention"}    │
│ k1   │ k3  │ references │ null                             │
│ k4   │ k1  │ references │ null                             │
│ k4   │ k3  │ references │ null                             │
│ k2   │ k1  │ references │ null                             │   bidirectional
│ k4   │ e3  │ references │ {"context": "deadline milestone"}│   cross-links target tabs too
└──────┴─────┴────────────┴──────────────────────────────────┘
```

Graph:
```
     Transformers ──references──> Attention
         │    ⇐────references────     │
         ↓ references       references ↓
     Backprop  ⇐──references──    RLHF ──references──> Timeline (a tab in a tabbed doc)
```

- Backlinks to `k1`: `get_relations_to(k1, "references")` → `[k4, k2]`.
- Forward links from `k4`: `get_relations_from(k4, "references")` → `[k1, k3, e3]`.
- Cycles (`k1 → k2 → k1`) are fine at the storage layer; traversal code is responsible for loop detection if it recurses.

### Example 5 — One entity, many simultaneous relations

```
entity_relations
┌──────┬─────┬──────────────┬──────────┐
│ from │ to  │ relation     │ position │
├──────┼─────┼──────────────┼──────────┤
│ n1   │ d1  │ contained_in │ 0        │   filed in Research folder
│ n1   │ l1  │ tagged_with  │ —        │   tagged "ml"
│ n1   │ l2  │ tagged_with  │ —        │   tagged "to-summarize"
│ n1   │ n2  │ references   │ —        │   cites another note
│ n2   │ n1  │ references   │ —        │   cited back
└──────┴─────┴──────────────┴──────────┘
```

Five different relations around `n1` coexist cleanly — `PRIMARY KEY (from_id, to_id, relation)` lets the same pair carry multiple semantic links (`n1` both references `n2` and is referenced by `n2`). The UI rendering `n1`'s metadata panel runs a handful of relation queries and gets backlinks, labels, folder context, and citations for free.

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

## Structure Type 2: Documents (entity composition)

Documents are not a dedicated schema. They are entities whose behaviour is determined by their `entity_type`:

- **`document::tabbed`** — a tabbed document. Its text lives in its child tabs (via `structure::contained_in` with `position = tab_index`). Tabs can nest arbitrarily using the same relation. The tabbed doc itself has no `content_block_id`.
- **Flat document kinds** — `document::note`, `document::todo`, `document::prompt`, `document::knowledge`, `document::context`, `document::intent`, `document::system_prompt`, `document::access_rule`. Each is an entity whose markdown text lives directly in its own `content_block_id`. No tabs.

### Model

```
document::tabbed ── structure::contained_in (pos) ──> document::tab
                                          ── structure::contained_in (pos) ──> document::tab (nested)
                                                │
                                                └─ content_block_id → content_blocks

document::note   ── content_block_id → content_blocks
```

### Operations

| Operation | Description |
|-----------|-------------|
| `create_tabbed_doc(user, title, origin?)` | Create a `document::tabbed` entity. No content_block. |
| `create_flat_doc(user, kind, title, content, origin?)` | Create a flat entity with its content block wired up. |
| `create_tab(doc_or_tab_id, position, title, icon, content)` | Create a `document::tab` entity; add `structure::contained_in` relation with position. |
| `update_entity_content(entity_id, text)` | New content block; swap `content_block_id`; orphan old block. |
| `delete_document(id)` | Orchestrator walks `structure::contained_in` subtree, deletes entities, GCs orphan content blocks. |

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

### 6. Cross-Reference

Any entity can reference any other entity. References are first-class.

```
Referenceable entities:
  - ContentBlock
  - Document or Tab
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

### 7. Structured Data

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
| Document | Entity + `structure::contained_in` children | Tabs compose; flat docs carry content directly |
| Collection | Tree + ordering + tags | Items reference anything |
| Content | Immutable blocks | One per entity via `content_block_id` |
| Links | Cross-references | `reference::to` connects any entities |

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

**Use Cases:** All - provides unified identity, content linkage, and relationships

| ID | Requirement | Priority |
|----|-------------|----------|
| FR-0.1 | Every addressable thing is an entity (conversations, documents, tabs, notes, directories, labels). Assets stay in their own table. | P0 |
| FR-0.2 | Entities have: id, type (namespaced string), name, user, privacy | P0 |
| FR-0.3 | @mentions resolve to an entity by name; the stored reference is a structured ref (entity_id), not a string handle | P1 |
| FR-0.4 | Entities carry at most one `content_block_id` (their text) | P0 |
| FR-0.5 | Entities carry a single URI-like `origin` column (`"<scheme>:<id>"`) — no separate source/source_id | P0 |
| FR-0.6 | `entity_relations.position` gives first-class ordering for sibling children | P0 |
| FR-0.7 | `entity_assets (entity_id, asset_id)` maps entity → referenced binary assets | P0 |
| FR-0.8 | Entity relations track: `structure::contained_in`, `reference::to`, `label::tagged_with`, `conversation::forked_from`, `conversation::spawned_from`, `collection::grouped_with` | P0 |
| FR-0.9 | Deleting entity doesn't DB-cascade to related entities; orchestrator owns transitive cleanup | P0 |
| FR-0.10 | Views replace conversations - view IS the conversation | P0 |

**Schema:**

```sql
CREATE TABLE entities (
    id                TEXT PRIMARY KEY,
    entity_type       TEXT NOT NULL,
    user_id           TEXT REFERENCES users(id),
    name              TEXT,
    is_private        INTEGER DEFAULT 0,
    content_block_id  TEXT REFERENCES content_blocks(id),
    origin            TEXT,
    metadata          TEXT,
    created_at        INTEGER NOT NULL,
    updated_at        INTEGER NOT NULL
);

CREATE INDEX idx_entities_user         ON entities(user_id);
CREATE INDEX idx_entities_type         ON entities(entity_type, user_id);
CREATE INDEX idx_entities_origin       ON entities(user_id, origin) WHERE origin IS NOT NULL;
CREATE INDEX idx_entities_has_content  ON entities(content_block_id) WHERE content_block_id IS NOT NULL;

CREATE TABLE entity_relations (
    from_id     TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    to_id       TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    relation    TEXT NOT NULL,
    position    INTEGER,
    metadata    TEXT,
    PRIMARY KEY (from_id, to_id, relation)
);

CREATE INDEX idx_entity_relations_to      ON entity_relations(to_id, relation);
CREATE INDEX idx_entity_relations_ordered ON entity_relations(to_id, relation, position);

CREATE TABLE entity_assets (
    entity_id   TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    asset_id    TEXT NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
    PRIMARY KEY (entity_id, asset_id)
);

CREATE INDEX idx_entity_assets_asset ON entity_assets(asset_id);
```

**Operations:**

```rust
trait EntityStore {
    // CRUD
    async fn create_entity(&self, entity_type: &str, user_id: &UserId) -> Result<EntityId>;
    async fn get_entity(&self, id: &EntityId) -> Result<Option<StoredEntity>>;
    async fn list_entities(&self, user_id: &UserId, entity_type: Option<&str>) -> Result<Vec<StoredEntity>>;
    async fn update_entity(&self, id: &EntityId, updates: EntityUpdate) -> Result<()>;
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
| FR-1.7 | Every text-bearing entity references at most one ContentBlock via `entities.content_block_id` | P0 |
| FR-1.8 | Entity→asset linkage lives in `entity_assets (entity_id, asset_id)` for GC joins | P0 |
| FR-1.9 | Updating an entity's content creates a new block and swaps the pointer — content blocks are never mutated | P0 |

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
-- entities table provides: id, name, user_id, is_private, content_block_id, origin
-- entity_relations provides: conversation::forked_from relationships

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

Documents are entity compositions, not a dedicated schema. There is no `documents` or `document_tabs` table — the shape is expressed with `entities` + `entity_relations` (see FR-0).

| ID | Requirement | Priority |
|----|-------------|----------|
| FR-3.1 | `document::tabbed` entity holds no content itself; its tabs are child entities linked via `structure::contained_in` with `position = tab_index` | P0 |
| FR-3.2 | Tabs can nest: a `document::tab` can itself be a `structure::contained_in` parent of further tabs | P0 |
| FR-3.3 | Each tab / flat document stores its live markdown via its own `content_block_id` | P0 |
| FR-3.4 | Flat document kinds (`document::note`, `::todo`, `::prompt`, `::knowledge`, …) have content directly; no tabs | P0 |
| FR-3.5 | Documents referenceable from conversations/collections via `reference::to` | P0 |
| FR-3.6 | Imported docs carry `origin = "google_drive:<gdoc_id>"` (or future schemes); only the import skill encodes that mapping | P0 |

**Schema:** none beyond FR-0. Queries:

- List tabs of a tabbed doc: `SELECT e.* FROM entity_relations r JOIN entities e ON e.id = r.from_id WHERE r.to_id = ? AND r.relation = 'structure::contained_in' ORDER BY r.position`.
- Find imported version of a gdoc: `SELECT * FROM entities WHERE origin = 'google_drive:<gdoc_id>' AND user_id = ?`.

**Acceptance Criteria:**
- [ ] Import a Google Doc — result is one `document::tabbed` with child `document::tab` entities carrying content blocks, all reachable via a single `structure::contained_in` walk.
- [ ] Create a `document::note` from the UI — stores text in its own content_block; no tab entities involved.
- [ ] Edit a note — creates a new content_block and swaps the entity's `content_block_id`.
- [ ] Delete: orchestrator walks `structure::contained_in` subtree, removes entities, GCs orphan blocks.

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

UCM provides hooks for future systems (temporality, dynamic content, automation) without coupling to specific implementations. See [ARCHITECTURE.md — Event & Intent System](ARCHITECTURE.md#event--intent-system) for the current design (supersedes the original [Hook System](obsolete/HOOK_SYSTEM.md)).

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
| Document | Entity + tab tree | Compose tabs via `structure::contained_in` |
| Collection | Tree of references | Organize anything |
| ContentBlock | Immutable text with origin | Store once, reference many |
| Asset | Binary blob (deduplicated) | Content-addressed storage |
