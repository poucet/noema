# Phase 3 Scratchpad

Working notes for continuation in new context.

---

## Current State (2026-01-12)

### What's Done

1. **noema-core compiles** - All legacy code removed, TurnStore is the only conversation storage API
2. **Legacy removed**:
   - Deleted `conversation/conversation_store.rs`
   - Removed all `Legacy*` types from `conversation/types.rs`
   - Removed legacy tables from schema (threads, span_sets, legacy_spans, legacy_span_messages)
3. **Session rewritten** - `session/sqlite.rs` uses TurnStore exclusively
4. **JOURNAL.md updated** - Full documentation of changes

### What's Broken

- `noema-desktop` doesn't compile - references removed types and methods

---

## Next Steps (User Decisions Made)

### 1. ConversationManagement Trait

**Location**: `storage/session/mod.rs` (user chose this)

**Purpose**: CRUD operations for conversations (separate from TurnStore which handles Turn/Span/Message/View)

**Methods needed**:
```rust
#[async_trait]
pub trait ConversationManagement: Send + Sync {
    /// List all conversations for a user
    async fn list_conversations(&self, user_id: &str) -> Result<Vec<ConversationInfo>>;

    /// Delete a conversation and all its data
    async fn delete_conversation(&self, conversation_id: &str) -> Result<()>;

    /// Rename a conversation
    async fn rename_conversation(&self, conversation_id: &str, name: Option<&str>) -> Result<()>;

    /// Get privacy setting
    async fn get_conversation_private(&self, conversation_id: &str) -> Result<bool>;

    /// Set privacy setting
    async fn set_conversation_private(&self, conversation_id: &str, is_private: bool) -> Result<()>;
}
```

**Implementation**: Add to `SqliteStore` in `session/sqlite.rs`

### 2. Desktop API Redesign

**Approach**: Expose Turn/Span/View concepts directly (breaking change for frontend)

**Scope**: Rust only for now, TypeScript types generated later

**Terminology mapping** (for reference when updating commands):

| Old Desktop API | New Desktop API | Notes |
|----------------|-----------------|-------|
| `list_conversation_threads()` | `list_views()` | Returns Vec<ViewInfo> |
| `switch_thread(thread_id)` | `switch_view(view_id)` | Load view path |
| `get_messages_with_alternates()` | `get_view_path()` | Returns Vec<TurnWithContent> |
| `get_span_set_alternates(span_set_id)` | `get_turn_spans(turn_id)` | Returns Vec<SpanInfo> |
| `set_selected_span(span_set_id, span_id)` | `select_span(view_id, turn_id, span_id)` | View-aware selection |
| `fork_from_span(span_id)` | `fork_view(view_id, turn_id)` | Fork at turn |
| `edit_user_message(span_id, content)` | `edit_turn(view_id, turn_id, content)` | Edit with optional fork |

### 3. Desktop Files to Update

- `noema-desktop/src-tauri/src/commands/init.rs` - Remove ConversationStore import
- `noema-desktop/src-tauri/src/commands/chat.rs` - Major rewrite
- `noema-desktop/src-tauri/src/types.rs` - Remove Legacy* type conversions

---

## Key Types Reference

From `noema-core/src/storage/conversation/types.rs`:

```rust
// Core types
pub struct TurnInfo { id, conversation_id, role, sequence_number, created_at }
pub struct SpanInfo { id, turn_id, model_id, message_count, created_at }
pub struct MessageInfo { id, span_id, sequence_number, role, created_at }
pub struct ViewInfo { id, conversation_id, name, is_main, forked_from_view_id, forked_at_turn_id, created_at }
pub struct ConversationInfo { id, name, turn_count, is_private, created_at, updated_at }

// Composite types
pub struct TurnWithContent { turn, span, messages: Vec<MessageWithContent> }
pub struct MessageWithContent { message, content: Vec<MessageContentInfo> }
pub struct MessageContentInfo { id, message_id, sequence_number, content: StoredContent }
```

From `noema-core/src/storage/conversation/turn_store.rs`:

```rust
pub trait TurnStore: Send + Sync {
    // Turn management
    async fn add_turn(&self, conversation_id, role) -> Result<TurnInfo>;
    async fn get_turns(&self, conversation_id) -> Result<Vec<TurnInfo>>;
    async fn get_turn(&self, turn_id) -> Result<Option<TurnInfo>>;

    // Span management
    async fn add_span(&self, turn_id, model_id) -> Result<SpanInfo>;
    async fn get_spans(&self, turn_id) -> Result<Vec<SpanInfo>>;
    async fn get_span(&self, span_id) -> Result<Option<SpanInfo>>;

    // Message management
    async fn add_message(&self, span_id, role, content) -> Result<MessageInfo>;
    async fn get_messages(&self, span_id) -> Result<Vec<MessageInfo>>;
    async fn get_messages_with_content(&self, span_id) -> Result<Vec<MessageWithContent>>;

    // View management
    async fn create_view(&self, conversation_id, name, is_main) -> Result<ViewInfo>;
    async fn get_views(&self, conversation_id) -> Result<Vec<ViewInfo>>;
    async fn get_main_view(&self, conversation_id) -> Result<Option<ViewInfo>>;
    async fn select_span(&self, view_id, turn_id, span_id) -> Result<()>;
    async fn get_selected_span(&self, view_id, turn_id) -> Result<Option<SpanId>>;
    async fn get_view_path(&self, view_id) -> Result<Vec<TurnWithContent>>;
    async fn fork_view(&self, view_id, at_turn_id, name) -> Result<ViewInfo>;
    async fn fork_view_with_selections(&self, view_id, at_turn_id, name, selections) -> Result<ViewInfo>;
    async fn get_view_context_at(&self, view_id, up_to_turn_id) -> Result<Vec<TurnWithContent>>;
    async fn edit_turn(&self, view_id, turn_id, messages, model_id, create_fork, fork_name) -> Result<(SpanInfo, Option<ViewInfo>)>;

    // Convenience
    async fn add_user_turn(&self, conversation_id, text) -> Result<(TurnInfo, SpanInfo, MessageInfo)>;
    async fn add_assistant_turn(&self, conversation_id, model_id, text) -> Result<(TurnInfo, SpanInfo, MessageInfo)>;
}
```

---

## Implementation Order

1. Add `ConversationManagement` trait to `storage/session/mod.rs`
2. Implement for `SqliteStore` in `storage/session/sqlite.rs`
3. Update `noema-desktop/src-tauri/src/commands/init.rs` (remove broken import)
4. Update `noema-desktop/src-tauri/src/types.rs` (remove Legacy* conversions, add new type conversions)
5. Rewrite `noema-desktop/src-tauri/src/commands/chat.rs` commands one by one
6. Verify desktop compiles with `cargo check -p noema-desktop`

---

## Commands to Verify

```bash
# Check noema-core compiles (should pass)
cd /Users/simplychris/projects/simply/noema/.jj-workspaces/noema-0.2
cargo check -p noema-core

# Check desktop compiles (will fail until fixed)
cargo check -p noema-desktop
```
