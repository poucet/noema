# Lumina — Tasks

**Phase:** Lumina (Discord Bot)
**Status:** In Progress
**Roadmap:** [ROADMAP.md](ROADMAP.md)
**Depends on:** Foundation (complete)

---

## Stage 1 — Lumina Crate (COMPLETE)

**Goal:** Minimal Lumina bot in the workspace, connects to Discord and simply-daemon.

| # | | Task | Priority | Size |
|---|---|------|----------|------|
| 1.1 | ✅ | Add `lumina/` crate to workspace | P0 | S |
| 1.2 | ✅ | Basic main.rs: serenity bot, connect to Discord gateway | P0 | S |
| 1.3 | ✅ | Connect to simply-daemon via RemoteDaemon (WS client) | P0 | S |
| 1.4 | ✅ | Slash commands: `/ping`, `/chat` (echo) | P0 | S |
| 1.5 | ✅ | Config: Discord bot token from env/config | P0 | S |

---

## Stage 2 — LLM Chat

**Goal:** Lumina chats with an LLM through the daemon.

| # | | Task | Priority | Size |
|---|---|------|----------|------|
| 2.1 | ⬜ | On `/chat`, open ephemeral session with daemon | P0 | M |
| 2.2 | ⬜ | System prompt from UCM: load by named doc (e.g. `lumina/system-prompt`), depends on Content Stage 2 named docs | P0 | M |
| 2.3 | ⬜ | Seed context from recent Discord channel messages | P0 | M |
| 2.4 | ⬜ | Send user message, receive streamed response, post to Discord | P0 | M |
| 2.5 | ⬜ | Port ChatCog: `/chat new`, `/chat pause`, `/chat resume`, `/chat model` | P0 | M |
| 2.6 | ⬜ | `on_message` listener for bot chat channels and @mentions | P0 | M |
| 2.7 | ⬜ | Response formatting as Discord embeds | P0 | S |
| 2.8 | ⬜ | Register Discord MCP tools for agent interaction | P1 | M |

---

## Stage 3 — Admin & Access Control

**Goal:** Port admin commands and role-based access control.

| # | | Task | Priority | Size |
|---|---|------|----------|------|
| 3.1 | ⬜ | Port AdminCog: `/admin set-access`, `/admin list-access` | P1 | S |
| 3.2 | ⬜ | Access level system (Full Access, Chat Only, No Access) | P1 | S |
| 3.3 | ⬜ | Permission checks in command handlers | P1 | S |

---

## Stage 4 — Todo & Notes

**Goal:** Port task and note management commands.

| # | | Task | Priority | Size |
|---|---|------|----------|------|
| 4.1 | ⬜ | Port TodoCog: `/todo add`, `list`, `done`, `undo`, `label`, `unlabel`, `delete`, `info` | P1 | M |
| 4.2 | ⬜ | Port NoteCog: `/note take`, `list`, `view`, `edit`, `delete`, `tag`, `untag`, `search`, `quick` | P1 | M |

---

## Stage 5 — Schedule

**Goal:** Port scheduled prompt execution system.

| # | | Task | Priority | Size |
|---|---|------|----------|------|
| 5.1 | ⬜ | Port ScheduleCog: `/schedule create` (interval, daily, weekly, monthly, cron, one_time) | P1 | M |
| 5.2 | ⬜ | `/schedule edit`, `list`, `delete`, `test` | P1 | S |
| 5.3 | ⬜ | Schedule service startup on bot ready | P1 | S |

---

## Stage 6 — Voice

**Goal:** Port voice channel features (STT → LLM → TTS pipeline).

| # | | Task | Priority | Size |
|---|---|------|----------|------|
| 6.1 | ⬜ | Port VoiceCog: `/voice join`, `/voice leave` | P0 | M |
| 6.2 | ⬜ | `/voice converse` — realtime STT → LLM → TTS | P0 | L |
| 6.3 | ⬜ | `/voice transcribe` — voice to text | P0 | M |
| 6.4 | ⬜ | `/voice say` — TTS playback | P1 | S |
| 6.5 | ⬜ | `/voice voices`, `tts_providers`, `set_tts` — TTS management | P2 | S |

---

## Stage 7 — Knowledge Base (RAG)

**Goal:** Port RAG document management and semantic search.

| # | | Task | Priority | Size |
|---|---|------|----------|------|
| 7.1 | ⬜ | Port RAGCog: `/rag add_attachment`, `/rag add_document` | P1 | M |
| 7.2 | ⬜ | `/rag list`, `remove`, `update`, `sync`, `sync_all` | P1 | M |
| 7.3 | ⬜ | `/rag query` — semantic search | P1 | M |
| 7.4 | ⬜ | `/rag chat` — RAG-enhanced chat session | P1 | M |

---

## Stage 8 — Brain & Analytics

**Goal:** Port AI task analytics and model management.

| # | | Task | Priority | Size |
|---|---|------|----------|------|
| 8.1 | ⬜ | Port BrainCog: `/brain last`, `tasks`, `active`, `task` | P2 | M |
| 8.2 | ⬜ | `/brain model` — view/change LLM model (with autocomplete) | P1 | S |

---

## Stage 9 — MCP Server Management

**Goal:** Port MCP server discovery and management commands.

| # | | Task | Priority | Size |
|---|---|------|----------|------|
| 9.1 | ⬜ | Port MCPServerCog: `/mcp list`, `info`, `toggle`, `delete` | P1 | M |
| 9.2 | ⬜ | `/mcp tools` — list available tools | P1 | S |
| 9.3 | ⬜ | `/mcp call`, `/mcp call_raw` — invoke tools | P1 | M |
| 9.4 | ⬜ | `/mcp add` — interactive server registration | P2 | M |

---

## Stage 10 — Context (Memory Garden)

**Goal:** Port context/mental state management system.

| # | | Task | Priority | Size |
|---|---|------|----------|------|
| 10.1 | ⬜ | Port ContextCog: `/context save`, `reload`, `list`, `show`, `delete` | P2 | M |
| 10.2 | ⬜ | `on_message` listener for context switch detection | P2 | S |

---

## Stage 11 — Google Integrations

**Goal:** Port Google service integrations (conditional on auth).

| # | | Task | Priority | Size |
|---|---|------|----------|------|
| 11.1 | ⬜ | Port AuthCog / GoogleAuthCog: `/authenticate`, `/authenticate_google` | P2 | M |
| 11.2 | ⬜ | Port DriveCog: `/drive list`, `show`, `read` | P2 | M |
| 11.3 | ⬜ | Port CalendarCog: `/calendar list` | P2 | S |

---

## Stage 12 — Server Management & Utilities

**Goal:** Port remaining server and utility commands.

| # | | Task | Priority | Size |
|---|---|------|----------|------|
| 12.1 | ⬜ | Port ServerCog: `/welcome` (set_channel, toggle, set_template, test) | P2 | M |
| 12.2 | ⬜ | `/serverinfo` — server information display | P2 | S |
| 12.3 | ⬜ | `on_member_join` listener for welcome messages | P2 | S |
| 12.4 | ⬜ | Port UtilityCog: `/export_messages` | P2 | S |
| 12.5 | ⬜ | Rich Discord embeds and interactive components | P2 | M |
