import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-shell";
import { listen } from "@tauri-apps/api/event";

interface GDocsOAuthStatus {
  serverRunning: boolean;
  serverUrl: string | null;
  credentialsConfigured: boolean;
  isAuthenticated: boolean;
}

export function GoogleDocsSettings() {
  const [status, setStatus] = useState<GDocsOAuthStatus | null>(null);
  const [clientId, setClientId] = useState("");
  const [clientSecret, setClientSecret] = useState("");
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [authenticating, setAuthenticating] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState<string | null>(null);

  const fetchStatus = async () => {
    try {
      const result = await invoke<GDocsOAuthStatus>("get_gdocs_oauth_status");
      setStatus(result);
      setError(null);
    } catch (e) {
      setError(`Failed to get status: ${e}`);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchStatus();

    // Listen for OAuth completion events
    const unlistenComplete = listen<string>("oauth_complete", (event) => {
      if (event.payload === "gdocs") {
        setSuccess("Successfully authenticated with Google!");
        setAuthenticating(false);
        fetchStatus();
      }
    });

    const unlistenError = listen<string>("oauth_error", (event) => {
      setError(`OAuth failed: ${event.payload}`);
      setAuthenticating(false);
    });

    return () => {
      unlistenComplete.then((fn) => fn());
      unlistenError.then((fn) => fn());
    };
  }, []);

  const handleSaveCredentials = async () => {
    if (!clientId.trim()) {
      setError("Client ID is required");
      return;
    }

    setSaving(true);
    setError(null);
    setSuccess(null);

    try {
      await invoke("configure_gdocs_oauth", {
        clientId: clientId.trim(),
        clientSecret: clientSecret.trim() || null,
      });
      setSuccess("Credentials saved successfully");
      setClientId("");
      setClientSecret("");
      await fetchStatus();
    } catch (e) {
      setError(`Failed to save credentials: ${e}`);
    } finally {
      setSaving(false);
    }
  };

  const handleStartOAuth = async () => {
    try {
      setError(null);
      setSuccess(null);
      setAuthenticating(true);
      const authUrl = await invoke<string>("start_mcp_oauth", { serverId: "gdocs" });
      await open(authUrl);
    } catch (e) {
      setError(`Failed to start OAuth: ${e}`);
      setAuthenticating(false);
    }
  };

  if (loading) {
    return (
      <div className="text-muted text-sm">Loading Google Docs status...</div>
    );
  }

  return (
    <div className="space-y-6">
      <div>
        <h3 className="text-lg font-medium text-foreground mb-2">
          Google Docs Integration
        </h3>
        <p className="text-sm text-muted">
          Connect to Google Docs to import and sync documents.
        </p>
      </div>

      {/* Status display */}
      <div className="bg-elevated rounded-lg p-4 space-y-2">
        <div className="flex items-center gap-2">
          <div
            className={`w-2 h-2 rounded-full ${
              status?.serverRunning ? "bg-green-500" : "bg-red-500"
            }`}
          />
          <span className="text-sm text-foreground">
            MCP Server: {status?.serverRunning ? "Running" : "Not Running"}
          </span>
        </div>
        {status?.serverUrl && (
          <div className="text-xs text-muted pl-4">{status.serverUrl}</div>
        )}
        <div className="flex items-center gap-2">
          <div
            className={`w-2 h-2 rounded-full ${
              status?.credentialsConfigured ? "bg-green-500" : "bg-yellow-500"
            }`}
          />
          <span className="text-sm text-foreground">
            Credentials:{" "}
            {status?.credentialsConfigured ? "Configured" : "Not Configured"}
          </span>
        </div>
        <div className="flex items-center gap-2">
          <div
            className={`w-2 h-2 rounded-full ${
              status?.isAuthenticated ? "bg-green-500" : "bg-gray-500"
            }`}
          />
          <span className="text-sm text-foreground">
            Authentication:{" "}
            {status?.isAuthenticated ? "Authenticated" : "Not Authenticated"}
          </span>
        </div>
      </div>

      {/* Credentials form */}
      {!status?.credentialsConfigured && (
        <div className="space-y-4">
          <div>
            <h4 className="text-sm font-medium text-foreground mb-2">
              Configure OAuth Credentials
            </h4>
            <p className="text-xs text-muted mb-4">
              To use Google Docs, you need to create OAuth credentials in the{" "}
              <a
                href="https://console.cloud.google.com/apis/credentials"
                target="_blank"
                rel="noopener noreferrer"
                className="text-teal-400 hover:underline"
              >
                Google Cloud Console
              </a>
              . Create an OAuth 2.0 Client ID for a "Desktop app" and enter the
              credentials below.
            </p>
          </div>

          <div className="space-y-3">
            <div>
              <label
                htmlFor="clientId"
                className="block text-sm font-medium text-foreground mb-1"
              >
                Client ID
              </label>
              <input
                id="clientId"
                type="text"
                value={clientId}
                onChange={(e) => setClientId(e.target.value)}
                placeholder="xxxx.apps.googleusercontent.com"
                className="w-full px-3 py-2 bg-surface border border-gray-700 rounded-md text-foreground placeholder-muted focus:outline-none focus:ring-2 focus:ring-teal-500"
              />
            </div>
            <div>
              <label
                htmlFor="clientSecret"
                className="block text-sm font-medium text-foreground mb-1"
              >
                Client Secret{" "}
                <span className="text-muted">(optional for desktop apps)</span>
              </label>
              <input
                id="clientSecret"
                type="password"
                value={clientSecret}
                onChange={(e) => setClientSecret(e.target.value)}
                placeholder="GOCSPX-..."
                className="w-full px-3 py-2 bg-surface border border-gray-700 rounded-md text-foreground placeholder-muted focus:outline-none focus:ring-2 focus:ring-teal-500"
              />
            </div>
          </div>

          <button
            onClick={handleSaveCredentials}
            disabled={saving || !clientId.trim()}
            className="px-4 py-2 bg-teal-600 text-white rounded-md hover:bg-teal-700 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
          >
            {saving ? "Saving..." : "Save Credentials"}
          </button>
        </div>
      )}

      {/* OAuth button */}
      {status?.credentialsConfigured && !status?.isAuthenticated && (
        <div className="space-y-3">
          <p className="text-sm text-muted">
            {authenticating
              ? "Complete the sign-in in your browser, then return here."
              : "Credentials are configured. Click below to authenticate with Google."}
          </p>
          <button
            onClick={handleStartOAuth}
            disabled={authenticating}
            className="px-4 py-2 bg-teal-600 text-white rounded-md hover:bg-teal-700 disabled:opacity-50 disabled:cursor-not-allowed transition-colors flex items-center gap-2"
          >
            {authenticating ? (
              <>
                <svg
                  className="w-5 h-5 animate-spin"
                  fill="none"
                  viewBox="0 0 24 24"
                >
                  <circle
                    className="opacity-25"
                    cx="12"
                    cy="12"
                    r="10"
                    stroke="currentColor"
                    strokeWidth="4"
                  />
                  <path
                    className="opacity-75"
                    fill="currentColor"
                    d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"
                  />
                </svg>
                Waiting for sign-in...
              </>
            ) : (
              <>
                <svg className="w-5 h-5" viewBox="0 0 24 24">
                  <path
                    fill="currentColor"
                    d="M22.56 12.25c0-.78-.07-1.53-.2-2.25H12v4.26h5.92c-.26 1.37-1.04 2.53-2.21 3.31v2.77h3.57c2.08-1.92 3.28-4.74 3.28-8.09z"
                  />
                  <path
                    fill="currentColor"
                    d="M12 23c2.97 0 5.46-.98 7.28-2.66l-3.57-2.77c-.98.66-2.23 1.06-3.71 1.06-2.86 0-5.29-1.93-6.16-4.53H2.18v2.84C3.99 20.53 7.7 23 12 23z"
                  />
                  <path
                    fill="currentColor"
                    d="M5.84 14.09c-.22-.66-.35-1.36-.35-2.09s.13-1.43.35-2.09V7.07H2.18C1.43 8.55 1 10.22 1 12s.43 3.45 1.18 4.93l2.85-2.22.81-.62z"
                  />
                  <path
                    fill="currentColor"
                    d="M12 5.38c1.62 0 3.06.56 4.21 1.64l3.15-3.15C17.45 2.09 14.97 1 12 1 7.7 1 3.99 3.47 2.18 7.07l3.66 2.84c.87-2.6 3.3-4.53 6.16-4.53z"
                  />
                </svg>
                Sign in with Google
              </>
            )}
          </button>
        </div>
      )}

      {/* Authenticated state */}
      {status?.isAuthenticated && (
        <div className="bg-green-900/20 border border-green-700 rounded-lg p-4">
          <p className="text-sm text-green-400">
            Successfully authenticated with Google Docs. You can now use the
            Google Docs MCP tools.
          </p>
        </div>
      )}

      {/* Error/Success messages */}
      {error && (
        <div className="bg-red-900/20 border border-red-700 rounded-lg p-3">
          <p className="text-sm text-red-400">{error}</p>
        </div>
      )}
      {success && (
        <div className="bg-green-900/20 border border-green-700 rounded-lg p-3">
          <p className="text-sm text-green-400">{success}</p>
        </div>
      )}
    </div>
  );
}
