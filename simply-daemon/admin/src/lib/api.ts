// Admin API client — all calls go to the daemon on the same origin.
//
// RPC service types are imported from generated types (ts-rs + build.rs).
// Admin-only types (not part of RPC services) are defined here.

import type {
  DocumentInfo,
  DocumentDetail,
  TabInfo,
  SearchHit,
  ReindexStatus,
  EmbeddingQueueStatus,
} from './generated/types';

// --- Admin-only types (no Rust struct, returned as inline JSON) ---

export interface SetupStatus {
  is_configured: boolean;
  api_keys: string[];
  daemon_port: number;
}

export interface Settings {
  user_email: string | null;
  default_model: string | null;
  daemon_port: number | null;
  api_keys: string[];
}

export interface UserInfo {
  id: string;
  email: string;
}

export interface ConnectionInfo {
  id: number;
  name: string | null;
  addr: string;
  connected_at: string;
}

export interface SessionListItem {
  id: string;
  model_id: string;
  persistence: string;
  created_at: string;
}

// --- User context scoping ---

const USER_KEY = 'simply-admin-user-id';

export function setCurrentUser(userId: string | null) {
  if (userId) localStorage.setItem(USER_KEY, userId);
  else localStorage.removeItem(USER_KEY);
}

export function getCurrentUser(): string | null {
  return localStorage.getItem(USER_KEY);
}

function contextHeaders(): Record<string, string> {
  const userId = getCurrentUser();
  if (!userId) return {};
  return {
    'X-Request-Context': JSON.stringify({ scope: { user_id: userId } }),
  };
}

async function json<T>(url: string, init?: RequestInit): Promise<T> {
  const headers = { ...contextHeaders(), ...(init?.headers ?? {}) };
  const res = await fetch(url, { ...init, headers });
  if (!res.ok) throw new Error(`${res.status}: ${await res.text()}`);
  return res.json();
}

// --- Admin API (hand-written endpoints, not part of RPC services) ---

export const api = {
  getSetupStatus: () => json<SetupStatus>('/admin/api/setup-status'),
  getSettings: () => json<Settings>('/admin/api/settings'),

  updateSettings: (data: Record<string, string>) =>
    json('/admin/api/settings', {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(data),
    }),

  setApiKey: (provider: string, apiKey: string) =>
    json('/admin/api/api-key', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ provider, api_key: apiKey }),
    }),

  removeApiKey: (provider: string) =>
    fetch(`/admin/api/api-key/${provider}`, { method: 'DELETE' }),

  getUsers: () => json<UserInfo[]>('/admin/api/users'),
  deleteUser: (id: string) =>
    fetch(`/admin/api/users/${id}`, { method: 'DELETE' }),
  createUser: (email: string) =>
    json<UserInfo>('/admin/api/users', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ email }),
    }),

  getConnections: () => json<ConnectionInfo[]>('/admin/api/connections'),

  getSessions: async (): Promise<SessionListItem[]> => {
    try { return await json<SessionListItem[]>('/api/session'); }
    catch { return []; }
  },

  killDaemon: () => fetch('/api/daemon/kill', { method: 'POST' }),

  // Documents (via /api prefix for RPC routes)
  listDocuments: () => json<DocumentInfo[]>('/api/document/all'),
  searchDocuments: (query: string) => json<DocumentInfo[]>(`/api/document/search?q=${encodeURIComponent(query)}`),
  getDocument: (id: string) => json<DocumentDetail>(`/api/document/${id}`),
  createDocument: (title: string, content?: string) =>
    json<DocumentInfo>('/api/document', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ title, content }),
    }),
  renameDocument: (id: string, title: string) =>
    json(`/api/document/${id}`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ title }),
    }),
  deleteDocument: (id: string) =>
    fetch(`/api/document/${id}`, { method: 'DELETE', headers: contextHeaders() }),
  getTab: (id: string) => json<TabInfo>(`/api/document/tab/${id}`),
  updateTab: (id: string, content: string) =>
    json(`/api/document/tab/${id}`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ request: { content } }),
    }),
  createTab: (documentId: string, title: string, content?: string) =>
    json<TabInfo>(`/api/document/${documentId}/tab`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ request: { title, content } }),
    }),
  deleteTab: (id: string) =>
    fetch(`/api/document/tab/${id}`, { method: 'DELETE', headers: contextHeaders() }),

  // Search / RAG
  search: (query: string, documentType?: string, topK?: number) =>
    json<SearchHit[]>('/api/search', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ query, document_type: documentType, top_k: topK }),
    }),
  reindex: () =>
    json<ReindexStatus>('/api/search/reindex', { method: 'POST' }),
  getQueueStatus: () => json<EmbeddingQueueStatus>('/api/search/status'),
};

export const PROVIDERS = [
  { id: 'anthropic', name: 'Anthropic', url: 'https://console.anthropic.com/settings/keys', placeholder: 'sk-ant-...' },
  { id: 'openai', name: 'OpenAI', url: 'https://platform.openai.com/api-keys', placeholder: 'sk-...' },
  { id: 'google', name: 'Google (Gemini)', url: 'https://aistudio.google.com/apikey', placeholder: 'AIza...' },
  { id: 'mistral', name: 'Mistral', url: 'https://console.mistral.ai/api-keys', placeholder: '' },
] as const;
