# Design Index

| # | Title | Status | Phase |
|---|-------|--------|-------|
| 1 | [Simply Platform: Rust Unification](#design-1-simply-platform--rust-unification) | draft | 5 |

---

## Design 1: Simply Platform — Rust Unification

**Status**: draft

### Problem

Lumina (Python Discord bot) and Noema (Rust desktop AI assistant) are converging on the same needs — LLM orchestration, MCP tools, voice pipeline, storage — but implemented independently in different languages. This creates several problems:

1. **Duplicated core logic** — both projects implement LLM provider abstraction, MCP integration, agent orchestration, and voice processing separately.
2. **Python voice limitations** — Python's Discord ecosystem lags on DAVE (Discord Audio Visual Encryption) protocol support, blocking reliable STT/TTS in voice channels.
3. **Platform lock-in** — Lumina's features are locked to Discord/Python. Noema's are locked to desktop/Tauri. Neither can easily extend to new platforms (Telegram, WhatsApp, WebRTC).
4. **Storage contention** — if both share data, two separate processes writing to the same SQLite is fragile.
5. **Maintenance burden** — maintaining two codebases with overlapping concerns in two languages.

### Goals

- **Unify Noema and Lumina** into a single Rust workspace where they share a common core and differ only in presentation layer.
- **Shared core service** (`simply-core`) that owns LLM, MCP, voice, agent orchestration, and storage — runs as a long-lived daemon process.
- **Lumina as a crate** in the Noema workspace — a Discord bot (serenity + songbird) that connects to the core service for all shared concerns.
- **Voice provider abstraction** in the core (starting with Voxtral/Mistral) — usable by both Noema (desktop mic via CPAL) and Lumina (Discord via songbird).
- **Architecture supports future platforms** (Telegram, WhatsApp, WebRTC/meet) without building them in v1.

### Non-goals (v1)

- Telegram, WhatsApp, or other messaging platform integrations.
- `simply-chris.ai/meet` WebRTC product (architecture supports it, doesn't build it).
- Unified command macro (single annotation for both Discord + MCP) — use serenity's native `#[command]` and separate MCP tool definitions.
- Google services integration migration.
- Full feature parity with Python Lumina — v1 focuses on Discord text commands + voice with DAVE.

### Architecture

See [designs/ARCHITECTURE.md](../designs/ARCHITECTURE.md) for the full platform architecture, including:
- Platform diagram and workspace structure
- Core service communication (MCP + gRPC)
- Features on UCM — content as convention
- Event & Intent System (triggers, Action AST, late binding, service registry)
- Voice architecture
- Extension points

---

### Feature Inventory — What Ports from Python Lumina

Features are grouped by where they land in the Rust workspace.

#### → simply-core (shared services, available to all platforms)

**Core infrastructure (dedicated services):**

| Feature | Python Source | Priority | Notes |
|---------|-------------|----------|-------|
| **Agent orchestration** | `agent/nous_agent.py`, `agent/task_manager.py` | P0 | Core agent loop, model selection, task tracking |
| **LLM providers** | nous library | P0 | Already exists as `noema-core/llm` → becomes `simply-llm` |
| **MCP server/client** | `mcp_protocol/server/`, `mcp_protocol/handlers/` | P0 | MCP tool hosting + external server connections |
| **Voice pipeline** | `services/discord/cogs/voice_cog.py` (VAD, STT, TTS) | P0 | Core motivation for rewrite. Voxtral first. |
| **Document CRUD** | multiple databases | P0 | Generic UCM document ops with frontmatter-aware queries |
| **Identity** | `services/identity/` | P1 | Cross-platform user identity, entity relations |
| **Event & Intent system** | `services/scheduler/` | P1 | Event bus + intent engine — replaces schedules |
| **Search / RAG** | `services/rag/` | P2 | Embeddings over all UCM content, unified search |
| **Brain / Analytics** | `services/brain/` | P2 | Aggregation queries over turn data |

**Content conventions (no dedicated service — just UCM documents with frontmatter):**

| Feature | Python Source | Priority | Notes |
|---------|-------------|----------|-------|
| **TODOs** | `services/database/todo_database.py` | P1 | `type: todo` documents, queried via generic document service |
| **Notes** | `services/database/note_database.py` | P1 | `type: note` documents |
| **Context / Memory** | `services/context/` | P2 | `type: context` documents |
| **Access control** | `services/access_control/` | P1 | `type: access_rule` documents |
| **MCP server config** | `services/mcp/` | P2 | `type: mcp_server` documents |

#### → lumina crate (Discord-specific presentation)

| Feature | Python Source | Priority | Notes |
|---------|-------------|----------|-------|
| **Discord gateway + bot** | `__main__.py`, discord.py bot | P0 | serenity-based, replaces discord.py |
| **Chat commands** | `cogs/chat_cog.py` | P0 | Channel management, message handling, model selection |
| **Voice I/O** | `cogs/voice_cog.py` | P0 | songbird backend, DAVE support, audio bridge to core |
| **Slash commands** | All cogs | P1 | serenity `#[command]` for each feature |
| **Discord embeds/UI** | `handlers/discord_handler.py`, cogs | P1 | Rich embeds, polls, buttons |
| **Admin commands** | `cogs/admin_cog.py` | P1 | Access control management |
| **Server management** | `cogs/server_cog.py` | P2 | Welcome messages, member tracking |
| **Command sync** | `cogs/sync_cog.py` | P2 | Slash command registration |
| **Message export** | `cogs/util_cog.py` | P3 | Export chat history |

#### → Deferred (not in v1, architecture supports later)

| Feature | Reason |
|---------|--------|
| **Google Auth/Drive/Calendar/Docs** | Complex OAuth flows, low priority for v1 |
| **Brave/Google Search** | Easy to add as MCP tool later |
| **Telegram/WhatsApp** | New presentation layer crates |
| **WebRTC / /meet** | New presentation layer crate |
| **Filesystem handler** | Simple to add, low priority |
| **Note → Google Doc export** | Depends on Google integration |

---

### Implementation Stages

Each stage is independently verifiable. We build from **noema's existing Rust foundations** — it already has LLM providers, MCP, UCM storage, audio, and agent orchestration. The strategy: extract shared pieces, get a minimal Lumina running early alongside Noema, and prioritize voice.

Python Lumina remains operational throughout — no big bang cutover.

#### Stage 1: Lumina Crate in Noema Workspace
**Goal:** Minimal Lumina bot exists in the noema workspace, connects to Discord, responds to commands.
**Verify (Lumina):** Bot comes online, responds to `/ping` and `/chat` with a hardcoded response.
**Verify (Noema):** Desktop app still works — nothing broken by adding the crate.

- Add `lumina/` crate to noema workspace `Cargo.toml`
- Basic `main.rs`: serenity bot, connect to Discord gateway
- Two slash commands: `/ping` (health check), `/chat` (echo for now)
- Lumina depends on `noema-core/llm` directly (no core service yet — same as Noema embeds it)

#### Stage 2: LLM Chat Through Shared Core
**Goal:** Both Noema and Lumina can chat with an LLM using the same `noema-core` code.
**Verify (Lumina):** `/chat hello` → LLM response appears in Discord.
**Verify (Noema):** Same conversation works in desktop — same LLM path, same providers.

- Lumina's `/chat` command creates a conversation, calls the agent, streams response to Discord
- Uses `noema-core` agent + LLM directly (in-process, not yet a service)
- Port `ChatCog` basics: message handling, response formatting as Discord embeds
- Single provider first (Claude)

#### Stage 3: Voice Pipeline — Core + Noema Desktop
**Goal:** Voice works on desktop first — speak into mic, get STT, agent responds, TTS plays back.
**Verify (Noema):** Voice conversation works via desktop mic/speaker using Voxtral.

- Create `simply-voice/` crate with STT/TTS provider traits
- Implement Voxtral provider (STT + TTS via Mistral realtime API)
- Wire into `noema-audio` (CPAL backend already exists)
- VAD → STT → agent → TTS pipeline working end-to-end on desktop
- This validates the voice pipeline without needing Discord

#### Stage 4: Voice Pipeline — Lumina Discord
**Goal:** Voice works in Discord — join channel, DAVE-encrypted audio, full STT/TTS loop.
**Verify (Lumina):** Join voice channel, speak, agent responds with voice. DAVE works.

- Add songbird to Lumina crate
- Implement songbird audio backend (bridges songbird PCM ↔ `simply-voice` pipeline)
- Port `VoiceCog` basics: `/voice join`, `/voice leave`, `/voice converse`
- songbird → PCM → STT → agent → TTS → PCM → songbird
- Verify DAVE protocol works with songbird (research/contribute upstream if needed)

#### Stage 4b: RTC Experimentation
**Goal:** Lumina can join a WebRTC call (Google Meet or custom) and transcribe/listen.
**Verify:** Bot joins an RTC session, STT produces a transcript, agent can optionally respond.

- Add WebRTC client crate (e.g., `webrtc-rs` or `str0m`)
- Implement WebRTC audio backend for `simply-voice` (same trait as CPAL/songbird)
- Minimal RTC join flow: connect to a room, receive audio stream
- Wire audio through existing voice pipeline: WebRTC PCM → STT → transcript
- TTS response optional at this stage — transcription/listening is the priority
- This is experimental — API and architecture will evolve, but validates the pipeline works across all three audio sources (desktop, Discord, RTC)

#### Stage 5: Core Service Extraction
**Goal:** `simply-core` runs as a daemon. Both Noema and Lumina connect as clients.
**Verify (both):** Start core service, start Noema, start Lumina. Both chat and voice work through the service. Create a document in one, see it in the other.

- Extract shared logic from `noema-core` into `simply-core` service crate
- gRPC server (tonic) with RPCs: `prompt`, `run_turn`, `list_models`, `transcribe`, `synthesize`
- Noema's Tauri backend becomes a core client (refactor from direct embedding)
- Lumina becomes a core client
- UCM storage owned by the service — single writer
- Voice pipeline runs in core, audio streams via gRPC bidirectional streaming

#### Stage 6: MCP Tools + Document CRUD
**Goal:** Agent can call tools. Documents with frontmatter conventions work.
**Verify (both):** Ask agent to create a todo → stored in UCM. Query it from other platform.

- MCP server/client in `simply-core`
- Expose MCP interface alongside gRPC
- Implement frontmatter-aware document queries in UCM storage
- Generic MCP tools: `create_document`, `query_documents`, `update_document`
- Port todo/note frontmatter conventions
- Port a few more Lumina cogs: `/todo`, `/note` (thin — just call core)

#### Stage 7: Event Bus + Timer Source
**Goal:** Basic event system — timer fires, intent executes a non-LLM action.
**Verify (Noema):** Create a recurring timer intent via chat, see it fire and notify.
**Verify (Lumina):** Same intent fires and posts to Discord channel.

- Implement event bus in `simply-core`
- Timer event source (cron, interval, one-shot, fuzzy time)
- Intent execution table (SQLite, alongside UCM)
- Action AST: `Expr` with `Literal` and `Template` variants
- Action handlers: `notify`, `emit_event`
- Intent documents loaded from UCM, deserialized into AST

#### Stage 8: Full Action AST + Service Registry
**Goal:** Late binding works. Service calls are protocol-agnostic.
**Verify:** Intent with `context_ref` resolves correctly. `call_service` invokes an MCP server.

- Full `Expr` enum: `EventField`, `ContextRef`, `Lookup`, `Template`
- Expression resolver
- Action handlers: `forward`, `update_document`, `call_service`
- Service registry with transport adapters (MCP adapter first, gRPC adapter)
- Register core itself and any external MCP servers as services

#### Stage 9: Platform Event Sources + LLM-Compiled Intents
**Goal:** Discord events flow into event bus. Users describe intents in natural language, LLM compiles them.
**Verify (Lumina):** New member joins → welcome message (no LLM). User says "remind me tomorrow to check PRs" → compiled intent fires next day.

- Lumina registers Discord event source with core
- Discord events (member_joined, message, etc.) emit into bus
- Noema registers desktop events if applicable
- MCP tool for intent creation from natural language
- LLM compiles description → AST frontmatter
- Fuzzy time resolution
- Re-compilation flow

#### Stage 10: Condition-Based Intents + Workflow
**Goal:** Multi-step workflows with conditions, chaining, and multi-agent orchestration.
**Verify:** Spawn two research sub-agents, mainline intent waits for both, resumes when done.

- Condition evaluation in intent engine
- `all` / `any` condition modes
- Compound triggers (condition + time)
- Intent completion events propagate to dependents
- Conversation resumption from intents

#### Stage 11: Remaining Lumina Cogs
**Goal:** Feature parity for the cogs we want in v1.
**Verify:** Each ported cog works via Discord slash commands.

- Port remaining priority cogs: `/admin`, `/brain`, `/schedule` (now intent-based), `/context`
- Discord-specific: embeds, rich formatting, autocomplete
- Access control via UCM documents

#### Future Stages (post-v1)
- Additional LLM providers in `simply-llm`
- Additional voice providers (ElevenLabs, OpenAI TTS)
- Additional event sources (GitHub, email, Notion, webhooks)
- Additional action handlers (send_email, send_telegram)
- Additional transport adapters (REST services)
- Telegram/WhatsApp presentation crates
- WebRTC / `/meet` presentation crate
- Google services integration
- RAG / embedding pipeline in UCM storage

---

### Resolved Questions

1. **Core service protocol** — Hybrid: MCP for agent-facing ops (tool calls), gRPC for platform-facing ops (storage, voice streaming, identity). See [ARCHITECTURE.md](../designs/ARCHITECTURE.md#core-service-communication).
2. **Storage model** — Lumina features map onto UCM primitives. No separate databases. See [ARCHITECTURE.md](../designs/ARCHITECTURE.md#features-on-ucm--content-as-convention).
3. **Command system** — Separate: serenity `#[command]` for Discord, separate MCP tool definitions. No unified macro.

### Open Questions

1. **Repo name** — `simply-platform`? `simply`? `simply-ai`? Keep `noema` for now since that's the GitHub repo?
2. **Songbird DAVE status** — need to verify songbird's current DAVE protocol support. If incomplete, may need to contribute upstream or work around.
3. **Config unification** — Noema uses encrypted API key storage. Lumina uses `.env`. Converge on one approach?
4. **UCM schema extensions** — do we need new entity/document types or metadata fields to support Lumina features (schedules, access control)? Or do existing UCM primitives cover it?
5. **Core service lifecycle** — does simply-core start independently (systemd/launchd), or does the first client (Noema or Lumina) spawn it?

### Related

- Supersedes: Praxis CRUD side-car design (Python-era, no longer applicable)
- Architecture: [designs/ARCHITECTURE.md](../designs/ARCHITECTURE.md)
- Roadmap: [FUTURE_ROADMAP.md](../FUTURE_ROADMAP.md)
