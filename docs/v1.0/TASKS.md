# v1.0 — Next Phase Tasks

**Roadmap:** [ROADMAP.md](ROADMAP.md)

Four parallel workstreams. Tasks within each are roughly sequential.

---

## 1. Content & RAG

UCM storage stays in the daemon (decided — see [UCM_SERVICE.md](../designs/proposals/UCM_SERVICE.md)). RAG is core to agent quality, not an add-on.

### Stage 1 — Document Type + Foundation

- 🔄 1.1 `DocumentType` constants module in `simply-core`
- ⬜ 1.2 `document_type` column on documents table + migration
- ⬜ 1.3 `document_type` in `DocumentInfo`/`DocumentDetail`/`CreateDocumentRequest` + type-filtered `list_documents`

### Stage 2 — Embedding Providers + Traits

**Design:** [EMBEDDING_AND_RAG.md](../designs/EMBEDDING_AND_RAG.md)

- ⬜ 2.1 `EmbeddingProvider` trait + `Embedding` struct in `simply-core`
- ⬜ 2.2 `Chunker` trait + `RecursiveCharacterChunker` impl in `simply-core`
- ⬜ 2.3 `VectorStore` trait + types (`VectorChunk`, `SearchQuery`, `SearchResult`, `SearchFilter`) in `simply-core`
- ⬜ 2.4 Embedding config in `settings.toml` (`[embedding]` section: provider, model, chunk_size, chunk_overlap)
- ⬜ 2.5 Mistral embedding provider (`mistral-embed`)
- ⬜ 2.6 OpenAI, Gemini, Claude/Voyage, Ollama embedding providers

### Stage 3 — Storage + Indexing

- ⬜ 3.1 sqlite-vec `VectorStore` implementation (chunks table + vec virtual table)
- ⬜ 3.2 Embedding queue — background worker, debounce, retry, startup scan for stale/missing
- ⬜ 3.3 Hook document tab writes to enqueue embedding jobs

### Stage 4 — Retrieval API

- ⬜ 4.1 `SearchApi` trait (`search` + `reindex` endpoints)
- ⬜ 4.2 `SearchService` implementation — embed query, vector search, return hits with doc metadata

### Stage 5 — Client Integration

- ⬜ 5.1 Lumina auto-RAG — query from last N messages, inject relevant chunks into system prompt
- ⬜ 5.2 Local ONNX embedding provider (`bge-small-en-v1.5` via ort) — optional, for no-network setups
- ⬜ 5.3 Noema search panel + document refs (deferred — depends on UI work)

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

## 3. Web Extension & RTC ⏸️

**Status:** Future — deprioritized in favor of Content, Events, and Multi-user.

A Chrome extension (`simply-web`) as a new daemon client, like Noema and Lumina. Connects to the daemon via WS+REST, provides browser-context capabilities.

**Why a Chrome extension (not a headless bot):**
- Google Meet has no production API for bots joining calls (Media API is developer preview only, not GA)
- The entire meeting bot industry uses headless browser automation (Puppeteer + Docker + virtual audio) — extremely fragile, high maintenance
- A Chrome extension avoids all of that: runs in the user's own browser, uses `chrome.tabCapture` or DOM scraping, no infrastructure
- Platform-agnostic: works with Meet, Zoom web, Teams web, any browser-based call

**Approach for meeting transcription:**
- Scrape Google Meet's live captions from the DOM (speaker-diarized, Google does the STT) rather than capturing mixed audio and doing our own STT
- Mixed-audio STT from `tabCapture` is a hard problem; caption scraping sidesteps it entirely

### Stage 1 — Extension Shell

- ⬜ 1.1 Chrome extension (Manifest V3) connecting to daemon via WS+REST
- ⬜ 1.2 Sidebar UI: chat with daemon agent
- ⬜ 1.3 MCP tools: page context (URL, selected text, page content)
- ⬜ 1.4 Content capture: send selected text/images to daemon for storage

### Stage 2 — Meet Caption Capture

- ⬜ 2.1 Detect Google Meet tab
- ⬜ 2.2 Scrape live captions from Meet DOM (speaker name + text)
- ⬜ 2.3 Stream caption events to daemon
- ⬜ 2.4 Store transcripts as documents in UCM
- ⬜ 2.5 Push `meet.transcript` events into the event bus

### Stage 3 — Audio Streaming (future)

- ⬜ 3.1 `chrome.tabCapture` audio capture -> common format -> daemon voice API
- ⬜ 3.2 TTS playback into tab (Web Audio API)
- ⬜ 3.3 Or: Google Meet Media API integration when it goes GA (per-participant audio, no extension needed)

