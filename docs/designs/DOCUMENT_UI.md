# Document UI & Editor

**Status:** draft
**Depends on:** Admin UI (in progress), Content & RAG (Stage 1)

---

## Problem

There's no way to create, view, or edit documents in the system. The daemon has a `DocumentApi` with full CRUD but no UI surface for it. Without content in the system, RAG and document-backed features can't be developed or tested.

## Goals

1. **Document browser** — list, search, create, delete documents in the daemon admin UI
2. **Markdown editor** — CodeMirror 6 with syntax highlighting for editing document tabs
3. **Tab-aware** — documents have a tab tree; editor navigates and edits individual tabs
4. **Daemon-native** — built into the daemon's web UI (Astro + React islands), no separate desktop app needed
5. **Frontmatter support** — documents can have YAML frontmatter (type, tags, etc.); editor preserves it

## Non-Goals

- Rich text / WYSIWYG editing (this is a code-editor-style markdown editor)
- Real-time collaborative editing
- Google Docs import (separate — comes via MCP server OAuth)
- Sharing components with Noema (if the daemon UI is good enough, Noema may not need its own document UI)

## Approach

### Tech Stack

- **CodeMirror 6** — markdown editing with syntax highlighting
- **React** — component islands in the Astro admin page (via `@astrojs/react`)
- **Tailwind CSS** — styling consistent with admin UI dark theme

### UI Structure

```
Admin UI (localhost:9800)
├── Setup Wizard (existing)
├── Dashboard (existing)
└── Documents (new)
    ├── Sidebar: document list + search + create button
    └── Main area:
        ├── Tab tree (left rail within editor)
        └── CodeMirror editor (right)
```

### Pages

- `/documents` — document browser + editor (new Astro page)
- Admin dashboard gets a "Documents" link/count

### API Surface

Uses existing `DocumentApi` endpoints:
- `GET /document` — list documents
- `POST /document` — create document
- `GET /document/{id}` — get document with tabs
- `PUT /document/{id}/title` — rename
- `DELETE /document/{id}` — delete
- `POST /document/{id}/tab` — create tab
- `GET /tab/{id}` — get tab content
- `PUT /tab/{id}` — update tab content
- `DELETE /tab/{id}` — delete tab

Plus search: `GET /document/search?q=...`

### Editor Behavior

- Open document → load tab tree → select first tab → show in CodeMirror
- Auto-save on blur or after idle (debounced PUT to update tab content)
- Frontmatter shown as part of the markdown (not a separate UI)
- New document: creates doc + initial tab in one step
- New tab: modal/inline input for tab title, inserted into tree

## Open Questions

1. **Navigation** — Astro multi-page (separate `/documents` route) or single-page with client-side tabs? Multi-page is simpler with Astro.
2. **Document list** — flat list or tree/folder structure? Documents are flat today, tabs provide hierarchy within a document.
3. **Keyboard shortcuts** — Cmd+S to save? Cmd+N for new doc? Worth doing early or polish later?
