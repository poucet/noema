# Phase 3: Journal

Chronological stream of thoughts, changes, and observations.

---

## Completed Work Summary

### Feature 3.1: Content Blocks ✅
- Content-addressed text storage with SHA-256 deduplication
- Origin tracking (user/assistant/system/import) with model provenance
- `ContentBlockStore` trait + `SqliteContentBlockStore` implementation
- Integrated into message storage via `content_id` column

### Feature 3.1b: Asset Storage ✅
- Binary blob storage with hash-based deduplication
- `AssetStore` trait + `SqliteAssetStore` implementation
- `StorageCoordinator` auto-externalizes inline images/audio to blob storage
- E2E verified: images display correctly in conversation

### Feature 3.2: Conversation Structure ✅
- Turn/Span/Message hierarchy replacing legacy thread/span_set model
- `TurnStore` trait with full SQLite implementation
- Dual-write during migration, then legacy removal
- Tables: `turns`, `spans`, `messages`, `message_content`, `views`, `view_selections`

### Feature 3.3 Parts A-C: Core UCM ✅
- **Part A**: Views and forking operations (`fork_view`, `edit_turn`, `fork_view_with_selections`, `get_view_context_at`)
- **Part B**: New `Session<S: TurnStore>` API with lazy content resolution
- **Part C**: Legacy cleanup - removed old SessionStore/SqliteSession/MemorySession
- **Part C.2**: Storage module restructure - `traits/`, `types/`, `implementations/` directories

---

## Architecture (Current State)

### Storage Layer
```
storage/
├── traits/           # AssetStore, BlobStore, ContentBlockStore, TurnStore, etc.
├── types/            # All type definitions
├── implementations/
│   ├── sqlite/       # SqliteStore (implements all traits)
│   ├── fs/           # FsBlobStore
│   └── memory/       # In-memory stores for testing
└── session/
    ├── session.rs    # Session<S: TurnStore> - DB-agnostic session
    ├── resolver.rs   # ContentBlockResolver, AssetResolver traits
    └── types.rs      # ResolvedContent, ResolvedMessage, PendingMessage
```

### Key Types
- **StoredContent** - Refs-only enum: `TextRef`, `AssetRef`, `DocumentRef`, `ToolCall`, `ToolResult`
- **ResolvedContent** - Text resolved, assets/docs cached lazily in-place
- **Session<S>** - Implements `ConversationContext` for ChatEngine

### TurnStore Operations Available
| Operation | Method | Status |
|-----------|--------|--------|
| Add turn | `add_turn()` | ✅ Used |
| Add span | `add_span()` | ✅ Used |
| Add message | `add_message()` | ✅ Used |
| Create view | `create_view()` | ✅ Used |
| Get main view | `get_main_view()` | ✅ Used |
| Get view path | `get_view_path()` | ✅ Used |
| Select span | `select_span()` | ⬜ Not wired to UI |
| Fork view | `fork_view()` | ⬜ Not wired to UI |
| Edit turn | `edit_turn()` | ⬜ Not wired to UI |
| Fork with selections | `fork_view_with_selections()` | ⬜ Not wired to UI |
| Get context at turn | `get_view_context_at()` | ⬜ Not wired to UI |

---

## Current Focus: Part D User Journeys

### Disabled UI Features (need implementation)
From journal entry 2026-01-12 "Fix Command get_messages_with_alternates not found":

| Legacy Command | New Command | Status |
|---------------|-------------|--------|
| `setSelectedSpan` | `select_span` | ⬜ Backend stub |
| `forkFromSpan` | `fork_conversation` | ⬜ Not implemented |
| `switchThread` | `switch_view` | ⬜ Not implemented |
| `editUserMessage` | `edit_message` | ⬜ Not implemented |
| `getSpanSetAlternates` | `get_turn_alternates` | ⬜ Returns empty |

### Frontend State
- Basic conversations work (send/receive messages)
- Fork functionality disabled with warning
- Span selection disabled with user-facing error
- Parallel response "Use this" button clears comparison without persisting

---

## Design Observations

