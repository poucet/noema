# v1.0 — Next Phase Tasks

**Roadmap:** [ROADMAP.md](ROADMAP.md)

Three workstreams. Tasks within each are roughly sequential.

---

## 1. UCM Unification

**Design:** [UNIFIED_CONTENT_MODEL.md](../designs/UNIFIED_CONTENT_MODEL.md)

Documents, tabs, and other structured content collapse onto `entities` + `entity_relations` + `content_blocks`. Daemon exposes a generic `EntityApi`; admin and Noema UIs become entity-first; only the import skill knows "a Google Doc becomes a `document::tabbed` with child `document::tab` entities." Non-breaking at every stage — daemon, Lumina, and Noema keep running.

### Stage 1 — Schema & constants

- ⬜ 1.1 Add `entities.content_block_id` (nullable TEXT → content_blocks.id) — at most one block per entity
- ⬜ 1.2 Add `entities.origin` (nullable TEXT, `"<scheme>:<id>"`) + partial index on `(user_id, origin)`
- ⬜ 1.3 Add `entity_relations.position` (nullable INTEGER) + index `(to_id, relation, position)`
- ⬜ 1.4 Drop `entity_relations.created_at` (unused); keep `metadata` optional
- ⬜ 1.5 New `entity_assets (entity_id, asset_id)` table for asset/blob GC joins
- ⬜ 1.6 Namespaced `EntityType` constants: `document::tabbed`, `document::note`, `document::todo`, `document::prompt`, `document::knowledge`, `document::context`, `document::intent`, `document::system_prompt`, `document::access_rule`, `document::tab`, `system::directory`, `system::label`
- ⬜ 1.7 Namespaced `RelationType` constants: `structure::contained_in`, `reference::to`, `label::tagged_with`, `conversation::forked_from`, `conversation::spawned_from`, `collection::grouped_with`
- ⬜ 1.8 `origin_scheme` constants module; `origin(scheme, id)` / `parse_origin(s)` helpers
- ⬜ 1.9 `EntityStore::add_relation(…, position: Option<i64>)`; `list_relations_{from,to}_ordered` helpers
- ⬜ 1.10 Delete `DocumentSource` enum

### Stage 2 — Coordinator: generic entity+content+relation primitives (additive)

Additive. `DocumentStore` and the old `documents` / `document_tabs` / `document_revisions` tables keep working unchanged — rewriting `DocumentStore` to back onto entities would force it to reach into `TextStore` (a layering violation), since its API returns inlined `content_markdown` while entity-backed storage keeps content in `content_blocks`. Old stuff stays live until Stage 7 deletes it.

- ⬜ 2.1 `create_entity_with_content(kind, user, name, content?, origin?) -> EntityId`
- ⬜ 2.2 `update_entity_content(id, text, content_origin) -> ContentBlockId` — new block, swap pointer, orphan old
- ⬜ 2.3 `resolve_entity_text(id) -> Option<String>`
- ⬜ 2.4 `get_entity_by_origin(user, origin)` / `list_entities_by_origin_scheme(user, scheme)` / `list_entities_by_type_prefix(user, prefix)`
- ⬜ 2.5 `add_child(parent, child, relation, position?, metadata?)` / `list_children(parent, relation)` / `list_children_recursive(parent, relation)`
- ⬜ 2.6 `move_entity(child, new_parent, new_position)` — atomic reparent + sibling renumber (used by drag-and-drop in Phase 2)
- ⬜ 2.7 `set_entity_assets(id, &[AssetId])` / `get_entity_assets(id)` / `entities_referencing(asset_id)` for blob GC
- ⬜ 2.8 `delete_entity_cascade(id, relations_to_follow)` — orchestrator walks `structure::contained_in`, removes entities, GCs orphan blocks

### Stage 3 — Daemon `EntityApi` alongside `DocumentApi`

Both APIs coexist. Clients migrate individually in later stages. Old `documents` / `document_tabs` / `document_revisions` tables remain untouched.

- ⬜ 3.1 New `simply-daemon-api/src/entity.rs` with `EntityApi` trait
- ⬜ 3.2 `list_entities(type_prefix?)`, `get_entity(id)`, `create_entity(req)`, `rename_entity(id, name)`, `delete_entity(id)`
- ⬜ 3.3 `get_entity_content(id) -> EntityContent`, `update_entity_content(id, req)`
- ⬜ 3.4 `list_children(parent, relation) -> Vec<ChildEntity>`, `add_child`, `remove_child`, `move_child(parent, child, new_position)`
- ⬜ 3.5 `search_entities(query, type_prefix?, limit)`
- ⬜ 3.6 `EntitySummary` wire type: `has_content: bool`, `child_counts: map<relation, u32>` — for capability-based UI rendering
- ⬜ 3.7 Lazy content fetch: structure responses omit content bodies
- ⬜ 3.8 `Daemon::entity()` accessor alongside `Daemon::document()`
- ⬜ 3.9 `EntityService` implements `EntityApi` over coordinator primitives

