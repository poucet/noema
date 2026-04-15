# Simply Platform Architecture

**Status:** Draft
**Version:** 1.0
**Previous:** [Noema 0.2 Architecture](obsolete/ARCHITECTURE-0.2.md)

---

## Vision

Simply is a unified AI platform where Noema (desktop), Lumina (Discord), and future clients (Telegram, WebRTC) share a common core service for LLM orchestration, voice, storage, and reactive automation.

### Guiding Principles

1. **Local-first**: Data lives on your machine. Cloud is opt-in.
2. **Content is immutable**: Text and assets are stored once, referenced many times.
3. **Structure is mutable**: Conversations, documents, views can reorganize without moving content.
4. **Everything is addressable**: @mention any entity. Fork, reference, organize.
5. **Code provides capabilities, content provides configuration**: Event sources and action handlers are code. All routing, wiring, and workflow is content (UCM documents).

---

## Platform Architecture

```
  Rich Clients (WebSocket + JSON)        Integration Services
  ┌─────────────┐ ┌─────────────┐        ┌──────────────┐  ┌──────────┐
  │ Noema       │ │ Lumina      │        │ github       │  │ shell    │
  │ (Tauri)     │ │ (serenity)  │        │ watcher      │  │ script   │
  │ React ──ws──┤ │ Discord ────┤        │ MCP ◄── daemon│  │          │
  │ Tauri (IPC) │ │ Songbird    │        │ REST ──► daemon│ │ REST ──► │
  └──────┬──────┘ └──────┬──────┘        └──────────────┘  └──────────┘
         │ WebSocket      │ WebSocket     REST inbound ▲    REST ▲
         │                │               MCP outbound │         │
   ┌─────▼────────────────▼───────────────────────────▼──────────▼──┐
   │                      simply-daemon                              │
   ├─ WebSocket server — rich client sessions                       │
   ├─ REST server — trigger events, service registration            │
   ├─ MCP client — connects to registered action services           │
   ├─ Session manager (in-memory, optionally UCM-backed)            │
   ├─ Peer registry (clients + services, capabilities, liveness)    │
   ├─ Global MCP tool registry (shared across all sessions)         │
   ├─ simply-core (internal library)                                │
   │   ├─ LLM providers (Claude, OpenAI, Gemini, Mistral, Ollama)  │
   │   ├─ MCP server/client                                        │
   │   └─ Agent orchestration                                      │
   ├─ UCM storage (SQLite, blobs)                                   │
   ├─ Event bus + intent engine                                     │
   └─ Voice pipeline                                                │
   └────────────────────────────────────────────────────────────────┘
```

**Key distinctions:**

- **simply-daemon** is the hub. Three interfaces: WebSocket (rich clients), REST (triggers), MCP outbound (action services). See [CORE_SERVICE.md](CORE_SERVICE.md) for the full protocol.
- **simply-core** is a library crate internal to the daemon. LLM providers, MCP, agent orchestration. No external crate depends on it.
- **Rich clients** (Noema, Lumina) connect via WebSocket + JSON. They seed conversation context, register MCP tools and event sources, and receive streamed agent responses. Noema's React frontend talks directly to the daemon — Tauri handles OS-level concerns only.
- **Trigger services** push events via REST. As simple as a curl one-liner. No persistent connection needed.
- **Action services** expose MCP servers. The daemon connects to them, discovers tools, and calls them when needed. Services register dynamically via REST.
- **All MCP tools are globally shared.** A Droplets service registered from Noema is available to Lumina sessions too. Platform-specific tools (Discord actions) are also global — action routing defers if the platform is unavailable.
- **Conversations are sessions** — ephemeral (in-memory) or persistent (UCM-backed), toggleable at runtime per conversation.
- **Service lifecycle is an event source.** `service.connected`, `service.disconnected` flow into the intent engine like any other event.

---

## Workspace Structure

