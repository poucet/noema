# Simply Platform v1.0 — Roadmap

**Design:** [GOAL.md](GOAL.md)
**Architecture:** [designs/ARCHITECTURE.md](../designs/ARCHITECTURE.md)
**Post-v1:** [FUTURE_ROADMAP.md](../FUTURE_ROADMAP.md)

---

## Overview

v1.0 is organized into phases. Foundation restructures the workspace and extracts the daemon. Lumina adds the minimal Discord bot. After that, Content, Events, Voice, RTC, and Discord run **in parallel** as independent workstreams.

Each phase has its own detailed roadmap in `phases/`.

```
Foundation (sequential)               Lumina (sequential)
  Crate rename → daemon        →      Lumina crate → shared LLM chat
                                                │
                    ┌───────────────┬────────────┼────────────────┐
                    ▼               ▼            ▼                ▼
              Content       Events        Voice         RTC            Discord
              MCP + docs    Event bus     simply-voice  Action service Remaining cogs
              Frontmatter   Intents       Desktop       WebRTC audio   Embeds, admin
                    │       AST+registry  Discord       Transcription
                    └─feeds─►Platform events
                              Conditions
```

---

## Phases

| Phase | Name | Priority | Complexity | Depends On | Roadmap |
|-------|------|----------|------------|------------|---------|
| **Foundation** | Crate renames + simply-daemon daemon | P0 | L | — | [phases/foundation/](phases/foundation/ROADMAP.md) |
| **Lumina** | Minimal Discord bot + shared LLM | P0 | M | Foundation | [phases/lumina/](phases/lumina/ROADMAP.md) |
| **Content** | MCP, document CRUD, frontmatter | P0 | M | Lumina | [phases/content/](phases/content/ROADMAP.md) |
| **Events** | Event bus, intents, workflow | P1 | XL | Lumina, soft on Content | [phases/events/](phases/events/ROADMAP.md) |
| **Voice** | Voice pipeline, desktop + Discord | P0 | L | Lumina | [phases/voice/](phases/voice/ROADMAP.md) |
| **RTC** | WebRTC action service | P1 | L | Voice Stage 2 | [phases/rtc/](phases/rtc/ROADMAP.md) |
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

RTC                                                  ██████████████████

Discord                                                            ████████████████
```

**Maximum parallelism:** After Lumina, five workstreams can advance independently. Voice library can optionally start during Lumina since it's a standalone crate. RTC starts after Voice Stage 2 (daemon voice API).
