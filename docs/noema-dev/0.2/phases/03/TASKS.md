# Phase 3: Unified Content Model

## Overview

Phase 3 establishes the **Unified Content Model** - separating immutable content from mutable structure. This enables parallel model responses, conversation forking, document versioning, and flexible organization.

**Core Principle**: Content (text, assets) is heavy and immutable. Structure (conversations, documents, collections) is lightweight and mutable.

## Task Table

| Status | Pri | # | Feature | Description |
|--------|-----|---|---------|-------------|
| ✅ | P0 | 3.1 | Content blocks | Content-addressed text storage with origin tracking |
| ✅ | P0 | 3.1b | Asset storage | Binary blob storage (images, audio, PDFs) |
| ✅ | P0 | 3.2 | Conversation structure | Turns, spans, messages with content references |
| 🔄 | P0 | 3.3 | Views, forking, and migration | Complete conversation model with user journeys |
| ⬜ | P1 | 3.3b | Subconversations | Spawned agent conversations linked to parent |
| ⬜ | P1 | 3.4 | Document structure | Documents with tabs and revision history |
| ⬜ | P1 | 3.5 | Collections | Tree organization with tags and fields |
| ⬜ | P1 | 3.6 | Cross-references | Links between any entities with backlinks |
| ⬜ | P2 | 3.7 | Temporal queries | Time-based activity summaries for LLM context |

Status: ⬜ todo, 🔄 in-progress, ✅ done, 🚫 blocked, ⏸️ deferred

---

## Microtasks (Commit-Sized Steps)

Each microtask is a single atomic commit. Complete in order within each feature.

**Commit Prefixes**: 🏗️ types/traits, 📦 schema/migration, ⚡ implementation, ✅ tests, 🔧 integration, 🧹 cleanup

### 3.1 Content Blocks (10 tasks)

| Status | # | Task |
|--------|---|------|
| ✅ | 3.1.1 | 🏗️ Define type-safe IDs module (`storage/ids.rs`) |
| ✅ | 3.1.2 | 🏗️ Create ContentOrigin and OriginKind types |
| ✅ | 3.1.3 | 🏗️ Define ContentBlockStore trait |
| ✅ | 3.1.4 | 📦 Add content_blocks table migration |
| ✅ | 3.1.5 | ⚡ Implement SqliteContentBlockStore |
| ⏸️ | 3.1.6 | 📦 Add content_block_tags table (deferred → 3.5 Collections) |
| ✅ | 3.1.7 | ✅ Unit tests for content block store |
| ✅ | 3.1.8 | 📦 Add `content_id` column to `span_messages` |
| ✅ | 3.1.9 | 🔧 Update `write_as_span()` to store text in content_blocks |
| ✅ | 3.1.10 | ✅ E2E verification (text still in both places, read path unchanged) |

### 3.1b Asset Storage (10 tasks)

| Status | # | Task |
|--------|---|------|
| ✅ | 3.1b.1 | 🏗️ Define AssetStore trait |
| ✅ | 3.1b.2 | 📦 Add assets table migration |
| ✅ | 3.1b.3 | ⚡ Implement SqliteAssetStore |
| ✅ | 3.1b.4 | 🏗️ Add AssetRef to StoredContent enum |
| ✅ | 3.1b.5 | ⚡ Implement asset resolution in payload |
| ✅ | 3.1b.6 | ✅ Unit tests for asset store |
| ✅ | 3.1b.7 | 🔧 Update store_asset command to use new API |
| ✅ | 3.1b.8 | 🔧 User: Run app via `noema` |
| ✅ | 3.1b.9 | 🔧 User: Attach image to message, send, verify image displays |
| ✅ | 3.1b.10 | 🔧 User: SQL verify `SELECT * FROM assets` shows new row |

### 3.2 Conversation Structure (13 tasks)

