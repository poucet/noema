# Phase 1 Implementation Scratchpad

## Overview

Phase 1 consists of 5 "Quick Wins" features focused on model display and privacy:

| Done | # | Feature | Files |
|------|---|---------|-------|
| [x] | 4 | Local vs non-local model icon (P1) | ModelSelector.tsx, lib.rs |
| [x] | 3 | Model metadata display (P1) | ModelSelector.tsx |
| [ ] | 2 | Truncate long model names (P2) | ModelSelector.tsx |
| [x] | 31 | Copy raw markdown (P1) | MessageBubble.tsx |
| [ ] | 32 | Private content flag (P0) | Settings.tsx, engine.rs, types |
| [x] | 33 | Toggle to disable tools (P1) | ChatInput.tsx, App.tsx, types.rs, tauri.ts |
| [ ] | 34 | Toggle to disable audio/image input (P1) | ChatInput.tsx |

---

## Key Files & Current State

### Frontend Components

**ModelSelector.tsx**
- Path: `noema-desktop/src/components/ModelSelector.tsx`
- Current: Shows model name, context window (e.g. "128K"), capability icons (text/vision/embedding)
- Has StarIcon for favorites, groups models by provider
- Line 165: `displayName` already has `truncate` CSS class
- Line 182-189: Context window display exists
- Provider name shown but no local/cloud icon

**MessageBubble.tsx**
- Path: `noema-desktop/src/components/MessageBubble.tsx`
- Current: Renders messages with fork button (hover, bottom-right)
- Line 117-124: Fork button infrastructure
- `message.content` contains raw markdown
- No copy button yet

**Settings.tsx**
- Path: `noema-desktop/src/components/Settings.tsx`
- Current: Modal with tabs: "MCP Servers", "API Keys", "Google Docs"
- Can add new tab for conversation/privacy settings

**App.tsx**
- Path: `noema-desktop/src/App.tsx`
- Error handling: simple error banner (lines 547-559)
- No toast system yet

### Backend (Rust)

**types.rs**
- Path: `noema-desktop/src-tauri/src/types.rs`
```rust
pub struct ModelInfo {
    pub id: String,
    pub display_name: String,
    pub provider: String,
    pub capabilities: Vec<String>,
    pub context_window: Option<u32>,
}
```

**engine.rs**
- Path: `noema-core/src/engine.rs`
- `EngineCommand` enum handles message sending
- Model creation via `create_model()` from llm crate
- Validation would go before sending

**registry.rs**
- Path: `noema-core/llm/src/registry.rs`
```rust
pub struct ProviderInfo {
    pub name: &'static str,
    pub api_key_env: Option<&'static str>,
    pub base_url_env: &'static str,
}
```

### Generated Types (TypeScript)

**ModelInfo.ts**
- Path: `noema-desktop/src/generated/ModelInfo.ts`
```typescript
export type ModelInfo = {
    id: string,
    displayName: string,
    provider: string,
    capabilities: Array<string>,
    contextWindow: number | null
};
```

---

## Provider Classification

**Local providers** (privacy-safe):
- `ollama`
- `llama.cpp` / `llamacpp`
- `localai`
- `lmstudio`

**Cloud providers** (data leaves device):
- `anthropic`
- `openai`
- `gemini`
- `openrouter`
- `groq`

---

## Icon Approach

Codebase uses **inline SVG icons** (no external library like Lucide).
Examples in ModelSelector.tsx:
- TextIcon, VisionIcon, EmbeddingIcon as const SVG components
- StarIcon for favorites

Will add:
- LaptopIcon (local) or ShieldIcon
- CloudIcon (cloud/remote)
- CopyIcon (for markdown copy)
- LockIcon (for private content)

---

## Implementation Order

1. **Feature 4**: Local vs non-local icon (simplest, no backend changes)
2. **Feature 3**: Model metadata display (enhance existing)
3. **Feature 2**: Truncate long model names (CSS only)
4. **Feature 31**: Copy raw markdown (add button + clipboard)
5. **Feature 32**: Private content flag (frontend + backend + validation)

---

## Progress Log

### Feature 4: Privacy Icon via Capabilities ✅ DONE

**Architecture decision:** Privacy is now a `ModelCapability` enum variant, not a hardcoded provider list. This is extensible for future capabilities like `Tools`, `Thinking`, `Streaming`, etc.

