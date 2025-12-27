# Parallel Model Comparison & Model Favorites

## Overview

Add the ability to send messages to multiple models in parallel, compare responses via tabs on the message itself, and switch between alternates. Also add model favoriting for quick access.

## Key Design Decision: Message Alternates (not Branches)

Instead of creating conversation branches/threads for parallel responses, we store **alternates at the message level**:

- Each message can have multiple alternate versions (from different models)
- One alternate is marked as "active" and shown by default
- User can switch between alternates using tabs on the message
- When user picks a different alternate, it becomes the new active one
- Conversation continues from whatever alternate is currently active
- User can explicitly fork from any alternate to create a separate branch

This is simpler than automatic branching, but still supports explicit forking when needed.

## User Requirements

- **Trigger**: Both regenerate button on messages AND pre-send multi-model selection
- **Display**: Tab-based UI on the message itself to switch between alternates
- **Continue**: Picking an alternate makes it the active one; conversation continues from there
- **Favorites**: Show at top of dropdown AND as quick-access chips near input

---

## Implementation Plan

### Phase 1: Model Favorites (Settings & UI)

**1.1 Add `favorite_models` to Settings**

File: [settings.rs](../config/src/settings.rs)

```rust
pub struct Settings {
    pub user_email: Option<String>,
    pub default_model: Option<String>,
    pub api_keys: HashMap<String, String>,
    pub favorite_models: Vec<String>,  // NEW: e.g. ["claude/claude-sonnet-4-5", "openai/gpt-4o"]
}
```

**1.2 Add Tauri commands for favorites**

File: [chat.rs](../noema-ui/src-tauri/src/commands/chat.rs)

- `get_favorite_models() -> Vec<String>`
- `toggle_favorite_model(model_id: String) -> Vec<String>`

**1.3 Enhance ModelSelector with favorites**

File: [ModelSelector.tsx](../noema-ui/src/components/ModelSelector.tsx)

- Add star icon toggle next to each model
- Show "Favorites" section at top of dropdown (pinned)
- Pass `favoriteModels` and `onToggleFavorite` props

**1.4 Create FavoriteModelChips component**

New file: `noema-ui/src/components/FavoriteModelChips.tsx`

- Displays favorite models as clickable chips above the input
- Multi-select mode: clicking toggles selection for parallel send
- Shows "Send to N models" button when models selected

---

### Phase 2: SpanSet Model (Data Layer)

**Design: Threads → SpanSets → Spans → Messages**

Since models can have multi-turn agentic behavior (tool calls, etc.), a single "response" may contain multiple messages. We model this as:

```
Thread
  └── SpanSet (sequence=1, type=user)
  │     └── Span 1 (model=null) ← selected
  │           └── Message (role=user, content="Hello")
  │
  └── SpanSet (sequence=2, type=assistant)
        └── Span 1 (model=claude-sonnet) ← selected
        │     └── Message (role=assistant, content="I'll help", tool_call=X)
        │     └── Message (role=tool, result=Y)
        │     └── Message (role=assistant, content="Here's the answer")
        │
        └── Span 2 (model=gpt-4o)
        │     └── Message (role=assistant, content="Sure thing")
        │
        └── Span 3 (model=gemini-pro)
              └── Message (role=assistant, content="Let me check", tool_call=A)
              └── Message (role=tool, result=B)
              └── Message (role=assistant, content="Based on that...")
```

- **SpanSet**: A position in the conversation (user turn or assistant turn)
- **Span**: One model's complete response (may be multiple messages if agentic)
- **Message**: Individual message within a span

**2.1 New tables**

File: [sqlite.rs](../noema-core/src/storage/sqlite.rs)

