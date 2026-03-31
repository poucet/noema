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

## Core Service Communication

`simply-core` runs as a daemon and exposes its API over two channels:

**MCP interface** — for agent-facing operations:
- Tool calls, model selection, MCP server management
- The LLM already speaks MCP — no translation layer needed
- Core exposes an MCP server that agents connect to directly

**gRPC interface** — for platform-facing operations:
- Storage CRUD, voice streaming, identity lookups, schedule management
- Strong typing via protobuf, bidirectional streaming for audio
- Internal service calls where type safety and performance matter

**Why hybrid:** MCP is designed for LLM↔tool communication, not service-to-service RPC. Forcing voice byte streaming or complex storage queries through MCP's flat `tool_name + args` model is awkward. gRPC handles typed requests, streaming, and structured responses naturally. Meanwhile, the agent path stays clean — the LLM calls tools via MCP without a translation layer.

**Why a service vs. shared library:**
- Single writer to storage — no SQLite contention
- Noema and Lumina can run independently or together
- Future platforms (/meet, Telegram) connect the same way
- Voice pipeline state is centralized (one conversation, multiple listeners)

**gRPC service surface (platform-facing):**
- Storage: conversation CRUD, entity management, blob storage, document operations
- Voice: `transcribe(stream<AudioChunk>)`, `synthesize(text) → stream<AudioChunk>`, `list_voices`
- Identity: user lookup, platform linking, role management
- Events & Intents: intent CRUD, event source registration, pause/resume, re-resolve fuzzy triggers
- Documents: generic CRUD with frontmatter-aware queries (covers todos, notes, contexts, configs)
- Config: model selection, provider management

**MCP service surface (agent-facing):**
- All feature operations as MCP tools (search, create_todo, query_knowledge, etc.)
- Agent orchestration: `run_turn`, `prompt`
- External MCP server passthrough

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

## Event & Intent System

The core provides an **event-driven reactive system** where intents (registered reactions) fire in response to events from any source. Time is just one event source among many.

**Core principle: code provides capabilities, content provides configuration.** Event sources and action handlers are code (traits implemented by developers). But all routing — which events trigger which actions, with what parameters, targeting whom — lives in UCM markdown documents.

```
Event Sources                    Intent Engine                Actions (typed, not all need LLM)
┌─────────────┐                 ┌──────────────┐            ┌──────────────────────┐
│ Timer/Cron   │──►             │              │         ──►│ forward (no LLM)     │
│ Discord      │──►             │  Match event │         ──►│ notify (no LLM)      │
│ WebRTC       │──►  events ──►│  against     │──► fire ──►│ emit_event (no LLM)  │
│ Conversation │──►             │  registered  │         ──►│ update_doc (no LLM)  │
│ Agent life-  │──►             │  intents     │         ──►│ platform_action      │
│  cycle       │──►             │              │         ──►│ execute_prompt (LLM) │
│ Document     │──►             │              │         ──►│ resume_conv (LLM)    │
│ User-defined │──►             │              │         ──►│ call_tool            │
└─────────────┘                 └──────────────┘            └──────────────────────┘
```

**Key property:** Actions can emit events. This enables chaining — agent A completes → emits event → triggers intent that resumes mainline → mainline completes → emits event → triggers reminder to user.

### Event Sources

Event sources are **open and extensible**. The core provides built-in sources; platforms register their own; users and agents can define custom sources.

| Source | Events | Built-in / Platform |
|--------|--------|-------------------|
| **Timer** | `time.exact`, `time.cron`, `time.interval` | Built-in |
| **Agent lifecycle** | `agent.task_completed`, `agent.task_failed`, `agent.heartbeat` | Built-in |
| **Conversation** | `conversation.turn_produced`, `conversation.completed` | Built-in |
| **Intent lifecycle** | `intent.fired`, `intent.completed`, `intent.failed` | Built-in |
| **Document** | `document.created`, `document.updated`, `document.deleted` | Built-in |
| **Discord** | `discord.member_joined`, `discord.message`, `discord.reaction` | Lumina |
| **WebRTC** | `rtc.user_joined`, `rtc.user_left`, `rtc.session_started` | /meet |
| **User-defined** | Any custom event name | Runtime |

Platform-specific sources are registered when the platform connects to simply-core. The core doesn't need to know about Discord — it just sees events with typed payloads.

### Intents

An intent is a registered reaction: **"when this event pattern occurs, do this action."** Intents are UCM documents (searchable, embeddable, LLM-readable) with a lightweight execution table for runtime state.

**Intent triggers can be:**

| Trigger Type | Example | Resolution |
|---|---|---|
| **Single event** | `discord.member_joined` | Fire on each matching event |
| **Time (exact)** | "April 5th at 3pm" | Timer source emits at datetime |
| **Time (fuzzy)** | "Tomorrow morning" | LLM resolves to concrete time, stored alongside original |
| **Time (recurring)** | Cron / interval | Timer source emits repeatedly |
| **Condition (all)** | Agent A done AND agent B done | Accumulate events, fire when all matched |
| **Condition (any)** | Agent A done OR timeout | Fire on first match |
| **Compound** | Agent A done AND 1h elapsed | Multiple trigger types combined |

