// Thin wrapper for the admin-only `/admin/api/*` endpoints that aren't part
// of the generated @simply/client surface. Mirrors the admin UI's local
// facade so the settings components look the same on both clients.
//
// (If noema and admin accumulate more shared settings UI, promoting this +
// the settings panels into a `@simply/admin-ui` package is the obvious next
// step — same play we did with @simply/entity-ui.)

import { getTransport } from '@simply/client';

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

const t = () => getTransport();

export const adminApi = {
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
};

export const PROVIDERS = [
  { id: 'anthropic', name: 'Anthropic', url: 'https://console.anthropic.com/settings/keys', placeholder: 'sk-ant-...' },
  { id: 'openai', name: 'OpenAI', url: 'https://platform.openai.com/api-keys', placeholder: 'sk-...' },
  { id: 'google', name: 'Google (Gemini)', url: 'https://aistudio.google.com/apikey', placeholder: 'AIza...' },
  { id: 'mistral', name: 'Mistral', url: 'https://console.mistral.ai/api-keys', placeholder: '' },
] as const;
