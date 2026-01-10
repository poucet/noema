# Phase 2: Core UX (Model-Independent)

## Overview

Phase 2 focuses on core UX improvements that don't depend on specific models. These features enhance the chat experience, file handling, and rendering.

## Task Table

| Status | Pri | # | Feature | Files |
|--------|-----|---|---------|-------|
| ✅ | P0 | 26 | @-mention file search beyond initial list | ChatInput.tsx, tauri.ts |
| ⏸️ | P1 | 28 | Parallel conversations with status indicators | App.tsx, SidePanel.tsx, engine.rs |
| ⏸️ | P2 | 5 | Copy-paste markdown into ChatInput | ChatInput.tsx |
| ⏸️ | P2 | 18 | Live markdown/Typst rendering with math notation | DocumentPanel.tsx |
| ⏸️ | P3 | 27 | Fix Google Docs import search | Settings.tsx |

Status: ⬜ todo, 🔄 in-progress, ✅ done, 🚫 blocked, ⏸️ deferred

---

## Feature Details

### Feature 26: @-Mention File Search

**Problem**: When typing `@` in ChatInput, only initially loaded files are shown. Files not in the initial list can't be found.

**Solution**: The search already debounces and calls `searchDocuments`. Verify it works with larger document sets and improve UX.

**Files**:
- `noema-desktop/src/components/ChatInput.tsx`
- `noema-desktop/src/tauri.ts`

**Acceptance Criteria**:
- [ ] @-mention shows search results from all documents
- [ ] Search is debounced (already implemented, verify)
- [ ] Dropdown shows reasonable number of results (5-10)

---

### Feature 28: Parallel Conversations with Status Indicators

**Problem**: When AI is responding in one conversation, user can't switch to another conversation and continue working. No visual indication of conversation state.

**Solution**:
- Allow switching conversations while AI is streaming a response
- Show status indicators in conversation list: busy (spinner), has new messages (badge)
- Background conversations continue processing independently

**Files**:
- `noema-desktop/src/App.tsx` (streaming state management)
- `noema-desktop/src/components/SidePanel.tsx` (status indicators)
- `noema-core/src/engine.rs` (per-conversation state)

**Acceptance Criteria**:
- [ ] Can switch conversations while AI is streaming
- [ ] Spinner icon shows on conversation currently streaming
- [ ] Returning to conversation shows continued/completed response

---

### Feature 5: Copy-Paste Markdown into ChatInput

**Problem**: Pasting formatted content loses markdown structure.

**Solution**: Convert HTML clipboard to markdown on paste using `turndown` package.

**Files**:
- `noema-desktop/src/components/ChatInput.tsx`

**Acceptance Criteria**:
- [ ] Pasting from rich text sources preserves markdown formatting
- [ ] Headings, lists, links, code blocks convert correctly
- [ ] Plain text paste still works normally

---

### Feature 18: Live Markdown/Typst Rendering

**Problem**: No live preview; math notation poorly supported.

**Solution**: Split view editor with live preview. Support both Markdown+KaTeX and Typst.

**Files**:
- `noema-desktop/src/components/DocumentPanel.tsx`

**Acceptance Criteria**:
- [ ] Math notation renders correctly (LaTeX/KaTeX)
- [ ] Live preview updates as user types
- [ ] Toggle between edit/preview/split modes

---

### Feature 27: Fix Google Docs Import Search

**Problem**: Search functionality in the Google Docs import screen is broken.

**Solution**: Debug and fix the search/filter in the docs import modal.

**Files**:
- `noema-desktop/src/components/Settings.tsx`

**Acceptance Criteria**:
- [ ] Search filters Google Docs list correctly
- [ ] Case-insensitive search
- [ ] Empty results state handled

---

## Key Files Reference

### Frontend Components
- `ChatInput.tsx`: Main input with @-mention, paste handling
- `SidePanel.tsx`: Conversation list with status indicators
- `DocumentPanel.tsx`: Document viewer/editor
- `Settings.tsx`: Settings modal including Google Docs import

### Backend
- `engine.rs`: Conversation/streaming state management
- `tauri.ts`: Tauri command bindings
