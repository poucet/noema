# Foundation — Stage 2 Handoff

Stage 1 (Workspace Restructure) is complete. This document has everything needed to start Stage 2 (Daemon).

---

## What was done in Stage 1

Restructured the workspace from `noema-*` naming to the target architecture:

| Before | After | Notes |
|--------|-------|-------|
| `noema-core/` | `simply-core/` | Library crate — LLM, MCP client, agents |
| `noema-core/llm/` | `simply-core/llm/` | LLM abstraction (unchanged internally) |
| `noema-audio/` | `simply-audio/` | Audio — Whisper STT, CPAL, voice coordination |
| `noema-mcp-core/` | `simply-daemon/src/mcp/` | Merged into daemon — depends on storage/sessions |
| *(new)* | `simply-daemon/` | Daemon crate with `DaemonApi` trait skeleton |

All `use noema_core::` → `use simply_core::`, all `use noema_audio::` → `use simply_audio::`.
Desktop uses `simply_daemon::mcp` instead of the old standalone crate.
Workspace compiles and Noema runs end-to-end.

---

## Current workspace structure

```
Cargo.toml              — workspace: simply-core, simply-core/llm, simply-audio,
                          simply-daemon, noema-ext, noema-desktop, noema-mcp-gdocs,
                          commands, config
simply-core/            — library: agents, MCP client, storage (still here for now)
simply-core/llm/        — LLM provider abstraction
simply-audio/           — audio capture, STT, voice coordination
simply-daemon/          — daemon crate (Stage 2 target)
  src/api.rs            — DaemonApi trait + all types (see below)
  src/lib.rs            — pub mod api; pub mod mcp;
  src/main.rs           — stub binary
  src/mcp/              — MCP server (moved from noema-mcp-core)
    mod.rs, server.rs, tools.rs
noema-desktop/          — Tauri 2 app (React frontend + Rust backend)
noema-ext/              — PDF extraction utilities
noema-mcp-gdocs/        — standalone Google Docs MCP server
commands/               — command framework
config/                 — config, paths, API key encryption
```

---

## DaemonApi trait (already defined in `simply-daemon/src/api.rs`)

The trait is complete and covers all protocol operations from CORE_SERVICE.md:

**Session lifecycle:**
- `create_session(options)` → `SessionId`
- `resume_session(id)` → `SessionInfo` (solves restart problem)
- `close_session(id)` — frees memory, keeps UCM data
- `seed_context(id, messages)` — replay history (Lumina re-sends Discord messages)
- `list_sessions()` → `Vec<SessionInfo>`
- `set_persistence(id, mode)` — toggle ephemeral ↔ persistent

**Conversation:**
- `send_message(id, message)` → `Vec<DaemonEvent>` (multimodal `UserMessage` with `ContentBlock::Text/Image/File`)
- `set_model(id, model_id)`
- `truncate(id, before_turn)`

**MCP tools:**
- `register_mcp(registration)` — tools become globally available
- `unregister_mcp(name)`
- `list_tools()` → tool names

**Events:**
- `push_event(event)` — trigger interface (InboundEvent with type + JSON payload)

**Voice:**
- `voice_connect(id)` → `VoiceHandle { audio_in: Sender<AudioFrame>, events: Receiver<VoiceEvent> }`
- Client handles platform audio (CPAL/songbird/WebRTC), daemon handles STT/LLM/TTS

---

## Key architectural decisions

1. **Trait-based with two impls.** Noema/Lumina depend on `DaemonApi`, never on a concrete impl.
   - **In-process** (`embedded.rs`) — daemon linked into the same binary. No networking. First to build.
   - **Remote** (future) — calls over WebSocket to a separate daemon process.

2. **In-process first.** Build the embedded impl, wire Noema to it, validate the API surface — *then* build WebSocket/REST.

3. **Storage stays in simply-core for now.** Task 2.10 moves it to the daemon, but only after the daemon is wired up and verified. This avoids a risky refactor before the wiring is tested.

4. **`send_message` returns `Vec<DaemonEvent>` for now.** Good enough for in-process. The remote impl will use a proper async stream/channel.

---

## Stage 2 task order

See [TASKS.md](TASKS.md) for full details. Recommended order:

```
2.1 (DaemonApi trait)  — already done (api.rs exists)
      ↓
2.2 (in-process impl)  — EmbeddedDaemon in simply-daemon/src/embedded.rs
      ↓                   Wire simply-core agent + MCP + storage
2.3 (wire Noema)        — Replace desktop's direct simply-core usage with DaemonApi
      ↓                   Validates the API is complete
2.4 (sessions)          — Session manager (feeds into 2.2, can overlap)
      ↓
2.5 (daemon binary)     — main.rs: startup, config, shutdown
      ↓
2.6 (WebSocket)         — WS server + remote DaemonApi client impl
      ↓
2.7 (REST)              — /events, /register, /health
      ↓
2.8 (peer registry)     — Track clients + services + global tool registry
      ↓
2.9 (MCP client)        — Connect to action services, discover tools
      ↓
2.10 (move storage)     — simply-core/src/storage/ → simply-daemon/
```

**Start with 2.1 (already done) → 2.2 → 2.3.** That's the critical path — everything else can wait until Noema works through the daemon API.

---

## Key files to read

| File | Why |
|------|-----|
| `simply-daemon/src/api.rs` | The DaemonApi trait — this is the contract |
| `docs/designs/CORE_SERVICE.md` | Full protocol spec (WebSocket messages, REST endpoints, MCP) |
| `docs/designs/ARCHITECTURE.md` | Three-interface hub pattern, content-as-config |
| `noema-desktop/src-tauri/src/state.rs` | Current Noema state — what needs to go through DaemonApi |
| `noema-desktop/src-tauri/src/commands/chat.rs` | Current chat flow — direct simply-core usage to replace |
| `noema-desktop/src-tauri/src/core_server.rs` | How MCP server is currently started in-process |
| `simply-core/src/manager.rs` | ConversationManager — the logic EmbeddedDaemon will wrap |
| `simply-daemon/src/mcp/tools.rs` | spawn_agent tool — already in the daemon |

---

## Rules

- Always commit with `jj commit` (not `git commit`)
- Update TASKS.md status emoji after each task completion, commit separately
- Do NOT run tests, builds, or type generation — user handles that