**Backend changes (`noema-core/llm/src/lib.rs`):**
- Expanded `ModelCapability` enum with new variants:
  - `Vision` (renamed from `Image`)
  - `AudioInput`, `AudioGeneration`, `ImageGeneration`
  - `Tools`, `Thinking`, `Streaming`
  - `Private` - data stays on device
- Each provider sets capabilities per-model

**Provider changes (`noema-core/llm/src/providers/`):**
- Ollama: adds `Private` capability to all models (local)
- Gemini: renamed `Image` → `Vision`

**Frontend changes (`ModelSelector.tsx`):**
- Removed hardcoded `LOCAL_PROVIDERS` list
- Added `PrivateIcon` (shield SVG) and `CloudIcon`
- Added `isPrivateModel()` and `isPrivateProvider()` helpers
- Provider headers show shield (green) for private, cloud (blue) for cloud
- Updated `getCapabilities()` to use `Vision` instead of `Image`

**Build verified:** `cargo build --package llm` succeeds

### Feature 3: Model Metadata Display ✅ DONE

**Changes (`ModelSelector.tsx`):**
- Enhanced current model button to show metadata at-a-glance
- Added privacy indicator icon (shield/cloud) to the left of model name
- Added context window badge (e.g., "128K") next to model name
- Tooltips show full details on hover

**Display layout:**
```
[🛡️] Claude 3.5 Sonnet [128K] ▼
      claude-3-5-sonnet-20241022
```

The dropdown already showed this metadata per-model; now it's visible without opening the dropdown.

### Feature 31: Copy Raw Markdown ✅ DONE

**Implementation:** Added copy button to assistant message bubbles that copies the raw markdown text to clipboard.

**Changes (`MessageBubble.tsx`):**
- Added `extractRawMarkdown()` helper function to convert DisplayContent to markdown string
- Added `CopyIcon` and `CheckIcon` SVG components
- Added `justCopied` state for visual feedback
- Added `handleCopyRawMarkdown()` handler using `navigator.clipboard.writeText()`
- Refactored action buttons into a flex container with both copy and fork buttons
- Copy button shows on hover for assistant messages only (not user/system)
- Shows green check icon for 2 seconds after successful copy

**UX:**
- Button appears on hover at bottom-right of assistant messages
- Click copies raw markdown to clipboard
- Brief green checkmark feedback on success
- Tooltip shows "Copy raw markdown" or "Copied!"

### Feature 33: Toggle to Disable Tools ✅ DONE

**Architecture decision:** Tool configuration uses a future-proof `ToolConfig` type that allows for granular control (specific servers, specific tools) while currently implementing a simple on/off toggle.

**Backend changes:**
- Added `ToolConfig` type to `types.rs` with fields:
  - `enabled: bool` - master toggle
  - `server_ids: Option<Vec<String>>` - filter by MCP server (future)
  - `tool_names: Option<Vec<String>>` - filter by tool name (future)
- Updated `send_message` command to accept optional `ToolConfig`
- Added `tool_config` field to `AppState` for engine to read

**Frontend changes (`ChatInput.tsx`):**
- Added `toolsEnabled` and `onToggleTools` props
- Added gear/cog icon toggle button next to voice button
- Purple when enabled, muted when disabled
- Tooltip shows current state
- Passes `ToolConfig` to `onSend` callback

**Frontend changes (`App.tsx`):**
- Added `toolsEnabled` state (default: true)
- Added `handleToggleTools` function
- Updated `handleSendMessage` to pass `ToolConfig` to tauri
- Voice transcriptions also respect tools state

**Frontend changes (`tauri.ts`):**
- Updated `sendMessage` to accept optional `ToolConfig`

**Generated types:**
- Added `ToolConfig.ts` to generated types
- Exported from `index.ts`

---

## Notes for Future Context

- No toast library installed - Feature 31 needs simple feedback mechanism
- Types auto-generate from Rust via ts-rs (`/src/generated/`)
- Use `jj commit` not `git commit` per project rules
- Each feature should be a separate commit
- Pre-existing TypeScript errors in codebase (19 errors) - not from Phase 1 changes
- Vite build works despite tsc errors
- `ModelCapability` enum is now the single source of truth for model features
