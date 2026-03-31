# Phase 1A: Content Platform

**Parent:** [v1.0 Roadmap](../../ROADMAP.md)
**Priority:** P0 — foundation for intents (Phase 1B) and all content features.
**Complexity:** M
**Depends on:** Phase 0 complete
**Parallel with:** Phase 1B (events & intents). 1B soft-depends on 1A.2 for intent document storage.

---

## Goal

Agent can call MCP tools through the core service. Documents with frontmatter conventions work as the universal content primitive. Cross-platform CRUD — create in Noema, query from Lumina, and vice versa.

---

## Stages

### 1A.1 — MCP in Core

**Goal:** MCP server/client runs inside `simply-core`, alongside the gRPC interface.

**Complexity:** M

**Tasks:**
- [ ] MCP server in `simply-core` — expose tools to agents
- [ ] MCP client in `simply-core` — connect to external MCP servers
- [ ] Agent tool calls route through core's MCP interface
- [ ] MCP server config: register external servers (e.g., filesystem, custom tools)
- [ ] Expose MCP alongside gRPC (agent-facing vs platform-facing split per ARCHITECTURE.md)

**Verify:** Agent can call a tool via MCP through the core service from both Noema and Lumina.

---

### 1A.2 — Document CRUD + Frontmatter Queries

**Goal:** Generic document operations with frontmatter-aware queries. The foundation that todos, notes, intents, and all other content types build on.

**Complexity:** M

**Tasks:**
- [ ] Implement frontmatter parsing + indexing in UCM storage layer
- [ ] Generic MCP tools: `create_document`, `query_documents`, `update_document`, `delete_document`
- [ ] Frontmatter-aware query syntax: filter by `type`, `tags`, `done`, `due`, etc.
- [ ] gRPC document service for platform clients
- [ ] Cross-platform test: create from one client, read from another

**Verify:**
- Create a document with `type: todo` frontmatter via Noema, query it from Lumina.
- Query documents by frontmatter fields (e.g., "all todos where done = false").

---

### 1A.3 — Content Conventions

**Goal:** Port todo/note frontmatter conventions. Thin Lumina commands that call core.

**Complexity:** S

**Tasks:**
- [ ] Define frontmatter conventions for `todo`, `note` document types
- [ ] Lumina slash commands: `/todo`, `/note` (thin wrappers → MCP tools → core)
- [ ] Noema UI: basic document creation with frontmatter templates
- [ ] Validate conventions: ensure LLM can read/write frontmatter natively

**Verify:**
- `/todo buy milk` creates a document with `type: todo`, `done: false` frontmatter.
- `/note meeting summary` creates a `type: note` document.
- Agent can list and complete todos via natural language.

---

## Dependencies

```
1A.1 → 1A.2 → 1A.3 (sequential within track)
```

Phase 1B (events) can start in parallel. 1B.1 needs basic document storage for intent documents — can use simple storage initially, then migrate to frontmatter-aware queries when 1A.2 lands.
