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
  UserMessageEvent,
  StreamingMessageEvent,
  MessageCompleteEvent,
  ErrorEvent,
  ModelChangedEvent,
  HistoryClearedEvent,
} from "./generated";
import type { TruncatedEvent } from "./generated/TruncatedEvent";

// Re-export event payload types for consumers
export type { UserMessageEvent, StreamingMessageEvent, MessageCompleteEvent, ErrorEvent, ModelChangedEvent, HistoryClearedEvent } from "./generated";

// Tauri commands
export async function initApp(): Promise<string> {
  return invoke<string>("init_app");
}

export async function getMessages(conversationId: string): Promise<DisplayMessage[]> {
  return invoke<DisplayMessage[]>("get_messages", { conversationId });
}

export async function sendMessage(
  conversationId: string,
  content: InputContentBlock[],
  toolConfig?: ToolConfig
): Promise<void> {
  return invoke<void>("send_message", { conversationId, content, toolConfig });
}


export async function clearHistory(): Promise<void> {
  return invoke<void>("clear_history");
}

export async function setModel(
  conversationId: string,
  modelId: string,
  provider: string
): Promise<string> {
  return invoke<string>("set_model", { conversationId, modelId, provider });
}

export async function listModels(): Promise<ModelInfo[]> {
  return invoke<ModelInfo[]>("list_models");
}

export async function listConversations(): Promise<ConversationInfo[]> {
  return invoke<ConversationInfo[]>("list_conversations");
}

export async function loadConversation(
  conversationId: string
): Promise<DisplayMessage[]> {
  return invoke<DisplayMessage[]>("load_conversation", { conversationId });
}

