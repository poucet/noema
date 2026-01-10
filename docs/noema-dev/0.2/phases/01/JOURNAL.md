# Phase 1: Journal

Chronological stream of thoughts, changes, and observations.

---

## Logged Changes (from DEVLOG.md)

### Feature 4: Privacy Icon via Capabilities
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

### Feature 3: Model Metadata Display
**Changes (`ModelSelector.tsx`):**
- Enhanced current model button to show metadata at-a-glance
- Added privacy indicator icon (shield/cloud) to the left of model name
- Added context window badge (e.g., "128K") next to model name
- Tooltips show full details on hover

### Feature 31: Copy Raw Markdown
**Implementation:** Added copy button to assistant message bubbles that copies the raw markdown text to clipboard.

**Changes (`MessageBubble.tsx`):**
- Added `extractRawMarkdown()` helper function to convert DisplayContent to markdown string
- Added `CopyIcon` and `CheckIcon` SVG components
- Added `justCopied` state for visual feedback
- Added `handleCopyRawMarkdown()` handler using `navigator.clipboard.writeText()`
- Refactored action buttons into a flex container with both copy and fork buttons
- Copy button shows on hover for assistant messages only (not user/system)
- Shows green check icon for 2 seconds after successful copy

### Feature 33: Toggle to Disable Tools
**Architecture decision:** Tool configuration uses a future-proof `ToolConfig` type that allows for granular control (specific servers, specific tools) while currently implementing a simple on/off toggle.

**Backend changes:**
- Added `ToolConfig` type to `types.rs` with fields:
  - `enabled: bool` - master toggle
  - `server_ids: Option<Vec<String>>` - filter by MCP server (future)
  - `tool_names: Option<Vec<String>>` - filter by tool name (future)
- Updated `send_message` command to accept optional `ToolConfig`
- Added `tool_config` field to `AppState` for engine to read

---

## Observations & Learnings (from OBSERVATIONS.md)

### Codebase Patterns
- **Icon System**: Codebase uses inline SVG icons (no external library like Lucide).
- **Type Generation**: Types auto-generate from Rust via ts-rs (`/src/generated/`). Run type generation after adding new Rust types with `#[derive(TS)]`.
- **Provider Classification**: 
    - Local: `ollama`, `llama.cpp`, `localai`, `lmstudio`
    - Cloud: `anthropic`, `openai`, `gemini`, `openrouter`, `groq`

### Technical Notes
- No toast library installed - used inline feedback (checkmark icon) instead.
- Pre-existing TypeScript errors (19 errors) - not from Phase 1 changes.
- `ModelCapability` enum is now the single source of truth for model features.

### Architecture Decisions
1. **Privacy as Capability**: Extensible and centralized logic.
2. **ToolConfig Future-Proofing**: Supports granular control later.

---

## Scratchpad & Planning (from SCRATCHPAD.md)

### Implementation Order
1. Feature 4: Local vs non-local icon (Done)
2. Feature 3: Model metadata display (Done)
3. Feature 2: Truncate long model names (Pending)
4. Feature 31: Copy raw markdown (Done)
5. Feature 32: Private content flag (Pending)

### Remaining Work
- [ ] Feature 2: Truncate long model names
- [ ] Feature 32: Private content flag (P0 - highest priority remaining)
- [ ] Feature 34: Toggle to disable audio/image input
