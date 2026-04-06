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

## Stage 2 — LLM Chat (COMPLETE)

**Goal:** Lumina chats with an LLM through the daemon.

| # | | Task | Priority | Size |
|---|---|------|----------|------|
| 2.1 | ✅ | `/chat new [name]` — create private text channel under "AI Chats" category | P0 | M |
| 2.2 | ✅ | `on_message` listener: respond in AI Chats category channels and @mentions | P0 | M |
| 2.3 | ✅ | Load Discord channel history as conversation context | P0 | M |
| 2.4 | ✅ | Open daemon session, send message, stream response back | P0 | M |
| 2.5 | 🚫 | System prompt from UCM (depends on Content Stage 2) | P0 | M |
| 2.6 | ✅ | `/chat pause`, `/chat resume`, `/chat model` | P0 | S |
| 2.7 | ✅ | Tool call display as Discord embeds | P1 | S |

---

## Stage 2.5 — Core Architecture Refactor (COMPLETE)

| # | | Task | Priority | Size |
|---|---|------|----------|------|
| 2.5.1–11 | ✅ | ToolService trait, ToolAgent, SessionManager, daemon session sharing, API cleanup | P0 | L |

---

## Stage 3 — Discord MCP Service (COMPLETE)

**Goal:** Lumina registers as MCP service, exposing Discord actions as tools.

| # | | Task | Priority | Size |
|---|---|------|----------|------|
| 3.1 | ✅ | Lumina registers as MCP service with daemon on connect | P0 | M |
| 3.2 | ✅ | Discord MCP tools: 15 tools via rmcp macros | P0 | L |
| 3.3 | ✅ | MCP instructions with guild/channel map from gateway cache | P0 | S |
| 3.4 | ✅ | `/tool call` + `/tool list` slash commands | P0 | M |
| 3.5 | ⬜ | Verify: Noema agent can use Discord tools through daemon | P0 | S |

---

## Stage 6 — Voice (COMPLETE)

**Goal:** Voice channel features — STT, TTS, voice conversation.

| # | | Task | Priority | Size |
|---|---|------|----------|------|
| 6.1 | ✅ | Songbird integration (DAVE support, decode to mono 16kHz) | P0 | M |
| 6.2 | ✅ | `/voice transcribe` — join voice, transcribe speech to voice channel text | P0 | M |
| 6.3 | ✅ | `/voice listen` — STT → LLM session (seeded with channel history) → TTS → play | P0 | L |
| 6.4 | ✅ | `/voice say <text>` — TTS → play in voice channel (auto-join) | P0 | S |
| 6.5 | ✅ | `/voice leave` — leave voice channel, clean up session | P0 | S |
| 6.6 | ✅ | `/voice list` — show all providers and voices | P0 | S |
| 6.7 | ✅ | `/voice status` — show current settings and session | P0 | S |
| 6.8 | ✅ | `/voice provider stt|tts <id>` — set provider (with autocomplete) | P0 | S |
| 6.9 | ✅ | `/voice set-voice <id>` — set TTS voice (with autocomplete) | P0 | S |
| 6.10 | ✅ | Voice config persistence in lumina.toml | P0 | S |
| 6.11 | ✅ | Graceful TTS fallback: error → text with 🔇 prefix | P0 | S |
| 6.12 | ✅ | Random voice selection when none configured | P1 | S |
| 6.13 | ✅ | Reset voice ID when TTS provider changes | P0 | S |
| 6.14 | ✅ | Transcripts post to voice channel text (not originating channel) | P0 | S |
| 6.15 | ⬜ | Hot-swap STT provider mid-session (without losing conversation) | P1 | M |
| 6.16 | ⬜ | Multi-user voice: distinguish speakers, turn-taking | P1 | M |
| 6.17 | ⬜ | `/voice set-voice` autocomplete for ElevenLabs voices | P1 | S |

---

## ~~Stage 4 — Admin & Access Control~~ (PUNTED)

Deferred — not needed for current workflow.

---

## ~~Stage 5 — Schedule~~ (DEFERRED → Events phase)

Deferred — belongs in Events phase.

---

## Stage 7 — Document Upload from Discord

| # | | Task | Priority | Size |
|---|---|------|----------|------|
| 7.1 | ⬜ | `/doc upload` — upload attachment or markdown into UCM storage | P1 | M |
| 7.2 | ⬜ | `/doc list`, `/doc view` — browse documents from Discord | P1 | S |

---

## Stage 8 — Brain & Analytics

| # | | Task | Priority | Size |
|---|---|------|----------|------|
| 8.1 | ⬜ | Port BrainCog: `/brain last`, `tasks`, `active`, `task` | P2 | M |
| 8.2 | ⬜ | `/brain model` — view/change LLM model | P1 | S |

---

## Stage 9 — Context (Memory Garden)

| # | | Task | Priority | Size |
|---|---|------|----------|------|
| 9.1 | ⬜ | Port ContextCog: `/context save`, `reload`, `list`, `show`, `delete` | P2 | M |
| 9.2 | ⬜ | `on_message` listener for context switch detection | P2 | S |

---

## Stage 10 — Google Integrations (Sidecar MCP Server)

| # | | Task | Priority | Size |
|---|---|------|----------|------|
| 10.1 | ⬜ | Scaffold sidecar MCP server with Google OAuth via MCP auth | P2 | M |
| 10.2 | ⬜ | Drive tools: `drive_list`, `drive_show`, `drive_read` | P2 | M |
| 10.3 | ⬜ | Calendar tools: `calendar_list` | P2 | S |

---

## Punted

| # | | Task | Priority | Size |
|---|---|------|----------|------|
| — | ⏸️ | Port TodoCog (UCM content conventions) | P2 | M |
| — | ⏸️ | Port NoteCog (UCM content conventions) | P2 | M |
