# Phase 0.3 Design Observations

## Decisions Made

### 1. TurnStore Trait Size

**Decision**: Keep unified for now.

The trait is large but Views have tight coupling to Turns. Can revisit later if testing becomes problematic.

### 2. ConversationManagement → ConversationStore

**Decision**: Rename to `ConversationStore` and keep in `conversation/` module.

It's conversation-level CRUD, belongs with conversation types.

### 3. Session Abstraction Design

**Decision**: Abstract session management properly.

Current `session/` code is starting to smell and adds unnecessary restrictions. Need clean separation:
- Session as runtime state (cache, pending messages)
- Persistence as injected dependency
- Remove SQLite coupling from session logic

**TODO**: Design proper session abstraction.

### 4. Content Resolution Architecture

**Decision**: Different resolution strategies for different consumers.

**For User Display (Desktop/UI)**:
- Do NOT resolve refs
- Keep refs so UI can link to content and show metadata
- UI decides how to present (link, thumbnail, preview)

**For LLM Context Injection**:
- DO resolve refs to actual content
- DocumentRef resolution needs custom logic beyond simple conversion
- Requires templating/formatting of the injected content
- This is where `DocumentFormatter` comes in

**Implication**: Resolution is NOT a single `resolve()` method. It's context-dependent:
```rust
// For UI - returns refs
fn get_messages_for_display(&self) -> Vec<MessageWithRefs>;

// For LLM - resolves and formats
fn get_messages_for_llm(&self, formatter: &DocumentFormatter) -> Vec<ChatMessage>;
```

### 5. Desktop Commands Approach

**Decision**: (Awaiting direction - dependent on session abstraction work)

## Verified Implementation Details

### SqliteSession Uses TurnStore Correctly

Verified that SqliteSession properly uses TurnStore:
- `write_turn` creates Turn → Span → Messages using TurnStore methods
- `write_parallel_turn` creates multiple spans at one turn for parallel models
- `open_conversation` loads via `get_main_view` → `get_view_path`
- Content resolution happens through StorageCoordinator

The core flow works:
1. ChatEngine calls `session.commit(tx)`
2. SqliteSession creates Turn + Span + Messages via TurnStore
3. Selections are stored in main view
4. Loading reverses this via `get_view_path`

## Next Actions

1. [x] TurnStore - keep unified
2. [ ] Rename `ConversationManagement` → `ConversationStore`
3. [ ] Design proper session abstraction (separate runtime state from persistence)
4. [ ] Split content resolution: display (refs) vs LLM (resolved + formatted)
5. [ ] Desktop commands - after session abstraction is done

---

## Reference: Desktop API Mapping

When updating desktop commands, use this mapping:

| Old Desktop API | New Desktop API | Notes |
|----------------|-----------------|-------|
| `list_conversation_threads()` | `list_views()` | Returns Vec<ViewInfo> |
| `switch_thread(thread_id)` | `switch_view(view_id)` | Load view path |
| `get_messages_with_alternates()` | `get_view_path()` | Returns Vec<TurnWithContent> |
| `get_span_set_alternates(span_set_id)` | `get_turn_spans(turn_id)` | Returns Vec<SpanInfo> |
| `set_selected_span(span_set_id, span_id)` | `select_span(view_id, turn_id, span_id)` | View-aware selection |
| `fork_from_span(span_id)` | `fork_view(view_id, turn_id)` | Fork at turn |
| `edit_user_message(span_id, content)` | `edit_turn(view_id, turn_id, content)` | Edit with optional fork |

## Reference: Key Types

```rust
// Core types (conversation/types.rs)
TurnInfo { id, conversation_id, role, sequence_number, created_at }
SpanInfo { id, turn_id, model_id, message_count, created_at }
MessageInfo { id, span_id, sequence_number, role, created_at }
ViewInfo { id, conversation_id, name, is_main, forked_from_view_id, forked_at_turn_id, created_at }
ConversationInfo { id, name, turn_count, is_private, created_at, updated_at }

// Composite types
TurnWithContent { turn, span, messages: Vec<MessageWithContent> }
MessageWithContent { message, content: Vec<MessageContentInfo> }
MessageContentInfo { id, message_id, sequence_number, content: StoredContent }
```
