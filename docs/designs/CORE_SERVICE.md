# Core Service Communication

**Status:** Draft (updated for REST-first transport)
**Version:** 1.0
**Parent:** [ARCHITECTURE.md](ARCHITECTURE.md)

---

## Overview

`simply-daemon` is the hub. It owns agent orchestration (via `simply-core`), UCM storage, event/intent engine, voice pipeline, and session management. It exposes three interfaces:

- **REST** — primary interface for all request/response operations. Used by rich clients (Noema, Lumina), admin webpage, and trigger services.
- **WebSocket** — streaming only. Session event streams (agent responses, tool calls) and future bidirectional channels (voice).
- **MCP outbound** — action services that the daemon connects to and calls tools on.

Additionally, daemon REST methods are exposed as **in-process tools** via the `ToolService` trait, so agents can call daemon capabilities (list conversations, manage MCP servers, etc.) the same way they call external MCP tools.

`simply-core` is a library crate internal to the daemon: LLM providers, MCP client/server, agent orchestration. No external crate depends on it.

---

## Architecture

```
simply-daemon
├─ simply-core (internal library)
│   ├─ LLM providers
│   ├─ MCP client/server
│   └─ Agent orchestration
├─ UCM storage (SQLite, blobs)
├─ Session manager (in-memory conversation state)
├─ Client registry (connected peers + capabilities)
├─ Global MCP tool registry (external MCP + daemon tools via ToolService)
├─ Event bus + intent engine
├─ Voice pipeline
│
├─ REST — all request/response operations
│   ▲           ▲           ▲           ▲
│   Noema       Lumina      Admin page  Trigger services
│
├─ WebSocket — streaming only
│   ▲           ▲
│   Noema       Lumina      (session events, voice)
│
└─ MCP outbound — action services
    ▼           ▼
    github      any MCP server
    watcher
```

---

## Interface 1: REST — Primary Request/Response

All non-streaming daemon operations are REST endpoints, auto-generated from trait annotations. See [RPC_FRAMEWORK.md](RPC_FRAMEWORK.md) for the annotation system.

### Characteristics

- **Standard HTTP** — GET/POST/PUT/DELETE with JSON bodies. Explorable with curl, browser, any HTTP client.
- **Macro-driven routing** — paths declared in trait annotations, no manual route wiring.
- **Admin-friendly** — the admin webpage calls the same REST endpoints via `fetch()`.
- **Curl-friendly** — trigger services push events with `POST /session/event`.

### Auth (v1)

**Localhost only.** The REST server binds to `127.0.0.1` — all callers are trusted. No auth middleware.

**Future (post-v1):** Localhost remains trusted. Remote access adds OAuth for admin and app tokens for Noema/Lumina.

### Client usage

Rich clients (Noema, Lumina) use REST for all non-streaming operations via `RemoteDaemon`, which generates HTTP calls from the same trait annotations. The public `DaemonApi` traits are unchanged — callers don't know the transport.

### Trigger services

Fire-and-forget event delivery:

```
POST /session/event
  { "type": "github.pr_opened", "payload": { "repo": "...", "pr": 42 } }
```

---

## Interface 2: WebSocket — Streaming Only

For real-time event streams. WebSocket is used exclusively for `#[rpc(stream)]` methods.

### What streams over WebSocket

- **Session events** — `create_session`, `resume_session`, `subscribe_session` return a `broadcast::Receiver<DaemonEvent>` carrying streamed tokens, tool calls, turn completions.
- **Future:** voice channels, video streams.

### What does NOT use WebSocket

All request/response operations (CRUD, configuration, queries) use REST. This includes:
- Sending messages (`POST /session/{id}/message`)
- Managing sessions, conversations, MCP servers, models
- Health checks, admin operations

### Protocol

```rust
struct WsRequest {
    id: u64,
    method: String,  // e.g., "session.create_session"
    params: serde_json::Value,
}

struct WsResponse {
    id: u64,
    result: Option<Value>,
    error: Option<WsError>,
}

struct WsNotification {  // Server → Client push (stream events)
    method: String,
    params: Value,
}
```

### Client identification

Built-in method: `client.identify(name)`. Server tracks connection ID + name for admin dashboard.

---

## Interface 3: MCP Outbound — Action Services

For services that expose tools the daemon's agent can call. The daemon connects to them as an MCP client.

### How It Works

1. Service starts up and exposes an MCP server (standard MCP protocol)
2. Service registers with the daemon via REST: `POST /mcp` with endpoint URL
3. Daemon connects to the MCP server, discovers available tools
4. Tools become available in the daemon's global tool registry — usable by any session on any client
5. When the daemon needs to call a tool, it invokes it via the MCP connection
6. If the MCP connection drops, tools become unavailable. Actions targeting those tools are deferred.

### Dynamic Registration

Action services can come and go at runtime:
- Register via REST with an MCP endpoint
- Daemon connects, discovers tools
- Service shuts down → MCP connection drops → tools become unavailable → actions deferred
- Service restarts → re-registers → daemon reconnects → tools available again

### Configuration via UCM

Services can also be configured declaratively as UCM documents:

```yaml
---
type: mcp_server
name: github-watcher
endpoint: localhost:9001
enabled: true
---
GitHub integration: watches repos, provides PR/issue tools
```

