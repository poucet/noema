// Admin API client — all calls go to the daemon on the same origin.
//
// RPC service types are imported from generated types (ts-rs + build.rs).
// Admin-only types (not part of RPC services) are defined here.

import {
  type SearchHit,
  type ReindexStatus,
  type EmbeddingQueueStatus,
  type ResolveVaultConflictRequest,
  type VaultConflictInfo,
  type VaultExportRequest,
  type VaultExportSummary,
  type VaultScanSummary,
  type Transport,
  createTransport,
  getCurrentUser,
  setCurrentUser,
  vaultApi,
} from '@simply/client';

// Re-export user context functions for existing consumers
export { getCurrentUser, setCurrentUser };

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
  vault_root: string | null;
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
  persistence: string | { Persistent: { conversation_id: string } };
  created_at: string;
}

// --- Shared transport instance ---

let _transport: Transport | null = null;
export function getTransport(): Transport {
  if (!_transport) _transport = createTransport();
  return _transport;
}

// --- Admin API (hand-written endpoints, not part of RPC services) ---

const t = () => getTransport();

export const api = {
  getSetupStatus: () =>
    t().rpc<SetupStatus>('admin.setup_status', 'GET', '/admin/api/setup-status'),
  getSettings: () =>
    t().rpc<Settings>('admin.settings', 'GET', '/admin/api/settings'),

  updateSettings: (data: Record<string, string>) =>
    t().rpc('admin.update_settings', 'PUT', '/admin/api/settings', data),

  setApiKey: (provider: string, apiKey: string) =>
    t().rpc('admin.set_api_key', 'POST', '/admin/api/api-key', { provider, api_key: apiKey }),

  removeApiKey: (provider: string) =>
    t().rpc('admin.remove_api_key', 'DELETE', `/admin/api/api-key/${provider}`),

  getUsers: () =>
    t().rpc<UserInfo[]>('admin.list_users', 'GET', '/admin/api/users'),
  deleteUser: (id: string) =>
    t().rpc('admin.delete_user', 'DELETE', `/admin/api/users/${id}`),
  createUser: (email: string) =>
    t().rpc<UserInfo>('admin.create_user', 'POST', '/admin/api/users', { email }),

  getConnections: () =>
    t().rpc<ConnectionInfo[]>('admin.connections', 'GET', '/admin/api/connections'),

  getSessions: async (): Promise<SessionListItem[]> => {
    try { return await t().rpc<SessionListItem[]>('session.list_sessions', 'GET', '/api/session'); }
    catch { return []; }
  },

  killDaemon: () =>
    t().rpc('daemon.kill', 'POST', '/api/daemon/kill'),

  // Entities (UCM-unified) — admin uses the generated `entityApi` client
  // directly via @simply/client; nothing in this wrapper needs to proxy it.

  // Search / RAG
  search: (query: string, documentType?: string, topK?: number) =>
    t().rpc<SearchHit[]>('search.search', 'POST', '/api/search', { query, document_type: documentType, top_k: topK }),
  reindex: () =>
    t().rpc<ReindexStatus>('search.reindex', 'POST', '/api/search/reindex'),
  getQueueStatus: () =>
    t().rpc<EmbeddingQueueStatus>('search.queue_status', 'GET', '/api/search/status'),

  // Vault-backed Markdown
  exportVaultDocuments: (request: VaultExportRequest): Promise<VaultExportSummary> =>
    vaultApi(t()).exportDocuments(request),
  scanVault: (): Promise<VaultScanSummary> =>
    vaultApi(t()).scan(),
  listVaultConflicts: (): Promise<VaultConflictInfo[]> =>
    vaultApi(t()).listConflicts(),
  resolveVaultConflict: (request: ResolveVaultConflictRequest): Promise<void> =>
    vaultApi(t()).resolveConflict(request),
};

export const PROVIDERS = [
  { id: 'anthropic', name: 'Anthropic', url: 'https://console.anthropic.com/settings/keys', placeholder: 'sk-ant-...' },
  { id: 'openai', name: 'OpenAI', url: 'https://platform.openai.com/api-keys', placeholder: 'sk-...' },
  { id: 'google', name: 'Google (Gemini)', url: 'https://aistudio.google.com/apikey', placeholder: 'AIza...' },
  { id: 'mistral', name: 'Mistral', url: 'https://console.mistral.ai/api-keys', placeholder: '' },
] as const;
