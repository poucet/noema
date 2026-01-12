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

## 2026-01-12: Feature 3.3.7 Edit Turn Implementation

### Module Refactoring

Refactored `storage/conversation/` module for clearer organization:

**Before:**
- `mod.rs` - Trait + legacy types + re-exports
- `types.rs` - New TurnStore types + trait

**After:**
- `mod.rs` - Just re-exports
- `types.rs` - All types (legacy and new)
- `conversation_store.rs` - ConversationStore trait (legacy)
- `turn_store.rs` - TurnStore trait (new)
- `sqlite.rs` - Both implementations

### Legacy Type Renaming

Renamed legacy types with `Legacy` prefix to make the distinction clear:
- `SpanType` → `LegacySpanType`
- `ConversationInfo` → `LegacyConversationInfo`
- `ThreadInfo` → `LegacyThreadInfo`
- `SpanInfo` → `LegacySpanInfo` (for legacy, new SpanInfo is in types.rs)
- `SpanSetInfo` → `LegacySpanSetInfo`
- `SpanSetWithContent` → `LegacySpanSetWithContent`

Updated all usages in:
- `noema-core/src/storage/conversation/sqlite.rs`
- `noema-core/src/storage/session/sqlite.rs`
- `noema-desktop/src-tauri/src/commands/chat.rs`
- `noema-desktop/src-tauri/src/types.rs`

### New TurnStore Methods (3.3.7)

Added three new methods to `TurnStore` trait:

1. **`edit_turn()`** - Creates a new span at an existing turn with new content
   - Optionally creates a forked view that selects the new span
   - Useful for regeneration (same turn, new span) and user edit (fork + new span)

2. **`fork_view_with_selections()`** - Forks a view with custom span selections
   - Enables "splicing" - reusing spans from original path after an edit
   - Use case: Edit turn 3, but reuse turns 4-5 from original conversation

3. **`get_view_context_at()`** - Gets view path up to (but not including) a specific turn
   - Returns all turns with selected spans before the specified turn
   - Useful for building context when editing mid-conversation

### Tests Added

Comprehensive tests for new methods:
- `test_edit_turn()` - Edit without fork, verify span selection
- `test_edit_turn_with_fork()` - Edit with fork, verify original unchanged
- `test_get_view_context_at()` - Context retrieval at various points
- `test_fork_view_with_selections()` - Custom splicing behavior

### Technical Debt Discussion

User raised concern about legacy system coexisting too long (3.4-3.7 before migration in 3.8-3.9). Consider:
- Complete 3.3 (core Turn/Span/Message model)
- Do 3.8 (Session Integration) and 3.9 (Migration) immediately
- Build 3.4-3.7 (Documents, Collections, etc.) cleanly on new foundation

**Feature 3.3.7 Complete**

---

## 2026-01-12: Feature 3.3.9 Schema Redesign (Message Content)

### Problem Identified

Initial approach tried to store messages with a single `content TEXT` JSON blob. User feedback:
1. `NewMessage` type reinvents role and doesn't cover multimodal
2. Storing protobufs as JSON is not ideal - should map to DB structure
3. Binary content (images, audio) in tool calls/results needs handling
4. Text should be stored in `content_blocks` for deduplication and search

### Solution: Normalized message_content Table

Each message now has content stored in a separate `message_content` table where each row is one `StoredContent` item:

**Schema:**
```sql
messages (
    id, span_id, sequence_number, role, created_at
    -- No content column
)

message_content (
    id, message_id, sequence_number, content_type,
    -- For text: reference to content_blocks (shared, searchable)
    content_block_id,
    -- For asset_ref: blob storage reference
    asset_id, mime_type, filename,
    -- For document_ref: RAG reference
    document_id, document_title,
    -- For tool_call/tool_result: structured JSON
    tool_data
)
```

**Type Changes:**
- `MessageInfo` no longer has content field
- `MessageWithContent` - message + content items
- `MessageContentInfo` - single content item row
- `MessageContentData` - enum for content variants (Text, AssetRef, DocumentRef, ToolCall, ToolResult)
- `ContentType` - discriminator enum
- Removed `NewMessage` - `add_message()` now takes `(role, &[StoredContent])`
- `TurnWithContent` now has `Vec<MessageWithContent>` instead of `Vec<MessageInfo>`

