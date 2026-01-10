# Phase 2: Handoff

Context and continuity for the next phase.

---

## System State

- Feature #26 (@-mention file search) completed with loading/empty states
- Features #5, #18, #27, #28 deferred to Phase 4 (post-content model)
- Frontend uses contenteditable with structured blocks for chat input
- Backend document search uses SQLite LIKE pattern

## Architectural Notes

- `searchDocuments` in tauri.ts invokes backend which queries SQLite
- Mention dropdown always shows when active (not just when results exist)
- Generated types use lowercase keys (`text`, `documentRef`)

## Open Questions / Risks

- Parallel conversations (#28) will need significant engine.rs changes
- Content model work in Phase 3 may obsolete current document storage

## Next Steps

Phase 3: Unified Content Model - Major architectural work defining the node system that will underpin all content types.
