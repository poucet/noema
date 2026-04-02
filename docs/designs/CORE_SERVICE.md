# Core Service Communication

**Status:** Draft
**Version:** 1.0
**Parent:** [ARCHITECTURE.md](ARCHITECTURE.md)

---

## Overview

`simply-daemon` is the hub. It owns agent orchestration (via `simply-core`), UCM storage, event/intent engine, voice pipeline, and session management. It exposes three interfaces for different needs:

- **WebSocket + JSON** — rich clients (Noema, Lumina) that need sessions, streaming, multimodal content
- **REST inbound** — trigger services that push events (curl-friendly, fire-and-forget)
- **MCP outbound** — action services that the daemon connects to and calls tools on

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
├─ Global MCP tool registry
├─ Event bus + intent engine
├─ Voice pipeline
│
├─ WebSocket + JSON — rich clients
│   ▲           ▲           ▲
│   Noema       Lumina      Future clients
│
├─ REST inbound — trigger services
│   ▲           ▲
│   github      shell script
│   watcher     cron job
│
└─ MCP outbound — action services
    ▼           ▼
    github      droplets
    watcher     any MCP server
```

---

## Interface 1: WebSocket + JSON — Rich Clients

For Noema, Lumina, and future clients that need interactive, multimodal, real-time communication.

### Why WebSocket?

- **Universal** — every platform has WebSocket support. Tauri, browsers, serenity bots, Python scripts.
- **No translation layers** — Noema's React frontend can talk to the daemon directly. No Tauri proxy needed for chat.
- **Bidirectional** — send messages and receive streamed responses over one connection.
- **Binary support** — WebSocket supports binary frames natively for non-realtime multimodal (images, files).
- **Type safety via Rust** — message types defined in Rust, exported to TypeScript for frontend consumption.

### Client → Daemon

| Message | Purpose |
|---------|---------|
| **CreateSession** | Start a new conversation (ephemeral or persistent) |
| **ResumeSession** | Reconnect to existing session (after daemon restart) |
| **SeedContext** | Provide conversation history (Lumina: Discord messages, Noema: ephemeral messages) |
| **SendMessage** | New user message for the agent (can include images, file references) |
| **SetPersistence** | Toggle conversation persistence (ephemeral ↔ UCM-backed) at any time |
| **RegisterMcp** | Register MCP tools (platform-specific or shared — all shared by default) |
| **RegisterEventSource** | Register platform-specific event sources |

### Daemon → Client

| Message | Purpose |
|---------|---------|
| **AgentResponse** | Agent's response (streamed tokens) |
| **AgentMultimodal** | Non-realtime multimodal content (generated image, file, etc.) |
| **ToolCall** | Agent wants to call a client-registered MCP tool |
| **EventNotification** | An intent fired that targets this client's platform |
| **SessionState** | Confirmation of session creation, persistence changes, etc. |

### Noema Specifically

```
Noema
├─ React frontend
│   ├─ WebSocket → simply-daemon (chat, sessions, agent responses)
│   └─ Tauri IPC → src-tauri (slash commands, OS integration, file access)
├─ src-tauri
│   └─ Local system access only (no chat proxy)
```

The React frontend talks directly to the daemon for all conversation/agent work. Tauri handles OS-level concerns: slash commands, file pickers, window management, notifications.

### Multimodal

Two kinds, different transports:

1. **Non-realtime** (images, files, documents) — sent as message content over WebSocket. Base64, blob references, or URLs.
2. **Realtime voice** — separate transport per platform. CPAL (desktop mic), songbird (Discord), WebRTC (browser). Audio streams do NOT go through the WebSocket.

---

## Interface 2: REST Inbound — Trigger Services

For services that push events into the daemon. The simplest possible API — a shell script with curl works.

### Endpoints

```
POST /events
  { "type": "github.pr_opened", "payload": { "repo": "...", "pr": 42 } }

POST /register
  { "name": "github-watcher", "mcp_endpoint": "localhost:9001",
    "events": ["github.pr_opened", "github.push"] }

DELETE /register/{name}

GET /health
```

### Characteristics

- **Stateless** — no persistent connection needed. POST and exit.
- **Curl-friendly** — `curl -X POST http://localhost:PORT/events -d '{"type": "github.push", ...}'`
- **No ordering guarantees** — events are processed as they arrive.
- **Auth** — simple token-based auth for local services.

---

## Interface 3: MCP Outbound — Action Services

For services that expose tools the daemon's agent can call. The daemon connects to them as an MCP client.

