# Simply Platform v1.0 — Roadmap

**Design:** [GOAL.md](GOAL.md)
**Architecture:** [designs/ARCHITECTURE.md](../designs/ARCHITECTURE.md)
**Post-v1:** [FUTURE_ROADMAP.md](../FUTURE_ROADMAP.md)

---

## Overview

v1.0 is organized into phases. Foundation restructures the workspace and extracts the daemon. Lumina is the full Discord bot port (crate setup through all cogs). After that, Content, Events, Voice, and RTC run **in parallel** as independent workstreams. The former Discord phase is merged into Lumina.

Each phase has its own detailed roadmap in `phases/`.

```
Foundation (Stages 1-2 sequential, Stage 3 parallel)
  Crate rename → daemon        →      Lumina (12 stages)
  REST-first transport ─────────┐     Crate → Chat → Admin → Todo/Notes → Schedule
                                │             │       → Voice → RAG → Brain → MCP
                    ┌───────────┴───┬────────────┤       → Context → Google → Server
                    ▼               ▼            ▼
              Content       Events        Voice         RTC
              MCP + docs    Event bus     simply-voice  Action service
              Frontmatter   Intents       Desktop       WebRTC audio
                    │       AST+registry  Discord       Transcription
                    └─feeds─►Platform events
                              Conditions
```

---

## Phases

| Phase | Name | Priority | Complexity | Depends On | Roadmap |
|-------|------|----------|------------|------------|---------|
| **Foundation** | Crate renames + simply-daemon + REST-first transport | P0 | L | — | [phases/foundation/](phases/foundation/ROADMAP.md) |
| **Lumina** | Full Discord bot (all cogs) | P0 | L | Foundation | [phases/lumina/](phases/lumina/ROADMAP.md) |
| **Content** | MCP, document CRUD, frontmatter | P0 | M | Lumina Stage 2 | [phases/content/](phases/content/ROADMAP.md) |
| **Events** | Event bus, intents, workflow | P1 | XL | Lumina Stage 2, soft on Content | [phases/events/](phases/events/ROADMAP.md) |
| **Voice** | Voice pipeline, desktop + Discord | P0 | L | Lumina Stage 2 | [phases/voice/](phases/voice/ROADMAP.md) |
| **RTC** | WebRTC action service | P1 | L | Voice Stage 2 | [phases/rtc/](phases/rtc/ROADMAP.md) |

---

## Parallelization

```
Timeline ──────────────────────────────────────────────────────────────────────────►

Foundation  ██████████████       ████████ (Stage 3: REST-first, parallel with Lumina)

Lumina                    ██████████████████████████████████████████████████████████

Content                                  ██████████████████

Events                                   ██████████████████████████████████

Voice                                    ████████████████████████

RTC                                                  ██████████████████
```

**Maximum parallelism:** Foundation Stage 3 (REST-first transport) runs in parallel with Lumina — zero API surface change means no coordination needed. After Lumina Stage 2, four workstreams can advance independently. Voice library can optionally start during Lumina since it's a standalone crate. RTC starts after Voice Stage 2 (daemon voice API).