Both runtime registration and UCM configuration work. UCM config is loaded at daemon startup; runtime registration is for dynamic services.

### A Service Can Be Both Trigger and Action

A GitHub watcher is a trigger service (pushes events via REST `POST /session/event`) AND an action service (exposes tools via MCP). Both interfaces compose naturally.

---

## Daemon as Tool Provider

REST methods on daemon traits are automatically exposed as tools via the `ToolService` trait:

```rust
pub trait ToolService: Send + Sync {
    async fn get_definitions(&self) -> Vec<ToolDefinition>;
    async fn call_tool(&self, name: &str, arguments: serde_json::Value) -> Result<Vec<ToolResultContent>>;
}
```

This is **in-process only** — no MCP server, no port, no protocol overhead. The daemon generates a `ToolService` impl from its REST-annotated trait methods (tool names, descriptions from doc comments, schemas from parameter types). This impl is registered in `McpToolRegistry` alongside external MCP tools.

**Result:** agents see daemon capabilities (list conversations, manage MCP servers, etc.) and external tools identically. Adding a new REST method to any daemon trait automatically makes it available as a tool.

Methods marked `#[rpc(no_tool)]` (e.g., `kill`) are excluded.

---

## MCP Tool Registry — Global and Shared

All tools are registered in a single global registry, regardless of source:

- **Daemon tools** (via `ToolService`): conversation CRUD, session management, model queries, etc.
- **Client-registered tools** (via WebSocket `RegisterMcp`): Discord tools from Lumina, filesystem tools from Noema, etc.
- **Service-registered tools** (via MCP outbound): GitHub tools, any external MCP server.

**All tools are shared by default.** The agent sees all tools regardless of which client is driving the session.

Platform-specific tools (like `send_discord_message`) naturally only *work* when Lumina is connected — but they're still visible to all sessions. If the agent calls a tool whose owning client/service is disconnected, the action is deferred until reconnection (or fails after timeout).

---

## Client Registry & Action Routing

The daemon maintains a registry of all connected peers:

- **Rich clients** (REST + WebSocket): which platforms are online, what tools and event sources they've registered
- **Action services** (MCP): which services are connected, what tools they expose
- **Trigger services** (REST): registered event sources

When an intent fires or an agent calls a tool:
1. Daemon checks the registry for the tool/action target
2. If the target is available → execute immediately
3. If unavailable → defer (queue for retry on reconnection, or fail after timeout)

---

## Conversation Sessions

Sessions are the daemon's unit of conversation state. They live in memory and are optionally backed by UCM storage.

- **Ephemeral by default differs per platform** — Lumina defaults ephemeral (Discord is source of truth), Noema defaults persistent (UCM). Toggleable at any time.
- **Persistence is per-conversation** — "save this conversation" from Discord writes in-memory state to UCM.
- **After daemon restart, clients re-seed** — Lumina re-sends recent Discord messages, Noema reloads from UCM (persistent) or re-sends from memory (ephemeral).

---

## Connection Resilience

The daemon can restart independently of clients. Clients must handle disconnection gracefully and reconnect automatically.

### Client Auto-Reconnect

When a WebSocket connection drops (for streaming), the client:

1. Detects disconnection (WebSocket close / error)
2. Enters reconnection loop with **exponential backoff** (100ms → 200ms → 400ms → ... capped at 30s)
3. On successful reconnect: re-registers MCP tools and event sources, resumes or re-seeds sessions
4. UI shows connection status — the client remains usable for local operations during disconnection

REST calls are stateless and retry-friendly — a daemon restart just means a brief period of failed HTTP requests until it comes back.

### Daemon Statelessness Across Restarts

On startup:
- UCM-backed sessions are reloadable from storage
- Ephemeral sessions are lost — clients re-seed from their own state
- MCP tool registrations are re-established by clients on reconnect
- The daemon does not persist connection state to disk

---

## Noema Specifically

```
Noema
├─ React frontend
│   ├─ REST → simply-daemon (all request/response: send message, list sessions, etc.)
│   ├─ WebSocket → simply-daemon (session event streams only)
│   └─ Tauri IPC → src-tauri (slash commands, OS integration, file access)
├─ src-tauri
│   └─ Local system access only (no chat proxy)
```

The React frontend talks directly to the daemon for all conversation/agent work. Tauri handles OS-level concerns.

### Multimodal

Two kinds, different transports:

1. **Non-realtime** (images, files, documents) — sent as message content via REST. Base64, blob references, or URLs.
2. **Realtime voice** — separate transport per platform. CPAL (desktop mic), songbird (Discord), WebRTC (browser). Audio streams do NOT go through REST or the primary WebSocket.

---

## Adding a New Rich Client

Create a new crate that:
1. Handles platform-specific I/O (gateway, commands, audio)
2. Uses `RemoteDaemon` (REST + lazy WebSocket) — public trait API unchanged
3. Registers platform-specific MCP tools (shared globally)
4. Registers platform-specific event sources
5. Handles session events via WebSocket stream subscription

## Adding a New Integration Service

1. Expose an MCP server with your tools
2. Register with the daemon via REST: `POST /mcp` with your endpoint
3. Optionally push events via `POST /session/event`
4. That's it — your tools are available to all sessions
