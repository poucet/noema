# v1.0 Feature Inventory

**Design:** [GOAL.md](GOAL.md)
**Roadmap:** [ROADMAP.md](ROADMAP.md)

Features grouped by where they land in the Rust workspace. Ported from Python Lumina (`~/projects/simply/lumina`) unless noted otherwise.

---

## → simply-core (shared services, available to all platforms)

### Core infrastructure (dedicated services)

| Feature | Python Source | Priority | Phase | Notes |
|---------|-------------|----------|-------|-------|
| **Agent orchestration** | `agent/nous_agent.py`, `agent/task_manager.py` | P0 | Foundation | ✅ ToolAgent, SessionManager, spawn_agent |
| **LLM providers** | nous library | P0 | Foundation | ✅ `simply-core/llm` — Claude, OpenAI, Gemini, Ollama |
| **MCP server/client** | `mcp_protocol/server/`, `mcp_protocol/handlers/` | P0 | Foundation | ✅ McpRegistry, ephemeral registration, rmcp |
| **Voice pipeline** | `services/discord/cogs/voice_cog.py` (VAD, STT, TTS) | P0 | Voice | In progress — simply-voice crate |
| **Document CRUD** | multiple databases | P0 | Content | Not started — DocumentApi, frontmatter queries |
| **Identity** | `services/identity/` | P1 | Content | Not started — cross-platform user identity |
| **Event & Intent system** | `services/scheduler/` | P1 | Events | Not started — event bus + intent engine |
| **Search / RAG** | `services/rag/` | P2 | post-v1 | Embeddings over all UCM content, unified search |
| **Brain / Analytics** | `services/brain/` | P2 | post-v1 | Aggregation queries over turn data |

### Content conventions (no dedicated service — just UCM documents with frontmatter)

| Feature | Python Source | Priority | Phase | Notes |
|---------|-------------|----------|-------|-------|
| **TODOs** | `services/database/todo_database.py` | P1 | Content | `type: todo` documents, queried via generic document service |
| **Notes** | `services/database/note_database.py` | P1 | Content | `type: note` documents |
| **Context / Memory** | `services/context/` | P2 | Content | `type: context` documents |
| **Access control** | `services/access_control/` | P1 | Lumina | Punted — not needed for current workflow |
| **MCP server config** | `services/mcp/` | P2 | Content | `type: mcp_server` documents |

---

## → lumina crate (Discord-specific presentation)

| Feature | Python Source | Priority | Phase | Notes |
|---------|-------------|----------|-------|-------|
| **Discord gateway + bot** | `__main__.py`, discord.py bot | P0 | Lumina | ✅ serenity-based, connects via RemoteDaemon |
| **Chat commands** | `cogs/chat_cog.py` | P0 | Lumina | ✅ Channel management, streaming, model selection |
| **MCP Discord tools** | `handlers/discord_handler.py` | P0 | Lumina | ✅ 15 tools via rmcp #[tool], ephemeral registration |
| **Tool invocation UI** | — | P0 | Lumina | ✅ `/tool call` (modal) + `/tool list` (paginated) |
| **Voice I/O** | `cogs/voice_cog.py` | P0 | Voice | Not started — songbird + DAVE |
| **Admin commands** | `cogs/admin_cog.py` | P1 | — | Punted |
| **Server management** | `cogs/server_cog.py` | P2 | — | Punted |
| **Command sync** | `cogs/sync_cog.py` | P2 | Lumina | ✅ `.sync` owner command |
| **Message export** | `cogs/util_cog.py` | P3 | — | Deferred |

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
