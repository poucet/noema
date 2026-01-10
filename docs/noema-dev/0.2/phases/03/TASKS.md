# Phase 3: Unified Content Model

## Overview

Phase 3 establishes the **Unified Content Model** - separating immutable content from mutable structure. This enables parallel model responses, conversation forking, document versioning, and flexible organization.

**Core Principle**: Content (text, assets) is heavy and immutable. Structure (conversations, documents, collections) is lightweight and mutable.

## Task Table

| Status | Pri | # | Feature | Description |
|--------|-----|---|---------|-------------|
| ⬜ | P0 | 3.1 | Content blocks | Content-addressed text storage with origin tracking |
| ⬜ | P0 | 3.1b | Asset storage | Binary blob storage (images, audio, PDFs) |
| ⬜ | P0 | 3.2 | Conversation structure | Turns, spans, messages with content references |
| ⬜ | P0 | 3.3 | Views and forking | Named paths through conversations, fork support |
| ⬜ | P1 | 3.4 | Document structure | Documents with tabs and revision history |
| ⬜ | P1 | 3.5 | Collections | Tree organization with tags and fields |
| ⬜ | P1 | 3.6 | Cross-references | Links between any entities with backlinks |
| ⬜ | P2 | 3.7 | Temporal queries | Time-based activity summaries for LLM context |
| ⬜ | P2 | 3.8 | Session integration | Connect engine to new conversation model |
| ⬜ | P2 | 3.9 | Migration and cleanup | Remove legacy tables |

Status: ⬜ todo, 🔄 in-progress, ✅ done, 🚫 blocked, ⏸️ deferred

---

## Feature Details

### Feature 3.1: Content Block Storage

**Problem**: Text content duplicated across messages, documents, revisions. No unified search or cross-referencing.

**Solution**: Content-addressed storage where all text lives in a single table, referenced by ID.

**Functional Requirements**:
- Store text content with type (plain, markdown, typst) and origin metadata
- Track who created content (user, assistant, system, import)
- Track provenance (which model, derived from which parent)
- Same text produces same hash (deduplication)
- Privacy flag marks content as local-only (never sent to cloud models)

**Acceptance Criteria**:
- [ ] Store text → get UUID back
- [ ] Retrieve text by ID
- [ ] Same text → same hash (deduplicated)
- [ ] Origin metadata preserved (user/assistant, model ID, parent ID)
- [ ] Full-text search across all content blocks

---

### Feature 3.1b: Asset Storage

**Problem**: Binary content (images, audio, PDFs) needs separate handling from text.

**Solution**: Content-addressed blob storage with inline references from content.

**Functional Requirements**:
- Store binary blobs by SHA-256 hash (deduplication)
- Track mime type, filename, size
- Privacy flag for local-only assets
- Assets referenced inline from messages/documents as `AssetRef { asset_id, mime_type }`
- Resolve asset references to inline data when sending to LLM

**Acceptance Criteria**:
- [ ] Store image → get hash ID back
- [ ] Same file → same hash (deduplicated)
- [ ] Create message with `AssetRef` pointing to asset
- [ ] Resolve payload converts `AssetRef` to inline base64
- [ ] Privacy flag prevents cloud model access

---

### Feature 3.2: Conversation Structure

**Problem**: Current model doesn't support parallel model responses, multi-step tool interactions, or comparing alternatives.

**Solution**: Conversations as sequences of turns, each with alternative spans containing messages.

**Functional Requirements**:
- Conversation contains ordered turns (position in sequence)
- Each turn has one or more spans (alternative responses)
- Each span contains ordered messages (for multi-step flows)
- Span has role (user/assistant) identifying owner
- Message has role for multi-step support (assistant → tool → assistant)
- Message references content block for text
- Tool calls/results stored inline in message

**Use Cases Enabled**:
- Parallel model responses: Multiple spans at same turn, compare them
- Tool interactions: Single span contains assistant → tool_call → tool_result → response
- User edits as alternatives: Edit creates new user span at same turn

**Acceptance Criteria**:
- [ ] Create conversation with turns and spans
- [ ] Span contains multiple messages (multi-step flow)
- [ ] Different spans at same turn can have different message counts
- [ ] Messages reference content blocks (text is searchable)
- [ ] Tool calls/results preserved in messages

---

### Feature 3.3: Views and Forking

**Problem**: No way to branch conversations, compare different paths, or edit mid-conversation.

**Solution**: Views select one span per turn, creating named paths through the conversation.

**Functional Requirements**:
- Views select which span to use at each turn
- Main view is default (created with conversation)
- Fork creates new view sharing selections up to fork point
- Span selection affects subsequent context
- Views are cheap (just selection pointers, content not duplicated)

**Use Cases Enabled**:
- Fork conversation: Branch from turn 3, explore different direction
- Edit and splice: New span at turn 3, reuse turns 4-5 from original
- A/B comparison: Two views selecting different spans

**Acceptance Criteria**:
- [ ] Create view for conversation
- [ ] View selects spans, forming coherent path
- [ ] Fork view at turn N shares turns 1..(N-1)
- [ ] Forked view can select different spans after fork point
- [ ] Get view path returns selected span messages in order

---

### Feature 3.4: Document Structure

**Problem**: Documents are flat with no structure. Can't organize sections or track where content came from.

**Solution**: Documents with hierarchical tabs, each tab having its own revision history.

