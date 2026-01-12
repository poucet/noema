import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import type {
  AddMcpServerRequest,
  McpServerInfo,
  McpToolInfo,
  ModelInfo,
  ConversationInfo,
  DocumentInfoResponse,
  DocumentContentResponse,
  DocumentTabResponse,
  DisplayMessage,
  InputContentBlock,
  ToolConfig,
} from "./generated";

// Tauri commands
export async function initApp(): Promise<string> {
  return invoke<string>("init_app");
}

export async function getMessages(): Promise<DisplayMessage[]> {
  return invoke<DisplayMessage[]>("get_messages");
}

export async function sendMessage(
  content: InputContentBlock[],
  toolConfig?: ToolConfig
): Promise<void> {
  return invoke<void>("send_message", { content, toolConfig });
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

export async function getConversationPrivate(
  conversationId: string
): Promise<boolean> {
  return invoke<boolean>("get_conversation_private", { conversationId });
}

export async function setConversationPrivate(
  conversationId: string,
  isPrivate: boolean
): Promise<void> {
  return invoke<void>("set_conversation_private", { conversationId, isPrivate });
}

export async function getModelName(): Promise<string> {
  return invoke<string>("get_model_name");
}

export async function getCurrentConversationId(): Promise<string> {
  return invoke<string>("get_current_conversation_id");
}

// Favorite models
export async function getFavoriteModels(): Promise<string[]> {
  return invoke<string[]>("get_favorite_models");
}

export async function toggleFavoriteModel(modelId: string): Promise<string[]> {
  return invoke<string[]>("toggle_favorite_model", { modelId });
}

// Parallel model execution
export async function sendParallelMessage(
  message: string,
  modelIds: string[]
): Promise<void> {
  return invoke<void>("send_parallel_message", { message, modelIds });
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

// Parallel execution events
export interface ParallelStreamingPayload {
  modelId: string;
  message: DisplayMessage;
}

export interface ParallelModelCompletePayload {
  modelId: string;
  messages: DisplayMessage[];
}

export interface ParallelAlternateInfo {
  spanId: string;
  modelId: string;
  modelDisplayName: string;
  messageCount: number;
  isSelected: boolean;
}

export interface ParallelCompletePayload {
  spanSetId: string;
  alternates: ParallelAlternateInfo[];
}

export interface ParallelModelErrorPayload {
  modelId: string;
  error: string;
}

export function onParallelStreamingMessage(
  callback: (payload: ParallelStreamingPayload) => void
): Promise<UnlistenFn> {
  return listen<ParallelStreamingPayload>("parallel_streaming_message", (event) =>
    callback(event.payload)
  );
}

export function onParallelModelComplete(
  callback: (payload: ParallelModelCompletePayload) => void
): Promise<UnlistenFn> {
  return listen<ParallelModelCompletePayload>("parallel_model_complete", (event) =>
    callback(event.payload)
  );
}

export function onParallelComplete(
  callback: (payload: ParallelCompletePayload) => void
): Promise<UnlistenFn> {
  return listen<ParallelCompletePayload>("parallel_complete", (event) =>
    callback(event.payload)
  );
}

export function onParallelModelError(
  callback: (payload: ParallelModelErrorPayload) => void
): Promise<UnlistenFn> {
  return listen<ParallelModelErrorPayload>("parallel_model_error", (event) =>
    callback(event.payload)
  );
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

// Voice events - status can also be "buffering:N" where N is count of queued messages
export type VoiceStatus = string;

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

// Settings
export async function getUserEmail(): Promise<string | null> {
  return invoke<string | null>("get_user_email");
}

export async function setUserEmail(email: string): Promise<void> {
  return invoke<void>("set_user_email", { email });
}

// API Key Settings
export interface ProviderInfo {
  name: string;
  requiresApiKey: boolean;
  apiKeyEnv: string | null;
}

export async function getApiKeyStatus(): Promise<Record<string, boolean>> {
  return invoke<Record<string, boolean>>("get_api_key_status");
}

export async function setApiKey(provider: string, apiKey: string): Promise<void> {
  return invoke<void>("set_api_key", { provider, apiKey });
}

export async function removeApiKey(provider: string): Promise<void> {
  return invoke<void>("remove_api_key", { provider });
}

export async function getProviderInfo(): Promise<ProviderInfo[]> {
  return invoke<ProviderInfo[]>("get_provider_info");
}

// Document commands (episteme-compatible)
export async function listDocuments(): Promise<DocumentInfoResponse[]> {
  return invoke<DocumentInfoResponse[]>("list_documents");
}

export async function getDocument(docId: string): Promise<DocumentInfoResponse | null> {
  return invoke<DocumentInfoResponse | null>("get_document", { docId });
}

export async function getDocumentByGoogleId(googleDocId: string): Promise<DocumentInfoResponse | null> {
  return invoke<DocumentInfoResponse | null>("get_document_by_google_id", { googleDocId });
}

export async function getDocumentContent(docId: string): Promise<DocumentContentResponse> {
  return invoke<DocumentContentResponse>("get_document_content", { docId });
}

export async function getDocumentTab(tabId: string): Promise<DocumentTabResponse | null> {
  return invoke<DocumentTabResponse | null>("get_document_tab", { tabId });
}

export async function deleteDocument(docId: string): Promise<boolean> {
  return invoke<boolean>("delete_document", { docId });
}

export async function syncGoogleDoc(docId: string): Promise<void> {
  return invoke<void>("sync_google_doc", { docId });
}

// Google Docs import types
export interface GoogleDocListItem {
  id: string;
  name: string;
  modifiedTime: string | null;
  createdTime: string | null;
}

// Google Docs import commands
export async function listGoogleDocs(
  query?: string,
  limit?: number
): Promise<GoogleDocListItem[]> {
  return invoke<GoogleDocListItem[]>("list_google_docs", { query, limit });
}

export async function importGoogleDoc(
  googleDocId: string
): Promise<DocumentInfoResponse> {
  return invoke<DocumentInfoResponse>("import_google_doc", { googleDocId });
}

export async function searchDocuments(
  query: string,
  limit?: number
): Promise<DocumentInfoResponse[]> {
  return invoke<DocumentInfoResponse[]>("search_documents", { query, limit });
}

// Turn/Span/View management (Phase 3 UCM model)
export interface SpanInfo {
  id: string;
  modelId: string | null;
  messageCount: number;
  isSelected: boolean;
  createdAt: number;
}

export async function getTurnAlternates(
  turnId: string
): Promise<SpanInfo[]> {
  return invoke<SpanInfo[]>("get_turn_alternates", { turnId });
}

export async function getSpanMessages(
  spanId: string
): Promise<DisplayMessage[]> {
  return invoke<DisplayMessage[]>("get_span_messages", { spanId });
}

// View management (replaces Thread/Fork management)
export interface ViewInfo {
  id: string;
  conversationId: string;
  parentViewId: string | null;
  name: string | null;
  isMain: boolean;
  createdAt: number;
}

export async function listConversationViews(
  conversationId: string
): Promise<ViewInfo[]> {
  return invoke<ViewInfo[]>("list_conversation_views", { conversationId });
}

export async function getCurrentViewId(): Promise<string | null> {
  return invoke<string | null>("get_current_view_id");
}

// Note: The following view-related commands are pending implementation:
// - selectSpan(viewId, turnId, spanId) - Select a specific span at a turn in a view
// - forkView(viewId, atTurnId, name) - Fork a view at a specific turn
// - switchView(viewId) - Switch to a different view
// - editTurn(viewId, turnId, newContent) - Edit content at a turn (creates new span)
