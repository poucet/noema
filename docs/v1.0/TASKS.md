# v1.0 — Next Phase Tasks

**Roadmap:** [ROADMAP.md](ROADMAP.md)

Four parallel workstreams. Tasks within each are roughly sequential.

---

## 1. Content & RAG

UCM storage stays in the daemon (decided — see [UCM_SERVICE.md](../designs/proposals/UCM_SERVICE.md)). RAG is core to agent quality, not an add-on.

### Stage 1 — Document CRUD

- ⬜ 1.1 `DocumentApi` trait on daemon — extract doc CRUD from Noema-only Tauri commands to a shared RPC trait
- ⬜ 1.2 Named documents — entity slugs so docs are addressed by path (e.g. `lumina/system-prompt`) not raw UUIDs
- ⬜ 1.3 Frontmatter parsing + indexing in UCM storage layer
- ⬜ 1.4 Frontmatter-aware query syntax — filter by `type`, `tags`, `done`, `due`, etc.
- ⬜ 1.5 MCP tools for agents: `create_document`, `query_documents`, `update_document`, `delete_document`, `get_document`
- ⬜ 1.6 Cross-platform verify: create from Noema, query from Lumina and vice versa

### Stage 2 — Embedding Providers

- ⬜ 2.1 `EmbeddingProvider` trait in `simply-core` — `async fn embed(texts: &[&str]) -> Vec<Vec<f32>>`
- ⬜ 2.2 OpenAI embeddings provider (`text-embedding-3-small`)
- ⬜ 2.3 Local embeddings provider (e.g. `all-MiniLM-L6-v2` via candle or ONNX on Apple Silicon)
- ⬜ 2.4 Embedding provider config in `settings.toml` (model, endpoint, API key)
- ⬜ 2.5 Vector storage in UCM — store embeddings alongside documents in SQLite (or sqlite-vec extension)
- ⬜ 2.6 Auto-embed on document create/update — daemon indexes document content automatically

### Stage 3 — RAG Pipeline

- ⬜ 3.1 Semantic search: `search(query, top_k)` — embed query, nearest-neighbor lookup over stored vectors
- ⬜ 3.2 Hybrid search — combine semantic similarity with frontmatter filters (e.g. "notes about voice" = semantic + `type: note`)
- ⬜ 3.3 Context injection — agent session can auto-inject relevant documents into LLM context before generation
- ⬜ 3.4 MCP tool: `search(query, filters?)` exposed to agents for explicit retrieval
- ⬜ 3.5 Lumina integration: system prompt from UCM document (replaces hardcoded), RAG-backed answers
- ⬜ 3.6 Noema integration: search panel, document references in chat

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

### Stage 1 — User Identity

- ⬜ 1.1 `User` model in daemon — unique identity with linked platform accounts
- ⬜ 1.2 `UserApi` trait on daemon — CRUD for users, account linking
- ⬜ 1.3 Discord user -> daemon user mapping — Lumina resolves Discord user ID to daemon user on each request
- ⬜ 1.4 Session-level user context — sessions know which user they belong to

### Stage 2 — OAuth & Account Linking

- ⬜ 2.1 OAuth2 flow in daemon — authorization code grant, token storage, refresh
- ⬜ 2.2 Google account linking — user authenticates via OAuth, daemon stores tokens per user
- ⬜ 2.3 Per-user MCP credentials — when an MCP tool needs Google API access, daemon injects the requesting user's tokens
- ⬜ 2.4 Lumina OAuth trigger — Discord command (e.g. `/account link google`) opens browser OAuth flow, links result to Discord user's daemon identity
- ⬜ 2.5 Token management — refresh, revoke, re-auth prompt when tokens expire

### Stage 3 — Permission Model

- ⬜ 3.1 `Permission` model — define which MCP tools require which permission level
- ⬜ 3.2 Role-based access — users have roles, roles grant permissions (e.g. `admin` can use `delete_document`, `member` cannot)
- ⬜ 3.3 Discord role sync — map Discord server roles to daemon permission roles
- ⬜ 3.4 Generalized role source — role mapping is pluggable so non-Discord platforms can provide roles too
- ⬜ 3.5 Tool call approval flow — tools can require user confirmation before execution (see [TOOL_APPROVAL.md](../designs/proposals/TOOL_APPROVAL.md))
  - `ToolCallPending` event, `confirm_tool_call` API
  - Session-level `ToolApproval` policy: `AutoApprove`, `RequireAll`, `AllowList`
  - Lumina: approval embed with approve/reject buttons
  - Noema: modal confirmation in chat UI
  - Timeout auto-reject (5 min default)
- ⬜ 3.6 Permission checks at tool dispatch — daemon enforces before execution, not clients

### Stage 4 — Admin UI

- ⬜ 4.1 Auth on daemon REST API — login endpoint, session tokens, middleware
- ⬜ 4.2 Admin web UI shell — SPA served from daemon's REST port (e.g. `/admin`)
- ⬜ 4.3 User management page — list users, view linked accounts, assign roles
- ⬜ 4.4 MCP tool browser — list all registered tools, view schemas, set permission requirements
- ⬜ 4.5 Service registry page — view connected clients and MCP services, health status
- ⬜ 4.6 Conversation browser — view active sessions, conversation history
- ⬜ 4.7 Model + provider config — view/edit LLM providers, voice providers, embedding providers
- ⬜ 4.8 Settings page — daemon config editing (API keys, voice settings, etc.)
- ⬜ 4.9 Intent dashboard — view/create/edit intents, see execution history (depends on Events)

---

## Dependencies

```
Content Stage 1 (DocumentApi) ──► Events Stage 1 (intent documents in UCM)
Content Stage 2 (Embeddings) ──► Content Stage 3 (RAG)
Events Stage 2 (Service Registry) ──► Events Stage 3 + 4 (parallel)
Events Stage 3 + 4 ──► Events Stage 5 (Conditions + Workflow)
Multi-user Stage 1 (Identity) ──► Multi-user Stage 2 (OAuth)
Multi-user Stage 2 ──► Multi-user Stage 3 (Permissions)
Multi-user Stage 3 ──► Multi-user Stage 4 (Admin UI enforces permissions)
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
