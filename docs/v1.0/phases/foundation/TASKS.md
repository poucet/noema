# Foundation — Tasks

**Phase:** Foundation
**Status:** In Progress
**Roadmap:** [ROADMAP.md](ROADMAP.md)

---

## Stage 1 — Workspace Restructure (Complete)

| # | | Task | Priority | Size |
|---|---|------|----------|------|
| 1.1 | ✅ | Rename `noema-core/` → `simply-core/`, update `Cargo.toml` package name + all workspace refs | P0 | S |
| 1.2 | ✅ | Rename `noema-audio/` → `simply-audio/`, update references | P0 | S |
| 1.3 | ✅ | Create `simply-daemon/` crate with `DaemonApi` trait | P0 | S |
| 1.4 | ✅ | Merge `noema-mcp-core/` into `simply-daemon/src/mcp/`, remove standalone crate | P0 | M |
| 1.5 | ✅ | Update workspace `Cargo.toml` members list | P0 | S |
| 1.6 | ✅ | Verify `noema-desktop` builds with restructured deps | P0 | S |

---

## Stage 2 — Daemon

**Goal:** All logic in the daemon so Lumina can be built on top of the same API.

| # | | Task | Priority | Size |
|---|---|------|----------|------|
| 2.1 | ✅ | `DaemonApi` trait: define the core API surface | P0 | M |
| 2.2 | ✅ | In-process implementation of `DaemonApi` | P0 | M |
| 2.3 | ✅ | Wire Noema desktop to use in-process daemon | P0 | L |
| 2.3.1 | ✅ | Decouple Noema from simply-core/llm — only use daemon traits; rename `noema-desktop` → `noema` | P0 | L |
| 2.3.2 | ✅ | Move MCP commands + OAuth flow into daemon (McpApi + OAuthApi) | P0 | M |
| 2.4 | ✅ | Stable OAuth callback port on daemon | P0 | S |
| 2.5 | ⏸️ | DocumentApi on daemon — store/index/query documents (blocked on sidecar design) | P0 | M |
| 2.5.1 | ⏸️ | Rewrite Noema gdocs.rs as thin wrappers (blocked, gdocs disabled temporarily) | P0 | M |
| 2.6 | ✅ | Daemon binary: startup, config loading, graceful shutdown | P0 | M |
| 2.7 | ✅ | WebSocket server + remote `DaemonApi` implementation | P0 | L |
| 2.7.1 | ✅ | Smart discovery: `connect_or_host()`, Noema uses `Arc<dyn DaemonApi>` | P0 | M |
| 2.7.2 | ✅ | `simply-rpc` proc macro: auto-generate WS server dispatch + client impls from trait definitions | P0 | M |
| 2.8 | ✅ | REST server for asset serving (`GET /asset/{hash}`) | P1 | S |
| 2.9 | ⬜ | Peer registry: connected clients, global MCP tool registry | P1 | M |
| 2.10 | ⬜ | MCP client: connect to action services, discover tools | P2 | M |

---

## Stage 3 — Voice Pipeline

**Goal:** Multi-provider voice (STT/TTS/VAD) in the daemon, usable by Noema and Lumina.

| # | | Task | Priority | Size |
|---|---|------|----------|------|
| 3.1 | ⬜ | `simply-voice` crate: provider-agnostic STT/TTS traits | P0 | M |
| 3.2 | ⬜ | Voxtral (Mistral) provider implementation | P0 | M |
| 3.3 | ⬜ | Whisper provider (extract from current simply-audio) | P1 | S |
| 3.4 | ⬜ | VAD in daemon: detect speech pauses, trigger STT | P0 | M |
| 3.5 | ⬜ | VoiceApi implementation: audio stream → STT → text events | P0 | L |
| 3.6 | ⬜ | TTS pipeline: text → audio stream back to client | P0 | M |
| 3.7 | ⬜ | Wire Noema: mic → daemon voice → transcript into chat conversation | P0 | M |
| 3.8 | ⬜ | Pure STT mode: client sends audio, gets text back (no LLM) | P1 | S |
| 3.9 | ⬜ | Pure TTS mode: client sends text, gets audio back | P1 | S |