**Intent frontmatter examples:**

```markdown
# Time-based: reminder
---
type: intent
trigger:
  source: timer
  original: "tomorrow morning"
  resolved: "2026-04-01T09:00:00"
  precision: fuzzy
action: resume_conversation
conversation_id: "conv-abc-123"
target: "user:chris@discord:12345"
created_by: "user:chris"
---
Check in on the voice migration progress
```

```markdown
# Event-based: welcome new Discord members
---
type: intent
trigger:
  source: discord
  event: member_joined
  recurrence: every
action: execute_prompt
prompt: "Welcome the new member {event.member.name} to the server"
target: "channel:welcome@discord:67890"
created_by: "user:chris"
---
Generate a personalized welcome message for new members
```

```markdown
# Condition: wait for subtasks, then resume
---
type: intent
trigger:
  mode: all
  conditions:
    - source: intent_lifecycle
      event: intent.completed
      intent_id: "intent-research-a"
    - source: intent_lifecycle
      event: intent.completed
      intent_id: "intent-research-b"
action: resume_conversation
conversation_id: "conv-mainline-456"
target: "user:chris@discord:12345"
created_by: "agent:lumina"
---
Synthesize findings from both research agents
```

### Actions

Actions are **typed and generic**. Not every action involves an LLM — many are pure plumbing.

| Action Type | Involves LLM? | Description |
|---|---|---|
| `forward` | No | Stream event payload to a target |
| `notify` | No | Send a fixed message to a target |
| `emit_event` | No | Emit another event into the bus (enables chaining) |
| `update_document` | No | Modify a UCM document |
| `platform_action` | No | Platform-specific side effect (add role, pin message, etc.) |
| `call_service` | No | Call a registered service by name — protocol-agnostic |
| `execute_prompt` | Yes | Run a prompt through the agent |
| `resume_conversation` | Yes | Reopen and continue a suspended conversation |

### Compile Once, Execute Forever — The Action AST

Users express intents in natural language. The LLM compiles this into a declarative AST once, and the engine executes the compiled form directly.

```rust
/// The compiled action AST — what the engine actually executes
enum Action {
    Forward { target: Expr },
    Notify { target: Expr, message: Expr },
    EmitEvent { event_type: Expr, payload: HashMap<String, Expr> },
    UpdateDocument { document_id: Expr, updates: HashMap<String, Expr> },
    PlatformAction { action: String, params: HashMap<String, Expr> },
    CallService { service: String, method: String, args: HashMap<String, Expr> },
    ExecutePrompt { prompt: Expr, target: Expr },          // LLM in the loop
    ResumeConversation { conversation_id: Expr },           // LLM in the loop
}

/// Expressions support late binding — values resolved at execution time
enum Expr {
    /// Known at compile time: "channel:general", 42, true
    Literal(Value),

    /// Resolved from the triggering event's payload
    EventField(FieldPath),

    /// Resolved from the creator's context at execution time
    ContextRef(ContextKey),

    /// Resolved by querying the system
    Lookup(LookupExpr),

    /// String interpolation: "Welcome {event.member.name} to {context.guild.name}!"
    Template(Vec<TemplatePart>),
}
```

**Late binding via `Expr`:** Not all values are known at compile time.

| Expr Type | Example | Resolved When |
|---|---|---|
| `Literal` | `"channel:general"`, `true`, `42` | Compile time (immediate) |
| `EventField` | `event.member.name`, `event.timestamp` | From the triggering event's payload |
| `ContextRef` | `context.channel`, `context.user`, `context.guild` | From the creator's runtime context at fire time |
| `Lookup` | `lookup(user, discord_id: event.member.id)` | System query at fire time |
| `Template` | `"Welcome {event.member.name}!"` | String interpolation after resolving inner expressions |

**The intent document IS the serialized AST.** Frontmatter is YAML serialization of the AST. Users who understand the schema can write it directly. Users who don't, describe what they want and the LLM writes it for them. Either way, the engine loads it into the same typed Rust structures.

**The separation:**

```
What developers build (code):        What users/agents configure (content):
┌────────────────────────────┐       ┌──────────────────────────────────────┐
│ EventSource implementations│       │ UCM documents (type: intent)         │
│  - discord, github, email  │       │                                      │
│  - timer, webhook, notion  │       │ "When discord.member_joined,         │
│                            │       │  forward to #welcome channel"        │
│ ActionHandler impls        │       │                                      │
│  - send_email, forward     │       │ "When github.pr_merged on main,      │
│  - send_discord, notify    │       │  notify user:chris on discord"       │
│  - update_document         │       │                                      │
│                            │       │ "Every morning at 9am,               │
│ Intent engine (fixed)      │       │  summarize overnight activity        │
│ Event bus (fixed)          │       │  and email it to the team"           │
└────────────────────────────┘       └──────────────────────────────────────┘
  Rarely changes.                      Changes constantly. No deploy needed.
  Deploy to add capabilities.          Create/edit/delete at runtime.
```

