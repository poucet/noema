# Noema 0.2 Changelog

**Released:** 2026-01-29

Version 0.2 establishes the **Unified Content Model (UCM)** - a complete rewrite of the storage layer that separates immutable content from mutable structure.

---

## Highlights

- **Content-first architecture**: All text stored in content blocks with origin tracking
- **Views replace conversations**: Fork, edit, and compare conversation paths
- **Turn/Span/Message model**: Support parallel model responses and tool iterations
- **Entity layer**: Unified identity, naming (@slug), and relationships
- **Document structure**: Tabs with per-tab revision history

---

## Phase 1: Quick Wins (Complete)

| # | Feature |
|---|---------|
| 2 | Truncate long model names |
| 3 | Model metadata display (context window, provider) |
| 4 | Local vs non-local model indicator |
| 31 | Copy raw markdown from assistant responses |
| 32 | Private content flag |
| 33 | Toggle to disable tools |
| 34 | Toggle to disable audio/image input |

---

## Phase 3: Unified Content Model (Complete)

### 3.1 Content Blocks
- Content-addressed text storage with SHA-256 hash
- Origin tracking: user/assistant/system/import
- Model ID, source ID, parent content ID for provenance
- Privacy flag for local-only content

### 3.1b Asset Storage
- Binary blob storage (images, audio, PDFs)
- Content-addressed deduplication
- Inline references from messages (AssetRef)
- Automatic resolution for LLM context

### 3.2 Conversation Structure
- **Turns**: Positions in conversation sequence
- **Spans**: Alternative responses at a turn (multiple messages each)
- **Messages**: Individual content within spans
- Supports parallel model responses and tool iterations

### 3.3 Views and Forking
- Views select one span per turn, forming paths
- Fork from any point (new view, shared history)
- Edit mid-conversation (new span at turn)
- Splice: edit turn 3, optionally keep turns 4+

### 3.3 Entity Layer
- Unified identity for views, documents, assets
- @slug for mentions
- Entity relations for fork ancestry (not column)
- Decoupled lifecycle (delete view, forks survive)

### 3.3b Subconversations
- Spawn child conversation from parent
- Child inherits context up to spawn point
- Link results back to parent
- `spawned_from` entity relation

### 3.4 Document Structure
- Documents with tabs (structural pointers to content)
- Nested tabs (sub-tabs)
- Per-tab revision history
- Promote message to document (reuses content block)

### 3.5 Collections
- Tree structure with ordered items
- Items reference any entity type
- Tags for cross-cutting organization
- Typed fields for table/kanban views
- Schema hints (advisory, not enforced)

### 3.6 Cross-References
- Reference any entity from any entity
- Optional relation types (cites, derived_from)
- Automatic backlink tracking

### 3.7 Temporal Queries
- Time-range queries on entities table
- Activity summaries for LLM context

---

## User Journeys Implemented

1. **Regenerate Response**: New span at turn, switch between alternatives
2. **Select Alternate**: View parallel responses, choose one
3. **Edit User Message**: Fork view, new span with edited content
4. **Fork Conversation**: New view sharing history up to fork point
5. **Switch View**: Navigate between conversation paths
6. **View Alternates**: See all spans at a turn, compare and select

---

## Storage Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    ADDRESSABLE LAYER                        │
│  entities + entity_relations                                │
└─────────────────────────────────────────────────────────────┘
                              │
┌─────────────────────────────────────────────────────────────┐
│                    STRUCTURE LAYER                          │
│  views, turns, spans, messages │ documents, tabs │ collections │
└─────────────────────────────────────────────────────────────┘
                              │
┌─────────────────────────────────────────────────────────────┐
│                     CONTENT LAYER                           │
│  content_blocks (text) + assets/blobs (binary)             │
└─────────────────────────────────────────────────────────────┘
```

---

## Deferred to Future Versions

See [FUTURE_ROADMAP.md](FUTURE_ROADMAP.md) for:

- Phase 4: Content model features (undo, auto-naming, summaries)
- Phase 5: Organization + search (embeddings, tags, skills)
- Phase 6: RAG + memories
- Phase 7: Agentic + multimodal
- Phase 8: Hook system automation

UI for documents, references, and subconversations also deferred.

---

## Design Documents

- [designs/UNIFIED_CONTENT_MODEL.md](designs/UNIFIED_CONTENT_MODEL.md) - Detailed UCM specification
- [designs/HOOK_SYSTEM.md](designs/HOOK_SYSTEM.md) - Event-driven automation (future)
- [designs/ARCHITECTURE.md](designs/ARCHITECTURE.md) - System architecture
