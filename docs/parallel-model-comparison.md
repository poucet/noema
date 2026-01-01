# Parallel Model Comparison & Model Favorites

## Overview

Add the ability to send messages to multiple models in parallel, compare responses via tabs on the message itself, and switch between alternates. Also add model favoriting for quick access.

## Key Design Decision: Span-Based Storage

All content is stored in a span-based hierarchy:

```
Conversation
  └── Thread (main, or forked from a span)
        └── SpanSet (position 1, 2, 3...)
              └── Span (alternative A, B, C...)
                    └── SpanMessage (for multi-turn within one "response")
```

- **SpanSet**: A position in the conversation (user turn or assistant turn)
- **Span**: One model's complete response (may be multiple messages if agentic)
- **SpanMessage**: Individual message within a span
- **Thread forking**: `thread.parent_span_id` points to the specific span being forked from

This unified model supports:
- Parallel model responses (multiple spans in one span_set)
- Editable user input (user edits create new spans)
- Clean thread forking from any specific span

## User Requirements

- **Trigger**: Both regenerate button on messages AND pre-send multi-model selection
- **Display**: Tab-based UI on the message itself to switch between alternates
- **Continue**: Picking an alternate makes it the active one; conversation continues from there
- **Favorites**: Show at top of dropdown AND as quick-access chips near input

---

## Implementation Status

### Step 1: Model Favorites ✅ COMPLETED
- Settings field + Tauri commands
- ModelSelector favorites section
- FavoriteModelChips component

**Commits:**
- `d0e2e7bc` - Add model favorites feature with star toggle in dropdown
- `ec95b6e4` - Clean up debug logging and fix dropdown position

**Test:**
- [x] Can star/unstar models in dropdown (☆/★ icons)
- [x] Favorites appear at top of dropdown with yellow "★ Favorites" header
- [x] Favorites persist across app restarts (saved to settings.toml)
- [x] Favorite chips component created (FavoriteModelChips.tsx)

---

### Step 2: Span-Based Storage ✅ COMPLETED
- Removed legacy `messages` table entirely
- All storage now uses span_sets → spans → span_messages
- Updated `threads.parent_span_id` for fork support
- Auto-create user when configured email doesn't exist

**Commits:**
- `612e0663` - Refactor storage to span-based model and add parallel model support
- `76f117fd` - Auto-create user when configured email doesn't exist

**What's done:**
- [x] Schema: `span_sets`, `spans`, `span_messages` tables only (no `messages` table)
- [x] `threads.parent_span_id` replaces `parent_message_id`
- [x] Storage methods:
  - `write_as_span()` - creates span_set + span + span_messages
  - `create_span_set`, `create_span`, `add_span_message`
  - `get_span_set_alternates`, `set_selected_span`
  - `get_span_messages`, `get_span_set_with_content`
- [x] All queries load from `span_messages` via selected spans
- [x] Tests pass: `cargo test -p noema-core --features sqlite`
- [x] Dropped Episteme compatibility (clean break)

---

### Step 3: Parallel Execution Backend + UI ✅ IMPLEMENTED
- Engine parallel execution logic
- Independent agentic loops per model
- New streaming events
- Tauri command: send_parallel_message
- Event listeners in App.tsx
- Parallel streaming grid view

**What's done:**
- [x] `EngineCommand::SendParallelMessage` - sends to multiple models in parallel
- [x] `EngineEvent` variants for parallel execution (streaming, model complete, all complete, errors)
- [x] `ParallelAlternateInfo` type for tracking model responses
- [x] `run_single_model_agent()` helper for isolated model execution
- [x] `send_parallel_message` Tauri command
- [x] Event loop handlers for all parallel events
- [x] TypeScript bindings: `sendParallelMessage()` + event listeners
- [x] App.tsx state: `isParallelMode`, `parallelStreaming`, `parallelAlternates`
- [x] Parallel streaming grid UI (shows model responses side by side)
- [x] FavoriteModelChips → handleSendMessage → parallel send when 2+ models selected

**Test:**
- [ ] Select 2+ models using favorite chips
- [ ] Send a message → should show parallel streaming grid
- [ ] Each model's response appears in its own panel
- [ ] When all complete, responses are shown
- [ ] Errors from individual models are displayed

---

### Step 4: MessageBubble Alternates UI ✅ COMPLETED
- Add alternates selector to MessageBubble
- Separate preview (viewing) from selection (committing to DB)
- Minimal tab-based UI with confirm button

**What's done:**
- [x] `AlternateInfo` type in Rust and TypeScript
- [x] `get_messages_with_alternates` Tauri command loads messages with span awareness
- [x] `DisplayMessage` extended with `spanSetId` and `alternates` optional fields
- [x] `AlternatesSelector` component with preview/confirm pattern:
  - Clicking tab previews content (fetches via `getSpanMessages`)
  - Checkmark icon on the currently saved selection
  - Small checkmark icon button appears when previewing a different alternate
  - Clicking icon button commits selection to database