```sql
-- SpanSets: positions in conversation
CREATE TABLE IF NOT EXISTS span_sets (
    id TEXT PRIMARY KEY,
    thread_id TEXT REFERENCES threads(id) ON DELETE CASCADE,
    sequence_number INTEGER NOT NULL,
    span_type TEXT CHECK(span_type IN ('user', 'assistant')) NOT NULL,
    selected_span_id TEXT,            -- which span is currently active
    created_at INTEGER NOT NULL
);

CREATE INDEX idx_span_sets_thread ON span_sets(thread_id, sequence_number);

-- Spans: alternative responses within a SpanSet
CREATE TABLE IF NOT EXISTS spans (
    id TEXT PRIMARY KEY,
    span_set_id TEXT REFERENCES span_sets(id) ON DELETE CASCADE,
    model_id TEXT,                    -- e.g. "claude/claude-sonnet-4-5" (null for user)
    created_at INTEGER NOT NULL
);

CREATE INDEX idx_spans_span_set ON spans(span_set_id);

-- Messages: individual messages within a span
CREATE TABLE IF NOT EXISTS messages (
    id TEXT PRIMARY KEY,
    span_id TEXT REFERENCES spans(id) ON DELETE CASCADE,
    sequence_number INTEGER NOT NULL,
    role TEXT CHECK(role IN ('user', 'assistant', 'system', 'tool')) NOT NULL,
    content TEXT NOT NULL,            -- JSON payload
    created_at INTEGER NOT NULL
);

CREATE INDEX idx_messages_span ON messages(span_id, sequence_number);
```

**2.2 Storage methods**

- `add_user_span_set(thread_id, content)` → creates span_set + span + message
- `add_assistant_span(span_set_id, model_id)` → creates new span for parallel response
- `add_span_message(span_id, role, content)` → adds message to span (for multi-turn)
- `get_thread_conversation(thread_id)` → returns span_sets with selected span's messages
- `get_span_set_alternates(span_set_id) -> Vec<SpanInfo>` → returns all spans
- `set_selected_span(span_set_id, span_id)` → changes selection

**2.3 Types**

```rust
pub struct SpanInfo {
    pub id: String,
    pub model_id: Option<String>,
    pub model_display_name: Option<String>,
    pub message_count: usize,
    pub is_selected: bool,
    pub created_at: i64,
}

pub struct SpanSetWithContent {
    pub id: String,
    pub span_type: SpanType,          // User or Assistant
    pub messages: Vec<StoredMessage>, // from selected span
    pub alternates: Vec<SpanInfo>,    // info about other spans
}
```

**2.4 Migration**

For existing messages:
- Group by thread and sequence
- Create span_set for each position
- Create single span with the existing message(s)
- Set as selected

---

### Phase 3: Parallel Model Execution (Backend)

**Key Design: Independent Agentic Loops**

Each model runs its own independent agentic loop. Models may:
- Complete immediately with a text response
- Make different numbers of tool/MCP calls
- Take different amounts of time
- Fail independently without affecting others

The parallel execution does NOT synchronize turns - each model runs to completion independently.

**3.1 Add parallel execution in engine**

File: [engine.rs](../noema-core/src/engine.rs)

- New command: `SendParallelMessage { payload, model_ids }`
- Spawns independent agent loops for each model using `tokio::spawn`
- Each agent runs its full loop (message → tool calls → message → ... → final response)
- Events are tagged with `model_id` so UI can track each independently

**3.2 New event types**

```rust
// Streaming text update for a specific model
ParallelStreamingMessage { model_id: String, content: String }

// Model made a tool call (for UI display)
ParallelToolCall { model_id: String, tool_name: String, tool_id: String }

// Model received tool result
ParallelToolResult { model_id: String, tool_id: String, result: String }

// One model finished its full agentic loop
ParallelModelComplete { model_id: String, final_content: StoredPayload }

// All models finished, here are the alternates
ParallelComplete { message_set_id: String, alternates: Vec<AlternateInfo> }
```

**3.3 Agentic loop per model**

Each spawned task runs:
```rust
async fn run_model_agent(model, context, user_message, tx) {
    loop {
        let response = model.chat(messages).await;

        if response.has_tool_calls() {
            // Execute tools, emit ParallelToolCall/ParallelToolResult events
            // Add tool results to messages
            continue;
        } else {
            // Final response - emit ParallelModelComplete
            break;
        }
    }
}
```

