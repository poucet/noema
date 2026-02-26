# Noema

A local-first AI assistant built as a native desktop application. Noema provides a rich conversation interface with support for multiple LLM providers, document management, audio processing, and an extensible tool system via the Model Context Protocol (MCP).

## Features

- **Multi-provider LLM support** — Claude, OpenAI, Gemini, Mistral, and Ollama (local)
- **Local-first architecture** — All data stays on your device; cloud is opt-in
- **Conversation forking** — Branch conversations at any turn with view-based navigation
- **Alternative responses** — Compare outputs from different models at the same turn
- **Document management** — Multi-tab documents with revision history
- **Content-addressable storage** — Immutable content blocks with origin tracking
- **MCP tool system** — Extensible agent capabilities via Model Context Protocol
- **Audio support** — Speech-to-text via Whisper
- **Google Docs integration** — MCP server for Google Docs access

## Architecture

Noema follows a three-layer data model:

| Layer | Purpose | Examples |
|-------|---------|---------|
| **Content** | Immutable data | Text blocks, binary assets, blob storage |
| **Structure** | Mutable organization | Turns, spans, messages, views, documents, tabs |
| **Identity** | Addressable entities | @mentions, naming, entity relationships |

### Workspace Crates

| Crate | Description |
|-------|-------------|
| `noema-desktop` | Tauri 2 desktop app (Rust backend + React/TypeScript frontend) |
| `noema-core` | Core library — agents, storage, conversation management |
| `noema-core/llm` | LLM abstraction layer with provider implementations |
| `noema-audio` | Audio processing (Whisper STT, CPAL backend) |
| `noema-ext` | Extension utilities (PDF extraction, document parsing) |
| `noema-mcp-core` | MCP server exposing internal tools |
| `noema-mcp-gdocs` | MCP server for Google Docs integration |
| `commands` | Command framework with proc-macro support |
| `config` | Configuration, path management, API key encryption |

## Tech Stack

- **Backend:** Rust (edition 2021), Tokio async runtime
- **Frontend:** React, TypeScript, Tailwind CSS, Vite
- **Desktop:** Tauri 2
- **Database:** SQLite
- **Protocols:** MCP (Model Context Protocol) via rmcp

## Prerequisites

- [Rust](https://rustup.rs/) (stable)
- [Node.js](https://nodejs.org/) and npm
- [Tauri CLI](https://v2.tauri.app/start/prerequisites/)

## Getting Started

```bash
# Clone the repository
git clone https://github.com/simply/noema.git
cd noema

# Install frontend dependencies
cd noema-desktop && npm install && cd ..

# Run in development mode
bin/noema gui

# Build for release
bin/noema build
```

### CLI Commands

```
bin/noema gui       Run the Tauri dev server
bin/noema build     Build release binaries
bin/noema install   Build and open the macOS installer
bin/noema nuke      Reset all local data
```

## Configuration

### API Keys

Set provider API keys as environment variables:

```bash
export CLAUDE_API_KEY="..."
export OPENAI_API_KEY="..."
export GEMINI_API_KEY="..."
```

Ollama runs locally and requires no API key.

### Data Directory

Noema stores data in `~/.local/share/noema/`:

```
database/noema.db    SQLite database
blob_storage/        Content-addressable file storage
config/              Settings
logs/                Application logs
```

## License

[MIT](LICENSE) — Christophe Poucet