**Functional Requirements**:
- Document contains tabs (structural pointers to content)
- Tabs can be nested (sub-tabs)
- Each tab has independent revision history
- Revisions reference content blocks (text is searchable, deduplicated)
- Track document source (user created, AI generated, imported, promoted from message)
- Promote message to document (reuses content block)

**Use Cases Enabled**:
- Multi-section documents: Overview tab, Details tab with sub-tabs
- Version history per section: Revert just one tab
- AI → Document pipeline: Save assistant response as document

**Acceptance Criteria**:
- [ ] Create document with initial tab
- [ ] Add nested tabs (hierarchy)
- [ ] Commit creates new revision for tab
- [ ] Branch revision from non-head
- [ ] Checkout moves tab to specific revision
- [ ] Promote message to document (reuses content block)

---

### Feature 3.5: Collections

**Problem**: No unified way to organize content across types. Can't create project views, task lists, or bookmarks.

**Solution**: Collections as a structural layer over any entity, with schema hints for UI and indexed fields for queries.

**Functional Requirements**:
- Collection items can reference any entity (document, conversation, content block, other collection)
- Items form tree structure (nested folders)
- Items have position (ordered)
- Items can have tags (cross-cutting organization)
- Items can have typed fields (for table/kanban views)
- Schema hints tell UI what fields to expect (advisory, not enforced)
- For document items: frontmatter is source of truth, fields are cached index

**Use Cases Enabled**:
- Project folder: Documents and conversations grouped
- Task list: Items with status, priority, due date fields
- Bookmarks: Mixed entity types in one list
- Kanban board: Items grouped by status field

**Acceptance Criteria**:
- [ ] Create collection with items referencing different entity types
- [ ] Nested items (tree structure)
- [ ] Reorder items (move within/between parents)
- [ ] Tag items and query by tag
- [ ] Set fields and query/filter by field value
- [ ] Schema hint guides UI field display

---

### Feature 3.6: Cross-References

**Problem**: No way to link content across conversations, documents, collections. No backlinks.

**Solution**: Generic reference system between any entity types with automatic backlink tracking.

**Functional Requirements**:
- Reference from any entity to any entity
- Optional relation type (cites, derived_from, etc.)
- Backlinks auto-computed (who references this?)
- Support @-mention syntax in content

**Use Cases Enabled**:
- Document cites conversation: "Generated from [chat X]"
- Message references document: "See @api-design for details"
- Backlinks panel: "Referenced by 3 conversations, 1 document"

**Acceptance Criteria**:
- [ ] Create reference between entities
- [ ] Query outgoing references from entity
- [ ] Query incoming references (backlinks) to entity
- [ ] References survive entity updates
- [ ] Delete reference when source entity deleted

---

### Feature 3.7: Temporal Queries

**Problem**: LLM needs activity context ("what have I been working on?") but no efficient time-based queries.

**Solution**: Indexed timestamps enabling time-range queries with summarization for LLM context.

**Functional Requirements**:
- Query content by time range (last hour, last day, last week)
- Group by entity type (conversations, documents)
- Generate activity summary for LLM injection
- Configurable detail level (brief, detailed)

**Use Cases Enabled**:
- "Summarize my work from last week"
- "What topics have I been exploring?"
- Proactive assistant: "I noticed you've been working on X..."

**Acceptance Criteria**:
- [ ] Query messages/content in time range
- [ ] Group results by conversation/document
- [ ] Generate markdown summary of activity
- [ ] Summary respects token budget

---

### Feature 3.8: Session Integration

**Problem**: Engine session uses old conversation model. Need to connect to new structure.

**Solution**: Adapter connecting SqliteSession to Turn/Span/Message model.

**Functional Requirements**:
- `commit()` creates turn + span + messages
- Message text stored via content block store
- `commit_parallel_responses()` creates one turn with multiple spans
- `open_conversation()` loads main view's selected spans
- Existing session API preserved (engine unchanged)

**Acceptance Criteria**:
- [ ] Send message → creates turn, span, message, content block
- [ ] Load conversation → returns messages from main view path
- [ ] Parallel responses → multiple spans at same turn
- [ ] Engine works without modification

---

### Feature 3.9: Migration and Cleanup

**Problem**: Legacy tables (threads, span_sets, spans, span_messages) need removal.

**Solution**: Remove old schema after session integration verified.

**Functional Requirements**:
- Verify all functionality works with new model
- Drop legacy tables
- Clean up old code paths

**Acceptance Criteria**:
- [ ] All tests pass with new model
- [ ] Legacy tables dropped
- [ ] No references to old table names in code

---

## Key Design Decisions

### Spans vs Messages

**Span** = an autonomous flow owned by one party (user or assistant)
**Message** = individual content within a span

A single assistant span can contain: thinking → tool_call → tool_result → response

This enables parallel model comparison where different models produce different numbers of messages.

### Content Deduplication

All text goes through content blocks. Same text = same hash = stored once.

Benefits:
- Deduplication across messages, documents, revisions
- Unified full-text search
- Cross-referencing ("as I said in message X")
- Origin tracking (who created, derived from what)

### Collections as Meta-Structure

Collections don't own content - they organize references to it.

For document items, frontmatter is the source of truth for fields. `item_fields` is a cached index regenerated on content change.

---

## Related Documents

- [PLAN.md](PLAN.md) - Detailed implementation plan with schema and API
- [UNIFIED_CONTENT_MODEL.md](../../design/UNIFIED_CONTENT_MODEL.md) - Design document
- [HOOK_SYSTEM.md](../../design/HOOK_SYSTEM.md) - Future extension points
