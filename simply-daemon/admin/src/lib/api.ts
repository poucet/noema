// Admin API client — all calls go to the daemon on the same origin.

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

export interface SessionInfo {
  id: string;
  model_id: string;
  persistence: string;
  created_at: string;
}

async function json<T>(url: string, init?: RequestInit): Promise<T> {
  const res = await fetch(url, init);
  if (!res.ok) throw new Error(`${res.status}: ${await res.text()}`);
  return res.json();
}

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
  createUser: (email: string) =>
    json<UserInfo>('/admin/api/users', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ email }),
    }),

  getConnections: () => json<ConnectionInfo[]>('/admin/api/connections'),

  getSessions: async (): Promise<SessionInfo[]> => {
    try { return await json<SessionInfo[]>('/session'); }
    catch { return []; }
  },

  killDaemon: () => fetch('/daemon/kill', { method: 'POST' }),
};

export const PROVIDERS = [
  { id: 'anthropic', name: 'Anthropic', url: 'https://console.anthropic.com/settings/keys', placeholder: 'sk-ant-...' },
  { id: 'openai', name: 'OpenAI', url: 'https://platform.openai.com/api-keys', placeholder: 'sk-...' },
  { id: 'google', name: 'Google (Gemini)', url: 'https://aistudio.google.com/apikey', placeholder: 'AIza...' },
  { id: 'mistral', name: 'Mistral', url: 'https://console.mistral.ai/api-keys', placeholder: '' },
] as const;
