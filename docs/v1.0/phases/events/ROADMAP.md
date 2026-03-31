# Events & Intents

**Parent:** [v1.0 Roadmap](../../ROADMAP.md)
**Priority:** P1
**Complexity:** XL
**Depends on:** Lumina complete
**Soft dependency:** Content phase (frontmatter-aware document queries for intent storage)
**Parallel with:** Content phase

---

## Goal

Reactive event system — timers fire, intents execute actions, Discord events flow into the bus, users describe intents in natural language and the LLM compiles them into declarative ASTs. Multi-step workflows with conditions and chaining.

See [ARCHITECTURE.md — Event & Intent System](../../../designs/ARCHITECTURE.md#event--intent-system) for the full design.

---

## Stages

### Stage 1 — Event Bus + Timer Source

**Goal:** Basic event system with timer-based intents.

**Complexity:** M

**Tasks:**
- [ ] Implement event bus in `simply-core` (pub/sub, typed event payloads)
- [ ] Timer event source: cron, interval, one-shot, fuzzy time expressions
- [ ] Intent execution table (SQLite, alongside UCM) — stores runtime state
- [ ] Intent documents in UCM with `type: intent` frontmatter
- [ ] Action AST: `Expr` with `Literal` and `Template` variants (minimal subset)
- [ ] Action handlers: `notify`, `emit_event`
- [ ] Engine loop: process queue → check timers → fire ready intents → sleep

**Verify:**
- Noema: Create a recurring timer intent via chat, see it fire and notify.
- Lumina: Same intent fires and posts to Discord channel.

---

### Stage 2 — Full Action AST + Service Registry

**Goal:** Late binding works. Service calls are protocol-agnostic.

**Complexity:** L

**Tasks:**
- [ ] Full `Expr` enum: `EventField`, `ContextRef`, `Lookup`, `Template`
- [ ] Expression resolver (evaluates `Expr` tree against event context)
- [ ] Action handlers: `forward`, `update_document`, `call_service`
- [ ] Service registry trait with transport adapters
- [ ] MCP transport adapter (wraps MCP servers as services)
- [ ] Internal transport adapter (wraps daemon's own services)
- [ ] Register core itself + external MCP servers as services

**Verify:**
- Intent with `context_ref` resolves correctly at fire time.
- `call_service` invokes an MCP server by name without knowing transport.

---

### Stage 3 — Platform Event Sources

**Goal:** Discord events flow into the event bus. Platform-agnostic event routing.

**Complexity:** M

*Can run in parallel with Stage 4.*

**Tasks:**
- [ ] Lumina registers Discord event source with core on connect
- [ ] Discord events emit into bus: `discord.member_joined`, `discord.message`, `discord.reaction`
- [ ] Noema registers desktop events if applicable (e.g., app focus, idle detection)
- [ ] Event source registration protocol (clients register via WebSocket, services via REST)

**Verify:**
- New member joins Discord → `discord.member_joined` event → intent fires → welcome message posted (no LLM needed).

---

### Stage 4 — LLM-Compiled Intents

**Goal:** Users describe intents in natural language, LLM compiles them to AST frontmatter.

**Complexity:** M

*Can run in parallel with Stage 3.*

**Tasks:**
- [ ] MCP tool for intent creation: `create_intent(description)` → LLM → AST frontmatter
- [ ] LLM compilation prompt: natural language → trigger + action + target YAML
- [ ] Fuzzy time resolution: "tomorrow morning" → concrete datetime + original text preserved
- [ ] Re-compilation flow: edit description → re-compile AST
- [ ] Validation: compiled AST is checked against registered event sources and action handlers

**Verify:**
- "Remind me tomorrow to check PRs" → compiled intent with timer trigger, fires next day.
- "When someone joins the server, welcome them" → compiled intent with Discord event trigger.

---

### Stage 5 — Conditions + Workflow

**Goal:** Multi-step workflows with conditions, chaining, and multi-agent orchestration.

**Complexity:** L

**Tasks:**
- [ ] Condition evaluation in intent engine (`all` / `any` modes)
- [ ] Compound triggers: condition + time combined
- [ ] Intent completion events (`intent.completed`, `intent.failed`) propagate to dependents
- [ ] Intent chaining: action output → next intent's trigger
- [ ] Conversation resumption from intents (reopen suspended conversation with context)
- [ ] Multi-agent orchestration: spawn sub-agents as intents, mainline waits

**Verify:**
- Spawn two research sub-agents, mainline intent waits for both (`all` condition), resumes when done.
- Chain: intent A completes → emits event → triggers intent B.

---

## Dependencies

```
Stage 1 → Stage 2 → Stage 3 ──→ Stage 5
                      ↘          ↗
                    Stage 4 ────
```

Stage 3 (platform events) and Stage 4 (LLM compilation) can run in parallel after Stage 2. Both feed into Stage 5 (conditions + workflow).
