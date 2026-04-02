# Lumina — Discord Bot

**Parent:** [v1.0 Roadmap](../../ROADMAP.md)
**Priority:** P0
**Complexity:** L
**Depends on:** Foundation complete

---

## Goal

Port the full Python Lumina Discord bot to Rust. Lumina connects to simply-daemon via WebSocket, delegates all agent/LLM work to the daemon, and provides Discord slash commands for chat, voice, scheduling, notes, todos, knowledge base, and server management.

---

## Stages

### Stage 1 — Lumina Crate (COMPLETE)

**Goal:** Minimal Lumina bot exists in the workspace, connects to Discord and simply-daemon.

**Status:** Complete

---

### Stage 2 — LLM Chat

**Goal:** Lumina chats with an LLM by sending messages to simply-daemon, which runs the agent.

**Complexity:** M

**Tasks:**
- [ ] On `/chat`, Lumina opens a session (ephemeral by default) with simply-daemon
- [ ] System prompt from UCM: configurable document ID loaded as system prompt (content-based, not hardcoded)
- [ ] Seeds context from recent Discord channel messages (rolling window of last N)
- [ ] Sends user message, receives streamed agent response, posts to Discord
- [ ] Port ChatCog: `/chat new`, `/chat pause`, `/chat resume`, `/chat model`
- [ ] `on_message` listener for bot chat channels and @mentions
- [ ] Response formatting as Discord embeds

**Verify:**
- Lumina: `/chat hello` → LLM response appears in Discord.
- Noema: Chat still works through simply-daemon.

---

### Stage 3 — Discord MCP Service

**Goal:** Lumina registers as an MCP service with the daemon, exposing Discord actions as tools. Any daemon client (Noema, other bots) can use Discord through the daemon.

**Complexity:** M

**Tasks:**
- [ ] Lumina registers as MCP service with daemon on connect
- [ ] Discord MCP tools: send message, read channel history, react, list channels
- [ ] Verify: Noema agent can send Discord messages through daemon → Lumina

**Verify:** From Noema, ask the agent to "post a message in #general" and it routes through daemon → Lumina MCP tools → Discord.

---

### Stage 4 — Admin & Access Control

**Goal:** Port admin commands and role-based access control.

**Complexity:** S

**Tasks:**
- [ ] Port AdminCog: `/admin set-access`, `/admin list-access`
- [ ] Access level system (Full Access, Chat Only, No Access)
- [ ] Permission checks in command handlers

**Verify:** Admin can manage role access levels via slash commands.

---

### Stage 5 — Schedule

**Goal:** Port scheduled prompt execution system.

**Complexity:** M

**Tasks:**
- [ ] Port ScheduleCog: `/schedule create` (interval, daily, weekly, monthly, cron, one_time)
- [ ] `/schedule edit`, `/schedule list`, `/schedule delete`, `/schedule test`
- [ ] Schedule service startup on bot ready

**Verify:** Scheduled prompts execute on configured intervals.

---

### Stage 6 — Voice

**Goal:** Port voice channel features (STT → LLM → TTS pipeline).

**Complexity:** L

**Tasks:**
- [ ] Port VoiceCog: `/voice join`, `/voice leave`
- [ ] `/voice converse` — realtime voice conversation (STT → LLM → TTS)
- [ ] `/voice transcribe` — voice to text
- [ ] `/voice say` — TTS playback
- [ ] `/voice voices`, `/voice tts_providers`, `/voice set_tts` — TTS management

**Verify:** Bot joins voice channel, transcribes speech, responds via TTS.

---

### Stage 7 — Document Upload from Discord

**Goal:** Upload documents to UCM storage from Discord. RAG/search is a UCM-level concern — all content in storage is automatically indexed and searchable. Lumina just needs a way to get documents in.

**Complexity:** S

**Tasks:**
- [ ] `/doc upload` — upload attachment or paste markdown into UCM storage
- [ ] `/doc list`, `/doc view` — browse documents from Discord

**Verify:** Upload a file via Discord, find it via Noema or semantic search.

---

### Stage 8 — Brain & Analytics

**Goal:** Port AI task analytics and model management.

**Complexity:** S

**Tasks:**
- [ ] Port BrainCog: `/brain last`, `/brain tasks`, `/brain active`, `/brain task`
- [ ] `/brain model` — view/change LLM model (with autocomplete)

**Verify:** Users can view tool usage history and switch models.

---

### Stage 9 — MCP Server Management

**Goal:** Port MCP server discovery and management commands.

**Complexity:** M

**Tasks:**
- [ ] Port MCPServerCog: `/mcp list`, `/mcp info`, `/mcp toggle`, `/mcp delete`
- [ ] `/mcp tools` — list available tools
- [ ] `/mcp call`, `/mcp call_raw` — invoke tools
- [ ] `/mcp add` — interactive server registration

**Verify:** MCP servers can be managed and tools invoked from Discord.

---

### Stage 10 — Context (Memory Garden)

**Goal:** Port context/mental state management system.

**Complexity:** M

**Tasks:**
- [ ] Port ContextCog: `/context save`, `/context reload`, `/context list`, `/context show`, `/context delete`
- [ ] `on_message` listener for context switch detection

**Verify:** Users can save and restore work contexts.

---

### Stage 11 — Google Integrations

**Goal:** Port Google service integrations (conditional on auth).

**Complexity:** M

**Tasks:**
- [ ] Port AuthCog / GoogleAuthCog: `/authenticate`, `/authenticate_google`
- [ ] Port DriveCog: `/drive list`, `/drive show`, `/drive read`
- [ ] Port CalendarCog: `/calendar list`

**Verify:** Authenticated users can access Drive files and Calendar events.

---

### Stage 12 — Server Management & Utilities

**Goal:** Port remaining server and utility commands.

**Complexity:** S

**Tasks:**
- [ ] Port ServerCog: `/welcome set_channel`, `/welcome toggle`, `/welcome set_template`, `/welcome test`
- [ ] `/serverinfo` — server information display
- [ ] `on_member_join` listener for welcome messages
- [ ] Port UtilityCog: `/export_messages`
- [ ] Rich Discord embeds and interactive components (buttons, select menus, autocomplete)

**Verify:** Welcome messages fire on join, server info displays correctly.

---

## Punted

**Todo & Notes** — deferred to Content Stage 3 (UCM content conventions). These are thin wrappers over document CRUD with frontmatter, not standalone Lumina features.

---

## Dependencies

```
Stage 1 → Stage 2 → Stage 3 (sequential: crate → chat → MCP service)
Stage 3 → Stages 4-12 (parallel after MCP service works)
Stage 6 depends on Voice phase (simply-voice crate)
Stage 7 depends on Content phase (document storage)
Todo/Notes punted to Content Stage 3
```