### Task Details

**3.1 — simply-voice crate**
- Provider-agnostic traits for STT and TTS
- `SttProvider`: `async fn transcribe(audio: AudioStream) -> Stream<TranscriptionEvent>`
- `TtsProvider`: `async fn synthesize(text: &str) -> Stream<AudioFrame>`
- Provider registry (like llm crate's model registry)
- Primary target: Voxtral (Mistral), secondary: Whisper (local)

**3.4 — VAD in daemon**
- Voice Activity Detection runs inside the daemon on incoming audio frames
- Detects speech start/end pauses
- On pause: sends accumulated audio to STT provider
- Emits events: `SpeechStart`, `SpeechEnd`, `Transcription(text)`
- Configurable: pause threshold, min speech duration

**3.5 — VoiceApi implementation**
- `voice_connect(session_id)` returns `VoiceHandle` (already defined)
- Client sends `AudioFrame` via `audio_in` channel
- Daemon runs VAD → STT on the frames
- Transcribed text either:
  - Streamed back to client as `VoiceEvent::Transcription`
  - Fed into a conversation as a user message (Noema mode)
- Over WS: binary frame multiplexing for audio transport

**3.7 — Wire Noema**
- User enables mic in Noema UI
- Audio frames stream to daemon via WS binary frames
- Daemon VAD → STT → transcription
- Transcription becomes a user message in the open conversation
- LLM responds, TTS converts response to audio
- Audio streams back to Noema, played through speaker

**3.8/3.9 — Pure STT/TTS modes**
- For Lumina use cases: record voice channel → get transcripts (no LLM)
- For `/say` commands: text → speech audio sent to voice channel
- For voice model training: just capture and store audio

---

## Task Details (Stage 2)

**2.4 — Stable OAuth callback port**
- Currently `OAuthService` spins up a temporary callback server on a random port per flow
- Daemon should start a single long-lived callback server on a configured port at startup
- Port comes from config (`~/.config/simply/config.toml` → `oauth_callback_port`)
- Enables predictable redirect URIs for Google OAuth console, cloud Lumina, etc.
- Refactor `OAuthService` to accept a shared callback server rather than creating per-flow

**2.5 — DocumentApi on daemon**
- New trait in `api/document.rs`: `import_document`, `list_documents`, `get_document`, `delete_document`, `sync_document`, `get_document_content`
- Daemon implementation uses `DocumentStore`/`StorageCoordinator` for persistence
- Google-specific fetching stays in `noema-mcp-gdocs` crate (pure Google API client)
- Daemon calls `GoogleDocsClient` to fetch, then stores via its own storage
- This separation means Lumina can import docs without any Google-specific code in the daemon trait

**2.7.2 — simply-rpc proc macro** (complete)
- Generic RPC framework at `simply-rpc/` (not daemon-specific)
- `#[rpc_service("prefix")]` annotates a trait → auto-generates dispatch + client
- Annotations: `#[rpc(skip)]`, `#[rpc(stream)]`, `#[rpc(base64_param)]`, `#[rpc(base64_return)]`
- `Dispatcher` with HashMap prefix routing, `ServiceMeta` for compat checking
- WS server is fully generic — takes `DispatchFn`, service wiring in main.rs
- REST server for asset serving (HTTP GET with caching)
- 43 tests
- Design: [RPC_FRAMEWORK.md](../../../designs/RPC_FRAMEWORK.md)

---

## Dependencies

```
Stage 2 (remaining):
  2.5 (DocumentApi) → 2.5.1 (rewrite gdocs.rs) — blocked on sidecar design

Stage 3:
  3.1 (simply-voice) → 3.2/3.3 (providers) → 3.4 (VAD) → 3.5 (VoiceApi impl)
  3.5 → 3.6 (TTS) → 3.7 (Noema integration)
  3.5 → 3.8 (pure STT)
  3.6 → 3.9 (pure TTS)
```
