# v1.0 Feature Inventory

**Design:** [GOAL.md](GOAL.md)
**Roadmap:** [ROADMAP.md](ROADMAP.md)

Features grouped by where they land in the Rust workspace. Ported from Python Lumina (`~/projects/simply/lumina`) unless noted otherwise.

---

## → simply-core (shared services, available to all platforms)

### Core infrastructure (dedicated services)

| Feature | Python Source | Priority | Phase | Notes |
|---------|-------------|----------|-------|-------|
| **Agent orchestration** | `agent/nous_agent.py`, `agent/task_manager.py` | P0 | 0 | Core agent loop, model selection, task tracking |
| **LLM providers** | nous library | P0 | 0 | Already exists as `noema-core/llm` → becomes `simply-llm` |
| **MCP server/client** | `mcp_protocol/server/`, `mcp_protocol/handlers/` | P0 | 1A | MCP tool hosting + external server connections |
| **Voice pipeline** | `services/discord/cogs/voice_cog.py` (VAD, STT, TTS) | P0 | 2 | Core motivation for rewrite. Voxtral first. |
| **Document CRUD** | multiple databases | P0 | 1A | Generic UCM document ops with frontmatter-aware queries |
| **Identity** | `services/identity/` | P1 | 1A | Cross-platform user identity, entity relations |
| **Event & Intent system** | `services/scheduler/` | P1 | 1B | Event bus + intent engine — replaces schedules |
| **Search / RAG** | `services/rag/` | P2 | post-v1 | Embeddings over all UCM content, unified search |
| **Brain / Analytics** | `services/brain/` | P2 | post-v1 | Aggregation queries over turn data |

### Content conventions (no dedicated service — just UCM documents with frontmatter)

| Feature | Python Source | Priority | Phase | Notes |
|---------|-------------|----------|-------|-------|
| **TODOs** | `services/database/todo_database.py` | P1 | 1A | `type: todo` documents, queried via generic document service |
| **Notes** | `services/database/note_database.py` | P1 | 1A | `type: note` documents |
| **Context / Memory** | `services/context/` | P2 | 1A | `type: context` documents |
| **Access control** | `services/access_control/` | P1 | 3 | `type: access_rule` documents |
| **MCP server config** | `services/mcp/` | P2 | 1A | `type: mcp_server` documents |

---

## → lumina crate (Discord-specific presentation)

| Feature | Python Source | Priority | Phase | Notes |
|---------|-------------|----------|-------|-------|
| **Discord gateway + bot** | `__main__.py`, discord.py bot | P0 | 0 | serenity-based, replaces discord.py |
| **Chat commands** | `cogs/chat_cog.py` | P0 | 0 | Channel management, message handling, model selection |
| **Voice I/O** | `cogs/voice_cog.py` | P0 | 2 | songbird backend, DAVE support, audio bridge to core |
| **Slash commands** | All cogs | P1 | 1A, 3 | serenity `#[command]` for each feature |
| **Discord embeds/UI** | `handlers/discord_handler.py`, cogs | P1 | 3 | Rich embeds, polls, buttons |
| **Admin commands** | `cogs/admin_cog.py` | P1 | 3 | Access control management |
| **Server management** | `cogs/server_cog.py` | P2 | 3 | Welcome messages, member tracking |
| **Command sync** | `cogs/sync_cog.py` | P2 | 3 | Slash command registration |
| **Message export** | `cogs/util_cog.py` | P3 | 3 | Export chat history |

---

## → Deferred (not in v1, architecture supports later)

| Feature | Reason |
|---------|--------|
| **Google Auth/Drive/Calendar/Docs** | Complex OAuth flows, low priority for v1 |
| **Brave/Google Search** | Easy to add as MCP tool later |
| **Telegram/WhatsApp** | New presentation layer crates |
| **WebRTC / /meet** | New presentation layer crate (2.5 is experimental only) |
| **Filesystem handler** | Simple to add, low priority |
| **Note → Google Doc export** | Depends on Google integration |
| **Search / RAG** | Post-v1 (see [FUTURE_ROADMAP.md](../FUTURE_ROADMAP.md)) |
| **Brain / Analytics** | Post-v1 |
