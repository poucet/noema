# Simply Platform — Post-v1 Roadmap

Features beyond the v1.0 scope. For v1.0 goals, see [v1.0/GOAL.md](v1.0/GOAL.md).

---

## Search & Knowledge

| Feature | Complexity | Description |
|---------|------------|-------------|
| Embedding infrastructure + semantic search | High | Vector storage, hybrid search over all UCM content |
| Full RAG pipeline | High | Query → embed → search → inject context → LLM |
| Wiki-style cross-linking | Medium | `[[doc:Title]]` syntax, backlinks panel |
| Hierarchical tags | High | Multi-tagging, tag hierarchy for documents and conversations |

---

## Intent System Use Cases

These build on the Events phase (event bus + intent engine) once it's delivered.

| Feature | Intent Pattern | Description |
|---------|----------------|-------------|
| Dynamic Typst functions | `render.before.*` → transform | Evaluate Typst functions at render time |
| Auto-journaling | `conversation.turn_produced` → `execute_prompt` | Extract insights from conversations |
| Active context engine | Intents + context documents | Contextual nudges and awareness |
| Scheduled prompts | `cron` → `execute_prompt` | Replaces Python Lumina's ScheduleCog |

---

## Multimodal

The `IntoContent`/`FromContent` trait system and MCP content blocks handle transport. These are the generation/extraction features that build on that.

| Feature | Complexity | Description |
|---------|------------|-------------|
| Image generation | Medium | Stable Diffusion, DALL-E, Flux — exposed as MCP tools |
| PDF extraction | Medium | OCR, image extraction, text conversion |
| Video transcription | Medium | Whisper on video audio tracks |

---

## External Integrations

Implemented as MCP tool servers that register with simply-daemon. Each integration is an independent binary that connects via `register_ephemeral_mcp` (same pattern as Lumina).

| Integration | Tools (MCP) | Events (future) |
|-------------|-------------|------------------|
| GitHub | `create_issue`, `comment_pr`, `list_prs` | `github.pr_opened`, `github.issue_created` |
| Notion | `update_page`, `create_page`, `search` | `notion.page_updated` |
| Google Calendar | `create_event`, `list_events` | `calendar.event_starting` |
| Email | `send_email`, `search_inbox` | `email.received` |
| Brave/Google Search | `web_search` | — |

---

## Future Platforms

New presentation layer crates that connect to simply-daemon via `RemoteDaemon`.

| Platform | Notes |
|----------|-------|
| Telegram | New crate, same `DaemonApi` + `RemoteDaemon` pattern as Lumina |
| WhatsApp | New crate, same pattern |
| WebRTC / simply-chris.ai/meet | New crate, voice + video via daemon's `VoiceApi` |
| Noema Web (browser) | The daemon already exposes REST + WS — needs a web frontend |
| Cloud sync / multi-device | Requires auth layer on daemon (currently localhost-only) |
