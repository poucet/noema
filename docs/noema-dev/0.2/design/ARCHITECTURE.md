# Noema 0.2 Architecture

**Status:** Active Development
**Version:** 0.2

---

## Vision

Noema is a local-first AI assistant with enterprise-grade capabilities. Version 0.2 establishes the **Unified Content Model (UCM)** - a foundation for advanced conversation features, document management, and future automation.

### Guiding Principles

1. **Local-first**: Data lives on your machine. Cloud is opt-in.
2. **Content is immutable**: Text and assets are stored once, referenced many times.
3. **Structure is mutable**: Conversations, documents, views can reorganize without moving content.
4. **Everything is addressable**: @mention any entity. Fork, reference, organize.
5. **Hooks, not hardcoding**: Behavior is data-driven and extensible.

---

## Three-Layer Architecture

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

### Layer Responsibilities

| Layer | What It Does | Key Insight |
|-------|--------------|-------------|
| **Addressable** | Identity, naming (@slug), relationships | Views ARE conversations |
| **Structure** | Organization within domain | Cheap to reorganize |
| **Content** | Store text/binary once | Provenance tracked |

---

## Core Concepts

### Views as Conversations

"Conversations" in the UI are really **views** with metadata. A view is a path through a sequence of turns, selecting one span at each position.

```
View A: [turn1:span1] → [turn2:span1] → [turn3:span1]
                                              ↗
View B: [turn1:span1] → [turn2:span1] → [turn3:span2]  ← different path
```

**Benefits**:
- Fork from any point (new view, shared history)
- Edit mid-conversation (new span, same turn)
- Compare model outputs (multiple spans at turn)
- Promote fork to standalone conversation (just rename)

### Spans as Autonomous Flows

A **span** is one party's complete response - potentially multiple messages.

```
Turn 3 (assistant):
  ├── Span A: [thinking] → [tool_call] → [tool_result] → [response]
  ├── Span B: [response]  ← different model, fewer steps
  └── Span C: [tool_call] → [response]
```

This enables:
- Parallel model comparison (same prompt, different spans)
- Tool iterations (all steps in one span)
- Regeneration (new span, same turn)

### Content with Origin

Every piece of text knows where it came from:

```rust
ContentOrigin {
    kind: user | assistant | system | import,
    user_id: Option<UserId>,
    model_id: Option<ModelId>,
    source_id: Option<String>,      // external ID
    parent_content_id: Option<Id>,  // if derived
}
```

Content is NOT deduplicated by hash - each block has unique provenance even if text matches.

---

## Crate Structure

```
noema/
├── noema-core/           # Core logic
│   ├── src/
│   │   ├── storage/      # UCM storage layer
│   │   │   ├── traits/   # Store interfaces
│   │   │   ├── types/    # Data types
│   │   │   ├── implementations/
│   │   │   │   ├── sqlite/   # Production
│   │   │   │   ├── memory/   # Testing
│   │   │   │   └── mock/     # Unit tests
│   │   │   └── session/  # Session management
│   │   ├── engine.rs     # Chat engine
│   │   └── providers/    # LLM providers
│   └── migrations/       # SQLite migrations
│
├── noema-desktop/        # Tauri desktop app
│   ├── src/              # Rust backend
│   └── ui/               # React frontend
│
├── noema-mcp/            # MCP server implementations
├── config/               # Configuration crate
└── docs/                 # Documentation
```

---

## Data Flow

### Message Send

```
User Input
    │
    ▼
┌─────────────┐
│   Session   │ ← manages view state
└─────────────┘
    │
    ├── commit() → ContentBlockStore.store() → content_blocks
    │            → TurnStore.add_message() → messages
    │
    ▼
┌─────────────┐
│ ChatEngine  │ ← LLM orchestration
└─────────────┘
    │
    ├── messages_for_llm() → resolve content, assets
    │
    ▼
┌─────────────┐
│  Provider   │ ← Anthropic, OpenAI, Ollama, etc.
└─────────────┘
    │
    ▼
Response Stream
    │
    ▼
Session.commit() → new span with messages
```

### Fork Operation

```
fork(view_id, at_turn_id)
    │
    ├── EntityStore.create() → new entity
    ├── TurnStore.create_view() → new view
    ├── TurnStore.copy_selections(up_to_turn) → shared history
    └── EntityStore.add_relation(forked_from) → track ancestry
```

---

## Storage Traits

All storage operations go through trait interfaces:

| Trait | Responsibility |
|-------|---------------|
| `EntityStore` | Entity CRUD, relations, @mentions |
| `ContentBlockStore` | Text storage with origin |
| `AssetStore` | Binary metadata |
| `BlobStore` | Binary content (filesystem) |
| `TurnStore` | Turns, spans, messages, views |
| `DocumentStore` | Documents, tabs, revisions |
| `UserStore` | User management |

### Implementation Strategy

```rust
pub trait StorageTypes: Clone + Send + Sync + 'static {
    type Blob: BlobStore;
    type ContentBlock: ContentBlockStore;
    type Asset: AssetStore;
    type Turn: TurnStore;
    type Document: DocumentStore;
    type User: UserStore;
    type Entity: EntityStore;
}

// Production: SqliteStorageTypes
// Testing: MemoryStorageTypes
// Mocking: MockStorageTypes
```

---

## Phase Roadmap