**3.4 UI implications**

- Each model's tab shows its current state (streaming, tool calling, complete)
- Tool calls can be shown inline or collapsed
- Models complete at different times - UI updates independently
- User can see one model still "thinking" while another is done

**3.5 Add Tauri commands**

File: [chat.rs](../noema-ui/src-tauri/src/commands/chat.rs)

- `send_parallel_message(message: String, model_ids: Vec<String>)` - sends to multiple models
- `regenerate_with_models(message_id: String, model_ids: Vec<String>)` - regenerate last response
- `get_message_alternates(message_id: String) -> Vec<AlternateInfo>` - get alternates for a message
- `set_active_alternate(message_id: String, alternate_id: String)` - switch active alternate

---

### Phase 4: Parallel Comparison UI

**4.1 Update DisplayMessage type**

File: [types.ts](../noema-ui/src/types.ts)

```typescript
interface AlternateInfo {
  id: string;
  modelId: string;
  modelDisplayName: string;
  isActive: boolean;
}

interface DisplayMessage {
  role: "user" | "assistant" | "system";
  content: DisplayContent[];
  messageId?: string;           // NEW: needed to fetch/set alternates
  alternates?: AlternateInfo[]; // NEW: available alternates (if any)
}
```

**4.2 Add parallel state to App.tsx**

File: [App.tsx](../noema-ui/src/App.tsx)

```typescript
// Track streaming responses per model during parallel execution
const [parallelStreaming, setParallelStreaming] = useState<Map<string, DisplayMessage>>(new Map());
const [isParallelMode, setIsParallelMode] = useState(false);
```

**4.3 Add parallel event listeners**

```typescript
tauri.onParallelStreamingMessage(({ modelId, message }) => {
  setParallelStreaming(prev => new Map(prev).set(modelId, message));
});

tauri.onParallelComplete(({ messageId, alternates }) => {
  // Update the message in the list with alternates info
  setIsParallelMode(false);
  setParallelStreaming(new Map());
});
```

**4.4 Enhance MessageBubble with alternates tabs**

File: [MessageBubble.tsx](../noema-ui/src/components/MessageBubble.tsx)

When a message has alternates:
- Show tab bar above the message content
- Each tab shows model name + active indicator
- Clicking a tab calls `setActiveAlternate` and updates the display
- Add regenerate button (icon) that opens model selection

**4.5 Create RegenerateModal component**

New file: `noema-ui/src/components/RegenerateModal.tsx`

- Checkbox list of available models (favorites pre-checked)
- "Regenerate with N models" button

**4.6 Show parallel streaming UI**

When `isParallelMode` is true:
- Show a special streaming view with tabs for each model
- Each tab shows that model's streaming response
- When complete, collapses into normal message with alternates

---

### Phase 5: Fork from Alternate

**5.1 Add fork capability to alternates UI**

In MessageBubble, add a "Fork" action on each alternate tab:
- Shows as an icon button (branch icon) next to each tab
- Clicking creates a new branch starting from that alternate

**5.2 Add Tauri command for forking**

File: [chat.rs](../noema-ui/src-tauri/src/commands/chat.rs)

- `fork_from_alternate(message_id: String, alternate_id: String) -> ThreadInfo`
  - Creates a new thread with `parent_message_id` set to this message
  - Copies the alternate's content as the starting point
  - Returns the new thread info

**5.3 Add BranchSwitcher component**

New file: `noema-ui/src/components/BranchSwitcher.tsx`

- Dropdown showing available branches for current conversation
- Displays: branch name (auto-generated or user-set), model used, message count
- Click to switch active branch
- Appears in conversation header when branches exist

**5.4 Add branch management commands**

- `list_branches(conversation_id: String) -> Vec<ThreadInfo>`
- `switch_branch(thread_id: String) -> Vec<DisplayMessage>`
- `rename_branch(thread_id: String, name: String)`
- `delete_branch(thread_id: String)`

---

## Files to Modify