- [x] Messages with alternates show tab bar (wraps on narrow screens)
- [x] Preview content fetched dynamically without affecting saved state
- [x] Selected alternate persists (saved to DB only on confirm)

**Still TODO:**
- [ ] Regenerate button on assistant messages

---

### Step 5: RegenerateModal (TODO)
- Modal for selecting models to regenerate with
- Wire up regenerate button → modal → parallel execution

**Test:**
- [ ] Clicking regenerate opens modal
- [ ] Can select multiple models
- [ ] Regenerate creates new alternates on the message
- [ ] New alternates appear in tabs

---

### Step 6: Fork from Alternate ✅ COMPLETED
- Fork command + storage logic
- Branch list in sidebar (under conversations)
- Thread management commands (rename, delete)

**What's done:**
- [x] `ThreadInfo` struct in storage layer
- [x] `create_fork_thread` - creates new thread with `parent_span_id` pointing to fork point
- [x] `list_conversation_threads` - lists all threads for a conversation
- [x] `get_thread_messages_with_ancestry` - walks ancestry chain to build full message history
- [x] `rename_thread` / `delete_thread` - thread management
- [x] Tauri commands: `fork_from_span`, `list_conversation_threads`, `switch_thread`, `rename_thread`, `delete_thread`
- [x] TypeScript bindings for all thread operations
- [x] `spanId` added to `DisplayMessage` for fork actions
- [x] Branch list UI in ConversationsPanel (shows under each conversation when forked)
- [x] Fork button on assistant messages (and alternates) - purple fork icon
- [x] App.tsx handlers for thread switching, renaming, deleting, forking

**Test:**
- [ ] Can fork from any alternate (click fork icon)
- [ ] New branch appears in sidebar under conversation
- [ ] Can switch between branches
- [ ] Conversation continues correctly on each branch
- [ ] Can rename/delete branches

---

### Step 7: Edit User Message (Creates Fork) ✅ COMPLETED
- Add edit capability to user messages
- Editing creates a new fork automatically
- Wire up with fork infrastructure from Step 6

**What's done:**
- [x] `edit_user_message` Tauri command creates fork from point before edited message
- [x] TypeScript binding `editUserMessage()` in tauri.ts
- [x] Edit button appears on hover for user messages (pencil icon)
- [x] Edit UI: textarea with Save & Fork / Cancel buttons
- [x] Cmd+Enter to save, Escape to cancel
- [x] UI hint: "Editing creates a new branch from this point"
- [x] App.tsx handler: `handleEditUserMessage` switches to new thread after edit

**Test:**
- [ ] Can edit a user message by clicking edit icon
- [ ] After editing, a new fork is created
- [ ] Original message is preserved on main branch
- [ ] Edited message appears on new fork

---

## Files Modified

| File | Changes |
|------|---------|
| [settings.rs](../config/src/settings.rs) | Add `favorite_models: Vec<String>` |
| [sqlite.rs](../noema-core/src/storage/sqlite.rs) | Span-based schema, removed messages table, fork/thread methods |
| [engine.rs](../noema-core/src/engine.rs) | Add parallel execution logic, new events |
| [chat.rs](../noema-ui/src-tauri/src/commands/chat.rs) | Add parallel, fork, thread, and edit commands |
| [App.tsx](../noema-ui/src/App.tsx) | Add parallel state, thread state, fork/edit handlers |
| [ModelSelector.tsx](../noema-ui/src/components/ModelSelector.tsx) | Add favorites section, star toggle |
| [tauri.ts](../noema-ui/src/tauri.ts) | Add command bindings for parallel, threads, fork, edit |
| [MessageBubble.tsx](../noema-ui/src/components/MessageBubble.tsx) | Add alternates UI, fork button, edit user message |
| [ConversationsPanel.tsx](../noema-ui/src/components/panels/ConversationsPanel.tsx) | Add thread/branch list under conversations |
| [SidePanel.tsx](../noema-ui/src/components/SidePanel.tsx) | Pass thread props to ConversationsPanel |
| [logging.rs](../noema-ui/src-tauri/src/logging.rs) | Fix duplicate logging |
| [init.rs](../noema-ui/src-tauri/src/commands/init.rs) | Auto-create user by email |
| [STORAGE.md](./STORAGE.md) | Updated schema documentation |

## New Files

| File | Purpose |
|------|---------|
| `noema-ui/src/components/FavoriteModelChips.tsx` | Quick-select chips above input |
| `noema-ui/src/generated/Parallel*.ts` | TypeScript types for parallel events |
