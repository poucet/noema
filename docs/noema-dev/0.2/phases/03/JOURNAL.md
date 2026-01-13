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

