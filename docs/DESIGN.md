# Noema Design Document

## 1. Overview

Noema is a local-first, voice-enabled AI assistant built with Rust and React. It is designed to be modular, privacy-focused, and extensible, featuring a sophisticated agentic architecture that supports the Model Context Protocol (MCP) for dynamic tool integration.

The project is structured as a Cargo workspace with multiple crates, separating core logic, audio processing, UI, and specific capabilities.

### 1.1 Design Philosophy

- **Local-First**: Privacy-focused with local speech recognition and optional local LLM support
- **Protocol-Native**: Built on open standards (MCP, JSON-RPC, OAuth 2.0) rather than proprietary integrations
- **Trait-Based Abstraction**: Pure Rust traits enable composability, testability, and storage/provider agnosticism
- **Streaming-First**: Real-time responses through async streams for responsive user interfaces

## 2. Architecture

### 2.1 High-Level Component Diagram

```mermaid
graph TD
    User[User] <--> UI[noema-ui (React + Tauri)]
    User <--> TUI[tui (Ratatui Terminal)]

    UI <--> Backend[Tauri Commands]
    TUI <--> Engine

    subgraph "Rust Backend"
        Backend --> Engine[ChatEngine (noema-core)]

        Engine --> Agent[Agent Trait]
        Engine --> Store[SessionStore Trait]

        Agent --> Models[LLM Providers]
        Agent --> Tools[Tool Registry]

        Tools -.-> MCP[MCP Clients (rmcp)]

        Engine -.-> Audio[noema-audio]
        Audio --> VAD[Voice Activity Detection]
        Audio --> Whisper[Whisper Transcription]

        Engine -.-> Ext[noema-ext]
    end

    Models --> APIs[External LLM APIs (Ollama, Gemini, Claude, OpenAI)]
    MCP <--> MCPServers[External MCP Servers]
```

### 2.2 Crate Structure

| Crate | Description |
|-------|-------------|
| `noema-core` | Core agent framework, engine, and storage abstractions |
| `noema-core/llm` | LLM provider abstraction and implementations |
| `noema-core/llm/llm_macros` | Procedural macros for provider delegation |
| `noema-ui` | Desktop application (Tauri + React) |
| `tui` | Terminal user interface (Ratatui) |
| `noema-audio` | Voice capture and Whisper transcription |
| `noema-ext` | Extensions (PDF extraction, attachments) |
| `config` | Configuration and path management |
| `commands` | CLI command definitions |

## 3. Standards and Protocols

### 3.1 Model Context Protocol (MCP)