### Stage 4 — gdocs skill → `EntityApi`

- ⬜ 4.1 gdocs skill swaps `daemon.document()` → `daemon.entity()`
- ⬜ 4.2 Import creates `document::tabbed` + child `document::tab` entities linked via `structure::contained_in` with `origin = "google_drive:<gdoc_id>"`
- ⬜ 4.3 Tab-tree re-parenting uses `add_child` with `position = tab_index`
- ⬜ 4.4 Verify re-import: existing entity with same origin is deleted + recreated (gdocs currently does this for docs; do it for the entity now)

### Stage 5 — Admin UI: entity-first rendering

- ⬜ 5.1 New `EntitiesPage.svelte` — list all entities, filter by type prefix (default `document::%`)
- ⬜ 5.2 New `EntityPage.svelte` — dispatches on `has_content` / `child_counts`
- ⬜ 5.3 Markdown editor loads content lazily via `get_entity_content`
- ⬜ 5.4 Tab tree nav sidebar + lazy-loaded tab content for `document::tabbed`
- ⬜ 5.5 Regenerate TS bindings via `ts-rs`
- ⬜ 5.6 Delete `DocumentsPage.svelte` / `DocumentPage.svelte` after cutover

### Stage 6 — Noema UI: document browsing + viewer

Complete the paused document work (see `noema/ui/src/lib/DocumentsPanel.svelte` staged change).

- ⬜ 6.1 Complete `DocumentsPanel.svelte` on top of `EntityApi.list_entities("document::")`
- ⬜ 6.2 New `DocumentView.svelte` — dispatch on `kind` / `has_content` / `child_counts`
- ⬜ 6.3 New `entities.ts` client module wrapping `@simply/client` `EntityApi` bindings
- ⬜ 6.4 Lazy-load tab content for `document::tabbed`; direct markdown for flat kinds
- ⬜ 6.5 Update chat `DocumentRef` resolution to use `EntityApi.get_entity_content`

### Stage 7 — Delete `DocumentApi` + `DocumentStore` + old tables

Zero callers remain after Stages 4–6. Cleanup commit.

- ⬜ 7.1 Delete `simply-daemon-api/src/document.rs` + `services/document.rs`
- ⬜ 7.2 Drop `Daemon::document()` accessor
- ⬜ 7.3 Delete `DocumentStore` trait and all impls
- ⬜ 7.4 Delete `Document` / `DocumentTab` / `DocumentRevision` types
- ⬜ 7.5 Drop `StorageTypes::Document` / `Stores::document()`
- ⬜ 7.6 Delete `document_resolver.rs` (superseded by `EntityApi.get_entity_content` + generic reference resolution)
- ⬜ 7.7 Drop `documents` / `document_tabs` / `document_revisions` tables

### Stage 8 — RAG pivot with entity-type filter + per-source frontmatter

- ⬜ 8.1 `VectorChunk` keyed on `content_block_id`; denormalized `entity_id`, `entity_kind`, `title` for display
- ⬜ 8.2 `vector_chunks.entity_kind` column for fast filter predicate
- ⬜ 8.3 `EntityFilter { include: Vec<EntityTypeMatcher>, exclude: Vec<…> }` with `Exact(String)` / `Prefix(String)`; ts-rs exported
- ⬜ 8.4 Chunker prepends per-source frontmatter before embedding (not stored)
- ⬜ 8.5 `EmbedJob { content_block_id, frontmatter, text, owner_entity_id, entity_kind, title }`
- ⬜ 8.6 `SearchHit { content_block_id, owner_entity_id, entity_kind, title, score, chunk_text }`
- ⬜ 8.7 `SearchRequest` carries `EntityFilter`; search applies filter predicate in SQL
- ⬜ 8.8 Reindex walks all `has_content` entities; frontmatter includes `{block_id, entity_id, entity_kind, title, ancestry}` (parent titles via `contained_in` walk)
- ⬜ 8.9 Lumina RAG default filter: include `document::%` + `document::tab`; exclude `document::system_prompt`, `document::access_rule`
- ⬜ 8.10 `lumina/src/chat.rs` dedupes hits by `content_block_id`; injects full blocks via `EntityApi.get_entity_content`

### Phase 2 — Directories UX (fast-follow after Stage 8)

- ⬜ P2.1 `EntityApi.move_child` atomic reparent + sibling renumber (backend already exists in Stage 2.6)
- ⬜ P2.2 Admin UI: directory tree in nav sidebar; "New folder" button; drag-and-drop filing
- ⬜ P2.3 Noema UI: directory tree in DocumentsPanel; drag-and-drop filing

---

## 2. Events & Intents

### Stage 1 — Event Bus + Timer Source

