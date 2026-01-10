# Phase 3: Journal

Chronological stream of thoughts, changes, and observations.

---

## Context from Phase 2

- Feature #26 (@-mention search) completed
- Remaining Phase 2 features deferred to Phase 4
- Design doc exists: `design/UNIFIED_CONTENT_MODEL.md`

See `../02/HANDOFF.md` for full context.

---

## 2026-01-10: Design Doc Extended

Extended `UNIFIED_CONTENT_MODEL.md` with detailed feature requirements:

- **FR-1: Content Storage** - ContentBlock (text, content-addressed) and Asset (binary)
- **FR-2: Conversation Structure** - Turns, alternatives as spans, messages, views
- **FR-3: Document Structure** - Revisions forming DAG
- **FR-4: Collection Structure** - Tree with items, tags, fields
- **FR-5: Cross-References** - Any-to-any with backlinks
- **FR-6: Views and Queries** - List, tree, table views
- **FR-7: Agent Context** - Templates with context injection
- **FR-8: Import/Export** - JSON and Markdown

Added SQL schemas for all tables, Rust trait signatures for ConversationStore, and implementation phases (3a-3d).

Key design decisions captured:
- Alternatives are spans of messages (not single messages)
- ContentBlock is text-only, Assets are binary-only
- Tool calls/results stay inline in messages
- Views select paths through alternatives

---

## 2026-01-10: IDEAS Vetting Against UCM & Hook System

Reviewed new IDEAS (#1, #4-12) against existing designs:

**Fully Covered:**
- #5 Dynamic Typst Functions → `is_dynamic` flag + `render.before.*` hooks
- #6 Proactive Check-ins → `temporal.idle.*` and `temporal.scheduled.*` triggers
- #8 Auto-journaling → `entity.created.message` hook + enqueue action
- #10 Reflexes → This IS the hook system (Input/Time/Context = hooks by type)
- #7 Endless Conversation → Views + context strategies (EP-5)

**Partially Covered (extension points exist):**
- #1 Access Control → `is_private` exists; ACL tables can be added later
- #4 Local Filesystem → `DocumentSource::Import` + asset `local_path`; bidirectional sync as future extension
- #9 Active Context Engine → Hooks provide foundation; nudge UI is future feature
- #11 Soft Schemas → Collections with advisory schema_hint + tags; tag inheritance can be added

**Not a Gap (naming):**
- #12 Neuro Nomenclature → Alignment opportunity, not structural change

**Conclusion:** UCM and Hook System designs are already future-proof for all new IDEAS. No changes needed to Phase 3 scope.

---

## 2026-01-10: Microtasks Format Finalized

Restructured TASKS.md microtasks for clarity:

1. **Compact tables** - Removed verbose per-task tables, kept simple `| Status | # | Task |` format
2. **Emoji prefixes** for commit categorization:
   - 🏗️ types/traits
   - 📦 schema/migration
   - ⚡ implementation
   - ✅ tests
   - 🔧 integration
   - 🧹 cleanup
3. **Detailed DoD** moved under Feature Details section with Create/Update/Implement/SQL/Test columns

Total: 77 microtasks across 10 features.

---