### TurnStore Size Question
The `TurnStore` trait is large (~20 methods). Could split into:
- `TurnStore` - Turn/Span/Message CRUD only
- `ViewStore` - View creation, selection, forking

**Decision**: Keep unified for now. Split if it becomes unwieldy.

### Session Architecture
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
│  - pending: Vec<ChatMessage>                                │
└────────────────────────────┬────────────────────────────────┘
                             │
                             ▼
┌─────────────────────────────────────────────────────────────┐
│                    TurnStore (trait)                        │
└─────────────────────────────────────────────────────────────┘
```

---

## 2026-01-12: Scope Update - Part D User Journeys

Extended 3.3 scope to include user journeys that verify UCM operations work end-to-end in the UI.

**6 Journeys in 3.3:**
1. Regenerate Response - add_span at existing turn
2. Select Alternate Span - select_span in view
3. Edit User Message - fork_view + edit_turn
4. Fork Conversation - fork_view at turn
5. Switch View - session with different view_id
6. View Alternates at Turn - get_spans with content

**Deferred to 3.3b:**
- Subconversations - spawn/link pattern for MCP agents (requires schema change + complex integration)

---

## 2026-01-13: Part D Backend Commands Implemented

Implemented three core view/fork commands:

### New Commands (chat.rs)

| Command | Purpose | Parameters |
|---------|---------|------------|
| `fork_conversation` | Fork view at turn | conversation_id, at_turn_id, name? |
| `switch_view` | Switch to different view | conversation_id, view_id |
| `select_span` | Select alternate at turn | conversation_id, turn_id, span_id |

### New Core Methods

**StorageCoordinator:**
- `open_session_with_view(view_id)` - Open session for specific view (not just main)

**Session:**
- `open_view(coordinator, conversation_id, view_id)` - Construct session for non-main view

### Frontend Bindings (tauri.ts)

Added TypeScript wrappers:
- `forkConversation(conversationId, atTurnId, name?)`
- `switchView(conversationId, viewId)`
- `selectSpan(conversationId, turnId, spanId)`

### Next Steps

Wire these to the frontend UI:
1. Fork button on turns/messages
2. View list in sidebar or dropdown
3. Span selection in alternates panel

---

## 2026-01-13: Architecture Cleanup

### Lazy Engine Creation

Previously `init_app` would create a session/engine for the most recent conversation at startup, even if the UI wasn't displaying it. Now:

- `init_app` only initializes: storage, user, model settings, MCP registry
- Engine/session created on-demand via `load_conversation` or `new_conversation`
- All engines share the same MCP registry via `ChatEngine::with_shared_registry()`

### Session No Longer Stores conversation_id

The view already belongs to a conversation (via `views.conversation_id`), so storing it redundantly on Session was unnecessary.

**Changes:**
- `Session` struct only stores `view_id`
- `StorageCoordinator::start_turn()` now takes only `view_id` and looks up conversation via `get_view()`
- Added `TurnStore::get_view(view_id)` method
- Removed `Session::conversation_id()` accessor
- `Session::new()` and `Session::open_view()` no longer require conversation_id parameter

This simplifies the API: the view is the source of truth for conversation context.

---

## 2026-01-13: Regenerate Response Backend Implementation

Implemented `regenerate_response` command for Journey 1: Regenerate Response.

### Design Decision: Create Span Only After LLM Succeeds

Rather than creating a span before running the LLM (which would leave orphan spans on failure), the regeneration flow is:

1. **Truncate context** - Session sets cache to messages before the target turn (no storage changes)
2. **Run LLM** - Agent generates response using truncated context
3. **Commit at turn** - Only on success, create span and store messages

This ensures no orphan spans are created if the LLM call fails.

### New Methods

**StorageCoordinator:**
- `get_context_before_turn(view_id, turn_id)` - Returns resolved messages up to (not including) the turn
- `add_span_at_turn(view_id, turn_id, model_id)` - Creates span at turn, selects it in view, returns SpanId

**Session:**
- `truncate_to_turn(turn_id)` - Sets session cache to context before the turn
- `commit_at_turn(turn_id, model_id)` - Creates span at turn and stores pending messages

**ChatEngine:**
- `EngineCommand::Regenerate { turn_id, tool_config }` - New command variant
- `regenerate(turn_id, tool_config)` - Public method to trigger regeneration

### Tauri Command

```rust
regenerate_response(conversation_id, turn_id, tool_config?)
```

### TypeScript Binding

```typescript
regenerateResponse(conversationId: string, turnId: string, toolConfig?: ToolConfig): Promise<void>
```

### Flow

1. Tauri command calls `session.truncate_to_turn(turn_id)` - sets cache and `commit_target`
2. Tauri calls `engine.process_pending()`
3. Engine runs agent to generate new response
4. On success, `session.commit()` checks `commit_target` and delegates to `commit_at_turn()`

### Next Steps

Wire frontend regenerate button to call `regenerateResponse()`.

---

## 2026-01-13: Engine Simplification

Refactored `ChatEngine` to reduce code duplication and simplify the command set.

### Design: Session Owns Commit Mode

Instead of the engine needing different code paths for normal messages vs regeneration, the session now tracks how the next commit should behave:

```rust
// In Session
commit_target: Option<TurnId>,  // None = new turn, Some = add span at turn
```

- `truncate_to_turn(turn_id)` sets `commit_target = Some(turn_id)`
- `commit()` checks `commit_target` and delegates to `commit_at_turn()` if set
- Engine just calls `commit()` - doesn't need to know about regeneration

### Simplified Engine Commands

Before:
- `SendMessage` - add message, run LLM, commit
- `ProcessPending` - run LLM, commit
- `Regenerate` - truncate, run LLM, commit at turn (duplicate logic)

After:
- `SendMessage` - add message, then shared execute_and_commit
- `ProcessPending` - shared execute_and_commit

The shared `execute_and_commit` helper handles LLM execution and commit for both cases.

### Tauri Layer Handles Truncation

The `regenerate_response` Tauri command now:
1. Calls `session.truncate_to_turn(turn_id)` directly
2. Calls `engine.process_pending()`

This keeps the engine simple while still supporting regeneration.

---

## 2026-01-13: Storage Types Cleanup

Major simplification of storage types, removing redundant wrapper types and streamlining APIs.

### Hashed<T> Wrapper

Introduced `Hashed<T>` as a composable wrapper for content-addressed storage, replacing the single-purpose `HashedContentBlock` struct:

```rust
pub struct Hashed<T> {
    pub content_hash: String,
    pub content: T,
}
```

Used as: `StoredTextBlock = Stored<ContentBlockId, Hashed<ContentBlock>>`

### TextStore Simplification

1. **Removed deduplication**: `store()` no longer deduplicates by hash. Each `ContentBlock` may have different metadata (origin, content_type, is_private) even with identical text, so each gets its own ID.

2. **Removed `find_by_hash()`**: No longer needed without deduplication.

3. **Simplified return type**: `store()` now returns `ContentBlockId` instead of `Keyed<ContentBlockId, ContentHash>`. The hash is still computed and stored internally, but callers don't need it.

4. **Removed `ContentHash` type**: Was only used internally. The `content_hash()` helper function in `storage/helper.rs` computes hashes as plain strings.

### SpanRole Removal

`SpanRole` was a redundant type with only `{User, Assistant}` variants, duplicating what `llm::Role` already provides. Changes:

- `Turn.role` is now `llm::Role` instead of `SpanRole`
- `TurnStore::create_turn()` takes `llm::Role`
- Removed `SpanRole` enum and all `From` implementations
- Updated all turn store implementations (memory, sqlite, mock)

### Summary of Removed Types

| Type | Reason |
|------|--------|
| `HashedContentBlock` | Replaced by `Hashed<ContentBlock>` |
| `ContentHash` | Simplified to plain string |
| `StoreResult` / `StoredContentRef` | `store()` now returns just `ContentBlockId` |
| `SpanRole` | Replaced by `llm::Role` |

---

## 2026-01-13: Regenerate Button Frontend (3.3.D1b)

Wired regenerate button to frontend for assistant messages.

### Changes

**New file: `components/message/RegenerateIcon.tsx`**
- Circular refresh arrow SVG icon

**MessageBubble.tsx:**
- Added `onRegenerate?: (turnId: string) => void` prop
- Added regenerate button for assistant messages (appears on hover)
- Button calls `onRegenerate(turnId)` when clicked

**App.tsx:**
- Added `handleRegenerate(turnId)` handler that calls `tauri.regenerateResponse()`
- Wired to main message list `MessageBubble` components

### UI Behavior

Hover over assistant message → see action buttons (copy, regenerate, fork) to the right of the bubble.

---

## 2026-01-13: Select Span and Fork Wiring (3.3.D2b, 3.3.D4b)

Wired span selection and fork functionality to frontend.

### Changes

**AlternatesSelector.tsx:**
- Changed from `spanSetId` to `turnId` (new UCM model terminology)
- `onConfirmSelection` now receives `(turnId, spanId)`

**MessageBubble.tsx:**
- Updated props to use `turnId` instead of `spanSetId` for switching alternates
- Fork handler now passes `turnId` instead of `spanId`
- Updated `canFork` check to use `turnId`

**App.tsx:**
- `handleSwitchAlternate` now calls `tauri.selectSpan(conversationId, turnId, spanId)`
- `handleFork` now immediately calls `forkConversation` + `switchView` (no more pending fork)
- Removed legacy `pendingForkSpanId` state
- Added `handleSwitchView` for view switching

**Cleanup:**
- Removed `pendingFork` prop from SidePanel, ChatInput, ConversationsPanel
- Simplified to `prefilledText` + `onClearPrefill` for user message editing after fork

---

## 2026-01-15: Design Notes - Unified Branch Model

Current UCM model has some conceptual friction when thinking about forking and regeneration:

### Current Model Issues

1. **Regenerate creates span, fork creates view** - But conceptually both are branch points. The difference is UI semantics:
   - Regenerate: "Give me a different response at this exact point"
   - Fork: "Let me take the conversation in a different direction"

2. **Spans are flat** - All spans at a turn are peers, but if a span's response triggers follow-up turns, those are "children" of that span. Current model doesn't capture parent-child relationship.

3. **View selections disconnected from tree** - Views maintain selection state via `view_selections` table, separate from the natural tree structure.

### Proposed: Hierarchical Branches

Each branch could own its downstream turns:

```
Turn 1 (User: "Hello")
├── Branch A (Assistant: "Hi!")
│   └── Turn 2 (User: "How are you?")
│       ├── Branch A1 (Assistant: "Good!")
│       └── Branch A2 (Assistant: "Great!") ← regeneration
└── Branch B (Assistant: "Hey there!") ← also regeneration of Turn 1
    └── Turn 3 (User: "Different question...")
