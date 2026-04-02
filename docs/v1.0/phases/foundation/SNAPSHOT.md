# Foundation Phase — Snapshot

**Updated:** 2026-04-02
**Stage:** 2 (Daemon)

---

## Open Design Questions

### OAuth callback server port stability
The callback server (`simply-daemon/src/oauth/callback.rs`) binds to a random local port. If Lumina runs in the cloud and needs OAuth, the callback needs a stable port or an alternative redirect strategy (fixed URL, reverse proxy).

### Voice recognition -> daemon
Voice recognition lives in Noema (`simply-audio` + `VoiceCoordinator` on `AppState`). Should move into the daemon so any client can use it. `VoiceApi` trait is stubbed and waiting.

### gdocs (task 2.3.3)
`noema/src-tauri/src/commands/gdocs.rs` still uses `EmbeddedDaemon` accessor escape hatches (`stores()`, `coordinator()`). Needs a `DocumentApi` trait or should be wired through existing daemon APIs. P1 — not blocking.
