# Lumina — Minimal Discord Bot

**Parent:** [v1.0 Roadmap](../../ROADMAP.md)
**Priority:** P0
**Complexity:** M
**Depends on:** Foundation complete

---

## Goal

Lumina exists in the workspace as a Discord bot that connects to the core service for LLM chat. This establishes the second platform so all subsequent phases can be tested cross-platform.

---

## Stages

### Stage 1 — Lumina Crate

**Goal:** Minimal Lumina bot exists in the workspace, connects to Discord, responds to commands.

**Complexity:** S

**Tasks:**
- [ ] Add `lumina/` crate to workspace `Cargo.toml`
- [ ] Basic `main.rs`: serenity bot, connect to Discord gateway
- [ ] Two slash commands: `/ping` (health check), `/chat` (echo for now)
- [ ] Lumina connects to `simply-core` as a client
- [ ] Config: Discord bot token loading (`.env` or shared config approach)

**Verify:**
- Lumina: Bot comes online, responds to `/ping` and `/chat` with echo.
- Noema: Desktop app still works — nothing broken by adding the crate.

---

### Stage 2 — Shared LLM Chat

**Goal:** Both Noema and Lumina chat with an LLM using the same code path through the core service.

**Complexity:** M

**Tasks:**
- [ ] Lumina's `/chat` command creates a conversation, calls the agent via core service, streams response to Discord
- [ ] Port ChatCog basics: message handling, response formatting as Discord embeds
- [ ] Single provider first (Claude)
- [ ] Conversation storage: both platforms use core service — single writer

**Verify:**
- Lumina: `/chat hello` → LLM response appears in Discord.
- Noema: Same conversation works in desktop — same LLM path, same providers.
- Create a conversation in one, see it in the other.

---

## Dependencies

```
Stage 1 → Stage 2 (sequential)
```
