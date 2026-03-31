# Simply Platform v1.0 — Roadmap

**Design:** [GOAL.md](GOAL.md)
**Architecture:** [designs/ARCHITECTURE.md](../designs/ARCHITECTURE.md)
**Post-v1:** [FUTURE_ROADMAP.md](../FUTURE_ROADMAP.md)

---

## Overview

v1.0 is organized into phases. After the Foundation, Content and Events run **in parallel** — orthogonal workstreams that converge when intents are stored as UCM documents. Voice is added on top once the core platform is solid and testable via Discord text. Discord Polish finishes remaining features.

Each phase has its own detailed roadmap in `phases/`.

```
Foundation (sequential)
  Crate rename → core service extraction → Lumina crate → shared LLM

Content Platform  ──────────►  Events & Intents
  MCP tools + doc CRUD  ╲       Event bus + timer
  Frontmatter queries  feeds    Action AST + service registry
                        into    Platform events (Discord text)
                          ╱     LLM-compiled intents
                         ▼      Conditions + workflow
              Intents stored as UCM documents

Voice
  simply-voice crate + Voxtral
  Desktop voice → core service → Discord (songbird) → RTC

Discord Polish
  Remaining Lumina cogs, embeds, admin, rich UI
```

---

## Phases

| Phase | Name | Priority | Complexity | Depends On | Roadmap |
|-------|------|----------|------------|------------|---------|
| **Foundation** | Workspace + core service | P0 | L | — | [phases/foundation/](phases/foundation/ROADMAP.md) |
| **Content** | MCP, document CRUD, frontmatter | P0 | M | Foundation | [phases/content/](phases/content/ROADMAP.md) |
| **Events** | Event bus, intents, workflow | P1 | XL | Foundation, soft on Content | [phases/events/](phases/events/ROADMAP.md) |
| **Voice** | Voice pipeline, all backends | P0 | XL | Foundation | [phases/voice/](phases/voice/ROADMAP.md) |
| **Discord** | Remaining cogs, rich UI | P2 | M | Foundation, Content, Events | [phases/discord/](phases/discord/ROADMAP.md) |

---

## Parallelization

```
Timeline ──────────────────────────────────────────────────────────────────────►

Foundation  ██████████████████████████

Content                               ██████████████████
                                                │
Events                                ██████████████████████████████████
                                                  Stages 3+4
                                                  parallel

Voice                                                      ████████████████████████

Discord                                                                    ████████
```

**Maximum parallelism:** Content and Events run as independent work streams after Foundation. Voice library can optionally start early since it's a standalone crate with no core service dependency.