| Status | # | Task |
|--------|---|------|
| ✅ | 3.2.1 | 🏗️ Define Turn, Span, Message types (`storage/conversation/types.rs`) |
| ✅ | 3.2.2 | 📦 Add turns table migration |
| ✅ | 3.2.3 | 📦 Add spans table migration (legacy renamed to `legacy_spans`) |
| ✅ | 3.2.4 | 📦 Add messages table migration (legacy renamed to `legacy_span_messages`) |
| ✅ | 3.2.5 | 📦 Add views, view_selections tables |
| ✅ | 3.2.6 | 🏗️ Define TurnStore trait |
| ✅ | 3.2.7 | ⚡ Implement add_turn, get_turns, get_turn |
| ✅ | 3.2.8 | ⚡ Implement add_span, get_spans, get_span |
| ✅ | 3.2.9 | ⚡ Implement add_message, get_messages, get_message |
| ✅ | 3.2.10 | ✅ Unit tests for TurnStore |
| ✅ | 3.2.11 | 🔧 Wire existing write paths to TurnStore (dual-write) |
| ✅ | 3.2.12 | 🔧 User: E2E verification in noema app |
| ✅ | 3.2.13 | 🔧 User: SQL verify data in new tables |

### 3.3 Views, Forking, and Migration (~40 tasks)

**Goal**: Complete the conversation model, remove legacy system, and verify all UCM operations via user journeys. After 3.3, the app runs entirely on Turn/Span/Message/View model with full UI support for forking, regeneration, and view switching.

#### Part A: Views and Forking (8 tasks)

| Status | # | Task |
|--------|---|------|
| ✅ | 3.3.1 | 📦 Add views table migration |
| ✅ | 3.3.2 | 📦 Add view_selections table migration |
| ✅ | 3.3.3 | ⚡ Implement create_view, get_views, get_main_view |
| ✅ | 3.3.4 | ⚡ Implement select_span, get_selected_span |
| ✅ | 3.3.5 | ⚡ Implement get_view_path |
| ✅ | 3.3.6 | ⚡ Implement fork_view |
| ✅ | 3.3.7 | ⚡ Implement edit_turn, fork_view_with_selections, get_view_context_at |
| ✅ | 3.3.8 | ✅ Unit tests for views and forking |

#### Part B: New Session API (6 tasks)

| Status | # | Task |
|--------|---|------|
| ✅ | 3.3.9 | 🏗️ Create `Session<S: TurnStore>` with `ResolvedContent`/`ResolvedMessage` types |
| ✅ | 3.3.10 | 🏗️ Create `ContentBlockResolver` and `AssetResolver` traits |
| ✅ | 3.3.11 | ⚡ Implement `Session::open()`, `commit()`, `commit_parallel()` |
| ✅ | 3.3.12 | ⚡ Implement `messages_for_display()` and `messages_for_llm()` with lazy caching |
| ✅ | 3.3.13 | 🔧 Update `ChatEngine` to use new `Session<S>` API |
| ⬜ | 3.3.14 | ✅ Integration tests with engine |

#### Part C: Legacy Cleanup (4 tasks)

| Status | # | Task |
|--------|---|------|
| ✅ | 3.3.15 | 🧹 Remove `SessionStore` and `StorageTransaction` traits |
| ✅ | 3.3.16 | 🧹 Remove `MemorySession`/`MemoryTransaction` (memory.rs) |
| ✅ | 3.3.17 | 🧹 Remove `SqliteSession`/`SqliteTransaction` from sqlite.rs |
| ✅ | 3.3.18 | 🧹 Update docs (lib.rs, mod.rs) for new API |

#### Part C.2: Storage Module Restructure (6 tasks)