| File | Changes |
|------|---------|
| [settings.rs](../config/src/settings.rs) | Add `favorite_models: Vec<String>` |
| [sqlite.rs](../noema-core/src/storage/sqlite.rs) | Add span tables, span methods |
| [engine.rs](../noema-core/src/engine.rs) | Add parallel execution logic, new events |
| [chat.rs](../noema-ui/src-tauri/src/commands/chat.rs) | Add new commands |
| [App.tsx](../noema-ui/src/App.tsx) | Add parallel state, event listeners |
| [ModelSelector.tsx](../noema-ui/src/components/ModelSelector.tsx) | Add favorites section, star toggle |
| [MessageBubble.tsx](../noema-ui/src/components/MessageBubble.tsx) | Add alternates tabs, regenerate button |
| [tauri.ts](../noema-ui/src/tauri.ts) | Add new command bindings and event types |
| [types.ts](../noema-ui/src/types.ts) | Add AlternateInfo, update DisplayMessage |

## New Files

| File | Purpose |
|------|---------|
| `noema-ui/src/components/FavoriteModelChips.tsx` | Quick-select chips above input |
| `noema-ui/src/components/RegenerateModal.tsx` | Model selection for regeneration |
| `noema-ui/src/components/BranchSwitcher.tsx` | Branch navigation dropdown |

---

## Implementation Order (with testing checkpoints)

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
- [ ] Can select/deselect chips for multi-model mode (wired but needs testing)

---

### Step 2: SpanSet Data Layer ✅ COMPLETED (Backend Only)
- Database schema changes (span_sets + spans + span_messages)
- Storage methods for span management
- Comprehensive tests

**Commits:**
- `6c53c785` - Add SpanSet data layer for parallel model responses

**What's done:**
- [x] New tables: `span_sets`, `spans`, `span_messages`
- [x] Types: `SpanType`, `SpanInfo`, `SpanSetInfo`, `SpanSetWithContent`
- [x] Storage methods:
  - `create_span_set`, `create_span`, `add_span_message`
  - `get_span_set_alternates`, `set_selected_span`
  - `get_span_messages`, `get_span_set_with_content`
  - `get_thread_span_sets`
  - Helper methods: `add_user_span_set`, `add_assistant_span_set`, `add_assistant_span`
- [x] Tests pass: `cargo test -p noema-core --features sqlite`

**Still TODO for Step 2:**
- [ ] Migration for existing messages (not yet implemented)
- [x] Tauri commands for span operations: `get_span_set_alternates`, `set_selected_span`, `get_span_messages`
- [ ] Integration with existing conversation loading

---

### Step 3: Parallel Execution Backend + UI ✅ READY FOR TESTING
- Engine parallel execution logic
- Independent agentic loops per model
- New streaming events (ParallelStreamingMessage, ParallelModelComplete, ParallelComplete, ParallelModelError)
- Tauri command: send_parallel_message
- Event listeners in App.tsx
- Parallel streaming grid view
- FavoriteModelChips triggers parallel send

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

### Step 4: MessageBubble Alternates Tabs
- Add alternates tabs to MessageBubble
- Switching tabs changes displayed content
- Add regenerate button

**Test & Commit:**
- [ ] Messages with alternates show tab bar
- [ ] Clicking tab switches displayed content
- [ ] Selected alternate persists (saved to DB)
- [ ] Regenerate button appears on assistant messages

---

### Step 5: RegenerateModal
- Modal for selecting models to regenerate with
- Wire up regenerate button → modal → parallel execution

**Test:**
- [ ] Clicking regenerate opens modal
- [ ] Can select multiple models
- [ ] Regenerate creates new alternates on the message
- [ ] New alternates appear in tabs

---

### Step 6: Fork from Alternate
- Fork command + storage logic
- BranchSwitcher component
- Branch management commands

**Test:**
- [ ] Can fork from any alternate
- [ ] New branch appears in BranchSwitcher
- [ ] Can switch between branches
- [ ] Conversation continues correctly on each branch
- [ ] Can rename/delete branches
