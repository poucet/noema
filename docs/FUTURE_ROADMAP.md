# Noema Future Roadmap

Features deferred from 0.2 for future versions.

---

## Phase 4: Content Model Features

Features that build on the Unified Content Model foundation.

| Pri | # | Feature | Complexity | Description |
|-----|---|---------|------------|-------------|
| P1 | 1 | Undo delete (soft delete) | Low | Toast notification + undo button, `deleted_at` column |
| P1 | 10 | Per-conversation system prompts | Low | Expose existing column via UI |
| P1 | 12 | Auto-naming via summarizer | Medium | Generate 3-6 word title after first response |
| P1 | 13 | Summaries for all content | Medium | Auto-generated summaries for hover/preview |
| P1 | 17 | Document editing with revision history | Medium | Full editing, auto-save, diff view |
| P2 | 6 | Drag-and-drop reordering | Low | `sort_order` column + dnd-kit |

---

## Phase 5: Organization + Search

| Pri | # | Feature | Complexity | Description |
|-----|---|---------|------------|-------------|
| P0 | 21 | Custom skills and slash commands | Medium | Extensible `/command` system |
| P1 | 14 | Embedding infrastructure + semantic search | High | Vector storage, hybrid search |
| P1 | 7 | Document hierarchy via nested tags | High | Multi-tagging, tag hierarchy |
| P1 | 11 | Conversation hierarchy via nested tags | High | Same as above for conversations |
| P2 | 16 | Wiki-style cross-linking | Medium | `[[doc:Title]]` syntax, backlinks panel |
| P2 | 22 | External integrations | Medium | Notion, Google Calendar, GitHub |

---

## Phase 6: RAG + Memories

| Pri | # | Feature | Complexity | Description |
|-----|---|---------|------------|-------------|
| P1 | 8 | Conversation memories | High | Hybrid auto-suggest + manual approval |
| P1 | 15 | Full RAG pipeline | High | Query → embed → search → inject → LLM |
| P1 | 19 | MCP coding agent tools | High | File ops, terminal, git, code analysis |
| P2 | 9 | Documentation generation | High | Generate docs from conversations |

---

## Phase 7: Agentic + Multimodal

| Pri | # | Feature | Complexity | Description |
|-----|---|---------|------------|-------------|
| P0 | 23 | Audio models (STT/TTS) | Medium | Whisper, Piper, ElevenLabs |
| P1 | 20 | Multi-agent conversations | Very High | Agent orchestration and delegation |
| P2 | 24 | Image generation | Medium | Stable Diffusion, DALL-E, Flux |
| P2 | 25 | PDF extraction | Medium | OCR, image extraction, conversion |

---

## Phase 8: Active Context & Automation

Features enabled by the Hook System design.

| Pri | # | Feature | Complexity | Description |
|-----|---|---------|------------|-------------|
| P1 | I5 | Dynamic Typst functions | Medium | `is_dynamic` flag + render hooks |
| P1 | I6 | Proactive AI check-ins | Medium | `temporal.idle.*` triggers |
| P1 | I10 | Reflexes (hook system) | Medium | Event-driven automation |
| P2 | I7 | Endless conversation mode | Medium | Views + context strategies |
| P2 | I8 | Auto-journaling | Medium | `entity.created.message` hook |
| P2 | I9 | Active context engine | High | Hook system + nudge UI |
| P2 | I11 | Soft schemas / tag hierarchy | Medium | Collections advisory schema |
| P3 | I1 | Access control model | High | ACL extension point |
| P3 | I4 | Local filesystem sync | Medium | Bidirectional document sync |

---

## Deferred from Phase 2

| Pri | # | Feature | Complexity | Description |
|-----|---|---------|------------|-------------|
| P1 | 28 | Parallel conversations | Medium | Background processing, status indicators |
| P2 | 5 | Copy-paste markdown into ChatInput | Medium | Convert HTML clipboard to markdown |
| P2 | 18 | Live markdown/Typst rendering | Medium | Split view editor with math |
| P3 | 27 | Fix Google Docs import search | Low | Debug search/filter in import modal |

---

## Deferred from Phase 3 (Needs UI)

Backend complete, needs frontend components.

| # | Feature | Description |
|---|---------|-------------|
| 3.8.D1 | Subconversation UI | Spawn from message, view results |
| 3.8.D2 | Document editor UI | Create, edit content, tabs, revisions |
| 3.8.D3 | Reference UI | Create refs, backlinks panel |

---

## Future / Out of Scope

| Feature | Notes |
|---------|-------|
| Noema Web (browser version) | Requires noema-backend extraction |
| Cloud sync / multi-device | Requires backend service |
| Neuro nomenclature | Naming convention - apply during refactoring |

---

## Design Documents

See [designs/](designs/) for detailed specifications:

- [UNIFIED_CONTENT_MODEL.md](designs/UNIFIED_CONTENT_MODEL.md) - Three-layer architecture, feature requirements
- [HOOK_SYSTEM.md](designs/HOOK_SYSTEM.md) - Event-driven automation design
- [ARCHITECTURE.md](designs/ARCHITECTURE.md) - System architecture overview
