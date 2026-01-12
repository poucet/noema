# Phase 3: Journal

Chronological stream of thoughts, changes, and observations.

---

## Context from Phase 2

- Feature #26 (@-mention search) completed
- Remaining Phase 2 features deferred to Phase 4
- Design doc exists: `design/UNIFIED_CONTENT_MODEL.md`

See `../02/HANDOFF.md` for full context.

---

## 2026-01-10: Design Doc Extended

Extended `UNIFIED_CONTENT_MODEL.md` with detailed feature requirements:

- **FR-1: Content Storage** - ContentBlock (text, content-addressed) and Asset (binary)
- **FR-2: Conversation Structure** - Turns, alternatives as spans, messages, views
- **FR-3: Document Structure** - Revisions forming DAG
- **FR-4: Collection Structure** - Tree with items, tags, fields
- **FR-5: Cross-References** - Any-to-any with backlinks
- **FR-6: Views and Queries** - List, tree, table views
- **FR-7: Agent Context** - Templates with context injection
- **FR-8: Import/Export** - JSON and Markdown

Added SQL schemas for all tables, Rust trait signatures for ConversationStore, and implementation phases (3a-3d).

Key design decisions captured:
- Alternatives are spans of messages (not single messages)
- ContentBlock is text-only, Assets are binary-only
- Tool calls/results stay inline in messages
- Views select paths through alternatives

---

## 2026-01-10: IDEAS Vetting Against UCM & Hook System

