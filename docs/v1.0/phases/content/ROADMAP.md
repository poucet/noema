# Content Platform

**Parent:** [v1.0 Roadmap](../../ROADMAP.md)
**Priority:** P0 — foundation for intents (Events phase) and all content features.
**Complexity:** M
**Depends on:** Lumina complete
**Parallel with:** Events phase. Events soft-depends on Content for intent document storage.

---

## Goal

Agent can call MCP tools through simply-daemon. Documents with frontmatter conventions work as the universal content primitive. Cross-platform CRUD — create in Noema, query from Lumina, and vice versa.

---

## Stages

### Stage 1 — MCP in Core

**Goal:** MCP server/client runs inside simply-daemon (via simply-core). Agent can call tools, daemon can connect to external MCP action services.

**Complexity:** M

**Tasks:**
- [ ] MCP server in simply-core — expose built-in tools to agents
- [ ] MCP client in simply-core — connect to external MCP action services
- [ ] Agent tool calls route through the daemon's global MCP tool registry
- [ ] Dynamic service registration: `POST /register` with MCP endpoint → daemon connects
- [ ] MCP server config via UCM documents (`type: mcp_server`) for persistent registrations

**Verify:** Agent can call a tool via MCP through the daemon from both Noema and Lumina.

---

### Stage 2 — Document CRUD + Frontmatter Queries

**Goal:** Generic document operations with frontmatter-aware queries. The foundation that todos, notes, intents, and all other content types build on.

**Complexity:** M

**Tasks:**
- [ ] `DocumentApi` trait on daemon — extract doc CRUD from Noema-only Tauri commands to a shared daemon RPC trait (like `ConversationApi`)
- [ ] Implement frontmatter parsing + indexing in UCM storage layer
- [ ] Named documents: wire entity slugs to documents so docs can be addressed by namespaced path (e.g. `lumina/system-prompt`) instead of raw UUIDs
- [ ] Generic MCP tools: `create_document`, `query_documents`, `update_document`, `delete_document`
- [ ] Frontmatter-aware query syntax: filter by `type`, `tags`, `done`, `due`, etc.
- [ ] Daemon web UI: lightweight markdown editor on the REST port for creating/editing documents without Noema
- [ ] Cross-platform test: create from one client, read from another

**Verify:**
- Create a document with `type: todo` frontmatter via Noema, query it from Lumina.
- Query documents by frontmatter fields (e.g., "all todos where done = false").
- Edit a document via daemon web UI at `http://localhost:9801/docs`.

---

### Stage 3 — Content Conventions

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
Stage 1 → Stage 2 → Stage 3 (sequential)
```

Events phase can start in parallel. Events Stage 1 needs basic document storage for intent documents — can use simple storage initially, then migrate to frontmatter-aware queries when Content Stage 2 lands.
