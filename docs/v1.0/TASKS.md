# v1.0 — Next Phase Tasks

**Roadmap:** [ROADMAP.md](ROADMAP.md)

Four workstreams. Tasks within each are roughly sequential.

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

## 4. Vault-Backed Markdown

**Design:** [VAULT_BACKED_MARKDOWN.md](../designs/VAULT_BACKED_MARKDOWN.md)

Human-authored `document::*` content becomes normal Markdown files in a configurable
vault. SQLite remains canonical for entity identity, relations, access policy,
runtime state, embeddings, and fast indexes. `content_blocks` remain immutable
snapshots so existing content APIs and RAG can migrate without a flag day.

### Stage 1 — Storage foundations

- ✅ 1.1 Add a real SQLite migration runner with `schema_migrations`; keep existing schema creation behavior as the initial migration baseline
- ✅ 1.2 Add configurable `vault_root` with default under Noema data dir; ensure directory creation is explicit and idempotent
- ✅ 1.3 Add `vault_files` projection table for `entity_id`, `path`, `file_key`, `mtime`, content hash, frontmatter hash, status, and last-seen timestamp
- ✅ 1.4 Add `vault_conflicts` table for duplicate IDs, changed IDs, missing files, unmanaged files, and invalid frontmatter
- ✅ 1.5 Add storage traits for reading/writing vault projection rows and conflict records; implement SQLite first

### Stage 2 — Markdown parser and serializer

- ✅ 2.1 Add pure frontmatter parser that splits YAML metadata from Markdown body without database side effects
- ✅ 2.2 Add serializer that preserves unknown user metadata and emits canonical system fields in stable order
- ✅ 2.3 Define opt-in frontmatter identity fields: `id`, `kind`, `origin`, and policy-controlled `privacy`
- ✅ 2.4 Define user-editable fields: `title`, `tags`, and preserved extra metadata
- ✅ 2.5 Add Markdown asset-reference extraction for relative links and image embeds, but do not update `entity_assets` yet

### Stage 3 — Read-only reconciliation

- ✅ 3.1 Add vault scanner that returns a reconciliation plan without mutating entities or files
- ✅ 3.2 Classify same-ID moves by updating the planned path for an existing `entity_id`
- ✅ 3.3 Classify missing known files without deleting or archiving the entity
- ✅ 3.4 Classify known path/file with removed or changed `id` as an identity conflict when frontmatter identity mode is enabled
- ✅ 3.5 Classify duplicate frontmatter IDs as conflicts; keep the prior canonical path when one exists
- ✅ 3.6 Classify unknown files with valid IDs, unknown files without IDs, invalid frontmatter, and unsupported kinds for frontmatter identity mode
- ✅ 3.7 Add projection-first identity mode so known plain Markdown files sync without Noema-owned frontmatter
- ✅ 3.8 Infer frontmatterless moves by unique body hash when the prior projected path is missing
- ✅ 3.9 Treat unknown plain Markdown as unmanaged/importable rather than a missing-ID conflict

### Stage 4 — Projection reconciliation

- ✅ 4.1 Persist scanner output to `vault_files` and `vault_conflicts` only; do not rewrite files automatically
- ✅ 4.2 Mark missing files with a recoverable status and a grace-policy hook
- ✅ 4.3 Add explicit conflict reasons and structured details for UI/API resolution
- ✅ 4.4 Add coordinator entry point for full vault scan at startup
- ✅ 4.5 Add coordinator entry point for scoped path rescans
- ✅ 4.6 Export `.noema/vault-index.json` sidecar snapshots from the vault projection without touching Markdown frontmatter

### Stage 5 — Initial export and read path

- ✅ 5.1 Export `document::tabbed` containers and ordered `document::tab` trees to vault directories/files
- ✅ 5.2 Export flat `document::*` entities with `content_block_id` as plain Markdown files
- ✅ 5.3 Populate sidecar identity entries during initial document export
- ✅ 5.4 Extend sidecar entries with tab-tree parent relation and position metadata for portable Google Doc recovery
- ✅ 5.5 Add optional frontmatter identity export profile for users who want self-contained files
- ✅ 5.6 Add an entity content resolver that reads vault-backed bodies from files and falls back to `content_blocks`
- ✅ 5.7 Route `EntityApi.get_entity_content` through the resolver instead of deciding storage location in the service method
- ✅ 5.8 Keep returned content shape stable for existing clients

### Stage 6 — Vault-backed writes

- ✅ 6.1 Route vault-backed `EntityApi.update_entity_content` through a vault writer coordinator
- ✅ 6.2 Validate access and metadata changes before accepting Markdown/sidecar/frontmatter-derived state
- ✅ 6.3 Write with temp-file plus atomic rename inside the vault
- ✅ 6.4 Detect stale editor state with content hash checks before overwriting external edits
- ✅ 6.5 After successful file write, store a fresh `content_blocks` snapshot and update `entities.content_block_id`
- ✅ 6.6 Enqueue embedding refresh keyed to the new `content_block_id`

### Stage 7 — External edits

- ✅ 7.1 Apply scanner-detected body edits to the entity snapshot by creating a new `content_blocks` record
- ✅ 7.2 Sync user-editable metadata such as title and tags through coordinator validation
- ✅ 7.3 Treat privacy changes from sidecar/frontmatter as access-policy requests, not direct writes
- ✅ 7.4 Keep identity field edits in conflict state until explicitly resolved when frontmatter identity mode is enabled
- ✅ 7.5 Rebuild `entity_assets` from parsed Markdown references after file changes

### Stage 8 — Polling watcher integration

- ✅ 8.1 Add polling filesystem watcher that only queues changed paths for scanner reconciliation
- ✅ 8.2 Debounce rapid editor and sync-tool write bursts before reconciling queued paths
- ✅ 8.3 Treat rename-like changes as delete/add path hints, not authoritative moves
- ✅ 8.4 Trigger startup/full scan regardless of watcher availability
- ✅ 8.5 Surface watcher errors without disabling manual/full scans

### Stage 9 — Conflict resolution API and UI

- ✅ 9.1 Add API to list vault conflicts with path, reason, canonical entity, and observed entity ID
- ✅ 9.2 Add "restore original ID" action for frontmatter-identity vaults whose ID was removed or changed
- ✅ 9.3 Add "fork as new document" action for copied files with duplicate IDs
- ✅ 9.4 Add "accept new path" action for copy/delete moves when the old canonical file is missing
- ✅ 9.5 Add sidecar-backed "ignore/unmanage file" action for files that should stay outside Noema control
- ✅ 9.6 Add "bind file to existing entity" action for explicit recovery flows

### Stage 10 — Broader hardening

- ✅ 10.1 Centralize access policy before syncing privacy-sensitive frontmatter
- ✅ 10.2 Centralize lifecycle and GC before deleting vault-backed documents
- ✅ 10.3 Add relation registry invariants before directory-tree vault UX depends on `structure::contained_in`
- ✅ 10.4 Batch entity summary queries before rendering large vaults
- ✅ 10.5 Move SQLite toward WAL and separate read/write access before watcher-driven background work grows

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

UCM Stage 3 (EntityApi) ──► Vault Stage 5 (content resolver)
Vault Stage 1 ──► Vault Stage 2 ──► Vault Stage 3 ──► Vault Stage 4
Vault Stage 4 ──► Vault Stage 5 ──► Vault Stage 6 ──► Vault Stage 7
Vault Stage 7 ──► Vault Stage 8 ──► Vault Stage 9
Vault Stage 10 tracks shared hardening and can be pulled forward as dependencies require
```