### How It Works

1. Service starts up and exposes an MCP server (standard MCP protocol)
2. Service registers with the daemon via REST: `POST /register { name, mcp_endpoint }`
3. Daemon connects to the MCP server, discovers available tools
4. Tools become available in the daemon's global tool registry — usable by any session on any client
5. When the daemon needs to call a tool, it invokes it via the MCP connection
6. If the MCP connection drops, tools become unavailable. Actions targeting those tools are deferred.

### Dynamic Registration

Action services can come and go at runtime:
- Register via REST `POST /register` with an MCP endpoint
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

A GitHub watcher is a trigger service (pushes `github.pr_opened` events via REST) AND an action service (exposes `comment_pr`, `create_issue` tools via MCP). Both interfaces compose naturally.

---

## MCP Tool Registry — Global and Shared

All MCP tools are registered in a single global registry, regardless of who registered them:

- **Client-registered tools** (via WebSocket `RegisterMcp`): Discord tools from Lumina, filesystem tools from Noema, etc.
- **Service-registered tools** (via MCP outbound): GitHub tools, Droplets tools, any external MCP server.
- **Daemon-built-in tools**: document CRUD, search, intent management.

**All tools are shared by default.** If you connect your Droplets MCP service from Noema, it's available to Lumina sessions too. The agent sees all tools regardless of which client is driving the session.

Platform-specific tools (like `send_discord_message`) naturally only *work* when Lumina is connected — but they're still visible to all sessions. If the agent calls a tool whose owning client/service is disconnected, the action is deferred until reconnection (or fails after timeout).

---

## Client Registry & Action Routing

The daemon maintains a registry of all connected peers:

- **Rich clients** (WebSocket): which platforms are online, what tools and event sources they've registered
- **Action services** (MCP): which services are connected, what tools they expose
- **Trigger services** (REST): registered event sources (liveness via MCP connection or TTL)

When an intent fires or an agent calls a tool:
1. Daemon checks the registry for the tool/action target
2. If the target is available → execute immediately
3. If unavailable → defer (queue for retry on reconnection, or fail after timeout)

---

## Conversation Sessions

Sessions are the daemon's unit of conversation state. They live in memory and are optionally backed by UCM storage.

- **Ephemeral by default differs per platform** — Lumina defaults ephemeral (Discord is source of truth), Noema defaults persistent (UCM). Toggleable at any time.
- **Persistence is per-conversation** — "save this conversation" from Discord writes in-memory state to UCM. A scratchpad in Noema creates an ephemeral session.
- **Sub-agent workflows can use UCM independently** — even if the parent conversation is ephemeral, a sub-agent doing multi-step tool work may store intermediate state in UCM.
- **After daemon restart, clients re-seed** — Lumina re-sends recent Discord messages, Noema reloads from UCM (persistent) or re-sends from memory (ephemeral).

---

## Connection Resilience

The daemon can restart independently of clients. Clients must handle disconnection gracefully and reconnect automatically.

### Client Auto-Reconnect

When a WebSocket connection drops, the client:

1. Detects disconnection (WebSocket close / error)
2. Enters reconnection loop with **exponential backoff** (e.g., 100ms → 200ms → 400ms → ... capped at 30s)
3. On successful reconnect: re-registers MCP tools and event sources, resumes or re-seeds sessions
4. UI shows connection status (disconnected / reconnecting / connected) — the client remains usable for local operations during disconnection

### Daemon Statelessness Across Restarts

The daemon does not assume clients stay connected across its own restarts. On startup:

- UCM-backed sessions are reloadable from storage
- Ephemeral sessions are lost — clients re-seed from their own state (Discord history, in-memory messages)
- MCP tool registrations are re-established by clients on reconnect
- The daemon does not persist WebSocket session state to disk

### Ordering

The client drives recovery. The daemon accepts fresh connections and treats reconnecting clients the same as new ones. Session resumption (matching a new connection to a prior session) uses session IDs, not connection identity.

---

## Adding a New Rich Client

Create a new crate that:
1. Handles platform-specific I/O (gateway, commands, audio)
2. Opens a WebSocket connection to simply-daemon
3. Registers platform-specific MCP tools (shared globally)
4. Registers platform-specific event sources
5. Handles `ToolCall` and `EventNotification` messages from the daemon

## Adding a New Integration Service

1. Expose an MCP server with your tools
2. `POST /register` to the daemon with your MCP endpoint
3. Optionally push events via `POST /events`
4. That's it — your tools are available to all sessions
