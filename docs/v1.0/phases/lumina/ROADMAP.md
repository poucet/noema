# Lumina — Minimal Discord Bot

**Parent:** [v1.0 Roadmap](../../ROADMAP.md)
**Priority:** P0
**Complexity:** M
**Depends on:** Foundation complete

---

## Goal

Lumina exists in the workspace as a Discord bot that uses `simply-core` with a Discord-backed `ExecutionContext` — the Discord channel *is* the conversation history. This establishes the second platform so all subsequent phases can be tested cross-platform.

Lumina can optionally connect to `simply-service` for features that need persistent storage (documents, events), but the basic chat path goes directly through `simply-core`.

---

## Stages

### Stage 1 — Lumina Crate

**Goal:** Minimal Lumina bot exists in the workspace, connects to Discord, responds to commands.

**Complexity:** S

**Tasks:**
- [ ] Add `lumina/` crate to workspace `Cargo.toml`
- [ ] Basic `main.rs`: serenity bot, connect to Discord gateway
- [ ] Two slash commands: `/ping` (health check), `/chat` (echo for now)
- [ ] Lumina depends on `simply-core` as a library
- [ ] Config: Discord bot token loading (`.env` or shared config approach)

**Verify:**
- Lumina: Bot comes online, responds to `/ping` and `/chat` with echo.
- Noema: Desktop app still works — nothing broken by adding the crate.

---

### Stage 2 — Shared LLM Chat

**Goal:** Lumina chats with an LLM using `simply-core` agent with a Discord-backed execution context.

**Complexity:** M

**Tasks:**
- [ ] Implement Discord-backed `ExecutionContext` — channel messages as conversation history
- [ ] Lumina's `/chat` command creates a conversation, calls the agent, streams response to Discord
- [ ] Port ChatCog basics: message handling, response formatting as Discord embeds
- [ ] Single provider first (Claude)
- [ ] Verify Noema still works through simply-service with UCM-backed context

**Verify:**
- Lumina: `/chat hello` → LLM response appears in Discord.
- Noema: Chat still works through simply-service — same LLM providers, different context backing.

---

## Dependencies

```
Stage 1 → Stage 2 (sequential)
```
