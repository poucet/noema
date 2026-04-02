# Design: UCM as Core vs. MCP Service

**Status:** Open question
**Affects:** Content phase, daemon architecture

---

## Question

Should UCM storage be a core part of the daemon, or a separate MCP service that the daemon connects to?

---

## Option A: UCM in Daemon (current design)

Daemon owns `simply-core` directly. Documents, storage, and RAG are built-in.

**Pros:**
- Simple deployment (one process)
- No serialization overhead for doc operations
- Transactions span agent + storage naturally

**Cons:**
- Daemon grows in responsibility (agent orchestration + storage + indexing + RAG)
- Can't scale storage/indexing independently
- Harder to swap storage backends

---

## Option B: UCM as MCP Service

UCM runs as a standalone service. Daemon connects to it like any other MCP service. Exposes tools: `create_document`, `query_documents`, `search`, etc.

```
┌──────────┐     MCP      ┌──────────────┐
│  Daemon   │◄────────────►│  UCM Service  │
│ (hub)     │              │  storage      │
└──────────┘              │  indexing     │
     ▲  ▲                 │  RAG/search   │
     │  │                 └──────────────┘
     │  └── MCP ──► Lumina (Discord)
     └───── WS ──── Noema (desktop)
```

**Pros:**
- Daemon stays thin: session/agent orchestration only
- UCM is independently deployable, scalable, testable
- RAG, embeddings, indexing can run on different hardware
- Consistent pattern: everything is an MCP service (Discord = Lumina, storage = UCM, etc.)
- Other agents/services can connect to UCM directly without going through daemon

**Cons:**
- More moving parts to deploy
- Network hop for every doc operation
- Need to solve: does the daemon cache documents? Or always fetch from UCM service?

---

## Hybrid: Core storage, MCP for heavy lifting

Daemon keeps basic document CRUD in-process (fast path). RAG, indexing, and embeddings run as a separate MCP service that watches for changes.

```
┌──────────────────┐     MCP      ┌──────────────┐
│  Daemon           │◄────────────►│  Index Service │
│  ├─ simply-core   │              │  embeddings    │
│  │  └─ doc CRUD   │              │  search        │
│  ├─ agent         │              └──────────────┘
│  └─ sessions      │
└──────────────────┘
```

**Pros:**
- Doc CRUD stays fast (no network hop)
- Heavy computation (embeddings, RAG) offloaded
- Simpler migration path from current design

**Cons:**
- Two places that know about documents
- Index service needs to be notified of changes

---

## Decision

TBD — needs discussion. Key question: does the "daemon is a thin hub" principle extend to storage, or is storage fundamental enough to be core?
