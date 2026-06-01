# Simply Platform

A local-first AI platform built in Rust. A shared daemon (`simply-daemon`) provides LLM orchestration, MCP tools, storage, and voice — with Noema (desktop), Lumina (Discord bot), and the Telegram bot as clients.

## Quick Start

```bash
# Install frontend dependencies (first time only)
cd noema && npm install && cd ..

# Run Noema desktop
bin/noema

# Or equivalently
cd noema && npm run tauri dev
```

### API Keys

```bash
export CLAUDE_API_KEY="..."
export OPENAI_API_KEY="..."
export GEMINI_API_KEY="..."
# Ollama runs locally — no key needed
```

## Workspace

| Crate | Description |
|-------|-------------|
| `simply-core` | Library — LLM providers, MCP client, agent orchestration |
| `simply-core/llm` | LLM abstraction with provider implementations |
| `simply-daemon` | Daemon — storage, sessions, MCP server, DaemonApi trait |
| `simply-audio` | Audio — Whisper STT, CPAL backend, voice coordination |
| `noema` | Tauri 2 desktop app (Rust backend + React/TypeScript frontend) |
| `telegram-bot` | Telegram bot client for daemon-backed chat and tools |
| `noema-mcp-gdocs` | Standalone MCP server for Google Docs |
| `commands` | Command framework with proc-macro support |
| `config` | Configuration, path management, API key encryption |

## Architecture

See [docs/v1.0/GOAL.md](docs/v1.0/GOAL.md) for the full design and [docs/v1.0/ROADMAP.md](docs/v1.0/ROADMAP.md) for the implementation plan.

## Other Commands

```
bin/noema build     Build release binaries
bin/noema install   Build and open the macOS installer
bin/noema nuke      Reset all local data
bin/lumina          Run the Discord bot
bin/telegram        Run the Telegram bot
```

## Data Directory

Noema stores data in `~/.local/share/noema/`:

```
database/noema.db    SQLite database
blob_storage/        Content-addressable file storage
config/              Settings
logs/                Application logs
```

### Telegram Bot

Set `TELEGRAM_BOT_TOKEN` or add `telegram.bot_token` to `~/.local/share/noema/config/telegram.toml`, then run:

```bash
bin/telegram
```

Optional allowlists live in `telegram.toml`:

```toml
[telegram]
allowed_user_ids = [123456789]
allowed_chat_ids = [-1001234567890]
```

## License

[MIT](LICENSE) — Christophe Poucet
