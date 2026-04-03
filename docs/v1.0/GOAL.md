# Design 1: Simply Platform — Rust Unification

**Status**: in progress
**Features:** [FEATURES.md](FEATURES.md)
**Roadmap:** [ROADMAP.md](ROADMAP.md)
**Architecture:** [designs/ARCHITECTURE.md](../designs/ARCHITECTURE.md)

---

## Problem

Lumina (Python Discord bot) and Noema (Rust desktop AI assistant) are converging on the same needs — LLM orchestration, MCP tools, voice pipeline, storage — but implemented independently in different languages. This creates:

1. **Duplicated core logic** — both projects implement LLM provider abstraction, MCP integration, agent orchestration, and voice processing separately.
2. **Python voice limitations** — Python's Discord ecosystem lags on DAVE (Discord Audio Visual Encryption) protocol support, blocking reliable STT/TTS in voice channels.
3. **Platform lock-in** — Lumina's features are locked to Discord/Python. Noema's are locked to desktop/Tauri. Neither can easily extend to new platforms.
4. **Storage contention** — if both share data, two separate processes writing to the same SQLite is fragile.
5. **Maintenance burden** — maintaining two codebases with overlapping concerns in two languages.

## Goals

- **Unify Noema and Lumina** into a single Rust workspace where they share a common core and differ only in presentation layer.
- **Shared daemon** (`simply-daemon`) that owns LLM, MCP, voice, agent orchestration, and storage — runs as a long-lived process. `simply-core` is its internal library.
- **Lumina as a crate** in the Noema workspace — a Discord bot (serenity + songbird) that connects to the core service.
- **Voice provider abstraction** in the core (starting with Voxtral/Mistral) — usable by both Noema (desktop mic via CPAL) and Lumina (Discord via songbird).
- **Architecture supports future platforms** (Telegram, WhatsApp, WebRTC/meet) without building them in v1.

## Non-goals (v1)

- Telegram, WhatsApp, or other messaging platform integrations.
- `simply-chris.ai/meet` WebRTC product (architecture supports it, doesn't build it).
- Unified command macro (single annotation for both Discord + MCP) — use serenity's native `#[command]` and separate MCP tool definitions.
- Google services integration migration.
- Full feature parity with Python Lumina — v1 focuses on Discord text commands + voice with DAVE.

---

## Resolved Questions

1. **Core service protocol** — Three interfaces: WebSocket + JSON for rich clients (Noema, Lumina), REST for trigger services, MCP outbound for action services. See [CORE_SERVICE.md](../designs/CORE_SERVICE.md).
2. **Storage model** — Lumina features map onto UCM primitives. No separate databases. See [ARCHITECTURE.md](../designs/ARCHITECTURE.md#features-on-ucm--content-as-convention).
3. **Command system** — Separate: serenity `#[command]` for Discord, separate MCP tool definitions. No unified macro.

## Open Questions

1. **Repo name** — keeping `noema` as the GitHub repo for now.
2. **Songbird DAVE status** — need to verify songbird's current DAVE protocol support. If incomplete, may need to contribute upstream or work around.
3. ~~**Config unification**~~ — Resolved: shared `config/` crate with `Settings::load()` + `.env` fallback.
4. ~~**UCM schema extensions**~~ — Resolved: existing UCM primitives cover it. Content conventions (todo, note, etc.) are just frontmatter on documents.
5. ~~**Core service lifecycle**~~ — Resolved: `simply-daemon` runs as standalone binary. Noema uses `EmbeddedDaemon` (in-process). Lumina uses `RemoteDaemon` (WS+REST).

---

## Related

- Supersedes: Praxis CRUD side-car design (Python-era, no longer applicable)
- Architecture: [designs/ARCHITECTURE.md](../designs/ARCHITECTURE.md)
- Post-v1 Roadmap: [FUTURE_ROADMAP.md](../FUTURE_ROADMAP.md)
