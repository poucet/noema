# Noema Project Analysis and Plan

## Project Overview

Noema is a sophisticated, agent-based AI assistant framework written in Rust, featuring a modular workspace structure. It supports multiple interfaces (TUI and a Tauri-based GUI) and integrates with various LLM providers (Gemini, Claude, OpenAI) and the Model Context Protocol (MCP).

## Current Architecture

### Workspace Structure
- **`noema-core`**: The heart of the system. Contains the `Agent` trait, `Session` management, `llm` provider abstractions, and `mcp` client logic.
- **`noema-ui`**: A Tauri application with a React frontend. It acts as the primary GUI, featuring persistent chat history (SQLite), voice input, and file attachment processing.
- **`tui`**: A terminal user interface.
- **`commands`**: A robust command registry and dispatch system, used by the TUI.
- **`noema-audio`**: Handles audio capture and playback. It now supports two backends:
    - **`cpal`**: Native audio capture/playback (feature: `backend-cpal`).
    - **`browser`**: WebAudio-based input for the UI (feature: `browser`).
- **`llm`**: Abstraction layer for LLM providers with procedural macros for tool definitions.
- **`noema-ext`**: Extension crate for file processing (PDFs, attachments).
- **`config`**: Centralized configuration and path management.

## Completed Work
- [x] **Audio Refactoring**: Created `noema-audio` with pluggable backends (`cpal`, `browser`).
- [x] **Mobile Voice**: Added on-demand model downloading and mobile path support.
- [x] **Core Refactoring**:
    - Introduced `config::PathManager` for unified path handling.
    - Extracted attachment processing to `noema-ext`.
- [x] **CLI Retirement**: Removed the legacy `cli` crate in favor of the `tui`.

## Future Plan