```
simply-{name}/                     # Renamed from noema
├── Cargo.toml                     # Workspace manifest
├── simply-core/                   # Internal library: LLM + MCP + agent (only simply-daemon depends on this)
│   ├── src/
│   │   ├── agent.rs               # Agent orchestration
│   │   ├── mcp/                   # MCP server/client
│   │   └── llm/                   # LLM providers (Claude, OpenAI, Gemini, Mistral, Ollama)
│   └── Cargo.toml
├── simply-daemon/                # The hub: wires core with storage, WebSocket/REST/MCP, events, voice
│   ├── src/
│   │   ├── main.rs                # Daemon entry point
│   │   ├── ws.rs                  # WebSocket server (rich clients)
│   │   ├── rest.rs                # REST server (triggers, registration)
│   │   ├── sessions/              # Session manager (ephemeral + persistent)
│   │   ├── registry.rs            # Peer registry + global MCP tool registry
│   │   ├── storage/               # UCM storage (SQLite, blobs)
│   │   ├── events/                # Event bus + intent engine
│   │   └── voice/                 # Voice pipeline coordination
│   └── Cargo.toml
├── simply-voice/                  # Voice provider abstraction
│   ├── src/
│   │   ├── traits.rs              # STT/TTS provider traits
│   │   ├── providers/             # Voxtral, (future: ElevenLabs, OpenAI, etc.)
│   │   ├── vad.rs                 # Voice activity detection
│   │   └── pipeline.rs            # Audio processing pipeline
│   └── Cargo.toml
├── simply-audio/                  # Audio backends
│   ├── src/
│   │   ├── cpal_backend.rs        # Desktop mic/speaker (Noema)
│   │   ├── browser_backend.rs     # Web audio (future)
│   │   └── traits.rs              # Backend abstraction
│   └── Cargo.toml
├── lumina/                        # Discord bot — WebSocket client to simply-daemon
│   ├── src/
│   │   ├── main.rs                # Entry point, WebSocket + Discord gateway setup
│   │   ├── cogs/                  # Discord slash commands (serenity #[command])
│   │   ├── voice/                 # Songbird integration, audio I/O bridge
│   │   └── context.rs             # Seeds conversation context from Discord channel history
│   └── Cargo.toml
├── noema-desktop/                 # Desktop presentation layer (Tauri)
│   ├── src-tauri/                 # Rust backend — Tauri IPC for OS-level (slash cmds, file access)
│   ├── src/                       # React frontend — WebSocket to simply-daemon for chat
│   └── Cargo.toml
├── noema-ext/                     # Extensions (PDF, attachments)
├── commands/                      # Command framework
├── config/                        # Configuration
└── docs/
```

---

## Unified Content Model (UCM)

The three-layer UCM from Noema 0.2 is the storage foundation for all platforms. See [UNIFIED_CONTENT_MODEL.md](UNIFIED_CONTENT_MODEL.md) for the full specification.

```
┌─────────────────────────────────────────────────────────────┐
│                    ADDRESSABLE LAYER                        │
│  Unified identity, naming, and relationships                │
│  entities + entity_relations                                │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                    STRUCTURE LAYER                          │
│  Domain-specific organization                               │
│                                                             │
│  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────┐ │
│  │  Conversations  │  │    Documents    │  │ Collections │ │
│  │  views, turns,  │  │  tabs, revisions│  │ tree, tags  │ │
│  │  spans, messages│  │                 │  │             │ │
│  └─────────────────┘  └─────────────────┘  └─────────────┘ │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                     CONTENT LAYER                           │
│  Immutable content with origin tracking                     │
│  content_blocks (text) + assets/blobs (binary)             │
└─────────────────────────────────────────────────────────────┘
```

### Features on UCM — Content as Convention

All features map onto UCM primitives. **Structured metadata lives in content as frontmatter, not in code as typed fields.** Most "features" are documents with different frontmatter conventions — no dedicated service needed.

