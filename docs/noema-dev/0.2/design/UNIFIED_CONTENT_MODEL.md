# Unified Content Model

**Status:** Draft
**Created:** 2026-01-10
**Related:** IDEAS #1, #2, #3

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

**Separate content (heavy, immutable) from structure (lightweight, mutable).**

```
┌─────────────────────────────────────────────────────────────┐
│                     CONTENT LAYER                           │
│  Immutable blobs with origin (who/what/when/derived-from)   │
└─────────────────────────────────────────────────────────────┘
                              ▲
                              │
┌─────────────────────────────────────────────────────────────┐
│                    STRUCTURE LAYER                          │
│                                                             │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐         │
│  │ Sequence +  │  │  Version    │  │   Tree +    │         │
│  │ Alternatives│  │   Chain     │  │  Ordering   │         │
│  │             │  │             │  │             │         │
│  │ Conversation│  │  Document   │  │ Collection  │         │
│  └─────────────┘  └─────────────┘  └─────────────┘         │
│         │                │                │                 │
│         └────────────────┼────────────────┘                 │
│                          ▼                                  │
│                   Cross-References                          │
│              (links between any entities)                   │
└─────────────────────────────────────────────────────────────┘
```

---

## Content Layer

### ContentBlock

Universal content primitive. All text, images, documents stored as content blocks.

```
ContentBlock {
    id: ContentHash           // SHA-256 of body, content-addressed
    content_type: String      // "text/markdown", "image/png", etc.
    body: Bytes
    origin: ContentOrigin
    created_at: Timestamp
}

ContentOrigin {
    kind: user | assistant | system | import | tool
    user_id: Option<UserId>                    // which user (multi-user)
    model_id: Option<ModelId>                  // which model (if AI)
    source_id: Option<String>                  // external ID (google doc, url)
    parent_content_id: Option<ContentHash>     // if edited/derived
}
```

**Benefits:**
- Deduplication (same content = same hash = stored once)
- Cross-referencing (same content in conversation AND document)
- Lineage tracking (who created, from what)
- Unified search/RAG across all content

---

## Structure Layer

### Three Structure Types

| Type | Abstraction | Used for |
|------|-------------|----------|
| **Sequence + Alternatives** | Ordered positions, each with alternatives; views select path | Conversations |
| **Version Chain** | Revisions with parent links, linear with branches | Documents |
| **Tree + Ordering** | Nested items with position | Collections |

All three reference the same content layer. Cross-references link between any entities.

---

## Structure Type 1: Sequence + Alternatives (Conversations)

### Model

```
Conversation
  └── Position (ordered slot in sequence)
        └── Alternative (one option at this position)
              └── Message → ContentBlock

View (named path through positions)
  └── selects one alternative at each position
```

### Key Insight: Alternatives are Shared

Views don't own alternatives—they **select** them. Multiple views can select the same alternative, or different alternatives at the same position.

```
View A: [pos1:alt1] → [pos2:alt1] → [pos3:alt1] → [pos4:alt1]
                                         ↗
View B: [pos1:alt1] → [pos2:alt1] → [pos3:alt2] → [pos4:alt1]  ← reuses pos4:alt1!
```

This enables splice: edit position 3, but keep position 4 from original.

### Operations

| Operation | Description |
|-----------|-------------|
| `add_position()` | Append new position to conversation |
| `add_alternative(position, model)` | Add alternative at position |
| `select(view, position, alternative)` | View selects which alternative |
| `fork(view, position)` | New view sharing selections up to position |
| `spawn_child(view, position)` | New conversation inheriting context |

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
Parent:  P1 → P2 → P3 → [spawn] ─────────────────→ P4(summary)
                    │                                   ▲
                    ▼                                   │
Child:            C1 → C2 → C3 → [result] ─────────────┘
                  (inherits P1-P3 context)