---

## 4. Multi-user & OAuth

**Design:** [AUTH_AND_IDENTITY.md](../designs/AUTH_AND_IDENTITY.md)

### Stage 1 — Connection Auth & User Identity

- ✅ 1.1 Auto-generate `daemon_secret` on first run, store in `settings.toml`
- ✅ 1.2 Auth middleware: validate `Authorization: Bearer {daemon_secret}` on all routes except `/auth/*`
- ✅ 1.3 `X-User-Id` header support — daemon resolves to UCM user, scopes operations
- ✅ 1.4 Noema sends admin user_id on all requests
- ✅ 1.5 Lumina sends Discord-mapped user_id (or omits for anonymous)
- ✅ 1.6 User tiers: admin (full access), authenticated (own data), anonymous (public only)
- ✅ 1.7 Document ownership — documents scoped to creating user, ownership checks on mutate

### Stage 2 — Single-Port OAuth & Admin Page

- ✅ 2.1 Merge OAuth callback server into main port (`/auth/callback` route)
- ✅ 2.2 Admin page Google OAuth login (`/auth/login` → Google → verify `admin_email`)
- ✅ 2.3 User self-service auth page (`/auth/login` → Google → create/link UCM user)
- ✅ 2.4 Discord user mapping table: `discord_user_id → ucm_user_id`
- ✅ 2.5 Lumina `/auth` command — generates link to daemon auth page, maps Discord user after OAuth

### Stage 3 — Per-User MCP OAuth

- ⬜ 3.1 Per-user per-MCP-server token storage: `(user_id, server_id) → tokens`
- ⬜ 3.2 MCP OAuth initiation: `/auth/mcp/{server_id}?user_id={user_id}` → provider OAuth → store tokens
- ⬜ 3.3 Token injection: daemon adds user's token to MCP requests as `Authorization` header
- ⬜ 3.4 `auth_required` error response when user has no token for a service
- ⬜ 3.5 Automatic token refresh using stored refresh tokens
- ⬜ 3.6 Token revocation (admin or self-service)

### Stage 4 — Discord Role-Based Access Control

- ⬜ 4.1 `[mcp_access]` config in `lumina.toml` — map Discord roles to MCP server access
- ⬜ 4.2 Lumina checks user's Discord roles before MCP tool calls
- ⬜ 4.3 Graceful denial: "You need the `developers` role to use GitHub tools"
- ⬜ 4.4 Tool call approval flow (see [TOOL_APPROVAL.md](../designs/proposals/TOOL_APPROVAL.md))

### Stage 5 — Admin UI

- ⬜ 5.1 Admin page protected by Google OAuth (`admin_email` in config)
- ⬜ 5.2 User management: list users, view linked accounts, revoke access
- ⬜ 5.3 MCP service management: connect/disconnect, view tools, per-service OAuth status
- ⬜ 5.4 Connection browser: view connected clients, active sessions
- ⬜ 5.5 Settings page: API keys, voice providers, model config

---

## Dependencies

```
Content Stage 1 (DocumentApi) ──► Events Stage 1 (intent documents in UCM)
Content Stage 2 (Embeddings) ──► Content Stage 3 (RAG)
Events Stage 2 (Service Registry) ──► Events Stage 3 + 4 (parallel)
Events Stage 3 + 4 ──► Events Stage 5 (Conditions + Workflow)
Multi-user Stage 1 (Connection Auth) ──► Multi-user Stage 2 (OAuth)
Multi-user Stage 2 (OAuth) ──► Multi-user Stage 3 (Per-User MCP OAuth)
Multi-user Stage 3 ──► Multi-user Stage 4 (Role-Based Access)
Multi-user Stage 4 ──► Multi-user Stage 5 (Admin UI)
Web Extension ──► blocked on nothing, but deprioritized
```

Recommended start order:
1. **Content Stage 1** (DocumentApi) — unblocks Events and RAG
2. **Multi-user Stage 1** (Identity) — unblocks OAuth
3. **Events Stage 1** (Event bus) — can start once Content Stage 1 lands
4. **Content Stage 2-3** (Embeddings + RAG) — independent of Events/Auth
5. **Multi-user Stage 2-3** (OAuth + Permissions) — independent of Content/Events
6. **Admin UI** — last, once the APIs it surfaces are built
7. **Web Extension & RTC** — when the active workstreams are done, or Meet Media API goes GA