**Benefits:**
- Text content goes to `content_blocks` (shared with documents, searchable)
- `StoredContent` maps directly to/from DB rows
- Ordering preserved via `sequence_number`
- No JSON blob for structured content
- Asset externalization layer above DB can convert inline binary to refs

### Implementation Complete

Updated TurnStore implementation:
- `add_message()` now takes `(role: MessageRole, content: &[StoredContent])`
- Added `get_messages_with_content()` for loading messages with their content
- Updated `edit_turn()` to use `Vec<(MessageRole, Vec<StoredContent>)>`
- Updated convenience methods to use new format
- Added `load_message_content()` helper for reading from message_content table
- Updated `sync_helpers::add_message_sync()` for new schema
- All tests updated for new API

**Next Steps:**
- Refactor StoredContent to be refs-only
- Update SqliteSession to use TurnStore directly (remove dual-write)
- Remove legacy tables and ConversationStore trait
- Update engine and commands

---

## 2026-01-12: StoredContent Refs-Only Redesign

### Problem Identified

Looking at `storage/content.rs`, `StoredContent` has both inline variants (`Text`, `Image`, `Audio`) and ref variants (`AssetRef`, `DocumentRef`). This creates issues:

1. **Tight coupling**: TurnStore implementation is entangled with content_block storage
2. **Inconsistent representation**: Some content inline, some as refs
3. **No single source of truth**: Text stored inline but should reference `content_blocks` for deduplication/search

User feedback:
> "I wonder if StoredContent being what's actually stored should represent text as refs to content_blocks"
> "StoredContent should have easy ways of resolving refs before sending back to UI or to the LLM"

### Solution: Refs-Only StoredContent

`StoredContent` becomes **what is actually stored in the database** - all refs, no inline data:

**Before:**
```rust
pub enum StoredContent {
    Text { text: String },           // Inline
    Image { data: String, ... },     // Inline base64
    Audio { data: String, ... },     // Inline base64
    AssetRef { asset_id, ... },      // Ref
    DocumentRef { id, title },       // Ref
    ToolCall(ToolCall),
    ToolResult(ToolResult),
}
```

**After:**
```rust
pub enum StoredContent {
    TextRef { content_block_id: ContentBlockId },  // Ref to content_blocks
    AssetRef { asset_id, mime_type, filename },    // Ref to blob storage
    DocumentRef { id, title },                     // Ref to documents
    ToolCall(ToolCall),                            // Inline (structured JSON)
    ToolResult(ToolResult),                        // Inline (structured JSON)
}
```

### Resolution Layer

Add a `ContentResolver` trait for converting refs back to full content:

```rust
#[async_trait]
pub trait ContentResolver: Send + Sync {
    async fn get_text(&self, id: &ContentBlockId) -> Result<String>;
    async fn get_asset(&self, id: &str) -> Result<(Vec<u8>, String)>; // (data, mime_type)
}

impl StoredContent {
    /// Resolve this ref to a ContentBlock for LLM/UI
    pub async fn resolve<R: ContentResolver>(&self, resolver: &R) -> Result<ContentBlock> {
        match self {
            StoredContent::TextRef { content_block_id } => {
                let text = resolver.get_text(content_block_id).await?;
                Ok(ContentBlock::Text { text })
            }
            StoredContent::AssetRef { asset_id, mime_type, .. } => {
                let (data, _) = resolver.get_asset(asset_id).await?;
                let encoded = STANDARD.encode(&data);
                if mime_type.starts_with("image/") {
                    Ok(ContentBlock::Image { data: encoded, mime_type: mime_type.clone() })
                } else if mime_type.starts_with("audio/") {
                    Ok(ContentBlock::Audio { data: encoded, mime_type: mime_type.clone() })
                } else {
                    // Other types handled as needed
                }
            }
            StoredContent::DocumentRef { id, title } => {
                Ok(ContentBlock::DocumentRef { id: id.clone(), title: title.clone() })
            }
            StoredContent::ToolCall(call) => Ok(ContentBlock::ToolCall(call.clone())),
            StoredContent::ToolResult(result) => Ok(ContentBlock::ToolResult(result.clone())),
        }
    }
}
```

### Storage Coordination

The `StorageCoordinator` handles the inverse - converting input content to stored refs:

1. **Text input** → Store in `content_blocks` → Create `StoredContent::TextRef`
2. **Inline image/audio** → Store in blob/assets → Create `StoredContent::AssetRef`
3. **DocumentRef, ToolCall, ToolResult** → Pass through

This keeps the DB layer simple (just storing/loading refs) while the coordination layer above handles conversion.

### Benefits

- **Clean separation**: DB layer stores refs, resolution layer converts
- **DB-agnostic**: Same `StoredContent` works for any backend
- **Deduplication**: All text goes through `content_blocks`
- **Search**: Text refs can be joined with searchable content
- **Single source of truth**: `StoredContent` = what's in DB

### Impact on Existing Code

- `StoredPayload::resolve()` → Replaced by `StoredContent::resolve()` with trait
- `From<ContentBlock> for StoredContent` → Removed (coordinator handles this)
- `TryFrom<StoredContent> for ContentBlock` → Replaced by async resolve
- `MessageContentData` in types.rs → Already uses refs, will align with StoredContent

### Implementation Complete

Updated files:
- [content.rs](noema-core/src/storage/content.rs) - Refs-only `StoredContent`, `ContentResolver` trait
- [coordinator.rs](noema-core/src/storage/coordinator.rs) - Updated to use `store_content()` API
- [conversation/types.rs](noema-core/src/storage/conversation/types.rs) - `MessageContentData::DocumentRef` now ID-only
- [conversation/sqlite.rs](noema-core/src/storage/conversation/sqlite.rs) - Updated for `TextRef`, removed `document_title`

Key changes:
1. `StoredContent::Text { text }` → `StoredContent::TextRef { content_block_id }`
2. `StoredContent::Image/Audio` → Removed (always use `AssetRef`)
3. `StoredContent::DocumentRef { id, title }` → `StoredContent::DocumentRef { document_id }` (title looked up separately)
4. `StorageCoordinator.store_content()` takes `Vec<ContentBlock>` and returns `Vec<StoredContent>`
5. `ContentResolver` trait enables resolution of refs back to `ContentBlock`
6. `TurnStore` convenience methods (`add_user_turn`, `add_assistant_turn`) store text in content_blocks first

Desktop code (chat.rs, types.rs) still uses legacy API - will be updated when removing dual-write.

### Type Consolidation

Removed redundant types:
- **ContentType enum** - Removed. Content type strings ("text", "asset_ref", etc.) used directly in SQL.
- **MessageContentData** - Removed. `MessageContentInfo` now uses `StoredContent` directly.
- **MessageRole vs llm::Role** - `MessageRole` kept for now with Tool variant for explicit tool messages.

`MessageContentInfo` simplified to use `StoredContent` directly instead of a separate `MessageContentData` enum.

---

## 2026-01-12: Legacy Removal Complete (noema-core)

### Session Rewrite Complete

Rewrote `noema-core/src/storage/session/sqlite.rs` to use TurnStore exclusively:
- Removed all legacy dual-write code
- Session now uses TurnStore methods directly via SqliteStore
- `write_turn()` and `write_parallel_turn()` use TurnStore API
- `store_message_content()` uses coordinator for content externalization
- `open_conversation()` loads via `get_view_path()`

### ConversationStore Trait Removed

Deleted files and cleaned up:
- **Deleted**: `conversation/conversation_store.rs` (entire file)
- **Cleaned**: `conversation/mod.rs` - removed legacy exports
- **Cleaned**: `conversation/types.rs` - removed all `Legacy*` types

### Legacy Tables Removed

Updated `conversation/sqlite.rs` schema:
- Removed: `threads`, `span_sets`, `legacy_spans`, `legacy_span_messages`
- Kept only: `conversations`, `turns`, `spans`, `messages`, `message_content`, `views`, `view_selections`

### Compilation Fixes

Fixed several issues found during cargo check:
1. **uuid crate**: Made uuid non-optional in Cargo.toml (required by ids.rs everywhere)
2. **OriginKind Copy**: Added `Copy` derive to `OriginKind` enum
3. **with_origin API**: Fixed calls to use `ContentOrigin` struct instead of 3 parameters
4. **Role match exhaustive**: Fixed From<llm::api::Role> impl (Role only has 3 variants, no wildcard needed)

### noema-core Compiles Successfully

After fixes, `cargo check` passes with only minor warnings (unused helper functions).

### Desktop Still Broken

