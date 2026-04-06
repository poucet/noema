# v1.0 — Test Journal

Non-blocking issues discovered during testing. Fix later.

---

## Model endpoint (`GET /model`)

- `display_name` is always null
- `context_window` is always null

## Provider endpoint (`GET /model/provider`)

- Response includes `api_key_env` field — shouldn't expose config internals via API

## Conversation endpoint (`POST /conversation`)

- Expects bare JSON string (`"test"`) not object (`{"name":"test"}`) — unconventional API shape

## Conversation endpoint (`GET /conversation/{id}`)

- No single-conversation GET endpoint — `GET /conversation/{id}` returns "not found" (only list and messages work)

## MCP tools (`GET /mcp/tools` + Lumina `/tool list`)

- Only lists MCP tools from Lumina (Discord) — daemon's built-in tools are missing from the tool list

## Tool call UX

- Tool params could support autocomplete from related tools (e.g. `get_asset` id field autocompletes from `list_assets` results). Needs a way to annotate param-to-tool relationships in tool schemas.

## API design

- Consider putting all REST API services under a common root (e.g. `/api/` or `/rpc/`) to separate from admin/WS paths

## General

- Responses lack trailing newline — zsh shows `%` at end of output
