# Simply Platform v1.0 — Roadmap

**Design:** [GOAL.md](GOAL.md)
**Architecture:** [designs/ARCHITECTURE.md](../designs/ARCHITECTURE.md)
**Post-v1:** [FUTURE_ROADMAP.md](../FUTURE_ROADMAP.md)

---

## Overview

v1.0 is organized into phases. After the sequential foundation (Phase 0), Phases 1A and 1B run **in parallel** — content/tools and events/intents are orthogonal workstreams that converge when intents are stored as UCM documents. Voice (Phase 2) is added on top once the core platform is solid and testable via Discord text. Phase 3 polishes remaining Discord features.

Each phase has its own detailed roadmap in `phases/`.

```
Phase 0: Foundation (sequential)
  Crate rename → Lumina crate → shared LLM → core service extraction

Phase 1A: Content Platform  ──────►  Phase 1B: Events & Intents
  MCP tools + doc CRUD        ╲         Event bus + timer
  Frontmatter queries      feeds into   Action AST + service registry
                                ╱       Platform events (Discord text)
                               ╱        LLM-compiled intents
                              ╱         Conditions + workflow
                             ▼
                    Intents stored as UCM documents

Phase 2: Voice
  simply-voice crate + Voxtral
  Desktop voice → core service → Discord (songbird) → RTC

Phase 3: Discord Polish
  Remaining Lumina cogs, embeds, admin, rich UI
```

---

## Phases

| Phase | Name | Priority | Complexity | Depends On | Roadmap |
|-------|------|----------|------------|------------|---------|
| **0** | Foundation | P0 | L | — | [phases/0/ROADMAP.md](phases/0/ROADMAP.md) |
| **1A** | Content Platform | P0 | M | Phase 0 | [phases/1a/ROADMAP.md](phases/1a/ROADMAP.md) |
| **1B** | Events & Intents | P1 | XL | Phase 0, soft on 1A | [phases/1b/ROADMAP.md](phases/1b/ROADMAP.md) |
| **2** | Voice | P0 | XL | Phase 0 | [phases/2/ROADMAP.md](phases/2/ROADMAP.md) |
| **3** | Discord Polish | P2 | M | Phases 0, 1A, 1B | [phases/3/ROADMAP.md](phases/3/ROADMAP.md) |

---

## Parallelization

```
Timeline ──────────────────────────────────────────────────────────────────►

Phase 0   ██████████████████████████
           0.0  0.1  0.2  0.3

Phase 1A                            ██████████████████
                                     1A.1 1A.2 1A.3
                                              │
Phase 1B                            ██████████████████████████████████
                                     1B.1 1B.2  1B.3  1B.4  1B.5
                                                └──┬──┘
                                                parallel

Phase 2                                                    ████████████████████████
                                                            2.1  2.2  2.3  2.4  2.5

Phase 3                                                                    ████████
                                                                            3.1-3.4
```

**Maximum parallelism:** During Phase 1, two work streams can independently advance content platform and event system. Voice library (2.1) can optionally start early since it's a standalone crate with no core service dependency.