`noema-desktop` code references removed types:
- `ConversationStore` trait methods
- `LegacyConversationInfo`, `LegacyThreadInfo`, `LegacySpanType`
- Legacy table operations (threads, span_sets, etc.)

### Terminology Mapping (Legacy → New)

| Legacy Concept | New Concept | Notes |
|---------------|-------------|-------|
| Thread | View | Named path through conversation |
| SpanSet | Turn | Position in sequence |
| Span (in SpanSet) | Span (at Turn) | Alternative response |
| span_set_id | turn_id | ID type change |
| thread_id | view_id | ID type change |
| `get_main_thread_id()` | `get_main_view()` | Returns ViewInfo |
| `get_thread_span_sets()` | `get_turns()` | Returns Vec<TurnInfo> |
| `get_span_set_alternates()` | `get_spans(turn_id)` | Returns Vec<SpanInfo> |
| `create_fork_thread()` | `fork_view()` | Creates forked view |

### Desktop Rewrite Scope

The desktop commands need significant rewrite to use TurnStore API:
- `list_conversations` - Need to add to SqliteStore
- `delete_conversation` - Need to add to SqliteStore
- `rename_conversation` - Need to add to SqliteStore
- `get/set_conversation_private` - Need to add to SqliteStore
- `get_messages_with_alternates` - Rewrite using `get_view_path()`
- `switch_thread` → `switch_view` - Use `get_view_path()`
- `fork_from_span` - Use `fork_view()`
- `edit_user_message` - Use `edit_turn()`

**Key Question**: Before adding convenience methods to SqliteStore, should we:
1. Add methods directly to SqliteStore (simple, tight coupling)?
2. Create a higher-level service/facade that coordinates TurnStore + conversation management?
3. Have desktop call TurnStore directly with thin adapter?

User requested not to make strong decisions without input.

### Files Changed

- `noema-core/Cargo.toml` - uuid now required (not optional)
- `noema-core/src/storage/mod.rs` - Updated docs, removed dead exports
- `noema-core/src/storage/coordinator.rs` - Fixed `ContentOrigin` usage
- `noema-core/src/storage/content_block/types.rs` - Added `Copy` to `OriginKind`
- `noema-core/src/storage/conversation/mod.rs` - Simplified exports
- `noema-core/src/storage/conversation/types.rs` - Removed legacy types, fixed Role impl
- `noema-core/src/storage/conversation/turn_store.rs` - Updated docs
- `noema-core/src/storage/conversation/sqlite.rs` - Removed legacy tables/impl (large)
- `noema-core/src/storage/session/sqlite.rs` - Complete rewrite (large)
- **Deleted**: `noema-core/src/storage/conversation/conversation_store.rs`

---

## 2026-01-12: Design Review and Architecture Questions

### Design Observations Captured

Created `OBSERVATIONS.md` to track architectural questions raised during development.

**Questions for decision:**
1. **TurnStore Size** - Split into TurnStore + ViewStore, or keep unified?
2. **ConversationManagement Placement** - Moved to `conversation/mod.rs`, confirm or rollback?
3. **Session Abstraction** - DB-agnostic Session struct, or keep current SqliteSession design?
4. **Desktop Update Strategy** - Patch incrementally, rewrite commands, or hold until core stable?

### Implementation Work Done

- Added `ConversationManagement` trait to `conversation/mod.rs`
- Implemented for `SqliteStore`
- Re-exported from `session/mod.rs`
- Started desktop patches (can be reverted if direction changes)

### SqliteSession Foundation Verified

Session properly uses TurnStore:
- `write_turn` creates Turn → Span → Messages
- `write_parallel_turn` handles parallel model responses
- `open_conversation` loads via `get_main_view` → `get_view_path`
- Content resolution through StorageCoordinator

### Files Changed

- **noema-core/src/storage/conversation/mod.rs** - Added ConversationManagement trait
- **noema-core/src/storage/session/mod.rs** - Re-export ConversationManagement
- **noema-core/src/storage/session/sqlite.rs** - Implement ConversationManagement for SqliteStore
- **docs/noema-dev/0.2/phases/03/OBSERVATIONS.md** - Design observations and questions

### noema-core Status

Compiles cleanly with only warnings about unused helper functions.

---

## 2026-01-12: Session Abstraction Design

### Problem Analysis

