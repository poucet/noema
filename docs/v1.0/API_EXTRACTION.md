# API Extraction & Daemon Cleanup Plan

**Date:** 2026-04-18
**Status:** Ready to implement

## Goal

Decouple daemon API traits from implementations so that:
- Skills depend on API traits without pulling in full daemon
- Lumina can build remote-only (no embedded daemon)
- EmbeddedDaemon is thin assembly, not service constructor

## Architecture

```
simply-core  →  simply-daemon-api  →  simply-daemon (implementations)
                     ↑                      ↑
                     ├── mcp-gdocs/skill    |
                     ├── lumina (remote)    lumina (embedded, optional)
                     └── other skills
```

## Steps

### 1. Create simply-daemon-api subcrate
- Move `simply-daemon/src/api/*.rs` → `simply-daemon/api/src/`
- Move API types (SessionId, DaemonEvent, ConversationInfo, etc.)
- Keep `#[rpc_service]` annotations
- ts-rs conditional behind `ts` feature
- `simply-daemon/src/api/mod.rs` → `pub use simply_daemon_api::*;`
- `EmbeddingQueueStatus` moves to API types

### 2. Clean up EmbeddedDaemon
- Takes pre-built services, not raw stores
- Voice construction → `main.rs` / helper
- Skills passed as `Vec<Arc<dyn Skill>>`
- Service construction = standalone function

### 3. Lumina feature flags
- `embedded` feature (default): full daemon
- `remote-only`: just `simply-daemon-api`

### 4. Skills use API traits
- GDocsSkill takes `Arc<dyn DocumentApi>` + `Arc<dyn AssetApi>`
- No more `SkillDocumentApi` / `SkillAssetApi` adapters

### 5. GDocsSkill loaded on demand
- `mcp-gdocs` skill feature depends on `simply-daemon-api`
- Daemon + Lumina construct and register at startup

## Files to Move
All `simply-daemon/src/api/*.rs` except `mod.rs` (which becomes re-export).
`EmbeddingQueueStatus` from `embedding_queue.rs` to API types.

## Files That Stay
All `*Service` impls, EmbeddedDaemon, RemoteDaemon, networking, tools.