**Specification**: [Model Context Protocol](https://modelcontextprotocol.io/)

Noema implements MCP as a **client**, enabling connection to any standards-compliant MCP server for dynamic tool integration.

#### Implementation Details

- **Transport**: Streamable HTTP client via `rmcp` crate
- **Features Used**: `client`, `transport-streamable-http-client`, `transport-streamable-http-client-reqwest`, `transport-worker`
- **Tool Discovery**: Dynamic at runtime - new servers immediately available without restart
- **Tool Definitions**: JSON Schema (via `schemars`) for input validation
- **Multimodal Results**: Text, images, and audio content from tool responses

#### Configuration Format

```toml
# ~/.noema/mcp.toml
[servers.example]
name = "Example Server"
url = "https://mcp.example.com"

[servers.example.auth]
type = "oauth"
client_id = "..."
client_secret = "..."
scopes = ["read", "write"]
```

### 3.2 JSON-RPC 2.0

**Specification**: [JSON-RPC 2.0](https://www.jsonrpc.org/specification)

MCP communication uses JSON-RPC 2.0 as its wire protocol, providing:

- Structured request/response format
- Error handling with standard error codes
- Batch requests (where supported)

### 3.3 OAuth 2.0

**Specifications**:
- [RFC 6749](https://datatracker.ietf.org/doc/html/rfc6749) - OAuth 2.0 Authorization Framework
- [RFC 6750](https://datatracker.ietf.org/doc/html/rfc6750) - Bearer Token Usage
- [RFC 8414](https://datatracker.ietf.org/doc/html/rfc8414) - OAuth 2.0 Authorization Server Metadata

MCP server authentication supports:

- **Client Credentials Grant** (RFC 6749 Section 4.4)
- **Bearer Token Authentication** (RFC 6750)
- **Well-Known Discovery** (RFC 8414) for OAuth endpoints
- **Token Refresh** with expiration tracking

#### Deep Link Handling

OAuth callbacks use the `noema://` custom URL scheme for:
- Receiving authorization codes
- Single-instance enforcement
- Cross-platform desktop support via Tauri

### 3.4 LLM Provider APIs

Each provider implements standard REST/HTTP APIs:

| Provider | API Standard | Streaming |
|----------|-------------|-----------|
| OpenAI | OpenAI Chat Completions API | Server-Sent Events |
| Claude | Anthropic Messages API | Server-Sent Events |
| Gemini | Google Generative AI API | Streaming responses |
| Ollama | Ollama API (OpenAI-compatible) | Newline-delimited JSON |

### 3.5 Audio Standards

| Standard | Usage |
|----------|-------|
| PCM 16kHz mono | Audio capture format (Whisper requirement) |
| 32-bit float (f32) | Internal sample representation |
| WebM/Opus | Browser audio encoding |
| GGML | Whisper model format (whisper.cpp) |

## 4. Core Components

### 4.1 Agent Architecture

The agent system is built on three foundational traits:

#### ConversationContext Trait

```rust
pub trait ConversationContext {
    fn iter(&self) -> impl Iterator<Item = &ChatMessage>;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool;
}

pub trait ConversationContextMut: ConversationContext {
    fn add(&mut self, message: ChatMessage);
    fn extend(&mut self, messages: impl IntoIterator<Item = ChatMessage>);
}
```

**Design Rationale**:
- Read-only + mutable variants for controlled access
- Iterator-based for zero-copy message access
- Enables windowed, filtered, and composed contexts

#### Agent Trait

```rust
#[async_trait]
pub trait Agent: Send + Sync {
    async fn execute(&self, context: &mut dyn ConversationContextMut) -> Result<()>;
    async fn execute_stream(
        &self,
        context: &mut dyn ConversationContextMut,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamEvent>> + Send>>>;
}
```

**Design Rationale**:
- Pure function transforming context into new messages
- Context-centric: all data (user input, tool results) lives in context
- Composable: agents can be stacked in pipelines
- Streaming-first: real-time output for responsive UIs

#### Agent Implementations

| Agent | Purpose | Tool Support |
|-------|---------|--------------|
| `SimpleAgent` | Single-turn conversation | None |
| `ToolAgent` | Multi-turn with static tools | `ToolRegistry` |
| `McpAgent` | Multi-turn with dynamic tools | `McpToolRegistry` |

### 4.2 LLM Abstraction

```rust
#[async_trait]
pub trait ChatModel: Send + Sync {
    async fn chat(&self, request: ChatRequest) -> Result<ChatMessage>;
    async fn stream_chat(&self, request: ChatRequest) -> Result<ChatStream>;
    fn name(&self) -> &str;
}
```

**Provider Architecture**:
- `ModelProvider` trait for listing and instantiating models
- `#[delegate_provider_enum]` macro for unified provider enum
- Environment-based configuration (`CLAUDE_API_KEY`, etc.)

**Multimodal Content**:

```rust
pub enum ContentBlock {
    Text { text: String },
    Image { data: String, mime_type: String },
    Audio { data: String, mime_type: String },
    ToolCall { id: String, name: String, arguments: Value },
    ToolResult { tool_call_id: String, content: Vec<ToolResultContent> },
}
```

### 4.3 Storage Layer

```rust
pub trait SessionStore: Send + Sync {
    type Transaction: StorageTransaction;
    fn messages(&self) -> Vec<ChatMessage>;
    fn begin(&mut self) -> Self::Transaction;
    fn commit(&mut self, transaction: Self::Transaction) -> Result<()>;
    fn clear(&mut self) -> Result<()>;
}

pub trait StorageTransaction {
    fn pending(&self) -> &[ChatMessage];
    fn committed(&self) -> &[ChatMessage];
    fn add(&mut self, message: ChatMessage);
}
```

**Implementations**:

| Store | Backend | Use Case |
|-------|---------|----------|
| `MemorySession` | In-memory Vec | Testing, TUI |
| `SqliteSession` | SQLite (rusqlite) | Desktop persistence |

**Database Schema**:

```sql
CREATE TABLE conversations (
    id TEXT PRIMARY KEY,
    name TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE messages (
    id TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL REFERENCES conversations(id),
    role TEXT NOT NULL,
    payload JSON NOT NULL,
    position INTEGER NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE INDEX idx_messages_conversation ON messages(conversation_id, position);
```

### 4.4 ChatEngine

```rust
pub struct ChatEngine<S: SessionStore> {
    // Command-event loop architecture
    command_tx: mpsc::Sender<EngineCommand>,
    event_rx: mpsc::Receiver<EngineEvent>,
}
```

**Commands**: `SendMessage`, `SetModel`, `ClearHistory`
**Events**: `Message`, `MessageComplete`, `Error`, `ModelChanged`

**Design Pattern**: MPSC channel-based command-event loop for non-blocking async operation.

### 4.5 Audio Pipeline

```
Audio Input → CPAL Capture → VAD → Whisper → ChatPayload → Engine
```

| Component | Implementation |
|-----------|----------------|
| Capture | `StreamingAudioCapture` trait (CPAL, Browser, Dummy backends) |
| VAD | Energy-based voice activity detection with state machine |
| Transcription | whisper.cpp via `whisper-rs` bindings |
| Coordination | `VoiceCoordinator` lifecycle management |

## 5. Innovations

### 5.1 Context-Centric Agent Model

**Traditional Approach**: Agents receive explicit input parameters (user message, conversation history).

**Noema Innovation**: Agents receive a mutable `ConversationContext` containing all data.

**Benefits**:
- **Uniform Interface**: User input, tool results, and history all accessed the same way
- **Natural Composition**: Output of one agent becomes input to the next
- **Flexible Triggering**: Agents can respond to any context change, not just user input
- **Zero-Copy Access**: Iterator-based access avoids message copying

### 5.2 Dynamic MCP Tool Registry

**Traditional Approach**: Tools registered at application startup, requiring restart for changes.

**Noema Innovation**: `McpToolRegistry` queries live MCP connections on each agent iteration.

```rust
impl McpToolRegistry {
    pub async fn get_all_definitions(&self) -> Vec<ToolDefinition> {
        // Queries all connected MCP servers in real-time
    }
}
```

**Benefits**:
- New servers immediately available without restart
- Tools appear/disappear as servers connect/disconnect
- No stale tool definitions

### 5.3 Streaming-First Architecture

Every layer supports streaming from the ground up:

```rust
// Agent level
async fn execute_stream(&self, context: &mut dyn ConversationContextMut)
    -> Result<Pin<Box<dyn Stream<Item = Result<StreamEvent>> + Send>>>;

// LLM level
async fn stream_chat(&self, request: ChatRequest) -> Result<ChatStream>;

// Engine level
pub enum EngineEvent {
    Message(StreamEvent),
    MessageComplete(ChatMessage),
    // ...
}
```

**Benefits**:
- Real-time UI updates during generation
- Lower perceived latency
- Graceful handling of long-running operations

### 5.4 Transactional Message Semantics

```rust
let mut tx = session.begin();
tx.add(user_message);
agent.execute(&mut tx).await?;  // Adds assistant messages
session.commit(tx)?;  // Only now persisted
```

**Benefits**:
- Explicit commit/rollback for error recovery
- Drop guard warns on uncommitted messages
- Separates in-progress from committed state
- Enables validation before persistence

### 5.5 Non-Blocking Engine Architecture

```rust
// Engine spawns background task
tokio::spawn(async move {
    loop {
        match command_rx.recv().await {
            Some(EngineCommand::SendMessage(payload)) => {
                // Execute without holding locks
                let result = agent.execute_stream(&mut context).await;
                event_tx.send(EngineEvent::Message(result)).await;
            }
        }
    }
});
```

**Benefits**:
- No lock contention during agent execution
- UI remains responsive during processing
- Concurrent operations supported

### 5.6 Trait-Only Core

The `noema-core` crate contains only traits and minimal implementations:

- `Agent`, `ConversationContext` - behavior contracts
- `SessionStore`, `StorageTransaction` - storage contracts
- `ChatModel` - LLM contracts

**Benefits**:
- Maximum flexibility for implementers
- Easy testing with mock implementations
- Clean dependency boundaries

### 5.7 Cross-Platform Voice with Platform Backends

```rust
pub trait StreamingAudioCapture: Send + Sync {
    fn start(&mut self) -> Result<AudioStream>;
    fn stop(&mut self) -> Result<()>;
}
```

**Implementations**:
- `CpalBackend` - Desktop (macOS, Windows, Linux)
- `BrowserBackend` - Web Audio API
- `DummyBackend` - Testing

**Benefits**:
- Same voice agent code works across platforms
- Platform-specific optimizations possible
- Easy to add new backends

## 6. User Interfaces

### 6.1 Desktop UI (Tauri + React)

**Stack**: Tauri 2.0 + React + TypeScript + Tailwind CSS

**Tauri Commands**:
- Chat: `init_app`, `send_message`, `get_messages`, `set_model`
- Voice: `toggle_voice`, `start_voice_session`, `process_audio_chunk`
- MCP: `list_mcp_servers`, `connect_mcp_server`, `start_mcp_oauth`
- Files: `save_file`, conversation management

**Event System**: Rust → Frontend via Tauri events
- `user_message`, `assistant_message`, `message_complete`, `error`

### 6.2 Terminal UI (Ratatui)

**Stack**: Ratatui + Crossterm

**Features**:
- Real-time streaming display
- Input history navigation
- Model and conversation switching
- MCP server management via commands
- Voice input integration

## 7. Configuration

### 7.1 Path Management

```rust
pub struct PathManager {
    data_dir: PathBuf,    // App data
    config_dir: PathBuf,  // Preferences
    cache_dir: PathBuf,   // Cache
    logs_dir: PathBuf,    // Logs
}
```

**Platform Paths**:

| Platform | Data | Config |
|----------|------|--------|
| macOS | `~/Library/Application Support/noema` | `~/Library/Preferences/noema` |
| Linux | `~/.local/share/noema` | `~/.config/noema` |
| Windows | `%APPDATA%\noema` | `%APPDATA%\noema` |

### 7.2 Environment Variables

| Variable | Purpose |
|----------|---------|
| `CLAUDE_API_KEY` | Anthropic API authentication |
| `GEMINI_API_KEY` | Google Generative AI authentication |
| `OPENAI_API_KEY` | OpenAI API authentication |
| `OLLAMA_BASE_URL` | Custom Ollama server URL |
| `*_BASE_URL` | Provider endpoint overrides |

Environment loaded from `~/.env` (lower precedence) and `./.env` (higher precedence).

## 8. Future Considerations

- **Plugin System**: UI widgets for specific MCP tools
- **Local RAG**: Document indexing and retrieval via `noema-ext`
- **Voice Output**: TTS integration for voice-to-voice interaction
- **Embeddings**: Vector storage for semantic search
- **Google Docs Integration**: Document import and synchronization

## 9. Appendix: Key File Locations

| Component | Path |
|-----------|------|
| Agent Trait | `noema-core/src/agent.rs` |
| Context Trait | `noema-core/src/context.rs` |
| Chat Engine | `noema-core/src/engine.rs` |
| MCP Registry | `noema-core/src/mcp/registry.rs` |
| MCP Config | `noema-core/src/mcp/config.rs` |
| Storage Traits | `noema-core/src/storage/traits.rs` |
| SQLite Storage | `noema-core/src/storage/sqlite.rs` |
| LLM API | `noema-core/llm/src/api.rs` |
| Tool Registry | `noema-core/llm/src/tools.rs` |
| Path Manager | `config/src/paths.rs` |
| TUI Main | `tui/src/main.rs` |
| Tauri Backend | `noema-ui/src-tauri/src/lib.rs` |