| Status | # | Task |
|--------|---|------|
| ✅ | 3.3.18a | 🧹 Create `storage/traits/` with all trait definitions |
| ✅ | 3.3.18b | 🧹 Create `storage/types/` with all type definitions |
| ✅ | 3.3.18c | 🧹 Move implementations to `storage/implementations/` (sqlite/, fs/, memory/) |
| ✅ | 3.3.18d | ⚡ Add in-memory store implementations (MemoryBlobStore, MemoryContentBlockStore, MemoryAssetStore) |
| ✅ | 3.3.18e | 🧹 Split sqlite/conversation.rs into turn.rs and conversation.rs to match traits layout |
| ✅ | 3.3.18f | ⚡ Add MemoryTurnStore, MemoryConversationStore, MemoryDocumentStore implementations |

#### Part D: User Journeys - UCM Verification (6 journeys, ~21 tasks)

**Goal**: Implement and verify all user-facing UCM operations. Each journey is a complete user workflow that exercises the underlying TurnStore/View operations.

##### Journey 1: Regenerate Response (3 tasks)

User clicks "regenerate" on assistant message → creates new span at same turn, selects it.

| Status | # | Task |
|--------|---|------|
| ✅ | 3.3.D1a | ⚡ Backend: `regenerate_response` command - add_span at turn, select in view |
| ⬜ | 3.3.D1b | 🔧 Frontend: Wire regenerate button to new command |
| ⬜ | 3.3.D1c | ✅ User: Verify regenerate creates alternate, can switch between |

##### Journey 2: Select Alternate Span (3 tasks)

User views parallel responses → selects one to use → view updates selection.

| Status | # | Task |
|--------|---|------|
| ✅ | 3.3.D2a | ⚡ Backend: `select_span` command - calls TurnStore::select_span |
| ⬜ | 3.3.D2b | 🔧 Frontend: Wire "Use this" button to select_span command |
| ⬜ | 3.3.D2c | ✅ User: Verify span selection persists, affects subsequent context |

##### Journey 3: Edit User Message (4 tasks)

User edits previous message → creates fork from that turn → new span with edited content.

| Status | # | Task |
|--------|---|------|
| ⬜ | 3.3.D3a | ⚡ Backend: `edit_message` command - fork_view + edit_turn |
| ⬜ | 3.3.D3b | 🔧 Frontend: Add edit button to user messages |
| ⬜ | 3.3.D3c | 🔧 Frontend: Edit modal/inline with submit action |
| ⬜ | 3.3.D3d | ✅ User: Verify edit creates fork, original unchanged |

##### Journey 4: Fork Conversation (4 tasks)

User forks from any turn → new view sharing history up to fork point.

| Status | # | Task |
|--------|---|------|
| ✅ | 3.3.D4a | ⚡ Backend: `fork_conversation` command - fork_view at turn |
| ⬜ | 3.3.D4b | 🔧 Frontend: Add fork button/menu to turns |
| ⬜ | 3.3.D4c | 🔧 Frontend: Show view list, allow switching |
| ⬜ | 3.3.D4d | ✅ User: Verify fork shares history, diverges after fork point |

##### Journey 5: Switch View (3 tasks)

User has multiple views → switches between them → conversation display updates.

| Status | # | Task |
|--------|---|------|
| ✅ | 3.3.D5a | ⚡ Backend: `switch_view` command - Session opens with different view_id |
| ⬜ | 3.3.D5b | 🔧 Frontend: View selector UI (sidebar or dropdown) |
| ⬜ | 3.3.D5c | ✅ User: Verify switching views shows different conversation paths |

##### Journey 6: View Alternates at Turn (4 tasks)

User inspects a turn → sees all spans (alternatives) → can compare and select.

| Status | # | Task |
|--------|---|------|
| ✅ | 3.3.D6a | ⚡ Backend: `get_turn_alternates` returns all spans with content |
| ⬜ | 3.3.D6b | 🔧 Frontend: Alternates panel/popover for turns with multiple spans |
| ⬜ | 3.3.D6c | 🔧 Frontend: Display span metadata (model, timestamp) |
| ⬜ | 3.3.D6d | ✅ User: Verify can see all alternatives, select any one |

