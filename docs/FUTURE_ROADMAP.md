# Simply Platform — Post-v1 Roadmap

Features beyond the v1.0 scope. For v1.0 implementation stages, see [v1.0/DESIGN.md](v1.0/DESIGN.md). For architecture, see [designs/ARCHITECTURE.md](designs/ARCHITECTURE.md).

---

## Search & Knowledge

| Feature | Complexity | Description |
|---------|------------|-------------|
| Embedding infrastructure + semantic search | High | Vector storage, hybrid search over all UCM content |
| Full RAG pipeline | High | Query → embed → search → inject → LLM |
| Wiki-style cross-linking | Medium | `[[doc:Title]]` syntax, backlinks panel |
| Hierarchical tags | High | Multi-tagging, tag hierarchy for documents and conversations |

---

## Intent System Use Cases

These build on the Event & Intent System delivered in v1.0 (Stages 7-10).

| Feature | Intent Pattern | Description |
|---------|----------------|-------------|
| Dynamic Typst functions | `render.before.*` → transform | Evaluate Typst functions at render time |
| Auto-journaling | `conversation.turn_produced` → `execute_prompt` | Extract insights from conversations |
| Active context engine | Intents + context documents | Contextual nudges and awareness |

---

## Multimodal

| Feature | Complexity | Description |
|---------|------------|-------------|
| Image generation | Medium | Stable Diffusion, DALL-E, Flux |
| PDF extraction | Medium | OCR, image extraction, conversion |
| Documentation generation | High | Generate docs from conversations |

---

## External Integrations

Implemented as `EventSource` + `ActionHandler` plugins — no core changes needed.

| Integration | Events | Actions |
|-------------|--------|---------|
| GitHub | `github.pr_opened`, `github.issue_created` | `create_issue`, `comment_pr` |
| Notion | `notion.page_updated` | `update_page` |
| Google Calendar | `calendar.event_starting` | `create_event` |
| Email | `email.received` | `send_email` |

---

## Future Platforms

New presentation layer crates that connect to simply-core.

| Platform | Notes |
|----------|-------|
| Telegram | New crate, connects to simply-core |
| WhatsApp | New crate, connects to simply-core |
| WebRTC / simply-chris.ai/meet | New crate, voice + video via simply-core |
| Noema Web (browser) | Requires noema-backend extraction |
| Cloud sync / multi-device | Requires backend service |
