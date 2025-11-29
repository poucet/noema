# Noema Design Document

## 1. Overview

Noema is a local-first, voice-enabled AI assistant built with Rust and React. It is designed to be modular, privacy-focused, and extensible, featuring a sophisticated agentic architecture that supports the Model Context Protocol (MCP) for dynamic tool integration.

The project is structured as a Cargo workspace with multiple crates, separating core logic, audio processing, UI, and specific capabilities.

## 2. Architecture

### 2.1 High-Level Component Diagram

```mermaid
graph TD
    User[User] <--> UI[noema-ui (React + Tauri)]
    UI <--> Backend[Tauri Commands]
    
    subgraph "Rust Backend"
        Backend --> Engine[ChatEngine (noema-core)]
        
        Engine --> Agent[Agent Trait]
        Engine --> Store[SqliteStore]
        
        Agent --> Models[LLM Providers]
        Agent --> Tools[Tool Registry]
        
        Tools -.-> MCP[MCP Clients (rmcp)]
        
        Engine -.-> Audio[noema-audio]
        Audio --> VAD[Voice Activity Detection]
        Audio --> Whisper[Whisper Transcription]
        
        Engine -.-> Ext[noema-ext]
    end
    
    Models --> APIs[External LLM APIs (Ollama, Gemini, etc.)]
    MCP <--> MCPServers[External MCP Servers]
```

### 2.2 Crate Structure

*   **`bin/noema`**: (Legacy/Dev) CLI binary entry point.
*   **`noema-ui`**: The primary user interface. A hybrid application using Tauri (Rust) for the backend and React (TypeScript) for the frontend.
*   **`noema-core`**: The heart of the application. Contains the `Agent` abstractions, `ChatEngine`, state management, and storage logic.
    *   **`llm`**: Sub-crate defining the `ChatModel` abstraction and provider implementations (Ollama, Gemini, Claude, OpenAI).
*   **`noema-audio`**: Handles audio input/output.
    *   **Input**: Uses `cpal` for capturing raw audio.
    *   **Processing**: Implements a custom energy-based Voice Activity Detector (VAD).
    *   **Transcription**: Wraps `whisper-rs` (bindings to `whisper.cpp`) for local speech-to-text.
*   **`noema-ext`**: Extensions for specific content processing, such as PDF text extraction.
*   **`commands`**: (Legacy) CLI command handling logic.
*   **`config`**: Configuration loading and path management.

## 3. Core Components

### 3.1 Agent Architecture (`noema-core/src/agents/`)

Noema uses a trait-based agent system defined in `noema-core/src/agent.rs`. The `Agent` trait defines the interface for executing turns in a conversation, supporting both streaming and non-streaming responses.

*   **`SimpleAgent`**: A basic chatbot agent that sends user input to the LLM and returns the response.
*   **`ToolAgent`**: An agent equipped with a static set of tools.
*   **`McpAgent`**: The advanced agent implementation. It integrates with the **Model Context Protocol (MCP)**.
    *   **Dynamic Discovery**: Instead of static tools, it queries an `McpToolRegistry` at the start of each turn.
    *   **Execution Loop**: It manages a loop (up to `max_iterations`) where the LLM can request tool calls, the agent executes them via connected MCP servers, and feeds the results back to the LLM.

### 3.2 LLM Abstraction (`noema-core/llm/`)

The `ChatModel` trait abstracts away the differences between LLM providers. It defines methods for `chat` (request-response) and `stream_chat` (async stream).

*   **Providers**: Implemented via the `delegate_provider_enum` macro.
    *   `Ollama`: Local inference.
    *   `Gemini`: Google's Gemini API.
    *   `Claude`: Anthropic's Claude API.
    *   `OpenAI`: OpenAI's GPT models.
*   **Data Models**: Uses a unified `ChatPayload` structure capable of representing text, images, audio, tool calls, and tool results (multimodal support).

### 3.3 Audio Pipeline (`noema-audio/`)

*   **Capture**: Reads raw f32 audio samples at 16kHz.
*   **VAD**: The `VoiceActivityDetector` monitors energy levels to detect speech segments. It maintains a state machine (`Silence` -> `PossibleSpeech` -> `Speech` -> `PossibleSilence`) to robustly identify user utterances while filtering brief noise.
*   **Transcription**: Once a speech segment is finalized, it is passed to the `Transcriber`, which uses a local Whisper model (via `whisper-rs`) to convert audio to text. This text is then injected into the chat engine as a user message.

### 3.4 Storage (`noema-core/src/storage/`)

Persistence is handled by `SqliteStore` using `rusqlite`.

*   **Schema**:
    *   `conversations`: Stores conversation metadata (ID, name, timestamps).
    *   `messages`: Stores individual chat messages with their role, position, and full JSON payload.
*   **Session Management**: `SqliteSession` acts as a cache-through layer. Messages are held in memory for quick access and batched written to SQLite upon `commit`.
*   **Lazy Creation**: Conversations are only persisted to the DB once the first message is committed.

## 4. Frontend-Backend Integration

The `noema-ui` crate uses Tauri to bridge Rust and Web technologies.

### 4.1 Tauri Commands (`noema-ui/src-tauri/src/commands/`)

The backend exposes async commands to the frontend:
*   `send_message`: Accepts text/attachments. Spawns a background task to poll the `ChatEngine` and emit events.
*   `init_app`: Sets up the DB, default model, and MCP registry.
*   `get_messages` / `list_conversations`: Data retrieval commands.
*   `voice`: Controls the audio pipeline (start/stop listening).

### 4.2 Event System

Communication from backend to frontend during long-running tasks (like LLM generation) happens via Tauri events:
*   `streaming_message`: A partial chunk of text from the LLM.
*   `message_complete`: Signals the end of a turn.
*   `model_changed`: Updates UI state when the model changes.
*   `error`: Reports failures.

## 5. Standards and Protocols

### 5.1 Model Context Protocol (MCP)

Noema implements the MCP standard (via the `rmcp` crate dependency) to act as an **MCP Client**. This allows it to connect to any standard MCP server to extend its capabilities without recompiling the core application. The `McpAgent` is responsible for orchestrating these interactions.

### 5.2 Audio

*   **Sample Rate**: Standardized on 16kHz mono for Whisper compatibility.
*   **Format**: 32-bit float (f32) samples.

## 6. Future Considerations

*   **Plugin System**: Expanding the MCP integration to support UI widgets for specific tools.
*   **Local RAG**: Deep integration of the `noema-ext` crate for local document indexing and retrieval.
*   **Voice Output**: Integrating a TTS engine for full voice-to-voice interaction.