#### Part E: Final Verification (3 tasks)

| Status | # | Task |
|--------|---|------|
| ⬜ | 3.3.19 | 🔧 User: E2E verification - all journeys work end-to-end |
| ⬜ | 3.3.20 | 🔧 User: SQL verify views and view_selections have correct data |
| ⬜ | 3.3.21 | ✅ Final E2E: fresh install, all conversation features work

### 3.3b Subconversations (5 tasks)

**Goal**: Support spawned subconversations for agents. Main conversation spawns subagent → subagent runs in own conversation → result linked back to parent.

```
Main:  Turn 1 → Turn 2 (ToolCall: spawn_agent)
                  ↓
       Sub:  Turn 1 → Turn 2 → Turn 3 (result)
                  ↓
       Turn 2 (ToolResult: {result, subconversation_id})  → Turn 3
```

| Status | # | Task |
|--------|---|------|
| ⬜ | 3.3b.1 | 🏗️ Schema: Add `parent_conversation_id`, `parent_turn_id` to conversations table |
| ⬜ | 3.3b.2 | ⚡ Backend: `spawn_subconversation` - create linked conversation with initial context |
| ⬜ | 3.3b.3 | ⚡ Backend: `link_subconversation_result` - attach result to parent turn |
| ⬜ | 3.3b.4 | 🔧 Integration: Wire MCP agent spawn to use subconversation API |
| ⬜ | 3.3b.5 | ✅ User: Verify subconversation runs, result appears in parent |

### 3.4 Document Structure (10 tasks)

| Status | # | Task |
|--------|---|------|
| ⬜ | 3.4.1 | 🏗️ Define Document, Tab, Revision types |
| ⬜ | 3.4.2 | 📦 Add documents table migration |
| ⬜ | 3.4.3 | 📦 Add document_tabs table migration |
| ⬜ | 3.4.4 | 📦 Add revisions table migration |
| ⬜ | 3.4.5 | 🏗️ Define DocumentStore trait |
| ⬜ | 3.4.6 | ⚡ Implement document CRUD |
| ⬜ | 3.4.7 | ⚡ Implement tab management |
| ⬜ | 3.4.8 | ⚡ Implement revision commit/checkout |
| ⬜ | 3.4.9 | ⚡ Implement promote_from_message |
| ⬜ | 3.4.10 | ✅ Unit tests for document structure |

### 3.5 Collections (12 tasks)

| Status | # | Task |
|--------|---|------|
| ⬜ | 3.5.1 | 🏗️ Define Collection, Item, View types |
| ⬜ | 3.5.2 | 📦 Add collections table migration |
| ⬜ | 3.5.3 | 📦 Add collection_items table migration |
| ⬜ | 3.5.4 | 📦 Add item_fields table migration |
| ⬜ | 3.5.5 | 📦 Add item_tags table migration |
| ⬜ | 3.5.6 | 📦 Add collection_views table migration |
| ⬜ | 3.5.7 | 🏗️ Define CollectionStore trait |
| ⬜ | 3.5.8 | ⚡ Implement collection CRUD |
| ⬜ | 3.5.9 | ⚡ Implement item management |
| ⬜ | 3.5.10 | ⚡ Implement field and tag operations |
| ⬜ | 3.5.11 | ⚡ Implement view creation and query |
| ⬜ | 3.5.12 | ✅ Unit tests for collections |

### 3.6 Cross-References (7 tasks)

| Status | # | Task |
|--------|---|------|
| ⬜ | 3.6.1 | 🏗️ Define Reference and EntityRef types |
| ⬜ | 3.6.2 | 📦 Add references table migration |
| ⬜ | 3.6.3 | 🏗️ Define ReferenceStore trait |
| ⬜ | 3.6.4 | ⚡ Implement create and delete |
| ⬜ | 3.6.5 | ⚡ Implement get_outgoing |
| ⬜ | 3.6.6 | ⚡ Implement get_backlinks |
| ⬜ | 3.6.7 | ✅ Unit tests for references |

