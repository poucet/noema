# v1.0 — Manual Test Checklist

Verification tests that should pass before moving to the next major milestone. Run these against a live daemon + Discord bot.

Daemon default ports: WS 9800, REST 9801.

---

## 1. Daemon REST APIs

```bash
BASE=http://127.0.0.1:9801
```

### Health & Info
- [ ] `curl $BASE/daemon` — returns `{"status":"ok"}`
- [ ] `curl $BASE/daemon/version` — returns version string
- [ ] `curl $BASE/model` — lists available models
- [ ] `curl $BASE/model/provider` — lists providers
- [ ] `curl $BASE/model/default` — returns default model ID

### Conversations
- [ ] `curl $BASE/conversation` — lists conversations (may be empty)
- [ ] `curl -X POST $BASE/conversation -d '{"name":"test"}'` — creates conversation, returns ID
- [ ] `curl $BASE/conversation/{id}/messages` — returns messages

### MCP Servers
- [ ] `curl $BASE/mcp` — lists configured MCP servers
- [ ] `curl $BASE/mcp/tools` — lists all tools across all servers

### Binary Assets
- [ ] Upload: `curl -X POST $BASE/asset -H 'Content-Type: image/png' --data-binary @test.png` — returns asset ID
- [ ] Download: `curl $BASE/asset/{hash}` — returns raw binary with correct Content-Type
- [ ] Verify immutable cache headers (Cache-Control, ETag) on asset download

## 2. Lumina LLM Chat

- [ ] Start daemon (`simply-daemon`) and Lumina (`lumina`)
- [ ] Lumina connects to Discord and posts status message
- [ ] Create a chat channel via `/chat new`
- [ ] Send a message in the AI chat channel — Lumina responds via LLM
- [ ] Response streams back with debounced edits (not all at once)
- [ ] `/chat model <model_id>` changes the model for the channel
- [ ] `/chat pause` stops responses, `/chat resume` restarts them
- [ ] Channel history is loaded as conversation context (check LLM sees prior messages)

## 3. Lumina MCP Tools (via daemon)

- [ ] Lumina registers as MCP service on connect (check daemon logs: "ephemeral MCP service registered")
- [ ] `/tool list` shows all tools from daemon + lumina-discord server
- [ ] Tool descriptions and param counts are correct
- [ ] Pagination works if tool list exceeds one page

## 4. /tool call — Text Tools

- [ ] `/tool call list_channels` — modal opens with guild_id field
- [ ] Fill in guild_id, submit — returns channel list as embed
- [ ] `/tool call send_message` — modal with channel_id + content fields
- [ ] Submit — message appears in the target Discord channel
- [ ] `/tool call get_channel_history` — returns message history as embed
- [ ] `/tool call search_messages` — finds messages matching query
- [ ] `/tool call list_guilds` — returns guilds (no params, executes immediately)
- [ ] Tools that error show red error embeds

## 5. /tool call — Multimodal Content

- [ ] Upload an image via `curl -X POST $BASE/asset -H 'Content-Type: image/png' --data-binary @test.png`
- [ ] `/tool call get_blob` with the hash — image returned as Discord attachment (not JSON text)
- [ ] Audio asset returns as audio file attachment
- [ ] Verify path: `BinaryResponse` → `RouteMeta.binary_response` → image/audio `ToolResultContent` → Discord attachment

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
