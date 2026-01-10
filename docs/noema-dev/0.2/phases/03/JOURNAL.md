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
