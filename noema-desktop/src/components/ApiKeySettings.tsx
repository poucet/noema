import { useState, useEffect, useCallback } from "react";
import * as tauri from "../tauri";
import type { ProviderInfo } from "../tauri";

export function ApiKeySettings() {
  const [providers, setProviders] = useState<ProviderInfo[]>([]);
  const [keyStatus, setKeyStatus] = useState<Record<string, boolean>>({});
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [editingProvider, setEditingProvider] = useState<string | null>(null);
  const [apiKeyInput, setApiKeyInput] = useState("");
  const [saving, setSaving] = useState(false);

  const loadData = useCallback(async () => {
    try {
      setLoading(true);
      const [providerList, status] = await Promise.all([
        tauri.getProviderInfo(),
        tauri.getApiKeyStatus(),
      ]);
      setProviders(providerList);
      setKeyStatus(status);
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadData();
  }, [loadData]);

  const handleSaveApiKey = async (provider: string) => {
    if (!apiKeyInput.trim()) return;

    try {
      setSaving(true);
      setError(null);
      await tauri.setApiKey(provider, apiKeyInput.trim());
      setKeyStatus((prev) => ({ ...prev, [provider]: true }));
      setEditingProvider(null);
      setApiKeyInput("");
    } catch (err) {
      setError(String(err));
    } finally {
      setSaving(false);
    }
  };

  const handleRemoveApiKey = async (provider: string) => {
    try {
      setError(null);
      await tauri.removeApiKey(provider);
      setKeyStatus((prev) => ({ ...prev, [provider]: false }));
    } catch (err) {
      setError(String(err));
    }
  };

  const startEditing = (provider: string) => {
    setEditingProvider(provider);
    setApiKeyInput("");
  };

  const cancelEditing = () => {
    setEditingProvider(null);
    setApiKeyInput("");
  };

  if (loading) {
    return (
      <div className="text-center py-8 text-muted">Loading providers...</div>
    );
  }

  // Separate providers that require API keys from those that don't
  const providersWithKeys = providers.filter((p) => p.requiresApiKey);
  const providersWithoutKeys = providers.filter((p) => !p.requiresApiKey);

  return (
    <div className="space-y-6">
      {/* Error banner */}
      {error && (
        <div className="px-4 py-2 bg-red-900/50 text-red-200 text-sm rounded-lg">
          {error}
          <button onClick={() => setError(null)} className="ml-2 underline">
            dismiss
          </button>
        </div>
      )}

      {/* Providers requiring API keys */}
      <div>
        <h3 className="text-sm font-medium text-gray-300 mb-3">
          API Key Required
        </h3>
        <ul className="space-y-3">
          {providersWithKeys.map((provider) => {
            const isConfigured = keyStatus[provider.name];
            const isEditing = editingProvider === provider.name;

            return (
              <li
                key={provider.name}
                className="border border-gray-700 rounded-lg p-4"
              >
                <div className="flex items-center justify-between">
                  <div className="flex items-center gap-3">
                    {/* Status indicator */}
                    <div
                      className={`w-3 h-3 rounded-full ${
                        isConfigured ? "bg-teal-500" : "bg-gray-500"
                      }`}
                    />
                    <div>
                      <h4 className="font-medium text-foreground capitalize">
                        {provider.name}
                      </h4>
                      <p className="text-xs text-muted">
                        {provider.apiKeyEnv
                          ? `Fallback: ${provider.apiKeyEnv}`
                          : "No environment variable fallback"}
                      </p>
                    </div>
                  </div>
                  <div className="flex items-center gap-2">
                    {isConfigured ? (
                      <>
                        <span className="text-xs text-teal-400">
                          Configured
                        </span>
                        <button
                          onClick={() => startEditing(provider.name)}
                          className="px-3 py-1.5 text-sm bg-elevated hover:bg-background text-gray-200 rounded"
                        >
                          Update
                        </button>
                        <button
                          onClick={() => handleRemoveApiKey(provider.name)}
                          className="px-3 py-1.5 text-sm bg-red-900/50 hover:bg-red-900 text-red-200 rounded"
                        >
                          Remove
                        </button>
                      </>
                    ) : (
                      <button
                        onClick={() => startEditing(provider.name)}
                        className="px-3 py-1.5 text-sm bg-teal-600 hover:bg-teal-700 text-white rounded"
                      >
                        Add Key
                      </button>
                    )}
                  </div>
                </div>

                {/* Edit form */}
                {isEditing && (
                  <div className="mt-4 pt-4 border-t border-gray-700">
                    <label className="block text-sm font-medium text-gray-300 mb-2">
                      API Key
                    </label>
                    <div className="flex gap-2">
                      <input
                        type="password"
                        value={apiKeyInput}
                        onChange={(e) => setApiKeyInput(e.target.value)}
                        placeholder="Enter your API key"
                        className="flex-1 px-3 py-2 border border-gray-600 rounded bg-elevated text-foreground"
                        autoFocus
                      />
                      <button
                        onClick={() => handleSaveApiKey(provider.name)}
                        disabled={!apiKeyInput.trim() || saving}
                        className="px-4 py-2 bg-teal-600 hover:bg-teal-700 disabled:bg-gray-600 text-white rounded"
                      >
                        {saving ? "Saving..." : "Save"}
                      </button>
                      <button
                        onClick={cancelEditing}
                        className="px-4 py-2 bg-elevated hover:bg-background text-gray-200 rounded"
                      >
                        Cancel
                      </button>
                    </div>
                    <p className="text-xs text-muted mt-2">
                      Your API key will be encrypted and stored locally.
                    </p>
                  </div>
                )}
              </li>
            );
          })}
        </ul>
      </div>

      {/* Providers not requiring API keys */}
      {providersWithoutKeys.length > 0 && (
        <div>
          <h3 className="text-sm font-medium text-gray-300 mb-3">
            No API Key Required
          </h3>
          <ul className="space-y-2">
            {providersWithoutKeys.map((provider) => (
              <li
                key={provider.name}
                className="border border-gray-700 rounded-lg p-4 flex items-center gap-3"
              >
                <div className="w-3 h-3 rounded-full bg-teal-500" />
                <div>
                  <h4 className="font-medium text-foreground capitalize">
                    {provider.name}
                  </h4>
                  <p className="text-xs text-muted">
                    Local provider - no API key needed
                  </p>
                </div>
              </li>
            ))}
          </ul>
        </div>
      )}

      {/* Info section */}
      <div className="text-sm text-muted bg-elevated rounded-lg p-4">
        <p className="mb-2">
          <strong>Priority:</strong> Settings API keys take priority over
          environment variables.
        </p>
        <p>
          If no API key is configured here, the app will fall back to
          environment variables (e.g., CLAUDE_API_KEY, OPENAI_API_KEY).
        </p>
      </div>
    </div>
  );
}