Current `SqliteSession` has several issues:
1. Tightly coupled to SQLite - can't test without DB
2. Eagerly resolves content on load - no way to get refs for UI
3. Mixes runtime state with persistence logic
4. ChatEngine expects `Vec<ChatMessage>` but UI needs refs

### Design: DB-Agnostic Session + Resolution

Created comprehensive design in OBSERVATIONS.md with 4 layers:

1. **SessionBackend (trait)** - Persistence abstraction
   - `load_view_path()` returns `Vec<TurnWithContent>` (unresolved)
   - `commit_turn()` / `commit_parallel()` for writes
   - Implemented by `SqliteSessionBackend`, `MemorySessionBackend`

2. **Session<B>** - Pure runtime state
   - conversation_id, view_id, pending messages
   - NO storage coupling, NO resolution
   - Generic over backend

3. **LLMResolver / DisplayResolver** - On-demand resolution
   - LLM: resolves all refs to ContentBlock, uses DocumentFormatter
   - Display: resolves text only, keeps asset/doc refs for UI

4. **Integration** - ChatEngine uses Session + LLMResolver

### Key Decisions

- **Lazy resolution**: Content resolved only when needed, not on load
- **Consumer-specific**: Different resolution for UI vs LLM
- **Testable**: MemorySessionBackend for unit tests
- **DocumentFormatter**: Used by LLMResolver for @-mentions

### Next Steps

1. Implement `SessionBackend` trait
2. Implement `SqliteSessionBackend` (thin wrapper around TurnStore)
3. Create `Session<B>` struct
4. Implement resolvers
5. Update ChatEngine
6. Update desktop commands

---

## 2026-01-12: Session Implementation

### Implemented Files

- **noema-core/src/storage/session/types.rs** - New types:
  - `PendingMessage` - Message waiting to be committed (uses `StoredContent`)
  - `ResolvedMessage` - Message with resolved content
  - `ResolvedContent` - Text resolved, assets/docs cached lazily in-place

- **noema-core/src/storage/session/resolver.rs** - Resolution traits:
  - `ContentBlockResolver` - Resolves text refs from content_blocks table
  - `AssetResolver` - Resolves assets (base64) and documents (formatted text)

- **noema-core/src/storage/session/session.rs** - Main `Session<S: TurnStore>`:
  - `open()` - Load and resolve text once from TurnStore
  - `commit()` - Write pending messages as a turn
  - `commit_parallel()` - Write parallel responses
  - `messages_for_display()` - Returns `&[ResolvedMessage]` (sync)
  - `messages_for_llm()` - Resolves assets/docs lazily, caches in-place

- **noema-core/src/storage/session/mod.rs** - Updated exports

### Key Design Decisions

1. **No new backend trait** - Session uses `TurnStore` directly
2. **In-place caching** - `ResolvedContent::Asset/Document` have `resolved: Option<ContentBlock>`
3. **Single vector** - One `Vec<ResolvedContent>` serves both display and LLM
4. **MessageRole → llm::Role** - Tool messages map to User role for LLM API

### Compiles

`cargo check --package noema-core` passes with only warnings about unused helper functions.

---

## 2026-01-12: Legacy Session Code Removed

### Removed Legacy Code

Cleaned up old session API in favor of new `Session<S: TurnStore>`:

**Removed:**
- `memory.rs` - Legacy `MemorySession`/`MemoryTransaction`
- `SessionStore` trait and `StorageTransaction` trait from mod.rs
- `SqliteSession` and `SqliteTransaction` from sqlite.rs
- Legacy session creation methods (`create_conversation`, `open_conversation`)

**Kept:**
- `SqliteStore` - Concrete store implementing TurnStore, ConversationStore, UserStore, etc.
- `ConversationStore` implementation

### New Session API (Complete)

The new DB-agnostic session API is now the only session API:

- `Session<S: TurnStore>` - Generic session over any TurnStore
- `ResolvedContent` - Text resolved, assets/docs cached lazily in-place
- `ResolvedMessage` - Message with resolved content
- `PendingMessage` - Message waiting to be committed
- `ContentBlockResolver` - Resolves text refs from content_blocks
- `AssetResolver` - Resolves assets (base64) and documents (formatted)

### Key Files

