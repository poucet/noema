# Phase 2: Journal

Chronological stream of thoughts, changes, and observations.

---

## Context from Phase 1

Key patterns established:
- Generated types use lowercase keys (`text`, `documentRef`, not `Text`, `DocumentRef`)
- ModelCapability enum is the source of truth for model features
- Inline SVG icons (no external library)
- Flex truncation requires `min-w-0` + `overflow-hidden`

See `../01/HANDOFF.md` for full context.

---

## 2026-01-10: Feature 26 - @-mention file search

**Changes:**
- Added `mentionLoading` state to track search in progress
- Updated search effect to set loading state and increased limit from 5 to 10 results
- Added loading state ("Searching...") to mention dropdown
- Added empty state ("No documents found") when search returns no results
- Dropdown now always shows when mention is active (not just when results exist)

**Files modified:**
- `noema-desktop/src/components/ChatInput.tsx`

**Notes:**
- The search was already querying all documents via `searchDocuments` Tauri command
- Backend uses SQLite `LIKE` with `%query%` pattern for case-insensitive title search
- UX improvements: loading indicator, empty state, increased result count

---

## 2026-01-10: Feature 5 - Copy-paste markdown into ChatInput

**Changes:**
- Added `turndown` package for HTML-to-markdown conversion
- Extended `handlePaste` to detect HTML clipboard content
- Convert HTML with formatting (headings, lists, links, code, etc.) to markdown
- Plain text paste still works normally (falls through to default behavior)

**Files modified:**
- `noema-desktop/package.json` - added turndown + types
- `noema-desktop/src/components/ChatInput.tsx` - paste handler

**Notes:**
- Uses regex to detect meaningful HTML structure before converting
- Turndown configured with ATX headings, fenced code blocks, dash bullets
- Inserts converted markdown at cursor position, then syncs state via input event

---
