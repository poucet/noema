# RTC — WebRTC Action Service

**Parent:** [v1.0 Roadmap](../../ROADMAP.md)
**Priority:** P1
**Complexity:** L
**Depends on:** Voice Stage 2 (daemon voice API must exist)

---

## Goal

An external action service that joins WebRTC/RTC sessions (Google Meet, custom rooms, etc.) on command and streams audio to/from simply-daemon's voice API. Validates the architecture: any audio source that speaks the common format can plug into the voice pipeline without daemon changes.

User says "join the meeting at https://..." → agent calls `join_rtc` tool → RTC service joins → audio flows through the daemon's STT/TTS pipeline.

---

## Stages

### Stage 1 — RTC Service Skeleton

**Goal:** A standalone service that registers with simply-daemon and exposes MCP tools.

**Complexity:** S

**Tasks:**
- [ ] New crate (or standalone binary): `simply-rtc/`
- [ ] Registers with daemon via `POST /register` with MCP endpoint
- [ ] Exposes MCP tools: `join_rtc(url)`, `leave_rtc()`, `list_sessions()`
- [ ] Registers event sources: `rtc.user_joined`, `rtc.user_left`, `rtc.session_started`

**Verify:** Service registers with daemon, tools appear in global MCP registry, agent can discover them.

---

### Stage 2 — WebRTC Audio

**Goal:** Service can join a WebRTC session and stream audio to/from the daemon.

**Complexity:** M

**Tasks:**
- [ ] Add WebRTC client crate (`webrtc-rs` or `str0m`)
- [ ] Minimal join flow: connect to a room, receive audio stream
- [ ] Convert WebRTC audio to common format, stream to daemon voice API
- [ ] Receive TTS audio back from daemon, play into RTC session
- [ ] Handle session lifecycle: join, leave, reconnect

**Verify:** Agent calls `join_rtc` → service joins RTC session → daemon receives audio → STT produces transcript.

---

### Stage 3 — Transcription & Participation

**Goal:** Full voice participation in RTC sessions — transcribe, listen, and optionally respond.

**Complexity:** M

**Tasks:**
- [ ] Multi-speaker handling: distinguish participants in the RTC session
- [ ] Transcription mode: STT only, no TTS response (passive listening)
- [ ] Participation mode: full STT → agent → TTS loop (active voice participation)
- [ ] Push events for conversation content: `rtc.transcript` events into the event bus
- [ ] Handle interruptions and turn-taking in multi-party calls

**Verify:** Bot joins an RTC session, transcribes conversation, can optionally participate with voice. Transcript events flow into the intent system.

---

## Dependencies

```
Stage 1 → Stage 2 → Stage 3 (sequential)
```

Depends on Voice Stage 2 (daemon voice API). Independent of Content, Events, Discord phases.