### 3.7 Temporal Queries (6 tasks)

| Status | # | Task |
|--------|---|------|
| ⬜ | 3.7.1 | 📦 Add temporal indexes to tables |
| ⬜ | 3.7.2 | 🏗️ Define TemporalStore trait |
| ⬜ | 3.7.3 | ⚡ Implement query_by_time_range |
| ⬜ | 3.7.4 | ⚡ Implement get_activity_summary |
| ⬜ | 3.7.5 | ⚡ Implement LLM context rendering |
| ⬜ | 3.7.6 | ✅ Unit tests for temporal queries |

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

**Microtask Details**:

| # | Create | Update | Implement | SQL | Test |
|---|--------|--------|-----------|-----|------|
| 3.1.1 | `storage/ids.rs` | `storage/mod.rs` | `define_id!` macro, all ID newtypes | — | compile |
| 3.1.2 | `storage/content_block/types.rs` | — | `OriginKind`, `ContentOrigin` | — | compile |
| 3.1.3 | `storage/content_block/mod.rs` | `storage/mod.rs` | `ContentBlockStore` trait, `ContentBlockInfo` | — | compile |
| 3.1.4 | — | schema/migrations | — | `content_blocks` table, indexes | fresh DB |
| 3.1.5 | `storage/content_block/sqlite.rs` | — | `SqliteContentBlockStore`, SHA-256 hash, dedup | — | compile |
| 3.1.6 | — | schema | `tag()`, `untag()`, `get_tags()`, `find_by_tag()` | `content_block_tags` | compile |
| 3.1.7 | `storage/content_block/tests.rs` | — | — | — | CRUD, dedup, origin, tags |

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

**Microtask Details**:

| # | Create | Update | Implement | SQL | Test |
|---|--------|--------|-----------|-----|------|
| 3.1b.1 | `storage/asset/mod.rs` | — | `AssetStore` trait, `AssetInfo` | — | compile |
| 3.1b.2 | — | schema/migrations | — | `assets` table | fresh DB |
| 3.1b.3 | `storage/asset/sqlite.rs` | — | `SqliteAssetStore`, blob storage, dedup | — | compile |
| 3.1b.4 | — | `storage/payload.rs` | `AssetRef` variant | — | compile |
| 3.1b.5 | — | `StoredPayload::resolve()` | fetch + base64 for Image/Audio | — | compile |
| 3.1b.6 | `storage/asset/tests.rs` | — | — | — | store, dedup, resolve, privacy |

---

### Feature 3.2: Conversation Structure

**Problem**: Current model doesn't support parallel model responses, multi-step tool interactions, or comparing different response options.

**Solution**: Conversations as sequences of turns, each with one or more spans containing messages.

**Functional Requirements**:
- Conversation contains ordered turns (position in sequence)
- Each turn has one or more spans (parallel responses)
- Each span contains ordered messages (for multi-step flows)
- Span has role (user/assistant) identifying owner
- Message has role for multi-step support (assistant → tool → assistant)
- Message references content block for text
- Tool calls/results stored inline in message

**Use Cases Enabled**:
- Parallel model responses: Multiple spans at same turn, compare them
- Tool interactions: Single span contains assistant → tool_call → tool_result → response
- User edits: Edit creates new user span at same turn

**Acceptance Criteria**:
- [ ] Create conversation with turns and spans
- [ ] Span contains multiple messages (multi-step flow)
- [ ] Different spans at same turn can have different message counts
- [ ] Messages reference content blocks (text is searchable)
- [ ] Tool calls/results preserved in messages

**Microtask Details**:

| # | Create | Update | Implement | SQL | Test |
|---|--------|--------|-----------|-----|------|
| 3.2.1 | `storage/conversation/types.rs` | — | `TurnInfo`, `SpanInfo`, `MessageInfo`, `SpanRole`, `NewMessage` | — | compile |
| 3.2.2 | — | schema/migrations | — | `turns` table, unique seq, idx | fresh DB |
| 3.2.3 | — | schema/migrations | — | `ucm_spans` table, idx | fresh DB |
| 3.2.4 | — | schema/migrations | — | `ucm_messages` table, FK content_id | fresh DB |
| 3.2.5 | — | schema/migrations | — | `views`, `view_selections` tables | fresh DB |
| 3.2.6 | `storage/conversation/types.rs` | — | `TurnStore` trait (signatures) | — | compile |
| 3.2.7 | `storage/conversation/sqlite.rs` | — | `add_turn()`, `get_turns()`, `get_turn()` | — | compile |
| 3.2.8 | — | sqlite.rs | `add_span()`, `get_spans()`, `get_span()` | — | compile |
| 3.2.9 | — | sqlite.rs | `add_message()`, `get_messages()`, `get_message()` | — | compile |
| 3.2.10 | `storage/conversation/tests.rs` | — | — | — | chain, multi-span, tool flow |
| 3.2.11 | — | session/sqlite.rs | Wire existing write paths to TurnStore | — | compile |
| 3.2.12 | — | — | 🔧 User: Run app via `noema`, send messages | — | E2E verify |
| 3.2.13 | — | — | 🔧 User: SQL verify `SELECT * FROM turns` shows data | — | data verify |

---

### Feature 3.3: Views, Forking, and Migration

**Problem**: No way to branch conversations, compare different paths, or edit mid-conversation. Additionally, legacy dual-write adds complexity and technical debt.

**Solution**: Views select one span per turn, creating named paths through the conversation. Complete the migration to the new model by replacing dual-write with TurnStore-only writes and removing legacy tables.

**Functional Requirements**:
- Views select which span to use at each turn
- Main view is default (created with conversation)
- Fork creates new view sharing selections up to fork point
- Span selection affects subsequent context
- Views are cheap (just selection pointers, content not duplicated)
- Session integration: `commit()` and `open_conversation()` use TurnStore exclusively
- Legacy cleanup: Remove old tables and ConversationStore trait

**Use Cases Enabled**:
- Fork conversation: Branch from turn 3, explore different direction
- Edit and splice: New span at turn 3, reuse turns 4-5 from original
- A/B comparison: Two views selecting different spans
- Clean codebase: No legacy code paths, single conversation model

**Acceptance Criteria**:
- [ ] Create view for conversation
- [ ] View selects spans, forming coherent path
- [ ] Fork view at turn N shares turns 1..(N-1)
- [ ] Forked view can select different spans after fork point
- [ ] Get view path returns selected span messages in order
- [ ] Session commit() writes only to TurnStore tables
- [ ] Session open_conversation() reads from main view path
- [ ] Legacy tables dropped, no dual-write code remains
- [ ] Fresh install works with new model only

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

**Microtask Details**:

| # | Create | Update | Implement | SQL | Test |
|---|--------|--------|-----------|-----|------|
| 3.4.1 | `storage/document/types.rs` | — | `DocumentInfo`, `TabInfo`, `RevisionInfo`, `DocumentSource` | — | compile |
| 3.4.2 | — | schema/migrations | — | `documents` table | fresh DB |
| 3.4.3 | — | schema/migrations | — | `document_tabs` table | fresh DB |
| 3.4.4 | — | schema/migrations | — | `revisions` table, FK content_id | fresh DB |
| 3.4.5 | `storage/document/mod.rs` | — | `DocumentStore` trait | — | compile |
| 3.4.6 | `storage/document/sqlite.rs` | — | `create()`, `get()`, `find_by_source()`, `list()`, `delete()` | — | compile |
| 3.4.7 | — | sqlite.rs | `add_tab()`, `add_tab_from_content()`, `get_tabs()`, `move_tab()` | — | compile |
| 3.4.8 | — | sqlite.rs | `commit()`, `branch()`, `checkout()`, `get_revisions()`, `get_content()` | — | compile |
| 3.4.9 | — | sqlite.rs | `promote_from_message()` reuses content_id | — | compile |
| 3.4.10 | `storage/document/tests.rs` | — | — | — | hierarchy, revisions, branch, promote |

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

