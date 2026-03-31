# Voice

**Parent:** [v1.0 Roadmap](../../ROADMAP.md)
**Priority:** P0 (core motivation for the rewrite)
**Complexity:** L
**Depends on:** Lumina complete
**Note:** Voice library (Stage 1) has no dependency on Lumina or Content/Events — can be built early. Integration (Stage 2+) requires the daemon.

---

## Goal

Voice works on desktop and Discord. Speak into mic, get STT, agent responds, TTS plays back. DAVE-encrypted audio in Discord voice channels.

**Key architecture:** STT and TTS live in simply-daemon (via simply-voice). Clients (Noema via CPAL, Lumina via songbird) convert platform audio to a common format and stream it to the daemon over a voice API. The daemon runs the full pipeline: VAD → STT → agent → TTS → audio back to client.

The voice streaming API is generic — any audio source can plug in. This means future sources like WebRTC don't need to be built into the daemon or any client. An RTC service is just an external action service that exposes MCP tools (`join_rtc`, `leave_rtc`) and streams audio to the daemon's voice API like any other client.

---

## Stages

### Stage 1 — Voice Library

**Goal:** Standalone `simply-voice` crate with provider traits and Voxtral implementation.

**Complexity:** M

**Tasks:**
- [ ] Create `simply-voice/` crate in workspace
- [ ] Define provider traits: `SttProvider`, `TtsProvider`
- [ ] Implement Voxtral provider (STT + TTS via Mistral realtime API)
- [ ] VAD (voice activity detection) module
- [ ] Audio format utilities: PCM conversion, sample rate handling, common format spec
- [ ] Provider configuration: API keys, model selection, voice selection

**Verify:** Unit tests pass for provider traits + Voxtral integration tests.

---

### Stage 2 — Voice in Daemon

**Goal:** Voice pipeline runs in simply-daemon. Clients stream audio in a common format, daemon handles STT/TTS.

**Complexity:** M

**Tasks:**
- [ ] Integrate `simply-voice` into simply-daemon
- [ ] Voice streaming API: clients send audio chunks, daemon returns transcriptions + TTS audio
- [ ] Full pipeline in daemon: VAD → STT → agent → TTS
- [ ] Define common audio format for client↔daemon streaming (PCM sample rate, encoding)
- [ ] Ensure the voice API is generic enough for any audio source (not coupled to CPAL or songbird)
- [ ] `list_voices` API

**Verify:** Voice pipeline works end-to-end through the daemon API (can test with a simple audio file client).

---

### Stage 3 — Desktop Voice

**Goal:** Voice conversation works via desktop mic/speaker through the daemon.

**Complexity:** M

**Tasks:**
- [ ] Noema captures audio via CPAL, converts to common format
- [ ] Streams to simply-daemon voice API
- [ ] Receives TTS audio back, plays through CPAL
- [ ] Audio session management: start/stop voice mode in Noema UI
- [ ] Handle interruptions: user speaks while TTS is playing

**Verify:** Voice conversation via desktop mic/speaker using Voxtral. Speak, hear response.

---

### Stage 4 — Discord Voice

**Goal:** Full voice in Discord with DAVE encryption.

**Complexity:** L

**Tasks:**
- [ ] Add songbird to Lumina crate
- [ ] Songbird captures Discord audio, converts to common format
- [ ] Streams to simply-daemon voice API, receives TTS audio back
- [ ] Port VoiceCog basics: `/voice join`, `/voice leave`, `/voice converse`
- [ ] Pipeline: songbird → common format → daemon STT → agent → daemon TTS → common format → songbird
- [ ] Verify/contribute DAVE protocol support in songbird
- [ ] Handle multi-user voice: distinguish speakers, manage turn-taking

**Verify:** Join Discord voice channel, speak, agent responds with voice. DAVE works.

---

## Dependencies

```
Stage 1 → Stage 2 → Stage 3
                  ↘
                   Stage 4
```

Stages 3 and 4 can run in parallel after Stage 2.

---

## Future: RTC and Other Audio Sources

RTC (WebRTC, Google Meet, etc.) is **not** a daemon feature or a client feature — it's an external action service. An RTC service would:

1. Expose MCP tools: `join_rtc(url)`, `leave_rtc()`
2. When the daemon calls `join_rtc`, connect to the RTC session
3. Stream audio to/from the daemon's voice API (same common format as CPAL/songbird)
4. Register event sources: `rtc.user_joined`, `rtc.user_left`

User flow: "join the meeting at https://meet.google.com/abc" → agent calls `join_rtc` tool → RTC service joins and starts streaming → daemon runs STT/TTS.

This validates the architecture: any audio source that can produce/consume the common format can plug into the daemon's voice pipeline without changes to the daemon, Noema, or Lumina. See [CORE_SERVICE.md](../../../designs/CORE_SERVICE.md) for the action service pattern.
