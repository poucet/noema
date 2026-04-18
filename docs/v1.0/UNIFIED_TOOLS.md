# Unified Tool Provider Design

**Date:** 2026-04-18
**Status:** Design

## Problem

MCP servers and skills are treated differently by the daemon:
- MCP servers: registered via `add_mcp_server`, connected via HTTP, tools in McpRegistry
- Skills: registered via `register_client_tools`, called via WS reverse RPC or direct
- Two separate code paths for tool listing, calling, auth, and management
- Lumina registers Discord tools as MCP server on a separate port (hacky)

## Design: Unified ToolProvider

A `ToolProvider` is anything that provides tools to the daemon. All providers
speak `rmcp` types (`Tool`, `CallToolRequestParams`, `CallToolResult`).

### ToolProvider trait

```rust
#[async_trait]
pub trait ToolProvider: Send + Sync {
    /// Unique ID (e.g., "google-docs", "discord", "gdocs-skill").
    fn id(&self) -> &str;
    
    /// Human-readable name.
    fn name(&self) -> &str;
    
    /// Available tools.
    fn tools(&self) -> Vec<rmcp::model::Tool>;
    
    /// OAuth requirements (if any).
    fn oauth_requirements(&self) -> Vec<OAuthRequirement> { vec![] }
    
    /// Call a tool. User context (tokens, user_id) is in the params via __ctx.
    async fn call_tool(&self, request: CallToolRequestParams) -> Result<CallToolResult>;
    
    /// Connection status.
    fn is_connected(&self) -> bool { true }
}
```

### Implementations

1. **McpToolProvider** — wraps an MCP server connection (existing `ConnectedServer`)
   - `call_tool` → HTTP POST to MCP server
   - `tools()` → cached from server handshake
   - Auth: OAuth tokens sent as Bearer header

2. **WsToolProvider** — tools registered by a WS client (Lumina, etc.)
   - `call_tool` → reverse RPC over the WS connection
   - `tools()` → sent during `tools.register`
   - Auth: tokens sent in `__ctx` alongside the reverse call

3. **EmbeddedToolProvider** — in-process skill
   - `call_tool` → direct function call
   - `tools()` → from Skill::tools()
   - Auth: tokens passed in SkillCallContext

### Registration

All providers registered the same way:

```rust
// MCP server (existing)
daemon.register_provider(McpToolProvider::connect(url, auth).await?);

// Skill from Lumina over WS
daemon.register_provider(WsToolProvider::from_connection(tools, ws_conn));

// Embedded skill
daemon.register_provider(EmbeddedToolProvider::new(skill));
```

### ToolRegistry replaces CompositeToolService

```rust
pub struct ToolRegistry {
    providers: RwLock<Vec<Arc<dyn ToolProvider>>>,
    daemon_rest_tools: Arc<DaemonToolService>, // daemon's own REST API as tools
}

impl ToolService for ToolRegistry {
    async fn get_definitions(&self) -> Vec<ToolDefinition> { ... }
    async fn call_tool(&self, name: &str, args: Value) -> Result<...> { ... }
}
```

### Auth

All providers declare `oauth_requirements()`. The daemon:
1. Creates auth routes for each (`/auth/{provider_id}`)
2. Stores tokens in TransientTokenStore keyed by `(user_id, provider_id)`
3. Injects tokens into tool calls automatically

### What changes

- `CompositeToolService` → `ToolRegistry` (simpler, just a list of providers)
- `McpToolRegistry` → one provider per MCP server
- `WsToolRegistry` → one provider per WS-registered client
- `UserToolServiceCache` → scoping logic moves into ToolRegistry
- Skills just implement `ToolProvider` (or a thin wrapper adapts Skill → ToolProvider)
- Lumina's Discord tools: implement ToolProvider, register via WS (no separate MCP port)

### Migration path

1. Define ToolProvider trait in simply-daemon-api
2. Implement McpToolProvider (wraps existing McpToolCaller)  
3. Implement WsToolProvider (wraps existing ws_tools)
4. Implement EmbeddedToolProvider (wraps Skill)
5. Replace CompositeToolService with ToolRegistry
6. Convert Lumina to register as ToolProvider instead of MCP server
7. Remove separate MCP server hosting from Lumina