Reviewed new IDEAS (#1, #4-12) against existing designs:

**Fully Covered:**
- #5 Dynamic Typst Functions → `is_dynamic` flag + `render.before.*` hooks
- #6 Proactive Check-ins → `temporal.idle.*` and `temporal.scheduled.*` triggers
- #8 Auto-journaling → `entity.created.message` hook + enqueue action
- #10 Reflexes → This IS the hook system (Input/Time/Context = hooks by type)
- #7 Endless Conversation → Views + context strategies (EP-5)

**Partially Covered (extension points exist):**
- #1 Access Control → `is_private` exists; ACL tables can be added later
- #4 Local Filesystem → `DocumentSource::Import` + asset `local_path`; bidirectional sync as future extension
- #9 Active Context Engine → Hooks provide foundation; nudge UI is future feature
- #11 Soft Schemas → Collections with advisory schema_hint + tags; tag inheritance can be added

**Not a Gap (naming):**
- #12 Neuro Nomenclature → Alignment opportunity, not structural change

**Conclusion:** UCM and Hook System designs are already future-proof for all new IDEAS. No changes needed to Phase 3 scope.

---

## 2026-01-10: Microtasks Format Finalized

Restructured TASKS.md microtasks for clarity:

1. **Compact tables** - Removed verbose per-task tables, kept simple `| Status | # | Task |` format
2. **Emoji prefixes** for commit categorization:
   - 🏗️ types/traits
   - 📦 schema/migration
   - ⚡ implementation
   - ✅ tests
   - 🔧 integration
   - 🧹 cleanup
3. **Detailed DoD** moved under Feature Details section with Create/Update/Implement/SQL/Test columns

Total: 77 microtasks across 10 features.

---

## 2026-01-10: Feature 3.1 Content Blocks Implementation

Started implementation of Content Block storage.

### Commits

1. **🏗️ Add storage/ids.rs with typed ID newtypes** (3.1.1)
   - Created `define_id!` macro for consistent ID newtype pattern
   - Defined all UCM IDs: ContentBlockId, AssetId, ConversationId, TurnId, SpanId, MessageId, ViewId, DocumentId, TabId, RevisionId, CollectionId, CollectionItemId, ReferenceId, UserId
   - Includes serde, Display, Hash, From impls

2. **🏗️ Add content origin types for provenance tracking** (3.1.2)
   - `OriginKind` enum: User, Assistant, System, Import
   - `ContentOrigin` struct with user_id, model_id, source_id, parent_id
   - `ContentType` enum: Plain, Markdown, Typst
   - Builder methods for each origin type

3. **🏗️ Add ContentBlockStore trait with async methods** (3.1.3)
   - `ContentBlock` - input struct with text, content_type, is_private, origin
   - `StoredContentBlock` - wraps ContentBlock + id, hash, created_at
   - `StoreResult` - id, hash, is_new flag for dedup feedback
   - `ContentBlockStore` trait: store(), get(), get_text(), exists(), find_by_hash()
   - User feedback led to shared schema between input/stored forms

4. **📦 Add content_blocks schema with hash, origin, privacy** (3.1.4)
   - Table with origin fields flattened (origin_kind, origin_user_id, origin_model_id, etc.)
   - Indexes: hash (dedup), origin (queries), private (filter), created (temporal)

5. **⚡ Implement SqliteContentBlockStore with SHA-256 deduplication** (3.1.5 + 3.1.7)
   - Full trait implementation for SqliteStore
   - SHA-256 hashing moved to `helper::content_hash()` for reuse
   - Hash-based deduplication on store()
   - Comprehensive unit tests: store/get, deduplication, origin, privacy

### Design Decisions

- **ContentBlock vs StoredContentBlock**: User feedback that "NewContentBlock" naming was confusing. Refactored to share schema - `ContentBlock` is input, `StoredContentBlock` wraps it with metadata.

- **content_hash in helper module**: Initially in sqlite.rs, moved to helper.rs since SHA-256 hashing isn't sqlite-specific.

---

## 2026-01-10: Feature 3.1 Integration Complete

Added integration tasks to ensure content blocks are actually used by the app.

### Integration Commits

6. **📦 Add content_id column to span_messages table** (3.1.8)
   - Added `content_id TEXT REFERENCES content_blocks(id)` to span_messages
   - Added index on content_id for joins
   - Updated TASKS.md with integration tasks (3.1.8-3.1.10)

7. **🔧 Store message text in content_blocks on write** (3.1.9)
   - Added `store_content_sync()` helper that takes `&Connection` directly
   - Updated `write_as_span()` to store text in content_blocks and set content_id
   - Updated `write_parallel_responses()` similarly
   - Origin tracking: role → origin_kind, user_id, model_id populated

### Approach

- **Dual-write for now**: Text is stored in both content_blocks AND text_content/content JSON
- **Read path unchanged**: App continues reading from existing columns
- **Verification**: Send messages → content_blocks table populated → conversations still work
- **Future**: Can remove text duplication once confident in content_blocks

### Feature 3.1 Complete

All 10 tasks done (3.1.6 tags deferred to 3.5 Collections as not critical for integration path).

---

## 2026-01-10: Feature 3.1b Asset Storage Implementation

Implemented asset storage following the ContentBlock pattern:

### Changes

1. **storage/asset/mod.rs** - Rewrote with UCM-aligned types:
   - `Asset` - input form with mime_type, size_bytes, filename, local_path, is_private
   - `StoredAsset` - wraps Asset with id and created_at
   - `AssetStoreResult` - id + is_new flag for dedup feedback
   - `AssetStore` trait: store(), get(), exists(), delete()
   - Removed legacy types (no backwards compat needed during dev rush)

2. **storage/asset/sqlite.rs** - Implemented SqliteAssetStore:
   - Updated schema with is_private column and indexes
   - Full trait implementation
   - Unit tests for all operations

### Notes

- AssetRef already existed in StoredContent (task 3.1b.4)
- StoredPayload.resolve() already handles asset resolution (task 3.1b.5)
- AssetId is the SHA-256 hash (provided by caller from BlobStore)

**Status**: Code complete, awaiting user verification (compile, tests, E2E).

---

## 2026-01-10: StorageCoordinator Implementation

Added `StorageCoordinator` to automatically externalize inline images/audio to blob/asset storage when messages are persisted.

### Architecture

The coordinator is **session-agnostic** - it only knows about `BlobStore` and `AssetStore` traits, not SQLite or any specific session implementation.

**Key Components**:
- `DynStorageCoordinator` - Type-erased version using `Arc<dyn BlobStore>` and `Arc<dyn AssetStore>`
- `StorageCoordinator<B, A>` - Generic version for when concrete types are known

**Flow**:
1. App init creates `DynStorageCoordinator` with blob_store and asset_store (SqliteStore implements AssetStore)
2. Coordinator is set on `SqliteStore` using interior mutability (`RwLock`)
3. When sessions are created, they receive the coordinator
4. During `write_as_span()` and `write_parallel_responses()`, payloads are processed through `coordinator.externalize_assets()`
5. Inline `Image { data, mime_type }` and `Audio { data, mime_type }` are converted to `AssetRef { asset_id, ... }`

### Key Files Changed

- [noema-core/src/storage/coordinator.rs](noema-core/src/storage/coordinator.rs) - NEW: `DynStorageCoordinator` and `StorageCoordinator`
- [noema-core/src/storage/mod.rs](noema-core/src/storage/mod.rs) - Added `pub mod coordinator`
- [noema-core/src/storage/session/sqlite.rs](noema-core/src/storage/session/sqlite.rs):
  - `SqliteStore.coordinator` field with `RwLock` for interior mutability
  - `set_coordinator()` and `coordinator()` methods
  - `SqliteSession` receives coordinator from store
  - `write_as_span()` and `write_parallel_responses()` now async, call `coordinator.externalize_assets()`
- [noema-desktop/src-tauri/src/commands/init.rs](noema-desktop/src-tauri/src/commands/init.rs):
  - Creates `DynStorageCoordinator` during `init_storage()`
  - Sets coordinator on store before storing in AppState

### Design Decisions

1. **Interior mutability for coordinator** - Using `RwLock` allows setting coordinator after store is wrapped in `Arc`
2. **Async write methods** - `write_as_span()` and `write_parallel_responses()` are now async to support coordinator's async blob storage
3. **Graceful fallback** - If externalization fails, falls back to storing inline data with warning

**Status**: Code complete, pending compilation and verification.

---

## 2026-01-10: Context Handoff Notes (Previous)

**Status**: Feature 3.1 Content Blocks is fully complete and integrated. Feature 3.1b Asset Storage code complete, pending verification.

**Next Steps** (in priority order):
1. **Verify 3.1b** - Compile, run tests, E2E check
2. **3.2 Conversation Structure** (P0) - Turns, spans, messages with content references

---

## 2026-01-10: Feature 3.1b Asset Storage Complete

User verified E2E functionality:
- App runs via `noema`
- Image attachment works - displays correctly in conversation
- Assets table populated with new rows

**Feature 3.1b Complete** - All 10 tasks done.

**Summary of 3.1b Implementation**:
- `AssetStore` trait with store/get/exists/delete operations
- `SqliteAssetStore` implementation with SHA-256 hash-based deduplication
- `StorageCoordinator` automatically externalizes inline images/audio to blob storage
- Integration with existing session write paths (`write_as_span()`, `write_parallel_responses()`)

**Next**: Feature 3.2 Conversation Structure (P0)

**Key Files for Next Context**:
- [noema-core/src/storage/content_block/sqlite.rs](noema-core/src/storage/content_block/sqlite.rs) - ContentBlockStore impl, `store_content_sync()` helper
- [noema-core/src/storage/content_block/types.rs](noema-core/src/storage/content_block/types.rs) - Origin types
- [noema-core/src/storage/ids.rs](noema-core/src/storage/ids.rs) - Type-safe ID newtypes
- [noema-core/src/storage/helper.rs](noema-core/src/storage/helper.rs) - `content_hash()` function
- [noema-core/src/storage/session/sqlite.rs](noema-core/src/storage/session/sqlite.rs) - Integration point, `write_as_span()`
- [noema-core/src/storage/coordinator.rs](noema-core/src/storage/coordinator.rs) - StorageCoordinator for asset externalization

**Pattern Established**:
- Build types/traits → schema → impl → tests → **integration into existing system**
- Each feature should wire into existing app, not build parallel systems
- Dual-write pattern for safe migration (write to both old and new, read from old)

---

## 2026-01-10: Feature 3.2 Conversation Structure Started

### 3.2.1 Types Definition Complete

Created `storage/conversation/types.rs` with UCM conversation hierarchy:

**Types Added:**
- `SpanRole` - user/assistant (identifies span owner)
- `MessageRole` - user/assistant/system/tool (for multi-step flows)
- `TurnInfo` - Position in conversation sequence
- `SpanInfo` - One response option at a turn (sequence of messages)
- `MessageInfo` - Individual content within a span
- `ViewInfo` - Path through spans (selects one per turn)
- `ViewSelection` - Selection of span at a turn
- `NewMessage` - Builder for creating messages
- `TurnWithContent`, `SpanWithMessages` - Composite query types

### Terminology Unification

Updated design doc and TASKS.md to use consistent terminology:
- "Alternative" → "Span" everywhere
- Schema: `alternatives` table → `spans` table
- Columns: `alternative_id` → `span_id`
- Trait methods: `add_alternative()` → `add_span()`, `select_alternative()` → `select_span()`

This aligns the design doc with the existing codebase naming (we already have `SpanInfo`).

### Schema Added (3.2.2-3.2.5)

Added new tables to sqlite schema (coexist with legacy):
- `turns` - positions in conversation sequence
- `ucm_spans` - alternative responses at a turn
- `ucm_messages` - individual messages within a span
- `views` - named paths through conversation
- `view_selections` - which span is selected at each turn

Tables use `ucm_` prefix to avoid conflict with existing `spans` table.

### TurnStore Trait and Implementation (3.2.6-3.2.9)

Created `TurnStore` trait in `types.rs` with methods for:
- Turn management: `add_turn`, `get_turns`, `get_turn`
- Span management: `add_span`, `get_spans`, `get_span`
- Message management: `add_message`, `get_messages`, `get_message`
- View management: `create_view`, `get_views`, `get_main_view`, `select_span`, `get_selected_span`, `get_view_path`, `fork_view`
- Convenience: `add_user_turn`, `add_assistant_turn`

Full SQLite implementation added. Messages store text in `content_blocks` table for searchability.

### Unit Tests (3.2.10)

Comprehensive tests added:
- Schema creation verification
- Turn sequencing
- Span creation with models
- Multi-message spans
- Multiple spans at same turn (parallel responses)
- View creation and span selection
- Convenience methods

### Next Steps

- 3.2.11: Wire existing write paths to TurnStore (dual-write)
- 3.2.12-13: User verification in app and SQL

---

## 2026-01-10: Table Naming Cleanup

Renamed tables to avoid `ucm_` prefix and use cleaner naming:

### Legacy Tables (renamed)
- `spans` → `legacy_spans` (references `span_set_id`)
- `span_messages` → `legacy_span_messages` (references `legacy_spans`)

### New Tables (clean names)
- `turns` - positions in conversation sequence
- `spans` - alternative responses at a turn (references `turn_id`)
- `messages` - individual messages within a span (references `spans`)
- `views` - named paths through conversation
- `view_selections` - span selections per turn per view

This allows the new structure to use intuitive table names while preserving backwards compatibility with the existing legacy schema during migration.

---

## 2026-01-10: Dual-Write Integration (3.2.11)

Implemented dual-write for session write paths. When messages are committed, they now write to both:
- **Legacy tables**: `threads`, `span_sets`, `legacy_spans`, `legacy_span_messages`
- **New tables**: `turns`, `spans`, `messages`, `views`, `view_selections`

### Key Changes

1. **[noema-core/src/storage/conversation/sqlite.rs](noema-core/src/storage/conversation/sqlite.rs)** - Added `sync_helpers` module with synchronous functions for writing to TurnStore tables:
   - `ensure_main_view()` - Creates main view if it doesn't exist
   - `add_turn_sync()` - Creates turn with sequence number
   - `add_span_sync()` - Creates span for a turn
   - `add_message_sync()` - Creates message with content block storage
   - `select_span_sync()` - Selects span in view

2. **[noema-core/src/storage/session/sqlite.rs](noema-core/src/storage/session/sqlite.rs)** - Updated `write_as_span()` and `write_parallel_responses()`:
   - Both methods now dual-write to legacy and new tables
   - Main view is auto-created for new conversations
   - Spans are auto-selected in the main view
   - Tool calls/results extracted to separate JSON columns

3. **[noema-core/src/storage/content.rs](noema-core/src/storage/content.rs)** - Added helper methods:
   - `tool_calls_json()` - Extracts tool calls as JSON
   - `tool_results_json()` - Extracts tool results as JSON

### Design Notes

- **Synchronous helpers**: The TurnStore trait uses async, but session write paths need sync access within mutex guards. Created sync helper functions that take `&Connection` directly.
- **Auto-selection**: New spans are automatically selected in the main view, matching legacy behavior where `selected_span_id` is set on span_sets.
- **Graceful degradation**: Errors writing to new tables are logged as warnings but don't fail the operation, ensuring backwards compatibility.

### Next Steps

- **3.2.12**: User E2E verification - run app, send messages, verify conversations work
- **3.2.13**: SQL verification - `SELECT * FROM turns`, `SELECT * FROM spans`, `SELECT * FROM messages` should show data

---

## 2026-01-12: Feature 3.2 Complete

### Verification Results (3.2.12-3.2.13)

User verified E2E and SQL:
- App runs, messages send/receive correctly
- All UCM tables populated:

```sql
-- turns: 1 row with conversation_id, role=user, sequence=0
-- spans: 1 row linked to turn
-- messages: 2 rows (user + assistant) with content_ids
-- views: main view created
-- view_selections: span selected for turn
```

### Bug Fix: Role::as_str()

Added `Role::as_str() -> &'static str` method to `llm::Role` enum. `ToString` now delegates to it. Keeps serialization logic with the type definition, not in business logic.

**Feature 3.2 Complete** - All 13 tasks done.

**Next**: Feature 3.3 Views and Forking (3.3.7 `edit_turn` remaining)

---
