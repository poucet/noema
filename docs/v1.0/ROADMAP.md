# Simply Platform v1.0 — Roadmap

**Design:** [GOAL.md](GOAL.md)
**Architecture:** [designs/ARCHITECTURE.md](../designs/ARCHITECTURE.md)
**Post-v1:** [FUTURE_ROADMAP.md](../FUTURE_ROADMAP.md)

---

## Overview

v1.0 is organized into phases. Foundation restructures the workspace and extracts the core service. Lumina adds the minimal Discord bot. After that, Content, Events, Voice, and Discord run **in parallel** as independent workstreams.

Each phase has its own detailed roadmap in `phases/`.

```
Foundation (sequential)               Lumina (sequential)
  Crate rename → core service    →      Lumina crate → shared LLM chat
                                                │
                    ┌───────────────┬────────────┼────────────────┐
                    ▼               ▼            ▼                ▼
              Content          Events         Voice           Discord
              MCP + docs       Event bus      simply-voice    Remaining cogs
              Frontmatter      Intents        Desktop + CPAL  Embeds, admin
                    │          AST + registry  Songbird + DAVE
                    └──feeds──►Platform events
                               Conditions
```

---

## Phases

| Phase | Name | Priority | Complexity | Depends On | Roadmap |
|-------|------|----------|------------|------------|---------|
| **Foundation** | Crate renames + simply-service daemon | P0 | L | — | [phases/foundation/](phases/foundation/ROADMAP.md) |
| **Lumina** | Minimal Discord bot + shared LLM | P0 | M | Foundation | [phases/lumina/](phases/lumina/ROADMAP.md) |
| **Content** | MCP, document CRUD, frontmatter | P0 | M | Lumina | [phases/content/](phases/content/ROADMAP.md) |
| **Events** | Event bus, intents, workflow | P1 | XL | Lumina, soft on Content | [phases/events/](phases/events/ROADMAP.md) |
| **Voice** | Voice pipeline, all backends | P0 | XL | Lumina | [phases/voice/](phases/voice/ROADMAP.md) |
| **Discord** | Remaining cogs, rich UI | P2 | M | Content, Events | [phases/discord/](phases/discord/ROADMAP.md) |

---

## Parallelization

```
Timeline ──────────────────────────────────────────────────────────────────────────►

Foundation  ██████████████

Lumina                    ██████████████

Content                                  ██████████████████

Events                                   ██████████████████████████████████

Voice                                    ████████████████████████

Discord                                                            ████████████████
```

**Maximum parallelism:** After Lumina, four workstreams can advance independently. Voice library can optionally start during Lumina since it's a standalone crate with no core service dependency.