```

**Structure needed:**
- Parent-child relationship between conversations
- Child inherits context (positions/alternatives) up to spawn point
- Summary content flows back as new content in parent

**Operations:**
- `spawn_child(parent_view, position)` → new conversation
- Child sees parent's context as read-only prefix
- `summarize()` → ContentBlock injected into parent's next position

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

Multiple alternatives at a position. User selects. Chain continues from selection.

```
Position 3:
  ├── Alt A (claude) ← selected
  ├── Alt B (gpt-4)
  └── Alt C (gemini)

Position 4 continues from Alt A's context
```

**Structure:** Multiple alternatives at position. View selection determines path.

**Operations:**
- `add_alternative(position, model)` → generate with model
- `select(view, position, alternative)` → choose winner
- Selection change = context change for subsequent positions

**UI consideration:**
- Short alternatives → tabs inline
- Many/long alternatives → dropdown or separate view

---

### 4. Fork Conversation

Branch from any point. Paths diverge independently.

```
Original: P1 → P2 → P3 → P4 → P5
                    │
                    ▼
Forked:   P1 → P2 → P3 → F4 → F5
          (shared)    (new positions)
```

**Structure:** New view sharing positions up to fork point. New positions after.

**Operations:**
- `fork(view, position)` → new view
- Positions 1-3 shared (same alternatives selected)
- Position 4+ are new positions in conversation

**UI consideration:**
- Show fork relationship in conversation list
- Breadcrumb: "Forked from [Original] at message 3"
- Lineage view: tree of related conversations

---

### 5. Edit & Splice

Edit a position. Optionally keep subsequent positions from original.

```
Original: P1 → P2 → P3 → P4 → P5
                    │
                    ▼
Edited:   P1 → P2 → P3' → P4 → P5
               (new alt)  (reused!)
```

**Key insight:** This is NOT a fork. It's:
1. New alternative at position 3
2. New view selecting: [alt1, alt1, alt_new, alt1, alt1]

The original P4, P5 are reused because alternatives are shared across views.

**Operations:**
- `add_alternative(position_3, edited_content)`
- `create_view(selections)` with mix of original and new alternatives

**Constraint:** Reusing P4/P5 only makes sense if they don't depend on P3's specific content. May need to regenerate.

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

Same content appears in multiple places. Documents can be referenced within conversations.

```
ContentBlock "Meeting summary"
  ↑
  ├── Alternative in Conversation, position 5
  ├── Revision 3 of Document "notes.md"
  └── Item in Collection "Important"

Conversation with DocumentRef:
  Position 1: [user: "Summarize this doc" + DocumentRef("notes.md")]
  Position 2: [assistant: response referencing doc content]
```

**Structure:** Content separate from usage. Multiple structures reference same ContentHash.

**References in conversations:**
- `DocumentRef` in message content → points to Document
- Document content injected into LLM context at render time
- Enables RAG-style document grounding in conversations

**Operations:**
- Any structure can reference any ContentBlock or Document
- `backlinks(content)` → all places referencing it
- `backlinks(document)` → all conversations referencing it

**UI:** "Used in: [Conversation X], [Document Y], [Collection Z]"

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
| Few short alternatives | Tabs inline |
| Many/long alternatives | List with previews |
| Forked conversations | Tree showing lineage |
| Subagent work | Collapsed summary, expandable |
| Edit history at position | "Edited" badge, hover for original |

### Navigation Needs

- **Conversation list:** Show fork relationships, group by lineage
- **Conversation detail:** Linear view with alternative indicators
- **Lineage view:** Tree of related conversations
- **Search:** Across all content, grouped by structure type

---

## Open Questions

1. **Regeneration on splice:** If P4 depends on P3, does edit invalidate it?
2. **Context inheritance:** How much parent context does subagent see?
3. **Approval workflow:** How does supervised agent communication flow?
4. **GC:** When is content orphaned?
5. **Large alternatives:** When do tabs become unwieldy?

---

## Summary

| Structure | Core abstraction | Key operation |
|-----------|------------------|---------------|
| Conversation | Positions + alternatives + views | View selects path |
| Document | Revision chain | Commit creates version |
| Collection | Tree + ordering + tags | Items reference anything |
| Content | Immutable blocks | Shared across structures |
| Links | Cross-references | Connect any entities |