### Phase 3: Unified Content Model (Current)

**Status**: Core complete, manual testing in progress

| Feature | Status | Description |
|---------|--------|-------------|
| 3.1 Content Blocks | Done | Text storage with origin tracking |
| 3.1b Asset Storage | Done | Binary blob storage |
| 3.2 Conversation Structure | Done | Turns, spans, messages |
| 3.3 Views and Forking | Done | Views, fork, entity layer |
| 3.3b Subconversations | Done | Agent spawn/link |
| 3.4 Document Structure | Done | Tabs, revisions |
| 3.45 Manual Testing | **Current** | Verify all features |
| 3.5 Collections | Planned | Tree organization, tags |
| 3.6 Cross-References | Planned | Entity linking, backlinks |
| 3.7 Temporal Queries | Planned | Time-based activity |

### Phase 4: Content Model Features

- Undo delete (soft delete)
- Per-conversation system prompts
- Auto-naming via summarizer
- Document editing with revisions

### Phase 5: Organization + Search

- Semantic search with embeddings
- Hierarchical tags
- Wiki-style cross-linking
- Custom skills and slash commands

### Phase 6: RAG + Memories

- Conversation memories
- Full RAG pipeline
- MCP coding agent tools

### Phase 7: Agentic + Multimodal

- Multi-agent conversations
- Audio models (STT/TTS)
- Image generation
- PDF extraction

### Phase 8: Active Context & Automation

- Hook system (event-driven automation)
- Dynamic Typst functions
- Proactive AI check-ins
- Auto-journaling

---

## Future: Hook System

The hook system provides event-driven automation without hardcoded behavior.

```
┌─────────────────────────────────────────────────────────────┐
│                      EVENT SOURCES                          │
│  Entity Lifecycle │ Temporal │ Render │ External            │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                      HOOK ENGINE                            │
│  Pattern Matcher → Action Executor                          │
│       ↑                                                     │
│  Hook Registry (pattern_content_id, action_content_id)     │
└─────────────────────────────────────────────────────────────┘
```

### Event Types (Extensible Strings)

```
entity.created.message      # Entity lifecycle
temporal.idle.conversation  # Time-based
render.before.llm           # Render pipeline
custom.namespace.name       # User-defined
```

### Patterns and Actions as Content

Patterns and actions are ContentBlocks, not code:

```yaml
# Pattern
event_type: "temporal.idle.conversation"
config:
  duration: "24h"

# Action
action: create_entity
entity_type: message
template: "Good morning! What's on your mind today?"
```

### Use Cases Enabled

| Feature | Pattern | Action |
|---------|---------|--------|
| Proactive check-ins | `temporal.idle.*` | Create greeting message |
| Context management | `conversation.context.overflow` | Summarize older messages |
| Auto-journaling | `entity.created.message` | Extract insights |
| Dynamic content | `render.before.llm` | Evaluate Typst functions |

---

## Extension Points

### Adding a New Store

1. Define trait in `storage/traits/`
2. Add types in `storage/types/`
3. Implement in `storage/implementations/sqlite/`
4. Add to `StorageTypes` trait
5. Wire into `SqliteStorageTypes`

### Adding a New Entity Type

1. Add to `EntityType` enum
2. Create structure tables (like `views`, `documents`)
3. Link to entities table via FK

### Adding a New Provider

1. Implement `LlmProvider` trait
2. Register in `ProviderRegistry`
3. Add model configuration

---

## Key Files

| Purpose | Location |
|---------|----------|
| Storage traits | `noema-core/src/storage/traits/` |
| Storage types | `noema-core/src/storage/types/` |
| SQLite implementation | `noema-core/src/storage/implementations/sqlite/` |
| Session management | `noema-core/src/storage/session/` |
| Chat engine | `noema-core/src/engine.rs` |
| Tauri commands | `noema-desktop/src/commands/` |
| React frontend | `noema-desktop/ui/src/` |

---

## Design Documents

| Document | Purpose |
|----------|---------|
| [UNIFIED_CONTENT_MODEL.md](UNIFIED_CONTENT_MODEL.md) | Detailed UCM design, feature requirements |
| [HOOK_SYSTEM.md](HOOK_SYSTEM.md) | Event-driven automation design |
| [STORAGE.md](../../STORAGE.md) | Database schema reference |
| [ROADMAP.md](../ROADMAP.md) | Full feature roadmap |

---

## Design Decisions

### Why NOT Deduplicate Content Blocks?

Each ContentBlock may have different metadata even with identical text:
- Different origin (user vs assistant)
- Different model
- Different privacy settings
- Different timestamps

The hash is for integrity, not deduplication.

### Why Views Instead of Threads?

Threads implied ownership and linear structure. Views are:
- Lightweight (just selection pointers)
- Shared (multiple views select same spans)
- First-class entities (can be @mentioned, named)

### Why Entity Relations for Forks?

Storing `forked_from_id` on views couples them. Relations:
- Decouple lifecycle (delete view, forks survive)
- Enable rich queries (fork tree, spawn tree)
- Extend to other relationships (references, citations)

### Why Spans Contain Multiple Messages?

Different models produce different numbers of messages:
- Claude with thinking: thinking → tool → result → response (4)
- GPT-4: tool → result → response (3)
- Simple model: response (1)

All are valid alternatives at the same turn position.
