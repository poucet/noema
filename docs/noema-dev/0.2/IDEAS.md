# Ideas

Lightweight inbox for ideas not yet in the roadmap.

## Inbox

| # | Idea | Notes | Added |
|---|------|-------|-------|

## Triaged

Ideas reviewed and assigned a disposition.

| # | Idea | Disposition | Notes |
|---|------|-------------|-------|
| 1 | Access control model | roadmap (P8) | ACL extension point exists; full implementation in Phase 8 |
| 2 | Editable/recomputable conversation history | roadmap | Captured in UCM Use Case 5 (Edit & Splice), FR-2.6, FR-2.7 |
| 3 | Unified content model | roadmap | See [design/UNIFIED_CONTENT_MODEL.md](design/UNIFIED_CONTENT_MODEL.md) |
| 4 | Local filesystem sync | roadmap (P8) | DocumentSource::Import + asset local_path; bidirectional sync in Phase 8 |
| 5 | Dynamic Typst functions | roadmap (P8) | Covered by `is_dynamic` flag + render.before.* hooks |
| 6 | Proactive AI check-ins | roadmap (P8) | Covered by temporal.idle.* and temporal.scheduled.* triggers |
| 7 | Endless conversation mode | roadmap (P8) | Views + context strategies enable this |
| 8 | Auto-journaling | roadmap (P8) | entity.created.message hook + enqueue action |
| 9 | Active context engine | roadmap (P8) | Hook system provides foundation; nudge UI as future feature |
| 10 | Reflexes | roadmap (P8) | This IS the hook system (Input/Time/Context hooks) |
| 11 | Soft schemas / tag hierarchy | roadmap (P8) | Collections have advisory schema; tag inheritance extension |
| 12 | Neuro nomenclature | defer | Naming convention - apply during future refactoring |
