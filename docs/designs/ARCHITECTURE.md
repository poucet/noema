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
Noema backend (Tauri)          Lumina (serenity)         Future: /meet, Telegram, etc.
├─ UCM view management         ├─ Discord gateway        ├─ WebRTC / platform gateway
├─ Document/tab orchestration  ├─ Slash commands          ├─ Platform-specific UI
├─ Frontend API (Tauri IPC)    ├─ Songbird voice I/O     └─ calls simply-core ──┐
├─ OAuth flows                 ├─ Channel management                            │
└─ calls simply-core ──────┐   └─ calls simply-core ──┐                        │
                           │                           │                        │
                     ┌─────▼───────────────────────────▼────────────────────────▼──┐
                     │                    simply-core service                       │
                     ├─ LLM providers (Claude, OpenAI, Gemini, Mistral, Ollama)    │
                     ├─ MCP server/client                                          │
                     ├─ Voice pipeline (Voxtral STT/TTS, future: ElevenLabs, etc.) │
                     ├─ Agent orchestration & context                               │
                     ├─ Event bus + intent engine                                   │
                     ├─ Storage (SQLite, blobs) — single writer                    │
                     └─────────────────────────────────────────────────────────────┘
```

**Key insight:** Presentation layers are NOT thin clients. Noema retains UCM management, document orchestration, and web-friendly frontend API. Lumina owns Discord gateway, slash commands, and songbird audio I/O. Each delegates *shared concerns* (LLM, voice, storage, MCP, events) to the core service.

---

## Workspace Structure

```
simply-{name}/                     # Renamed from noema
├── Cargo.toml                     # Workspace manifest
├── simply-core/                   # Shared core service (daemon)
│   ├── src/
│   │   ├── service.rs             # gRPC/Unix socket service API
│   │   ├── agent.rs               # Agent orchestration
│   │   ├── mcp/                   # MCP server/client
│   │   ├── storage/               # UCM storage (SQLite, blobs)
│   │   ├── events/                # Event bus + intent engine
│   │   └── voice/                 # Voice pipeline coordination
│   └── Cargo.toml
├── simply-llm/                    # LLM provider abstraction
│   ├── src/
│   │   ├── api.rs                 # Core types (ChatMessage, Role, ToolCall, etc.)
│   │   ├── client.rs              # HTTP client
│   │   ├── providers/             # Claude, OpenAI, Gemini, Mistral, Ollama
│   │   ├── registry.rs            # Model registration
│   │   └── tools.rs               # Tool definitions
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
├── lumina/                        # Discord bot presentation layer
│   ├── src/
│   │   ├── main.rs                # Entry point, service wiring
│   │   ├── cogs/                  # Discord slash commands (serenity #[command])
│   │   ├── voice/                 # Songbird integration, audio I/O bridge
│   │   ├── mcp/                   # Lumina-specific MCP handlers
│   │   └── core_client.rs         # Client to simply-core service
│   └── Cargo.toml
├── noema-desktop/                 # Desktop presentation layer (Tauri)
│   ├── src-tauri/                 # Rust backend
│   ├── src/                       # React frontend
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
simply-core/src/services/
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
| [AGENTIC.md](AGENTIC.md) | Event & Intent engine — triggers, Action AST, service registry, engine loop |
| [CORE_SERVICE.md](CORE_SERVICE.md) | Core service communication — MCP + gRPC interfaces |
| [VOICE.md](VOICE.md) | Voice pipeline architecture — providers, backends, orchestration |
| [obsolete/ARCHITECTURE-0.2.md](obsolete/ARCHITECTURE-0.2.md) | Previous Noema 0.2 architecture |
| [obsolete/HOOK_SYSTEM.md](obsolete/HOOK_SYSTEM.md) | Previous hook system design (superseded by Agentic System) |
| [../FUTURE_ROADMAP.md](../FUTURE_ROADMAP.md) | Future feature roadmap |
| [../v1.0/GOAL.md](../v1.0/GOAL.md) | v1.0 design goals and decisions |
| [../v1.0/ROADMAP.md](../v1.0/ROADMAP.md) | v1.0 phased implementation roadmap |