```markdown
# A "todo" is just a UCM document:
---
type: todo
done: false
due: 2026-04-15
labels: [lumina, urgent]
assignee: chris
---
Pick up groceries for the team dinner
```

**Why frontmatter over typed fields in code:**
- **Future-proof** — new metadata fields don't require schema migrations or code changes
- **LLM-native** — models can read/write frontmatter natively, no serialization layer
- **Reduces services** — the core needs generic document CRUD + frontmatter-aware queries, not per-feature services

**Feature mapping:**

| Feature | UCM Primitive | Frontmatter Convention |
|---------|--------------|----------------------|
| **Notes** | `Document` | `type: note`, `tags: [...]` |
| **TODOs** | `Document` | `type: todo`, `done: bool`, `due: date`, `labels: [...]`, `assignee: str` |
| **Context / Memory** | `Document` | `type: context`, `project: str`, `goals: [...]`, `energy: str` |
| **MCP Server Config** | `Document` | `type: mcp_server`, `url: str`, `enabled: bool` |
| **Access Control** | `Document` | `type: access_rule`, `role: str`, `level: str` |
| **RAG Knowledge** | `Document` | `type: knowledge`, `source: url/pdf/gdoc`, `ingested_at: date` |
| **Intents** | `Document` + execution table | Content in UCM, execution state in lightweight table |
| **Identity** | `Entity` | Needs platform linking logic — entity relations, not just frontmatter |
| **Tool Usage / Brain** | `Turn` metadata | Tool calls already live in turns. Analytics = queries over turn data. |

**What still needs dedicated services:**

```
simply-daemon/src/services/
├── documents.rs      # Generic document CRUD with frontmatter-aware queries
├── events.rs         # Event bus — sources, routing, subscriptions
├── intents.rs        # Intent engine — matching, execution, chaining
├── identity.rs       # Platform linking, entity relations, role resolution
├── brain.rs          # Analytics/aggregation queries over turn data
├── search.rs         # Unified search/RAG — embeddings over all UCM content
└── voice.rs          # Voice pipeline orchestration
```

Only features with **active behavior** (event processing, intent execution, identity resolution, voice orchestration) or **complex queries** (analytics, RAG search) need dedicated services. Pure CRUD features (notes, todos, contexts, configs) are just document conventions.

---

## Design Documents

| Document | Purpose |
|----------|---------|
| [UNIFIED_CONTENT_MODEL.md](UNIFIED_CONTENT_MODEL.md) | UCM three-layer architecture (detailed specification) |
| [STORAGE.md](STORAGE.md) | UCM database schema reference |
| [EMBEDDING_AND_RAG.md](EMBEDDING_AND_RAG.md) | Embedding providers, vector storage, retrieval API, Lumina RAG integration |
| [proposals/ACTIONS.md](proposals/ACTIONS.md) | Action system — unified capability primitive, composition chains, MCP projection (proposal) |
| [proposals/AGENTIC.md](proposals/AGENTIC.md) | Event & Intent engine — triggers, Action AST, service registry, engine loop (proposal) |
| [CORE_SERVICE.md](CORE_SERVICE.md) | Daemon communication — WebSocket, REST, MCP interfaces |
| [VOICE.md](VOICE.md) | Voice pipeline architecture — providers, backends, orchestration |
| [obsolete/ARCHITECTURE-0.2.md](obsolete/ARCHITECTURE-0.2.md) | Previous Noema 0.2 architecture |
| [obsolete/HOOK_SYSTEM.md](obsolete/HOOK_SYSTEM.md) | Previous hook system design (superseded by Agentic System) |
| [../FUTURE_ROADMAP.md](../FUTURE_ROADMAP.md) | Future feature roadmap |
| [../v1.0/GOAL.md](../v1.0/GOAL.md) | v1.0 design goals and decisions |
| [../v1.0/ROADMAP.md](../v1.0/ROADMAP.md) | v1.0 phased implementation roadmap |
