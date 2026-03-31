# Core Service Communication

**Status:** Draft
**Version:** 1.0
**Parent:** [ARCHITECTURE.md](ARCHITECTURE.md)

---

## Overview

`simply-core` runs as a daemon and exposes its API over two channels:

**MCP interface** — for agent-facing operations:
- Tool calls, model selection, MCP server management
- The LLM already speaks MCP — no translation layer needed
- Core exposes an MCP server that agents connect to directly

**gRPC interface** — for platform-facing operations:
- Storage CRUD, voice streaming, identity lookups, schedule management
- Strong typing via protobuf, bidirectional streaming for audio
- Internal service calls where type safety and performance matter

---

## Why Hybrid?

MCP is designed for LLM↔tool communication, not service-to-service RPC. Forcing voice byte streaming or complex storage queries through MCP's flat `tool_name + args` model is awkward. gRPC handles typed requests, streaming, and structured responses naturally. Meanwhile, the agent path stays clean — the LLM calls tools via MCP without a translation layer.

---

## Why a Service vs. Shared Library?

- Single writer to storage — no SQLite contention
- Noema and Lumina can run independently or together
- Future platforms (/meet, Telegram) connect the same way
- Voice pipeline state is centralized (one conversation, multiple listeners)

---

## gRPC Service Surface (platform-facing)

- **Storage:** conversation CRUD, entity management, blob storage, document operations
- **Voice:** `transcribe(stream<AudioChunk>)`, `synthesize(text) → stream<AudioChunk>`, `list_voices`
- **Identity:** user lookup, platform linking, role management
- **Events & Intents:** intent CRUD, event source registration, pause/resume, re-resolve fuzzy triggers
- **Documents:** generic CRUD with frontmatter-aware queries (covers todos, notes, contexts, configs)
- **Config:** model selection, provider management

---

## MCP Service Surface (agent-facing)

- All feature operations as MCP tools (search, create_todo, query_knowledge, etc.)
- Agent orchestration: `run_turn`, `prompt`
- External MCP server passthrough

---

## Adding a New Platform

Create a new crate that:
1. Handles platform-specific I/O (gateway, commands, audio)
2. Connects to `simply-core` as a client (gRPC + MCP)
3. Registers platform-specific `EventSource` implementations
4. Registers platform-specific `ActionHandler` implementations
