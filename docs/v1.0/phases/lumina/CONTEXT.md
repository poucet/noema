# Lumina Phase — Context for New Agent

You're building a Rust Discord bot (Lumina) that connects to the existing simply-daemon.

---

## What exists

### The daemon (simply-daemon)
A working daemon with WebSocket + REST servers. It handles LLM conversations, MCP tool registry, OAuth, session management. Key traits:

- **SessionApi** — create/resume/close sessions, send messages, get responses (streamed via broadcast channel)
- **ConversationApi** — CRUD on persistent conversations
- **McpApi** — register/connect MCP servers, discover tools
- **ModelApi** — list models, set default model
- **OAuthApi** — OAuth flows for MCP servers

All traits are annotated with `#[rpc_service("prefix")]` and auto-generate WS dispatch + client macros.

### RemoteDaemon (simply-daemon/src/remote.rs)
A WS client that implements all daemon traits. Use it like this:

```rust
use simply_daemon::RemoteDaemon;
use simply_daemon::api::*;

let daemon = RemoteDaemon::connect("127.0.0.1:9800").await?;
// daemon implements SessionApi, ConversationApi, McpApi, etc.

let (info, mut events) = daemon.create_session(CreateSessionOptions::default()).await?;
daemon.send_message(&info.id, UserMessage { content: vec![...], tool_filter: None }).await?;

// Events stream via broadcast channel
while let Ok(event) = events.recv().await {
    match event {
        DaemonEvent::AssistantContent(block) => { /* streaming response */ }
        DaemonEvent::TurnComplete => { break; }
        _ => {}
    }
}
```

### The Python Lumina (~/simply/lumina)
The existing Python bot to port from. Study its codebase to understand features, commands, and Discord integration patterns. Key areas:
- Discord slash commands (ChatCog, VoiceCog, AdminCog, etc.)
- Context seeding from Discord channel history
- LLM integration (multi-provider)
- MCP tool registration for Discord actions
- Voice channel integration (songbird equivalent in Python)

---

## What to build

### Stage 1: Lumina crate
- New `lumina/` directory in workspace root
- `serenity` for Discord gateway + slash commands
- `RemoteDaemon` for daemon connection
- Start with `/ping` and `/chat` (echo) commands

### Stage 2: LLM chat through daemon
- `/chat` opens an ephemeral session, sends user message, streams response back to Discord
- Seed conversation context from recent Discord channel history
- Format LLM responses as Discord embeds
- Register Discord-specific MCP tools so the agent can read channels, react, etc.

---

## Key files to read

| File | Purpose |
|------|---------|
| `simply-daemon/src/api/` | All 7 API trait definitions |
| `simply-daemon/src/remote.rs` | RemoteDaemon — how to connect as a client |
| `simply-daemon/src/api/types.rs` | Shared types (SessionId, DaemonEvent, etc.) |
| `simply-daemon/src/api/session.rs` | SessionApi — session lifecycle + events |
| `simply-rpc/src/client.rs` | RpcClient trait that RemoteDaemon implements |
| `noema/src-tauri/src/commands/chat.rs` | How Noema uses the daemon (reference for Lumina) |
| `noema/src-tauri/src/commands/init.rs` | Service wiring + dispatch setup (reference) |
| `docs/designs/CORE_SERVICE.md` | WS protocol design |
| `docs/designs/RPC_FRAMEWORK.md` | simply-rpc framework design |

---

## Architecture constraints

1. **Lumina depends only on `simply-daemon` (for types + RemoteDaemon) and `simply-rpc` (for RpcClient)**. It does NOT depend on `simply-core` or `llm` directly.
2. **All LLM work happens in the daemon.** Lumina sends messages, daemon runs the agent.
3. **The daemon must be running.** Lumina connects via WS — no embedded daemon mode.
4. **Sessions are ephemeral by default** for Discord (no persistence across bot restarts).
5. **Use `jj commit` for version control**, not `git commit`.
6. **Do not run tests or builds** — the user handles that.
