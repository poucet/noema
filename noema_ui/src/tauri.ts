import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import type {
  DisplayMessage,
  ModelInfo,
  ConversationInfo,
  McpServerInfo,
  McpToolInfo,
  AddMcpServerRequest,
} from "./types";

// Tauri commands
export async function initApp(): Promise<string> {
  return invoke<string>("init_app");
}

export async function getMessages(): Promise<DisplayMessage[]> {
  return invoke<DisplayMessage[]>("get_messages");
}

export async function sendMessage(message: string): Promise<void> {
  return invoke<void>("send_message", { message });
}

export async function clearHistory(): Promise<void> {
  return invoke<void>("clear_history");
}

export async function setModel(
  modelId: string,
  provider: string
): Promise<string> {
  return invoke<string>("set_model", { modelId, provider });
}

export async function listModels(): Promise<ModelInfo[]> {
  return invoke<ModelInfo[]>("list_models");
}

export async function listConversations(): Promise<ConversationInfo[]> {
  return invoke<ConversationInfo[]>("list_conversations");
}

export async function switchConversation(
  conversationId: string
): Promise<DisplayMessage[]> {
  return invoke<DisplayMessage[]>("switch_conversation", { conversationId });
}

export async function newConversation(): Promise<string> {
  return invoke<string>("new_conversation");
}

export async function deleteConversation(
  conversationId: string
): Promise<void> {
  return invoke<void>("delete_conversation", { conversationId });
}

export async function renameConversation(
  conversationId: string,
  name: string
): Promise<void> {
  return invoke<void>("rename_conversation", { conversationId, name });
}

export async function getModelName(): Promise<string> {
  return invoke<string>("get_model_name");
}

export async function getCurrentConversationId(): Promise<string> {
  return invoke<string>("get_current_conversation_id");
}

// Event listeners
export function onUserMessage(
  callback: (message: DisplayMessage) => void
): Promise<UnlistenFn> {
  return listen<DisplayMessage>("user_message", (event) =>
    callback(event.payload)
  );
}

export function onStreamingMessage(
  callback: (message: DisplayMessage) => void
): Promise<UnlistenFn> {
  return listen<DisplayMessage>("streaming_message", (event) =>
    callback(event.payload)
  );
}

export function onMessageComplete(
  callback: (messages: DisplayMessage[]) => void
): Promise<UnlistenFn> {
  return listen<DisplayMessage[]>("message_complete", (event) =>
    callback(event.payload)
  );
}

export function onError(callback: (error: string) => void): Promise<UnlistenFn> {
  return listen<string>("error", (event) => callback(event.payload));
}

export function onModelChanged(
  callback: (name: string) => void
): Promise<UnlistenFn> {
  return listen<string>("model_changed", (event) => callback(event.payload));
}

export function onHistoryCleared(callback: () => void): Promise<UnlistenFn> {
  return listen<void>("history_cleared", () => callback());
}

// Voice commands
export async function isVoiceAvailable(): Promise<boolean> {
  return invoke<boolean>("is_voice_available");
}

export async function toggleVoice(): Promise<boolean> {
  return invoke<boolean>("toggle_voice");
}

export async function getVoiceStatus(): Promise<string> {
  return invoke<string>("get_voice_status");
}

// Voice events
export type VoiceStatus = "disabled" | "enabled" | "listening" | "transcribing";

export function onVoiceStatus(
  callback: (status: VoiceStatus) => void
): Promise<UnlistenFn> {
  return listen<VoiceStatus>("voice_status", (event) => callback(event.payload));
}

export function onVoiceTranscription(
  callback: (text: string) => void
): Promise<UnlistenFn> {
  return listen<string>("voice_transcription", (event) => callback(event.payload));
}

export function onVoiceError(
  callback: (error: string) => void
): Promise<UnlistenFn> {
  return listen<string>("voice_error", (event) => callback(event.payload));
}

// MCP Server commands
export async function listMcpServers(): Promise<McpServerInfo[]> {
  return invoke<McpServerInfo[]>("list_mcp_servers");
}

export async function addMcpServer(request: AddMcpServerRequest): Promise<void> {
  return invoke<void>("add_mcp_server", { request });
}

export async function removeMcpServer(serverId: string): Promise<void> {
  return invoke<void>("remove_mcp_server", { serverId });
}

export async function connectMcpServer(serverId: string): Promise<number> {
  return invoke<number>("connect_mcp_server", { serverId });
}

export async function disconnectMcpServer(serverId: string): Promise<void> {
  return invoke<void>("disconnect_mcp_server", { serverId });
}

export async function getMcpServerTools(serverId: string): Promise<McpToolInfo[]> {
  return invoke<McpToolInfo[]>("get_mcp_server_tools", { serverId });
}

export async function testMcpServer(serverId: string): Promise<number> {
  return invoke<number>("test_mcp_server", { serverId });
}

export async function startMcpOauth(serverId: string): Promise<string> {
  return invoke<string>("start_mcp_oauth", { serverId });
}

export async function completeMcpOauth(serverId: string, code: string): Promise<void> {
  return invoke<void>("complete_mcp_oauth", { serverId, code });
}

// OAuth events (from deep link handler)
export function onOauthComplete(
  callback: (serverId: string) => void
): Promise<UnlistenFn> {
  return listen<string>("oauth_complete", (event) => callback(event.payload));
}

export function onOauthError(
  callback: (error: string) => void
): Promise<UnlistenFn> {
  return listen<string>("oauth_error", (event) => callback(event.payload));
}

// Browser voice commands (WebAudio-based)
export async function startVoiceSession(): Promise<void> {
  return invoke<void>("start_voice_session");
}

export async function processAudioChunk(samples: number[]): Promise<void> {
  return invoke<void>("process_audio_chunk", { samples });
}

export async function stopVoiceSession(): Promise<string | null> {
  return invoke<string | null>("stop_voice_session");
}

// File commands
export async function saveFile(
  data: string,
  filename: string,
  mimeType: string
): Promise<boolean> {
  return invoke<boolean>("save_file", { data, filename, mimeType });
}

// Logging
export async function logDebug(
  level: string,
  source: string,
  message: string
): Promise<void> {
  return invoke<void>("log_debug", { level, source, message });
}