export async function newConversation(name?: string): Promise<string> {
  return invoke<string>("new_conversation", { name });
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

// Favorite models
export async function getFavoriteModels(): Promise<string[]> {
  return invoke<string[]>("get_favorite_models");
}

export async function toggleFavoriteModel(modelId: string): Promise<string[]> {
  return invoke<string[]>("toggle_favorite_model", { modelId });
}

// Event listeners
export function onUserMessage(
  callback: (payload: UserMessageEvent) => void
): Promise<UnlistenFn> {
  return listen<UserMessageEvent>("user_message", (event) =>
    callback(event.payload)
  );
}

export function onStreamingMessage(
  callback: (payload: StreamingMessageEvent) => void
): Promise<UnlistenFn> {
  return listen<StreamingMessageEvent>("streaming_message", (event) =>
    callback(event.payload)
  );
}

export function onMessageComplete(
  callback: (payload: MessageCompleteEvent) => void
): Promise<UnlistenFn> {
  return listen<MessageCompleteEvent>("message_complete", (event) =>
    callback(event.payload)
  );
}

export function onError(callback: (payload: ErrorEvent) => void): Promise<UnlistenFn> {
  return listen<ErrorEvent>("error", (event) => callback(event.payload));
}

export function onModelChanged(
  callback: (payload: ModelChangedEvent) => void
): Promise<UnlistenFn> {
  return listen<ModelChangedEvent>("model_changed", (event) => callback(event.payload));
}

export function onHistoryCleared(callback: (payload: HistoryClearedEvent) => void): Promise<UnlistenFn> {
  return listen<HistoryClearedEvent>("history_cleared", (event) => callback(event.payload));
}

export function onTruncated(callback: (payload: TruncatedEvent) => void): Promise<UnlistenFn> {
  return listen<TruncatedEvent>("truncated", (event) => callback(event.payload));
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

/**
 * Create a new user document
 */
export async function createDocument(title: string): Promise<DocumentInfoResponse> {
  return invoke<DocumentInfoResponse>("create_document", { title });
}

/**
 * Update a document's title
 */
export async function updateDocumentTitle(
  docId: string,
  title: string
): Promise<void> {
  return invoke<void>("update_document_title", { docId, title });
}

/**
 * Create a new tab in a document
 */
export async function createDocumentTab(
  docId: string,
  title: string,
  parentTabId?: string,
  content?: string
): Promise<DocumentTabResponse> {
  return invoke<DocumentTabResponse>("create_document_tab", {
    docId,
    title,
    parentTabId,
    content,
  });
}

/**
 * Update a document tab's content
 */
export async function updateDocumentTabContent(
  tabId: string,
  content: string
): Promise<void> {
  return invoke<void>("update_document_tab_content", { tabId, content });
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

// Turn/Span management (Phase 3 UCM model)
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

// Fork management
// Matches the backend ForkInfoResponse type
export interface ForkInfo {
  conversationId: string;
  forkedAtTurnId: string;
  turnCount: number;
  createdAt: number;
}

export async function listConversationForks(
  conversationId: string
): Promise<ForkInfo[]> {
  return invoke<ForkInfo[]>("list_conversation_views", { conversationId });
}

/**
 * Regenerate response at a specific turn
 * Creates a new span at the turn and triggers the LLM to generate a new response.
 */
export async function regenerateResponse(
  conversationId: string,
  turnId: string,
  toolConfig?: ToolConfig
): Promise<void> {
  return invoke<void>("regenerate_response", { conversationId, turnId, toolConfig });
}

/**
 * Fork a conversation at a specific turn
 * Creates a new conversation that shares history up to but not including the specified turn.
 * Returns the new conversation's ID.
 */
export async function forkConversation(
  conversationId: string,
  atTurnId: string,
  name?: string
): Promise<string> {
  return invoke<string>("fork_conversation", { conversationId, atTurnId, name });
}

/**
 * Select a specific span at a turn
 * Updates the conversation selection to use the specified span at the given turn.
 */
export async function selectSpan(
  conversationId: string,
  turnId: string,
  spanId: string
): Promise<void> {
  return invoke<void>("select_span", { conversationId, turnId, spanId });
}

/**
 * Response from editing a message
 */
export interface EditMessageResponse {
  newConversationId: string;
  messages: DisplayMessage[];
}

/**
 * Edit a user message, creating a fork with the new content
 * Creates a new conversation forked at the specified turn with the edited content,
 * then triggers the AI to respond to the edited message.
 */
export async function editMessage(
  conversationId: string,
  turnId: string,
  content: InputContentBlock[],
  toolConfig?: ToolConfig
): Promise<EditMessageResponse> {
  return invoke<EditMessageResponse>("edit_message", { conversationId, turnId, content, toolConfig });
}

// Subconversation types
export interface ParentConversationInfo {
  parentConversationId: string;
  atTurnId: string;
  atSpanId: string | null;
}

export interface SubconversationInfo {
  conversationId: string;
  atTurnId: string;
  atSpanId: string | null;
}

/**
 * Spawn a subconversation from a parent conversation.
 * Creates a new conversation linked to the parent via spawned_from relation.
 */
export async function spawnSubconversation(
  parentConversationId: string,
  atTurnId: string,
  atSpanId?: string,
  name?: string
): Promise<string> {
  return invoke<string>("spawn_subconversation", {
    parentConversationId,
    atTurnId,
    atSpanId,
    name,
  });
}

/**
 * Get the parent conversation for a subconversation.
 * Returns null if the conversation has no parent.
 */
export async function getParentConversation(
  conversationId: string
): Promise<ParentConversationInfo | null> {
  return invoke<ParentConversationInfo | null>("get_parent_conversation", {
    conversationId,
  });
}

/**
 * List all subconversations spawned from a parent conversation.
 */
export async function listSubconversations(
  parentConversationId: string
): Promise<SubconversationInfo[]> {
  return invoke<SubconversationInfo[]>("list_subconversations", {
    parentConversationId,
  });
}

/**
 * Get the final result from a subconversation.
 * Returns the text content of the last assistant message.
 */
export async function getSubconversationResult(
  subconversationId: string
): Promise<string | null> {
  return invoke<string | null>("get_subconversation_result", {
    subconversationId,
  });
}

/**
 * Link a subconversation's result back to the parent conversation.
 * Creates a ToolResult message in the parent span.
 */
export async function linkSubconversationResult(
  subconversationId: string,
  parentConversationId: string,
  parentSpanId: string,
  parentTurnId: string,
  toolCallId: string,
  toolName: string
): Promise<void> {
  return invoke<void>("link_subconversation_result", {
    subconversationId,
    parentConversationId,
    parentSpanId,
    parentTurnId,
    toolCallId,
    toolName,
  });
}

// Forked conversation types
export interface ForkedConversationInfo {
  conversationId: string;
  atTurnId: string;
}

/**
 * List all conversations forked from a given conversation.
 */
export async function listForkedConversations(
  conversationId: string
): Promise<ForkedConversationInfo[]> {
  return invoke<ForkedConversationInfo[]>("list_forked_conversations", {
    conversationId,
  });
}

// Cross-reference types
export interface ReferenceInfo {
  id: string;
  fromEntityId: string;
  toEntityId: string;
  relationType: string | null;
  context: string | null;
  createdAt: number;
}

/**
 * Create a cross-reference from one entity to another.
 */
export async function createReference(
  fromEntityId: string,
  toEntityId: string,
  relationType?: string,
  context?: string
): Promise<string> {
  return invoke<string>("create_reference", {
    fromEntityId,
    toEntityId,
    relationType,
    context,
  });
}

/**
 * Delete a cross-reference.
 */
export async function deleteReference(referenceId: string): Promise<boolean> {
  return invoke<boolean>("delete_reference", { referenceId });
}

/**
 * Get all references from an entity (outgoing links).
 */
export async function getEntityReferences(
  entityId: string
): Promise<ReferenceInfo[]> {
  return invoke<ReferenceInfo[]>("get_entity_references", { entityId });
}

/**
 * Get all backlinks to an entity (incoming references).
 */
export async function getEntityBacklinks(
  entityId: string
): Promise<ReferenceInfo[]> {
  return invoke<ReferenceInfo[]>("get_entity_backlinks", { entityId });
}
