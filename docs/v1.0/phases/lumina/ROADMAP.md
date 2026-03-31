# Lumina — Minimal Discord Bot

**Parent:** [v1.0 Roadmap](../../ROADMAP.md)
**Priority:** P0
**Complexity:** M
**Depends on:** Foundation complete

---

## Goal

Lumina exists in the workspace as a Discord bot that connects to simply-daemon via WebSocket. It seeds conversation context from Discord channel history, registers Discord MCP tools, and delegates all agent work to the daemon. No dependency on simply-core.

---

## Stages

### Stage 1 — Lumina Crate

**Goal:** Minimal Lumina bot exists in the workspace, connects to Discord and simply-daemon.

**Complexity:** S

**Tasks:**
- [ ] Add `lumina/` crate to workspace `Cargo.toml`
- [ ] Basic `main.rs`: serenity bot, connect to Discord gateway
- [ ] WebSocket client to simply-daemon
- [ ] Two slash commands: `/ping` (health check), `/chat` (echo for now)
- [ ] Config: Discord bot token loading (`.env` or shared config approach)

**Verify:**
- Lumina: Bot comes online, responds to `/ping` and `/chat` with echo.
- Noema: Desktop app still works — nothing broken by adding the crate.

---

### Stage 2 — Shared LLM Chat

**Goal:** Lumina chats with an LLM by sending messages to simply-daemon, which runs the agent.

**Complexity:** M

**Tasks:**
- [ ] On `/chat`, Lumina opens a session (ephemeral by default) with simply-daemon
- [ ] Seeds context from recent Discord channel messages (rolling window of last N)
- [ ] Sends user message, receives streamed agent response, posts to Discord
- [ ] Registers Discord MCP tools so the agent can interact with Discord
- [ ] Port ChatCog basics: message handling, response formatting as Discord embeds
- [ ] Verify Noema still works through simply-daemon with persistent sessions

**Verify:**
- Lumina: `/chat hello` → LLM response appears in Discord.
- Noema: Chat still works through simply-daemon.

---

## Dependencies

```
Stage 1 → Stage 2 (sequential)
```
