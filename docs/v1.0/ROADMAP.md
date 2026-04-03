# Simply Platform v1.0 — Roadmap

**Design:** [GOAL.md](GOAL.md)
**Architecture:** [designs/ARCHITECTURE.md](../designs/ARCHITECTURE.md)
**Post-v1:** [FUTURE_ROADMAP.md](../FUTURE_ROADMAP.md)
**Manual tests:** [TODO.md](TODO.md)

---

## Overview

v1.0 is organized into phases. Foundation restructured the workspace and built the daemon hub. Lumina ported the Discord bot with MCP tool infrastructure. Voice, Content, and Events run as independent workstreams after Lumina's core is functional.

Each phase has its own detailed roadmap in `phases/`.

```
Foundation (complete)
  Crate rename → daemon → REST-first → service extraction → typed content dispatch

Lumina (Stage 3 complete, paused)
  Crate → Chat → MCP Service → /tool command
  (Admin punted, Schedule deferred to Events)

Voice (in progress)                Content (not started)         Events (not started)
  simply-voice crate                 Document CRUD                Event bus
  Daemon voice pipeline              Frontmatter queries          Intents
  Desktop voice (Noema)              Content conventions          Scheduled prompts
  Discord voice (Lumina)
```

---

## Phases

| Phase | Status | Priority | Complexity | Depends On | Roadmap |
|-------|--------|----------|------------|------------|---------|
| **Foundation** | Complete | P0 | L | — | [phases/foundation/](phases/foundation/ROADMAP.md) |
| **Lumina** | Paused (Stage 3) | P0 | L | Foundation | [phases/lumina/](phases/lumina/ROADMAP.md) |
| **Voice** | In Progress | P0 | L | Foundation | [phases/voice/](phases/voice/ROADMAP.md) |
| **Content** | Not Started | P0 | M | Foundation | [phases/content/](phases/content/ROADMAP.md) |
| **Events** | Not Started | P1 | XL | Foundation, soft on Content | [phases/events/](phases/events/ROADMAP.md) |
| **RTC** | Not Started | P1 | L | Voice Stage 2 | [phases/rtc/](phases/rtc/ROADMAP.md) |

---

## What's been delivered

### Foundation
- `simply-daemon` hub with 8 API traits, axum REST + WS server
- `simply-rpc` framework: `#[rpc_service]` proc macro, REST dispatch, binary transfer
- `IntoContent`/`FromContent` traits + `rest_dispatch_as_content` for typed MCP content
- Service extraction: McpService, ModelService, AssetService, etc.

### Lumina (through Stage 3)
- Discord bot connects to daemon via `RemoteDaemon`
- LLM chat: channel management, streaming responses, model selection
- MCP service: 15 Discord tools via rmcp `#[tool]` macros, ephemeral registration
- `/tool call` (modal form from schema) + `/tool list` (paginated embed)
- Dynamic channel map in MCP instructions (refreshes on Discord events)
- Daemon API: `list_all_tools`, `call_tool_direct` using rmcp types natively

### Lumina deferred
- Admin/access control — punted (not needed for current workflow)
- Schedule — deferred to Events phase (intents/triggers architecture)
- Verification (3.5) — pending manual test, see [TODO.md](TODO.md)

---

## Parallelization

```
Timeline ──────────────────────────────────────────────────────────────────────►

Foundation  ████████████████████████████████████  (complete)

Lumina      ·················████████████████████  (Stage 1-3 complete, paused)

Voice                                       ████████████████████████  (in progress)

Content                                     ██████████████████

Events                                      ██████████████████████████████████

RTC                                                  ██████████████████
```

Voice, Content, and Events can advance independently. RTC starts after Voice Stage 2 (daemon voice API).