**Microtask Details**:

| # | Create | Update | Implement | SQL | Test |
|---|--------|--------|-----------|-----|------|
| 3.5.1 | `storage/collection/types.rs` | — | `CollectionInfo`, `ItemInfo`, `CollectionViewInfo`, `ItemTarget`, `FieldDefinition`, `ViewConfig`, `ViewType` | — | compile |
| 3.5.2 | — | schema/migrations | — | `collections` table | fresh DB |
| 3.5.3 | — | schema/migrations | — | `collection_items` table, indexes | fresh DB |
| 3.5.4 | — | schema/migrations | — | `item_fields` table, idx | fresh DB |
| 3.5.5 | — | schema/migrations | — | `item_tags` table, idx | fresh DB |
| 3.5.6 | — | schema/migrations | — | `collection_views` table | fresh DB |
| 3.5.7 | `storage/collection/mod.rs` | — | `CollectionStore` trait | — | compile |
| 3.5.8 | `storage/collection/sqlite.rs` | — | `create()`, `get()`, `update_schema_hint()`, `delete()` | — | compile |
| 3.5.9 | — | sqlite.rs | `add_item()`, `move_item()`, `remove_item()`, `get_items()` tree | — | compile |
| 3.5.10 | — | sqlite.rs | `update_item_fields()`, `reindex_item_fields()`, `tag()`, `untag()`, `find_by_tag()` | — | compile |
| 3.5.11 | — | sqlite.rs | `create_view()`, `query_view()` with filter/sort | — | compile |
| 3.5.12 | `storage/collection/tests.rs` | — | — | — | tree, reorder, tags, fields, views |

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

**Microtask Details**:

| # | Create | Update | Implement | SQL | Test |
|---|--------|--------|-----------|-----|------|
| 3.6.1 | `storage/reference/types.rs` | — | `ReferenceInfo`, `EntityRef { entity_type, entity_id }` | — | compile |
| 3.6.2 | — | schema/migrations | — | `references` table, UNIQUE, indexes | fresh DB |
| 3.6.3 | `storage/reference/mod.rs` | — | `ReferenceStore` trait | — | compile |
| 3.6.4 | `storage/reference/sqlite.rs` | — | `create()`, `delete()` | — | compile |
| 3.6.5 | — | sqlite.rs | `get_outgoing(from)` → `Vec<ReferenceInfo>` | — | compile |
| 3.6.6 | — | sqlite.rs | `get_backlinks(to)` → `Vec<ReferenceInfo>` | — | compile |
| 3.6.7 | `storage/reference/tests.rs` | — | — | — | create, outgoing, backlinks, delete |

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

**Microtask Details**:

| # | Create | Update | Implement | SQL | Test |
|---|--------|--------|-----------|-----|------|
| 3.7.1 | — | schema/migrations | — | `idx_*_created` on content_blocks, messages, revisions; `idx_conversations_updated` | fresh DB |
| 3.7.2 | `storage/temporal/mod.rs` | — | `TemporalStore` trait, `TemporalContent`, `ActivitySummary`, `ContentType` | — | compile |
| 3.7.3 | `storage/temporal/sqlite.rs` | — | `query_by_time_range()` across entities, filter, limit | — | compile |
| 3.7.4 | — | sqlite.rs | `get_activity_summary()` counts, active conversations | — | compile |
| 3.7.5 | — | sqlite.rs | `render_activity_context()` markdown, headers, timestamps, token budget | — | compile |
| 3.7.6 | `storage/temporal/tests.rs` | — | — | — | range query, summary, render |

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