- [session/mod.rs](noema-core/src/storage/session/mod.rs) - Clean exports
- [session/types.rs](noema-core/src/storage/session/types.rs) - Core types
- [session/resolver.rs](noema-core/src/storage/session/resolver.rs) - Resolution traits
- [session/session.rs](noema-core/src/storage/session/session.rs) - `Session<S>` implementation
- [session/sqlite.rs](noema-core/src/storage/session/sqlite.rs) - `SqliteStore` only

### Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                        Consumers                            │
├────────────────────────┬────────────────────────────────────┤
│     ChatEngine         │           Desktop UI               │
│  needs Vec<ChatMessage>│    needs Vec<ResolvedMessage>      │
└────────────┬───────────┴─────────────────┬──────────────────┘
             │                             │
             ▼                             ▼
┌────────────────────────┐   ┌────────────────────────────────┐
│    AssetResolver       │   │      (direct access)           │
│  - resolves assets     │   │  messages_for_display()        │
│  - formats documents   │   │                                │
└────────────┬───────────┘   └─────────────────┬──────────────┘
             │                                 │
             └────────────────┬────────────────┘
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                   Session<S: TurnStore>                     │
│  - conversation_id, view_id                                 │
│  - cache: Vec<ResolvedMessage> (in-place caching)           │
│  - pending: Vec<PendingMessage>                             │
└────────────────────────────┬────────────────────────────────┘
                             │
                             ▼
