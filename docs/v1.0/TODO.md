# v1.0 — Manual Test Checklist

Verification tests that should pass before moving to the next major milestone. Run these against a live daemon + Discord bot.

Daemon default port: 9800 (REST + WebSocket on single port).

---

## 1. Daemon REST APIs

```bash
BASE=http://127.0.0.1:9800
```

### Health & Info
- [x] `curl $BASE/daemon` — returns `{"status":"ok"}`
- [x] `curl $BASE/daemon/version` — returns version string
- [x] `curl $BASE/model` — lists available models
- [x] `curl $BASE/model/provider` — lists providers
- [x] `curl $BASE/model/default` — returns default model ID

### Conversations
- [x] `curl $BASE/conversation` — lists conversations (may be empty)
- [x] `curl -X POST $BASE/conversation -H 'Content-Type: application/json' -d '"test"'` — creates conversation, returns ID
- [x] `curl $BASE/conversation/{id}/messages` — returns messages

### MCP Servers
- [x] `curl $BASE/mcp` — lists configured MCP servers
- [x] `curl $BASE/mcp/tools` — lists all tools across all servers

### Binary Assets
- [ ] Upload: `curl -X POST $BASE/asset -H 'Content-Type: image/png' --data-binary @test.png` — returns asset ID
- [ ] Download: `curl $BASE/asset/{hash}` — returns raw binary with correct Content-Type
- [ ] Verify immutable cache headers (Cache-Control, ETag) on asset download

## 2. Lumina LLM Chat

- [x] Start daemon (`simply-daemon`) and Lumina (`lumina`)
- [x] Lumina connects to Discord and posts status message
- [x] Create a chat channel via `/chat new`
- [x] Send a message in the AI chat channel — Lumina responds via LLM
- [x] Response streams back with debounced edits (not all at once)
- [x] `/chat model <model_id>` changes the model for the channel
- [x] `/chat pause` stops responses, `/chat resume` restarts them
- [x] Channel history is loaded as conversation context (check LLM sees prior messages)

## 3. Lumina MCP Tools (via daemon)

- [x] Lumina registers as MCP service on connect (check daemon logs: "ephemeral MCP service registered")
- [x] `/tool list` shows all tools from daemon + lumina-discord server
- [x] Tool descriptions and param counts are correct
- [x]] Pagination works if tool list exceeds one page

## 4. /tool call — Text Tools

- [x] `/tool call list_channels` — modal opens with guild_id field
- [x] Fill in guild_id, submit — returns channel list as embed
- [x] `/tool call send_message` — modal with channel_id + content fields
- [x] Submit — message appears in the target Discord channel
- [x] `/tool call get_channel_history` — returns message history as embed
- [x] `/tool call search_messages` — finds messages matching query
- [x] `/tool call list_guilds` — returns guilds (no params, executes immediately)
- [x] Tools that error show red error embeds

## 5. /tool call — Multimodal Content

- [x] `/tool call get_blob` with the hash — image returned as Discord attachment (not JSON text)
- [ ] Audio asset returns as audio file attachment
- [x] Verify path: `BinaryResponse` → `RouteMeta.binary_response` → image/audio `ToolResultContent` → Discord attachment

## 6. MCP Instructions (Channel Map)

- [ ] After Discord `ready` event, MCP server instructions contain guild/channel map
- [ ] Channel names, IDs, and types (text/voice/forum) are listed
- [ ] Channels grouped by category
- [ ] Create a new channel in Discord — instructions refresh
- [ ] Delete a channel — instructions update

## 7. Known Bugs to Fix

- [ ] LLM sends Discord snowflake IDs as floats (`1.145e+18`) losing precision — tool calls fail. Need to coerce integer args or use string IDs in tool schemas.
- [ ] LLM tried to use `get_channel_history` to answer "what was the first thing I said" instead of using the conversation context. System prompt should clarify when to use tools vs conversation history.

## 8. Cross-Client Tool Calls

- [ ] From Noema, start a chat session with tools enabled
- [ ] Ask the LLM to send a message to a Discord channel by name
- [ ] LLM uses `send_message` tool with correct channel_id (from MCP instructions)
- [ ] Message appears in Discord
- [ ] Ask the LLM to list Discord channels — `list_channels` tool call works