```

**Key insight**: Branches should own their downstream turns, not just be alternatives at a single point.

### Questions to Resolve

- Does this simplify or complicate sub-conversations (MCP agent spawns)?
- How do we handle "splicing" - inserting content mid-conversation?
- Should views be derived from branch selections, or is explicit selection still needed?

**Status**: Captured for future discussion. Continuing with current model for now.

---

## 2026-01-13: View Selector UI (3.3.D4c, 3.3.D5b)

Added ViewSelector component for switching between conversation views (forks).

### New Component: ViewSelector.tsx

Dropdown in top bar showing all views for current conversation:
- Only appears when conversation has multiple views
- Shows current view name with fork icon
- Dropdown lists all views with main/fork icons
- Click to switch views

### App.tsx Integration

- Added `views` and `currentViewId` state
- Load views when selecting conversation or on init
- Update views after forking
- Added `handleSwitchView` function
- ViewSelector in top bar next to privacy toggle

---

## 2026-01-15: Entity Layer Design

Discussed and documented a major architectural refactor: the **Entity Layer**.

**Key insight:** Views ARE the conversation structure. "Conversations" are just organizational metadata that can be attached/detached from views.

**Summary:**
- Views become first-class addressable entities (can be @mentioned)
- `conversations` table eliminated - metadata moves to `entities` table
- Fork ancestry moves from `views.forked_from_view_id` to `entity_relations`
- Deleting a view doesn't affect its forks (independent entities)
- Documents and assets also become entities (unified addressing)

**Canonical design:** See [UNIFIED_CONTENT_MODEL.md](../../design/UNIFIED_CONTENT_MODEL.md)
- Updated three-layer architecture (Addressable → Structure → Content)
- Added FR-0: Addressable Layer requirements
- Added Phase 4: Entity Layer migration plan
- Fixed incorrect claim about ContentBlock deduplication (NOT deduplicated)

---

## 2026-01-15: Edit Message Backend (3.3.E3a)

Implemented `edit_message` Tauri command for Journey 3: Edit User Message.

### New Command

```rust
edit_message(conversation_id, turn_id, content: Vec<DisplayInputContent>)
  -> EditMessageResponse { view, messages }
