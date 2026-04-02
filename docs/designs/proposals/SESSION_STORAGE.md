# Design: Decouple Sessions from Storage

**Status:** Open
**Affects:** simply-core, ConversationManager, Session, daemon embedded

---

## Problem

Currently `Session` and `ConversationManager` require a `ConversationId` and `StorageCoordinator` at construction — even for ephemeral sessions that never persist anything. The storage layer is baked into the session lifecycle (commit happens inside the agent turn).

This means:
- Ephemeral sessions create a throwaway `ConversationId` that's never used
- The session/manager code has storage concerns mixed with runtime concerns
- Can't have a lightweight session without the full storage stack

---

## Proposed Architecture

Sessions are pure runtime objects identified by `SessionId`. Persistence is an optional subscriber.

```
┌──────────────────────┐
│  Session (runtime)    │ ← identified by SessionId only
│  ├─ pending messages  │
│  ├─ LLM cache        │
│  └─ event stream      │
└──────────┬───────────┘
           │ events
           ▼
┌──────────────────────┐
│  StorageSubscriber    │ ← optional, subscribes to session events
│  (persistent only)    │
│  ├─ ConversationId    │
│  ├─ StorageCoordinator│
│  └─ commits on turn   │
└──────────────────────┘
```

**Session** owns:
- Message buffer (pending + resolved for LLM context)
- Event broadcast channel
- Model reference

**StorageSubscriber** owns:
- ConversationId mapping
- Commit logic (write turns to storage)
- Subscribes to the session's event stream

**Benefits:**
- Ephemeral sessions have zero storage overhead
- Sessions are testable without a database
- Storage logic is isolated and swappable
- Clean separation: session = runtime, subscriber = persistence

---

## Migration Path

1. Extract commit logic from `ConversationManager::run_agent_and_commit` into a separate subscriber
2. Make `Session::new()` not require `StorageCoordinator`
3. `ConversationManager` becomes the runtime agent loop only
4. For persistent sessions, spawn a `StorageSubscriber` that listens to events and commits

---

## Open Questions

1. How does the subscriber get notified? Via the existing broadcast channel, or a dedicated storage channel?
2. Should the subscriber be async (commits in background) or sync (blocks the turn)?
3. How does `resume_session` work — subscriber reconstructs the session from storage?
