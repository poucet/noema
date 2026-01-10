# Phase 1: Development Log

Chronological record of changes made during Phase 1.

---

## Feature 4: Privacy Icon via Capabilities

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
- Gemini: renamed `Image` -> `Vision`

**Frontend changes (`ModelSelector.tsx`):**
- Removed hardcoded `LOCAL_PROVIDERS` list
- Added `PrivateIcon` (shield SVG) and `CloudIcon`
- Added `isPrivateModel()` and `isPrivateProvider()` helpers
- Provider headers show shield (green) for private, cloud (blue) for cloud
- Updated `getCapabilities()` to use `Vision` instead of `Image`

**Build verified:** `cargo build --package llm` succeeds

---

## Feature 3: Model Metadata Display

**Changes (`ModelSelector.tsx`):**
- Enhanced current model button to show metadata at-a-glance
- Added privacy indicator icon (shield/cloud) to the left of model name
- Added context window badge (e.g., "128K") next to model name
- Tooltips show full details on hover

**Display layout:**
```
[shield] Claude 3.5 Sonnet [128K] v
         claude-3-5-sonnet-20241022
```

The dropdown already showed this metadata per-model; now it's visible without opening the dropdown.

---

## Feature 31: Copy Raw Markdown

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

---

## Feature 33: Toggle to Disable Tools

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