┌─────────────────────────────────────────────────────────────┐
│                    TurnStore (trait)                        │
│  - get_view_path() → Vec<TurnWithContent>                   │
│  - add_turn() + add_span() + add_message()                  │
│  - get_main_view() / create_view()                          │
└─────────────────────────────────────────────────────────────┘
```

### Next Steps

1. **Update `ChatEngine`** - Currently references old `SessionStore`/`StorageTransaction`
2. **Update desktop commands** - After engine is updated

---

## 2026-01-12: ConversationContext Made Async + Session Implements Directly

### API Changes

1. **ConversationContext trait** - Now async with guard pattern:
   ```rust
   #[async_trait]
   pub trait ConversationContext: Send + Sync {
       async fn messages(&mut self) -> Result<MessagesGuard<'_>>;
       fn len(&self) -> usize;
       fn add(&mut self, message: ChatMessage);
       fn pending(&self) -> &[ChatMessage];
       async fn commit(&mut self) -> Result<()>;
   }
   ```

2. **MessagesGuard** - Returns reference to cached `&[ChatMessage]`:
   - Avoids allocation on each `messages()` call
   - Session maintains `llm_cache: Vec<ChatMessage>` populated lazily
   - Guard derefs to `&[ChatMessage]`

3. **Agent trait** - Updated to use `dyn ConversationContext`:
   ```rust
   async fn execute(
       &self,
       context: &mut dyn ConversationContext,
       model: Arc<dyn ChatModel + Send + Sync>,
   ) -> Result<()>;
   ```

4. **Session** - Now implements `ConversationContext` directly:
   - Store is `Arc<S>` (shared, not owned)
   - `pending` is `Vec<ChatMessage>` (not `PendingMessage`)
   - `messages()` populates and returns guard to `llm_cache`
   - `add()` pushes to pending
   - `commit()` returns error (use `commit_pending()` with coordinator)

5. **ContentStorer trait** - For converting `ChatMessage` to `StoredContent`:
   ```rust
   pub trait ContentStorer: Send + Sync {
       async fn store_chat_message(&self, message: &ChatMessage) -> Result<Vec<StoredContent>>;
   }
   ```

### Files Changed

- `context.rs` - Async trait with `MessagesGuard`
- `agent.rs` - Updated trait signature
- `agents/simple_agent.rs` - Updated impl
- `agents/tool_agent.rs` - Updated impl
- `agents/mcp_agent.rs` - Updated impl
- `storage/session/session.rs` - `Session<S>` implements `ConversationContext`
- `storage/session/mod.rs` - Removed `AgentContext`, `PendingMessage`
- `engine.rs` - Simplified to use Session as ConversationContext

### Key Design Decisions

- **No AgentContext** - Session IS the context (no adapter)
- **Arc<S> for store** - Shared ownership, multiple sessions possible
- **Lazy LLM cache** - Text resolved on `messages()`, not on open
- **Pending = ChatMessage** - Agent adds `ChatMessage`, commit converts to `StoredContent`
- **commit() needs coordinator** - Can't implement fully in trait, use `commit_pending()`

### Compilation Status

`cargo check --package noema-core --package noema-ext` passes. Desktop needs separate update.

---

## 2026-01-12: Storage Module Restructure

### Problem

Storage implementation was tightly coupled to trait definitions. All stores were in domain-specific directories (asset/, blob/, content_block/, conversation/, document/, user/) mixing traits, types, and implementations.

User feedback highlighted several issues:
1. SQLite store shouldn't be under session/ - too much tight coupling
2. Need ability to use different backends for different stores (e.g., SQLite for assets, something else for conversations)
3. Need in-memory implementations for testing without SQLite
4. ContentBlockResolver should be derivable from ContentBlockStore, not a separate trait

### Solution: Clean Module Structure

Restructured storage module into three directories:

**`storage/traits/`** - All trait definitions:
- `AssetStore` - Asset metadata storage
- `BlobStore` - Content-addressable binary storage
- `ContentBlockStore` - Content-addressed text storage (added `require_text()` method)
- `ConversationStore` - Conversation-level CRUD
- `DocumentStore` - Document, tab, revision storage
- `TurnStore` - Turn/Span/Message conversation storage
- `UserStore` - User account management

**`storage/types/`** - All type definitions:
- Asset types (Asset, StoredAsset, AssetStoreResult)
- Blob types (StoredBlob)
- ContentBlock types (ContentBlock, StoredContentBlock, StoreResult, ContentOrigin, OriginKind)
- Conversation types (TurnInfo, SpanInfo, MessageInfo, ViewInfo, etc.)
- Document types (DocumentInfo, DocumentTabInfo, DocumentRevisionInfo, FullDocumentInfo)
- User types (UserInfo)

**`storage/implementations/`** - Backend implementations:
- `sqlite/` - SqliteStore implementing all traits (moved from scattered locations)
- `fs/` - FsBlobStore for filesystem blob storage
- `memory/` - In-memory implementations for testing:
  - `MemoryBlobStore` - Content-addressable blob storage with SHA-256
  - `MemoryContentBlockStore` - Content block storage with deduplication
  - `MemoryAssetStore` - Asset metadata storage

**`storage/document_resolver.rs`** - DocumentFormatter and DocumentResolver for RAG (restored from deleted module)

### Key Changes

1. **ContentBlockStore::require_text()** - New method that returns `Result<String>`, errors if not found. Replaces separate `ContentBlockResolver` trait.

2. **Clean re-exports from storage/mod.rs** - All commonly used items re-exported for convenience:
   ```rust
   pub use traits::{AssetStore, BlobStore, ContentBlockStore, ...};
   pub use types::{Asset, StoredAsset, ContentBlock, ...};
   pub use session::{Session, ResolvedContent, ResolvedMessage, ...};
   pub use implementations::sqlite::SqliteStore;
   pub use implementations::fs::FsBlobStore;
   ```

3. **In-memory stores** - Thread-safe using `Mutex`, comprehensive test coverage. Enable unit testing without SQLite dependencies.

### Files Changed

**New files:**
- `storage/traits/mod.rs` + individual trait files
- `storage/types/mod.rs` + individual type files
- `storage/implementations/mod.rs`
- `storage/implementations/sqlite/mod.rs` (moved from storage/sqlite/)
- `storage/implementations/fs/mod.rs` + blob.rs (moved from storage/blob/)
- `storage/implementations/memory/mod.rs` + blob.rs, content_block.rs, asset.rs
- `storage/document_resolver.rs` (restored)

**Updated files:**
- `storage/mod.rs` - New structure with re-exports
- All sqlite implementation files - Updated imports
- `engine.rs` - Added DocumentResolver import
- Desktop commands - Updated import paths

**Deleted files:**
- Old domain directories (asset/, blob/, content_block/, conversation/, document/, user/)
- Their individual mod.rs, types.rs, sqlite.rs files (content moved to new structure)

### Commits

1. "Restructure storage module with traits/, types/, implementations/"
2. "Fix missing DocumentResolver import and visibility of store_content_sync"
3. "Fix find_by_hash return type in coordinator test mock"
4. "Add in-memory storage implementations for testing"

### Test Results

All 45 tests pass:
- 32 existing tests (coordinator, session, types, fs blob)
- 13 new tests for memory implementations

---
