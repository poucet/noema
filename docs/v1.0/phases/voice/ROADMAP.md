# Voice

**Parent:** [v1.0 Roadmap](../../ROADMAP.md)
**Priority:** P0 (core motivation for the rewrite)
**Complexity:** XL
**Depends on:** Foundation complete
**Note:** Voice library (2.1) has no dependency on Foundation or Content/Events — can be built early. Integration (2.3+) requires the core service.

---

## Goal

Voice works on desktop and Discord. Speak into mic, get STT, agent responds, TTS plays back. DAVE-encrypted audio in Discord voice channels. Experimental RTC support validates the pipeline across all audio sources.

See [ARCHITECTURE.md — Voice Architecture](../../../designs/ARCHITECTURE.md#voice-architecture) for the design.

---

## Stages

### 2.1 — Voice Library

**Goal:** Standalone `simply-voice` crate with provider traits and Voxtral implementation.

**Complexity:** M

**Tasks:**
- [ ] Create `simply-voice/` crate in workspace
- [ ] Define provider traits: `SttProvider`, `TtsProvider`
- [ ] Implement Voxtral provider (STT + TTS via Mistral realtime API)
- [ ] VAD (voice activity detection) module
- [ ] Audio format utilities: PCM conversion, sample rate handling
- [ ] Provider configuration: API keys, model selection, voice selection

**Verify:** Unit tests pass for provider traits + Voxtral integration tests.

---

### 2.2 — Desktop Voice

**Goal:** Voice conversation works via desktop mic/speaker.

**Complexity:** M

**Tasks:**
- [ ] Wire `simply-voice` into `simply-audio` (CPAL backend already exists)
- [ ] VAD → STT → agent → TTS pipeline working end-to-end on desktop
- [ ] Audio session management: start/stop voice mode in Noema
- [ ] Handle interruptions: user speaks while TTS is playing

**Verify:** Voice conversation via desktop mic/speaker using Voxtral. Speak, hear response.

---

### 2.3 — Voice in Core Service

**Goal:** Voice pipeline runs in `simply-core` with gRPC streaming.

**Complexity:** M

**Tasks:**
- [ ] Move voice pipeline orchestration into `simply-core`
- [ ] gRPC bidirectional streaming: `transcribe(stream<AudioChunk>) → stream<TranscriptChunk>`
- [ ] gRPC streaming: `synthesize(text) → stream<AudioChunk>`
- [ ] `list_voices` RPC
- [ ] Noema desktop refactored: CPAL captures audio → streams to core → receives TTS audio

**Verify:** Desktop voice works through the core service (gRPC streaming, not in-process).

---

### 2.4 — Discord Voice

**Goal:** Full voice in Discord with DAVE encryption.

**Complexity:** L

**Tasks:**
- [ ] Add songbird to Lumina crate
- [ ] Implement songbird audio backend: bridges songbird PCM ↔ core's gRPC voice streaming
- [ ] Port VoiceCog basics: `/voice join`, `/voice leave`, `/voice converse`
- [ ] Pipeline: songbird → PCM → core STT → agent → core TTS → PCM → songbird
- [ ] Verify/contribute DAVE protocol support in songbird
- [ ] Handle multi-user voice: distinguish speakers, manage turn-taking

**Verify:** Join Discord voice channel, speak, agent responds with voice. DAVE works.

---

### 2.5 — RTC Experimentation

**Goal:** Validate the voice pipeline works across a third audio source (WebRTC).

**Complexity:** L

**Tasks:**
- [ ] Add WebRTC client crate (`webrtc-rs` or `str0m`)
- [ ] Implement WebRTC audio backend for `simply-voice` (same trait as CPAL/songbird)
- [ ] Minimal RTC join flow: connect to a room, receive audio stream
- [ ] Wire through voice pipeline: WebRTC PCM → core STT → transcript
- [ ] TTS response optional at this stage — transcription is the priority

**Verify:** Bot joins an RTC session, STT produces a transcript.

**Note:** This is experimental. API and architecture will evolve, but it validates the pipeline across all three audio sources (desktop, Discord, RTC).

---

## Dependencies

```
2.1 → 2.2 → 2.3 → 2.4
                  ↘
                   2.5
```

2.4 (Discord) and 2.5 (RTC) can run in parallel after 2.3.