- ⬜ 1.1 Event bus in `simply-core` — pub/sub with typed event payloads
- ⬜ 1.2 Timer event source: cron, interval, one-shot, fuzzy time expressions
- ⬜ 1.3 Intent documents in UCM with `type: intent` frontmatter
- ⬜ 1.4 Intent execution table (SQLite) — stores runtime state (last fired, next fire, status)
- ⬜ 1.5 Action AST: `Expr` with `Literal` and `Template` variants (minimal subset)
- ⬜ 1.6 Action handlers: `notify`, `emit_event`
- ⬜ 1.7 Engine loop: process queue -> check timers -> fire ready intents -> sleep

### Stage 2 — Full Action AST + Service Registry

- ⬜ 2.1 Full `Expr` enum: `EventField`, `ContextRef`, `Lookup`, `Template`
- ⬜ 2.2 Expression resolver — evaluates `Expr` tree against event context
- ⬜ 2.3 Action handlers: `forward`, `update_document`, `call_service`
- ⬜ 2.4 Service registry trait with transport adapters
- ⬜ 2.5 MCP transport adapter (wraps MCP servers as services)
- ⬜ 2.6 Internal transport adapter (wraps daemon's own services)

### Stage 3 — Platform Event Sources

- ⬜ 3.1 Lumina registers Discord event source with daemon on connect
- ⬜ 3.2 Discord events emit into bus: `discord.member_joined`, `discord.message`, `discord.reaction`
- ⬜ 3.3 Noema registers desktop events: app focus, idle detection
- ⬜ 3.4 Event source registration protocol (clients via WS, services via REST)

### Stage 4 — LLM-Compiled Intents

- ⬜ 4.1 MCP tool: `create_intent(description)` — LLM compiles natural language to AST frontmatter
- ⬜ 4.2 LLM compilation prompt: natural language -> trigger + action + target YAML
- ⬜ 4.3 Fuzzy time resolution: "tomorrow morning" -> concrete datetime + original text preserved
- ⬜ 4.4 Re-compilation flow: edit description -> re-compile AST
- ⬜ 4.5 AST validation against registered event sources and action handlers

### Stage 5 — Conditions + Workflow

- ⬜ 5.1 Condition evaluation in intent engine (`all` / `any` modes)
- ⬜ 5.2 Compound triggers: condition + time combined
- ⬜ 5.3 Intent chaining: action output -> next intent's trigger
- ⬜ 5.4 Conversation resumption from intents (reopen suspended conversation with context)
- ⬜ 5.5 Multi-agent orchestration: spawn sub-agents as intents, mainline waits

---

## 3. Multi-user Polish

**Design:** [AUTH_AND_IDENTITY.md](../designs/AUTH_AND_IDENTITY.md)

Stages 1-2 (connection auth, single-port OAuth, admin page) are complete.

### Stage 3 — Per-User MCP OAuth

- ✅ 3.1 Per-user per-MCP-server token storage: TransientTokenStore `(user_id, server_id) → tokens`
- ✅ 3.2 MCP OAuth initiation + callback + token storage
- ✅ 3.3 Token injection via `RequestContext.tokens` into ToolProvider calls
- ⬜ 3.4 `auth_required` error response when user has no token for a service
- ⬜ 3.5 Automatic token refresh using stored refresh tokens
- ⬜ 3.6 Token revocation (admin or self-service)
- ⬜ 3.7 Persist tokens across daemon restarts (encrypt-at-rest in SQLite)

### Stage 4 — Discord Role-Based Access Control

- ⬜ 4.1 `[mcp_access]` config in `lumina.toml` — map Discord roles to MCP server access
- ⬜ 4.2 Lumina checks user's Discord roles before MCP tool calls
- ⬜ 4.3 Graceful denial: "You need the `developers` role to use GitHub tools"
- ⬜ 4.4 Tool call approval flow (see [TOOL_APPROVAL.md](../designs/proposals/TOOL_APPROVAL.md))

### Stage 5 — Admin UI User Management

- ⬜ 5.1 User management: list users, view linked accounts, revoke access
- ⬜ 5.2 Connection browser: view connected clients, active sessions
- ⬜ 5.3 Per-service OAuth status display

---

## Dependencies

```
UCM Stage 1 (schema + constants) ──► UCM Stage 2 (coordinator primitives)
UCM Stage 2 ──► UCM Stage 3 (EntityApi)
UCM Stage 3 ──► UCM Stages 4–6 (gdocs, admin UI, Noema UI — can run in parallel)
UCM Stages 4+5+6 ──► UCM Stage 7 (delete old API + old tables)
UCM Stage 3 ──► UCM Stage 8 (RAG pivot — can start once EntityApi is usable)

Events Stage 2 (Service Registry) ──► Events Stage 3 + 4 (parallel)
Events Stage 3 + 4 ──► Events Stage 5 (Conditions + Workflow)

Multi-user Stage 3 (Per-User MCP OAuth) ──► Multi-user Stage 4 (Role-Based Access)
Multi-user Stage 4 ──► Multi-user Stage 5 (Admin UI)
```
