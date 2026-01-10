# Phase 1 Implementation Scratchpad

## Overview

Phase 1 consists of 5 "Quick Wins" features focused on model display and privacy:

| # | Feature | Files |
|---|---------|-------|
| 32 | Private content flag (P0) | Settings.tsx, engine.rs, types |
| 3 | Model metadata display (P1) | ModelSelector.tsx |
| 4 | Local vs non-local model icon (P1) | ModelSelector.tsx |
| 31 | Copy raw markdown (P1) | MessageBubble.tsx |
| 2 | Truncate long model names (P2) | ModelSelector.tsx |

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

## Notes for Future Context

- No toast library installed - Feature 31 needs simple feedback mechanism
- Types auto-generate from Rust via ts-rs (`/src/generated/`)
- Use `jj commit` not `git commit` per project rules
- Each feature should be a separate commit
