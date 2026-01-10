# Phase 1: Quick Wins

## Overview

Phase 1 consists of "Quick Wins" features focused on model display and privacy.

## Task Table

| Status | Pri | # | Feature | Files |
|--------|-----|---|---------|-------|
| done | P1 | 4 | Local vs non-local model icon | ModelSelector.tsx, lib.rs |
| done | P1 | 3 | Model metadata display | ModelSelector.tsx |
| todo | P2 | 2 | Truncate long model names | ModelSelector.tsx |
| done | P1 | 31 | Copy raw markdown | MessageBubble.tsx |
| todo | P0 | 32 | Private content flag | Settings.tsx, engine.rs, types |
| done | P1 | 33 | Toggle to disable tools | ChatInput.tsx, App.tsx, types.rs, tauri.ts |
| todo | P1 | 34 | Toggle to disable audio/image input | ChatInput.tsx |

---

## Feature Details

### Feature 2: Truncate Long Model Names

**Problem**: Long model names (e.g., `llama-3.3-70b-instruct-q4_K_M`) push star icon off-screen.

**Solution**: CSS truncation with ellipsis, star icon outside truncated container.

**File**: `noema-desktop/src/components/ModelSelector.tsx`

---

### Feature 32: Private Content Flag

**Problem**: Users want to ensure sensitive content is never sent to cloud/non-local models.

**Solution**: Add "private" flag to content items. Private content blocks non-local model usage.
- Toggle in conversation/document settings
- Visual indicator (lock icon) on private items
- When attempting to use cloud model with private content:
  - Show warning dialog explaining data will leave device
  - Require explicit confirmation to proceed
  - Option to switch to local model instead

**Behavior**:
- Private conversation -> blocks cloud models for that conversation
- Private document attached -> blocks cloud models for that message
- Inherits: private tag -> all children are private

**Files**:
- `noema-desktop/src/components/ConversationSettings.tsx`
- `noema-core/src/engine.rs` (model selection validation)

---

### Feature 34: Toggle to Disable Audio/Image Input

**Problem**: Some models don't support multimodal input. When users try to attach media, it fails or gets ignored.

**Solution**: Add toggles to disable audio/image input with placeholder UI.
- When model lacks `Vision` capability, image input is disabled
- When model lacks `AudioInput` capability, audio input is disabled
- Show placeholder instead of input button: "Model doesn't support images"
- Easy toggle to force-enable (for testing or model capability updates)
- Per-conversation override possible

**File**: `noema-desktop/src/components/ChatInput.tsx`

---

## Key Files Reference

### Frontend Components

**ModelSelector.tsx**
- Path: `noema-desktop/src/components/ModelSelector.tsx`
- Shows model name, context window, capability icons
- Has StarIcon for favorites, groups models by provider

**MessageBubble.tsx**
- Path: `noema-desktop/src/components/MessageBubble.tsx`
- Renders messages with fork button (hover, bottom-right)

**Settings.tsx**
- Path: `noema-desktop/src/components/Settings.tsx`
- Modal with tabs: "MCP Servers", "API Keys", "Google Docs"

**ChatInput.tsx**
- Path: `noema-desktop/src/components/ChatInput.tsx`
- Main input component with tool toggle

### Backend (Rust)

**types.rs**: `noema-desktop/src-tauri/src/types.rs`
**engine.rs**: `noema-core/src/engine.rs`
**registry.rs**: `noema-core/llm/src/registry.rs`

### Generated Types

**ModelInfo.ts**: `noema-desktop/src/generated/ModelInfo.ts`
**ToolConfig.ts**: `noema-desktop/src/generated/ToolConfig.ts`
