# Lumina — Tasks

**Phase:** Lumina (Minimal Discord Bot)
**Status:** Not Started
**Roadmap:** [ROADMAP.md](ROADMAP.md)
**Depends on:** Foundation (complete)

---

## Stage 1 — Lumina Crate

**Goal:** Minimal Lumina bot in the workspace, connects to Discord and simply-daemon.

| # | | Task | Priority | Size |
|---|---|------|----------|------|
| 1.1 | ✅ | Add `lumina/` crate to workspace | P0 | S |
| 1.2 | ✅ | Basic main.rs: serenity bot, connect to Discord gateway | P0 | S |
| 1.3 | ✅ | Connect to simply-daemon via RemoteDaemon (WS client) | P0 | S |
| 1.4 | ✅ | Slash commands: `/ping`, `/chat` (echo) | P0 | S |
| 1.5 | ✅ | Config: Discord bot token from env/config | P0 | S |

---

## Stage 2 — Shared LLM Chat

**Goal:** Lumina chats with an LLM through the daemon.

| # | | Task | Priority | Size |
|---|---|------|----------|------|
| 2.1 | ⬜ | On `/chat`, open ephemeral session with daemon | P0 | M |
| 2.2 | ⬜ | Seed context from recent Discord channel messages | P0 | M |
| 2.3 | ⬜ | Send user message, receive streamed response, post to Discord | P0 | M |
| 2.4 | ⬜ | Register Discord MCP tools for agent interaction | P1 | M |
| 2.5 | ⬜ | Response formatting as Discord embeds | P0 | S |
