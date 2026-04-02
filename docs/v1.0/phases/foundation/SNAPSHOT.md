# Foundation Phase — Snapshot

**Updated:** 2026-04-02
**Stage:** 2 (Daemon)

---

## Open Design Questions

### Voice recognition -> daemon
Voice recognition lives in Noema (`simply-audio` + `VoiceCoordinator` on `AppState`). Should move into the daemon so any client can use it. `VoiceApi` trait is stubbed and waiting.

### gdocs accessor escape hatches
`EmbeddedDaemon` still exposes `stores()`, `coordinator()`, `mcp_registry()` for gdocs. These should be removed once DocumentApi (2.5) and gdocs rewrite (2.5.1) are done.
