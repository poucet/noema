# v1.0 — Next Phase Tasks

**Roadmap:** [ROADMAP.md](ROADMAP.md)

Three workstreams. Tasks within each are roughly sequential.

---

## 1. Events & Intents

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

## 2. Web Extension & RTC (paused)

**Status:** Future — deprioritized in favor of Events and Multi-user.

A Chrome extension (`simply-web`) as a new daemon client, like Lumina. Connects to the daemon via WS+REST.

**Why a Chrome extension (not a headless bot):**
- Google Meet has no production API for bots joining calls
- A Chrome extension runs in the user's own browser, uses `chrome.tabCapture` or DOM scraping
- Platform-agnostic: works with Meet, Zoom web, Teams web

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

- ⬜ 3.1 `chrome.tabCapture` audio capture -> daemon voice API
- ⬜ 3.2 TTS playback into tab (Web Audio API)
- ⬜ 3.3 Or: Google Meet Media API integration when it goes GA

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
Events Stage 2 (Service Registry) ──► Events Stage 3 + 4 (parallel)
Events Stage 3 + 4 ──► Events Stage 5 (Conditions + Workflow)
Multi-user Stage 3 (Per-User MCP OAuth) ──► Multi-user Stage 4 (Role-Based Access)
Multi-user Stage 4 ──► Multi-user Stage 5 (Admin UI)
Web Extension ──► blocked on nothing, but deprioritized
```
