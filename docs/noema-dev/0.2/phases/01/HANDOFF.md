# Phase 1: Handoff

Context and learnings to carry forward to Phase 2.

---

## Completed Features

All Phase 1 features completed:
- Feature 4: Local vs non-local model icon (via ModelCapability enum)
- Feature 3: Model metadata display (context window, privacy icon)
- Feature 2: Truncate long model names (formatModelName with middle truncation)
- Feature 31: Copy raw markdown (uses lowercase keys from ts-rs types)
- Feature 32: Private content flag (conversation-level with warning dialog)
- Feature 33: Toggle to disable tools (ToolConfig with future granularity)
- Feature 34: Toggle to disable audio/image input (capability-based filtering)

## Key Patterns Established

### Type System
- **Generated types use lowercase keys**: `text`, `documentRef`, `toolCall` (not `Text`, `DocumentRef`)
- **ModelCapability enum**: Single source of truth for model features (Vision, AudioInput, Private, Tools, etc.)
- **ToolConfig**: Future-proof type allowing granular control (serverIds, toolNames)

### UI Patterns
- **Inline SVG icons**: No external icon library, use inline SVGs
- **Flex truncation**: Use `min-w-0` + `overflow-hidden` on flex containers for text truncation
- **formatModelName()**: Strip provider prefix, truncate middle preserving end (quantization info)
- **Capability-based UI**: Components check model capabilities to enable/disable features

### State Management
- **Conversation-level settings**: `is_private` stored in SQLite, exposed via Tauri commands
- **Props drilling for capabilities**: Model capabilities passed from App.tsx to child components

## Files Modified

### Frontend
- `ModelSelector.tsx`: formatModelName, capability icons, context window badge
- `MessageBubble.tsx`: Copy raw markdown (extractRawMarkdown)
- `ChatInput.tsx`: Tools toggle, vision/audio capability filtering
- `App.tsx`: Model capability helpers, privacy toggle, ChatInput props

### Backend
- `noema-core/llm/src/lib.rs`: ModelCapability enum expansion
- `noema-core/llm/src/providers/`: Per-provider capability settings
- `noema-core/src/sqlite.rs`: is_private column and methods
- `noema-desktop/src-tauri/src/types.rs`: ToolConfig, ConversationInfo
- `noema-desktop/src-tauri/src/chat.rs`: Privacy commands

## Open Questions for Phase 2

1. **Parallel conversations**: How to track streaming state per-conversation?
2. **@-mention search**: Should search be debounced? How many results to show?
3. **Markdown paste**: Which turndown options for HTML→Markdown conversion?
