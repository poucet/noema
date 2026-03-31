# Foundation

**Parent:** [v1.0 Roadmap](../../ROADMAP.md)
**Priority:** P0 — everything else depends on this.
**Complexity:** L

---

## Goal

Restructure the workspace to match the target architecture, add Lumina as a crate, establish shared LLM chat across both platforms, and extract the core daemon service.

Python Lumina remains operational throughout — no big bang cutover.

---

## Stages

### 0.0 — Workspace Restructure

**Goal:** Rename crates to match the [ARCHITECTURE.md](../../../designs/ARCHITECTURE.md) workspace structure.

**Complexity:** S

| Current | Target | Change |
|---------|--------|--------|
| `noema-core/` | `simply-core/` | Rename |
| `noema-core/llm/` | `simply-llm/` | Extract to top-level crate + rename |
| `noema-audio/` | `simply-audio/` | Rename |
| `noema-mcp-core/` | merge into `simply-core/src/mcp/` | Merge |
| `noema-desktop/` | `noema-desktop/` | No change |
| `noema-ext/` | `noema-ext/` | No change |
| `noema-ui/` | `noema-ui/` | No change (frontend) |
| `noema-mcp-gdocs/` | — | Deferred (Google integration is post-v1) |
| `commands/` | `commands/` | No change |
| `config/` | `config/` | No change |

**Tasks:**
- [ ] Rename `noema-core/` → `simply-core/`, update `Cargo.toml` package name + all workspace references
- [ ] Extract `noema-core/llm/` → top-level `simply-llm/`, update dependencies
- [ ] Rename `noema-audio/` → `simply-audio/`, update references
- [ ] Merge `noema-mcp-core/` into `simply-core/src/mcp/`, remove standalone crate
- [ ] Update workspace `Cargo.toml` members list
- [ ] Verify `noema-desktop` builds with renamed dependencies

**Verify:** `cargo check --workspace` passes. Noema desktop still launches.

---

### 0.1 — Lumina Crate

**Goal:** Minimal Lumina bot exists in the workspace, connects to Discord, responds to commands.

**Complexity:** S

**Tasks:**
- [ ] Add `lumina/` crate to workspace `Cargo.toml`
- [ ] Basic `main.rs`: serenity bot, connect to Discord gateway
- [ ] Two slash commands: `/ping` (health check), `/chat` (echo for now)
- [ ] Lumina depends on `simply-llm` directly (no core service yet — same as Noema embeds it)
- [ ] Config: Discord bot token loading (`.env` or shared config approach)

**Verify:**
- Lumina: Bot comes online, responds to `/ping` and `/chat` with echo.
- Noema: Desktop app still works — nothing broken by adding the crate.

---

### 0.2 — Shared LLM Chat

**Goal:** Both Noema and Lumina chat with an LLM using the same code path.

**Complexity:** M

**Tasks:**
- [ ] Lumina's `/chat` command creates a conversation, calls the agent, streams response to Discord
- [ ] Uses `simply-core` agent + LLM directly (in-process, not yet a service)
- [ ] Port ChatCog basics: message handling, response formatting as Discord embeds
- [ ] Single provider first (Claude)
- [ ] Conversation storage: both platforms write to the same UCM store

**Verify:**
- Lumina: `/chat hello` → LLM response appears in Discord.
- Noema: Same conversation works in desktop — same LLM path, same providers.

---

### 0.3 — Core Service Extraction

**Goal:** `simply-core` runs as a daemon. Both Noema and Lumina connect as clients.

**Complexity:** L

**Tasks:**
- [ ] Extract shared logic from in-process usage into `simply-core` service binary
- [ ] gRPC server (tonic) with initial RPCs: `prompt`, `run_turn`, `list_models`
- [ ] Noema's Tauri backend becomes a core client (refactor from direct embedding)
- [ ] Lumina becomes a core client
- [ ] UCM storage owned by the service — single writer, no SQLite contention
- [ ] Service lifecycle: decide startup approach (standalone daemon vs. first-client-spawns)

**Verify:**
- Start core service, start Noema, start Lumina.
- Both chat through the service.
- Create a conversation in one, see it in the other.

---

## Dependencies

```
0.0 → 0.1 → 0.2 → 0.3 (sequential)
```

All stages are sequential — each builds on the previous.
