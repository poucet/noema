# Noema Project Analysis and Plan

## Project Overview

Noema is a sophisticated, agent-based AI assistant framework written in Rust, featuring a modular workspace structure. It supports multiple interfaces (CLI, TUI, and a Tauri-based GUI) and integrates with various LLM providers (Gemini, Claude, OpenAI) and the Model Context Protocol (MCP).

## Current Architecture

### Workspace Structure
- **`noema-core`**: The heart of the system. Contains the `Agent` trait, `Session` management, `llm` provider abstractions, and `mcp` client logic.
- **`noema-ui`**: A Tauri application with a React frontend. It acts as the primary GUI, featuring persistent chat history (SQLite), voice input, and file attachment processing.
- **`cli`**: A lightweight command-line interface. currently uses in-memory sessions and a simple local command loop, bypassing the more complex `commands` crate.
- **`tui`**: A terminal user interface (work in progress).
- **`commands`**: A robust command registry and dispatch system, seemingly designed for the TUI or a more advanced CLI, but currently underutilized by the main `cli` crate.
- **`noema-audio`**: Handles audio capture and playback. It now supports two backends:
    - **`cpal`**: Native audio capture/playback (feature: `backend-cpal`).
    - **`browser`**: WebAudio-based input for the UI (feature: `browser`).
- **`llm`**: Abstraction layer for LLM providers with procedural macros for tool definitions.

## Key Findings & Issues

1.  **CLI vs. `commands` Crate Disconnect**:
    - The `cli` crate implements its own simple `enum Command` and parsing logic in `main.rs`.
    - The `commands` crate provides a flexible `CommandRegistry` and tokenization system, but it is not effectively used by the current CLI.
    - **Impact**: Duplication of logic and lack of extensibility in the CLI.

2.  **Persistence Inconsistency**:
    - `noema-ui` uses `SqliteStore` for persistent conversations.
    - `cli` uses `MemorySession`, meaning all history is lost on exit.
    - **Impact**: Inconsistent user experience across interfaces.

3.  **Platform Safety**:
    - Usage of `directories` crate is present but potentially inconsistent.
    - Some path construction logic (e.g., `.noema` directory) might need standardization across crates (`config`, `noema-core`, `noema-ui`).

4.  **Frontend/Backend Logic Leakage**:
    - `noema-ui/src-tauri/src/commands/chat.rs` contains significant business logic (PDF processing, message formatting) that belongs in `noema-core` or `noema-ext`.

## Completed Work (Phase 1)
- [x] **Abstract Audio Backend**: Introduced `AudioStreamer` and `AudioPlayer` traits in `noema-audio`.
- [x] **Platform Feature Flags**: `cpal` backend is now behind `backend-cpal` feature.
- [x] **Browser Backend**: Implemented `BrowserAudioController` and `BrowserAudioStreamer` for `noema-ui` behind `browser` feature.
- [x] **Cleanup**: Removed legacy `audio.rs` and `browser_voice.rs`.
- [x] **Mobile/Cross-Platform Support**: Updated `noema-ui` to support on-demand model download and `app_data_dir` for mobile compatibility. `VoiceAgent` is now backend-agnostic.

## Future Plan

### Phase 2: CLI & Persistence Unification
- [ ] **Integrate `commands` crate**: Refactor `cli` to use the `commands` crate's `CommandRegistry` instead of the ad-hoc enum.
- [ ] **Add Persistence to CLI**: Update `cli` to support `SqliteStore`. Add a flag (e.g., `--session <ID>` or `--db <PATH>`) to resume conversations.
- [ ] **Shared Configuration**: Ensure `cli` and `noema-ui` share the same configuration logic for database paths and API keys.

### Phase 3: Core Refactoring
- [ ] **Move UI Logic**: Extract PDF processing and complex message handling from `noema-ui/.../chat.rs` into `noema-core` or `noema-ext`.
- [ ] **Platform Paths**: Create a central `PathManager` in `config` or `noema-core` to handle all platform-specific paths (DB, logs, config) using `directories` safely and consistently.

### Phase 4: TUI & Features
- [ ] **Revive TUI**: Update the `tui` crate to use the unified `commands` system and `SqliteStore`.
- [ ] **MCP Enhancements**: Expand MCP support in the CLI (currently it seems mostly UI-focused via deep links).