```

### Design

The command follows this flow:
1. Get current view_id from the loaded manager
2. Convert `DisplayInputContent` → `InputContent` → `StoredContent` via coordinator
3. Call `TurnStore::edit_turn()` with `create_fork=true`:
   - Creates new span with edited content at the target turn
   - Creates forked view that selects the new span
4. Open session for the new view
5. Replace the conversation's manager with one using the new view
6. Return `EditMessageResponse` with the new view info and resolved messages

### Response Type

```rust
pub struct EditMessageResponse {
    pub view: ThreadInfoResponse,  // New forked view info
    pub messages: Vec<DisplayMessage>,  // Messages in the new view
}
```

The frontend can use the response to:
- Update the view selector with the new fork
- Display the edited conversation

### Next: Frontend (3.3.E3b)

Need to add edit button to user messages and wire to `edit_message` command.

---

## 2026-01-15: Edit Button for User Messages (3.3.E3b)

Added edit button and TypeScript binding for editing user messages.

### Changes

**New file: `components/message/EditIcon.tsx`**
- Pencil icon SVG for edit button

**tauri.ts:**
- Added `EditMessageResponse` interface
- Added `editMessage()` function binding

**MessageBubble.tsx:**
- Added `onEdit?: (turnId: string, currentText: string) => void` prop
- Added `handleEditClick()` handler that extracts text content and calls `onEdit`
- Added edit button (pencil icon) next to fork button for user messages
- Both buttons now appear in a flex container on hover

### UI Behavior

Hover over user message → see edit (pencil) and fork buttons inside the bubble.

### Next: Edit Modal (3.3.E3c)

Need to implement the edit modal/inline editor and wire `onEdit` handler in App.tsx.

---

## 2026-01-15: Edit Message Modal (3.3.E3c)

Implemented the edit message modal and wired it to the App.tsx handlers.

### New Component: EditMessageModal.tsx

Modal dialog for editing user messages:
- Text area with auto-resize
- Pre-populated with current message text (selected on mount)
- Submit creates a fork with edited content
- Keyboard shortcuts: Cmd+Enter to submit, Esc to cancel
- Submit button disabled if text unchanged

### App.tsx Integration

**New state:**
- `editingMessage: { turnId: string; text: string } | null`

**New handlers:**
- `handleEdit(turnId, currentText)` - Opens the edit modal
- `handleEditSubmit(newText)` - Calls `tauri.editMessage()`, updates state with new view/messages

**Wiring:**
- `onEdit={handleEdit}` prop passed to MessageBubble
- EditMessageModal rendered when `editingMessage` is set

### Complete Flow

1. User hovers over their message → sees edit button (pencil icon)
2. Click edit → modal opens with message text selected
3. User edits text → clicks "Submit Edit" (or Cmd+Enter)
4. Backend creates fork with new content at that turn
5. UI updates to show the new forked view with edited message
6. View selector shows the new fork

### Journey 3: Edit User Message - Complete

All three tasks complete:
- 3.3.E3a: Backend `edit_message` command
- 3.3.E3b: Frontend edit button on user messages
- 3.3.E3c: Edit modal with submit action

---

## 2026-01-15: Session Integration Tests (3.3.14)

Added comprehensive integration tests for the Session API with memory-based storage.

### New Files

**`storage/implementations/memory/user.rs`**
- In-memory `MemoryUserStore` implementation for testing
- Implements `UserStore` trait with user CRUD operations

**`storage/implementations/memory/mod.rs`**
- Added `MemoryUserStore` export
- Added `MemoryStorage` type bundle (implements `StorageTypes`)

**`storage/session/tests.rs`**
- 15 integration tests covering Session API with StorageCoordinator

### Test Coverage

| Category | Tests |
|----------|-------|
| Session Creation | `test_session_new_creates_empty_session`, `test_session_open_loads_existing_messages` |
| Message Management | `test_session_add_to_pending`, `test_session_commit_moves_to_resolved`, `test_session_all_messages_combines_resolved_and_pending` |
| Context Interface | `test_session_as_conversation_context`, `test_session_context_commit` |
| Truncation | `test_session_truncate_clears_all`, `test_session_truncate_at_turn` |
| Cache Management | `test_session_clear_cache`, `test_session_clear_pending` |
| View Management | `test_session_open_view` |
| CommitMode | `test_session_commit_at_turn_regeneration` |
| Content | `test_session_preserves_message_content`, `test_session_multi_content_message` |

### Key Test Scenarios

1. **Full lifecycle**: Create session → add messages → commit → open new session → verify messages loaded
2. **Regeneration flow**: Commit messages → truncate at turn → add new response → commit at same turn
3. **Fork/view**: Create messages in main view → fork → open forked view → verify isolation
4. **ConversationContext trait**: Verify Session implements trait correctly for agent compatibility

---

## 2026-01-28: Documentation Update

Updated core documentation to reflect the Unified Content Model architecture.

### Changes

**STORAGE.md** (rewritten):
- Restructured around three-layer UCM model (Addressable, Structure, Content)
- Added entity layer tables (`entities`, `entity_relations`)
- Updated conversation structure to Turn → Span → Message hierarchy
- Added document structure with tabs and revisions
- Updated storage traits section for new API
- Added Session API documentation
- Included migration notes from 1.x

**ARCHITECTURE.md** (new):
- High-level vision and guiding principles (local-first, content immutable, structure mutable)
- Three-layer architecture diagram and explanation
- Core concepts documentation (Views as Conversations, Spans as Flows, Content with Origin)
- Crate structure overview
- Data flow diagrams (message send, fork operations)
- Phase roadmap summary (Phase 3 through Phase 8)
- Future hook system overview
- Extension points (adding stores, entity types, providers)
- Key design decisions with rationale

These documents capture where Noema 0.2 is headed architecturally.

---

## 2026-01-15: Bug Fixes for Edit Message Flow

### Bug 1: Edit Doesn't Trigger AI Response

After editing a user message and creating a fork, the AI wasn't responding - user had to send another message. This was because `edit_message` created the fork but didn't trigger the LLM.

**Fix:**
1. Added `run_agent()` method to `ConversationManager` that sends `RunAgent` command without truncating first
2. `edit_message` now calls `manager.run_agent(tool_config)` after creating the new manager

The difference from `regenerate()`:
- `regenerate()` truncates context first (to regenerate at a specific turn)
- `run_agent()` just starts the agent on current pending (for edit flow where fork already has content)

### Bug 2: Gemini thought_signature Error

Forked conversations with Gemini tool calls were failing with:
```
Function call is missing a thought_signature...
```

**Fix:**
Extended `ToolCall` struct with `extra: serde_json::Value` field to preserve provider-specific metadata. For Gemini, `thought_signature` is captured when receiving tool calls and echoed back when sending tool results.

**Changes:**
- `llm::api::ToolCall` - Added `extra` field
- `providers/gemini/chat/api.rs` - Added `thought_signature` field to `Part` struct
- All providers updated to initialize `extra` (null for non-Gemini)
- Gemini conversion functions preserve/restore `thought_signature` via `extra`

---

