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

**Goal:** Lumina chats with an LLM through the daemon. Channel-based: `/chat new` creates a private channel, bot responds to all messages in "AI Chats" category or @mentions.

| # | | Task | Priority | Size |
|---|---|------|----------|------|
| 2.1 | ✅ | `/chat new [name]` — create private text channel under "AI Chats" category with permissions | P0 | M |
| 2.2 | ✅ | `on_message` listener: respond in AI Chats category channels and @mentions | P0 | M |
| 2.3 | ✅ | Load Discord channel history as conversation context (last N messages → daemon session) | P0 | M |
| 2.4 | ✅ | Open daemon session, send message, stream response back with debounced edits | P0 | M |
| 2.5 | 🚫 | System prompt from UCM: load by named doc (e.g. `lumina/system-prompt`), depends on Content Stage 2 | P0 | M |
| 2.6 | ✅ | `/chat pause`, `/chat resume`, `/chat model`, `/chat history_limit` | P0 | S |
| 2.7 | ✅ | Tool call display as Discord embeds | P1 | S |

---

## Stage 2.5 — Core Architecture Refactor (COMPLETE)

**Goal:** Simplify the session/agent/manager architecture. One manager (SessionManager), one agent (ToolAgent), trait-based tool service, clean separation between sessions and conversations.

| # | | Task | Priority | Size |
|---|---|------|----------|------|
| 2.5.1 | ✅ | ToolService trait + EmptyToolService, McpToolRegistry implements it | P0 | M |
| 2.5.2 | ✅ | ToolAgent: single agent using ToolService, streams tool calls/results | P0 | M |
| 2.5.3 | ✅ | Agent trait: single execute_stream method, remove non-streaming execute | P0 | S |
| 2.5.4 | ✅ | Delete ConversationManager, LightManager, LightSession — one SessionManager | P0 | L |
| 2.5.5 | ✅ | SessionManager: Persistence enum, create() factory, no StorageHook | P0 | M |
| 2.5.6 | ✅ | Session commit via ConversationContext trait, coordinator.append_message | P0 | M |
| 2.5.7 | ✅ | Remove ToolConfig, ToolFilter, ToolEnricher, CommitMode, ForkInfoResponse | P0 | S |
| 2.5.8 | ✅ | Daemon: shared sessions (same conversation reuses session), session reaper | P0 | M |
| 2.5.9 | ✅ | API: remove resume_session/set_persistence/seed_context/reload, move get_messages to ConversationApi | P0 | M |
| 2.5.10 | ✅ | Noema: conversation_id for loading, session_id for streaming | P0 | M |
| 2.5.11 | ✅ | Restructure: agents/ → agent/, move context/in_memory_context/tool_service into agent/ | P0 | S |

---

## Stage 3 — Discord MCP Service

**Goal:** Lumina registers as an MCP service with the daemon, exposing Discord actions as tools. Noema (or any daemon client) can use Discord tools through the daemon.

| # | | Task | Priority | Size |
|---|---|------|----------|------|
| 3.1 | ✅ | Lumina registers as MCP service with daemon on connect | P0 | M |
| 3.2 | ✅ | Discord MCP tools: send_message, reply_to_message, create_embed, get_channel_history, search_messages, list_channels, list_guilds, get_emoji_list, get_voice_states, get_message_stats, get_channel_peak_hours, get_trending_content, get_active_threads, get_user_activity, create_poll (via rmcp #[tool] macros + schemars) | P0 | L |
| 3.3 | ✅ | Populate MCP server instructions with guild/channel map from gateway cache | P0 | S |
| 3.4 | ✅ | `/tool call` + `/tool list` slash commands: modal form from schema, paginated tool listing, multimodal result rendering | P0 | M |
| 3.5 | ⬜ | Verify: Noema agent can send Discord messages through daemon → Lumina | P0 | S |

> **Phase paused** — switching to Voice phase. Stage 3 functional, 3.5 verify pending manual test (see [v1.0/TODO.md](../../TODO.md)).

---

## ~~Stage 4 — Admin & Access Control~~ (PUNTED)

**Deferred** — not needed for current workflow. Revisit when multi-user access is required.

---

## ~~Stage 5 — Schedule~~ (DEFERRED → Events phase)

**Deferred** — schedule system belongs in the Events phase where it can build on the intents/triggers architecture.

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

## Stage 7 — Document Upload from Discord

**Goal:** Upload documents to UCM storage from Discord. RAG/search is a UCM-level concern, not Lumina-specific.

| # | | Task | Priority | Size |
|---|---|------|----------|------|
| 7.1 | ⬜ | `/doc upload` — upload attachment or paste markdown into UCM storage | P1 | M |
| 7.2 | ⬜ | `/doc list`, `/doc view` — browse documents from Discord | P1 | S |

---

## Stage 8 — Brain & Analytics

**Goal:** Port AI task analytics and model management.

| # | | Task | Priority | Size |
|---|---|------|----------|------|
| 8.1 | ⬜ | Port BrainCog: `/brain last`, `tasks`, `active`, `task` | P2 | M |
| 8.2 | ⬜ | `/brain model` — view/change LLM model (with autocomplete) | P1 | S |

---

## Stage 9 — Context (Memory Garden)

**Goal:** Port context/mental state management system.

| # | | Task | Priority | Size |
|---|---|------|----------|------|
| 9.1 | ⬜ | Port ContextCog: `/context save`, `reload`, `list`, `show`, `delete` | P2 | M |
| 9.2 | ⬜ | `on_message` listener for context switch detection | P2 | S |

---

## Stage 10 — Google Integrations (Sidecar MCP Server)

**Goal:** Separate sidecar MCP server exposing Google service integrations, with authentication handled through MCP.

| # | | Task | Priority | Size |
|---|---|------|----------|------|
| 10.1 | ⬜ | Scaffold sidecar MCP server crate with Google OAuth via MCP auth | P2 | M |
| 10.2 | ⬜ | Drive tools: `drive_list`, `drive_show`, `drive_read` | P2 | M |
| 10.3 | ⬜ | Calendar tools: `calendar_list` | P2 | S |

---

## Punted

**Goal:** Deferred — these will be handled as UCM content conventions (Content Stage 3) rather than standalone Lumina stages.

| # | | Task | Priority | Size |
|---|---|------|----------|------|
| — | ⏸️ | Port TodoCog: `/todo add`, `list`, `done`, `undo`, `label`, `unlabel`, `delete`, `info` | P2 | M |
| — | ⏸️ | Port NoteCog: `/note take`, `list`, `view`, `edit`, `delete`, `tag`, `untag`, `search`, `quick` | P2 | M |