### Storage Split

```
UCM Document (the "what")              Intent Execution Table (the "when/status")
┌──────────────────────────┐           ┌──────────────────────────────────┐
│ ---                      │           │ intent_id: <ucm doc id>          │
│ type: intent             │     ┌────►│ status: pending | waiting |      │
│ trigger: { ... }         │─────┘     │         active | completed       │
│ action: resume           │           │ next_fire: 2026-04-01T09:00:00   │
│ target: user:chris       │           │ last_fired: null                 │
│ ---                      │           │ fire_count: 0                    │
│ Body text / context      │           │ conditions_met: { ... }          │
└──────────────────────────┘           │ recurrence: <serialized rule>    │
                                       └──────────────────────────────────┘
```

### Engine Loop

```
1. Process event queue:
   - For each incoming event, find intents subscribed to that event type
   - Update conditions_met for matching intents
   - If all/any conditions satisfied, mark intent as ready

2. Check timer-based intents: poll for next_fire <= now, mark as ready

3. For each ready intent:
   a. Read UCM document for full context
   b. Execute action (notify, resume conversation, run prompt, heartbeat, emit event)
   c. Update execution table (last_fired, fire_count, compute next_fire if recurring)
   d. If one-shot and complete, mark status = completed
   e. Emit intent.completed event (enables chaining)

4. Sleep until next timer fires or new event arrives
```

### Service Registry — Protocol-Agnostic Service Calls

The `call_service` action calls registered services by name. The core is **agnostic of transport protocol**.

```rust
/// A registered service — the engine doesn't know or care about the transport
trait Service: Send + Sync {
    fn name(&self) -> &str;
    async fn call(&self, method: &str, args: HashMap<String, Value>) -> Result<Value>;
    fn methods(&self) -> Vec<ServiceMethod>;  // For discovery / validation
}

/// Transport adapters — each wraps a different protocol
struct McpService { client: McpClient, server_name: String }
struct GrpcService { channel: tonic::Channel, service_descriptor: ServiceDesc }
struct RestService { base_url: Url, auth: Option<AuthConfig> }
```

The intent author doesn't know or care whether `praxis` is MCP, `calendar` is gRPC, or `github-api` is REST. They just call `service: name, method: name`. The registry handles transport.

---

## Extension Points

### Adding a New Event Source (developer)

Implement the `EventSource` trait and register with the core.

```rust
trait EventSource: Send + Sync {
    fn source_id(&self) -> &str;
    async fn start(&self, bus: EventBus) -> Result<()>;
}
```

### Adding a New Action Handler (developer)

Implement the `ActionHandler` trait and register with the engine.

```rust
trait ActionHandler: Send + Sync {
    fn action_type(&self) -> &str;
    async fn execute(&self, intent: &IntentDocument, event: &Event) -> Result<ActionOutcome>;
}
```

### Adding a New Service (developer)

Implement the `Service` trait with a transport adapter and register at startup.

### Adding a New Platform (developer)

Create a new crate that:
1. Handles platform-specific I/O (gateway, commands, audio)
2. Connects to `simply-core` as a client (gRPC + MCP)
3. Registers platform-specific `EventSource` implementations
4. Registers platform-specific `ActionHandler` implementations

---

## Voice Architecture

```
                  simply-voice (providers)
                  ├─ VoxtralProvider (STT + TTS)
                  ├─ (future: ElevenLabs, OpenAI, etc.)
                  │
┌─────────────────▼──────────────────┐
│         simply-core service         │
│  voice pipeline: VAD → STT → Agent → TTS  │
└──────┬──────────────────────┬──────┘
       │                      │
  simply-audio            lumina/voice
  (CPAL backend)         (songbird backend)
       │                      │
  Desktop mic/speaker    Discord voice channel
  (Noema)                (Lumina)
```

- **simply-voice** defines provider traits (`SttProvider`, `TtsProvider`) and implementations
- **simply-core** orchestrates the pipeline (VAD → STT → agent → TTS)
- **Audio backends** are platform-specific: CPAL for desktop, songbird for Discord, WebRTC for /meet
- Each backend converts platform audio to/from the core's expected format (PCM)

---

## Design Documents

| Document | Purpose |
|----------|---------|
| [UNIFIED_CONTENT_MODEL.md](UNIFIED_CONTENT_MODEL.md) | UCM three-layer architecture (detailed specification) |
| [obsolete/ARCHITECTURE-0.2.md](obsolete/ARCHITECTURE-0.2.md) | Previous Noema 0.2 architecture |
| [obsolete/HOOK_SYSTEM.md](obsolete/HOOK_SYSTEM.md) | Previous hook system design (superseded by Event & Intent System) |
| [STORAGE.md](STORAGE.md) | UCM database schema reference |
| [../FUTURE_ROADMAP.md](../FUTURE_ROADMAP.md) | Future feature roadmap |
| [../v1.0/DESIGN.md](../v1.0/DESIGN.md) | v1.0 migration plan and implementation stages |
