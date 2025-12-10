import { useState, useEffect, useCallback } from "react";
import { open } from "@tauri-apps/plugin-shell";
import type { McpServerInfo, McpToolInfo } from "../types";
import * as tauri from "../tauri";

interface McpSettingsProps {
  onClose: () => void;
}

export function McpSettings({ onClose }: McpSettingsProps) {
  const [servers, setServers] = useState<McpServerInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [showAddForm, setShowAddForm] = useState(false);
  const [expandedServer, setExpandedServer] = useState<string | null>(null);
  const [serverTools, setServerTools] = useState<Record<string, McpToolInfo[]>>({});
  const [oauthPending, setOauthPending] = useState<string | null>(null);
  const [addingServer, setAddingServer] = useState(false);

  // Form state - just name and URL, auth is auto-detected
  const [formName, setFormName] = useState("");
  const [formUrl, setFormUrl] = useState("");

  const loadServers = useCallback(async () => {
    try {
      setLoading(true);
      const list = await tauri.listMcpServers();
      setServers(list);
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadServers();
  }, [loadServers]);

  // Listen for OAuth completion events from deep link handler
  useEffect(() => {
    const unlisteners: (() => void)[] = [];

    tauri.onOauthComplete((_serverId) => {
      // OAuth completed successfully via deep link
      setOauthPending(null);
      loadServers();
    }).then((unlisten) => unlisteners.push(unlisten));

    tauri.onOauthError((err) => {
      setError(`OAuth error: ${err}`);
      setOauthPending(null);
    }).then((unlisten) => unlisteners.push(unlisten));

    return () => {
      unlisteners.forEach((unlisten) => unlisten());
    };
  }, [loadServers]);

  // Generate a simple ID from the name
  const generateId = (name: string) => {
    return name.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "");
  };

  const handleAddServer = async () => {
    try {
      setError(null);
      setAddingServer(true);
      const id = generateId(formName);

      // Use "auto" to let backend probe .well-known and detect OAuth
      await tauri.addMcpServer({
        id,
        name: formName,
        url: formUrl,
        authType: "auto",
      });

      // Reset form
      setFormName("");
      setFormUrl("");
      setShowAddForm(false);

      await loadServers();
    } catch (err) {
      setError(String(err));
    } finally {
      setAddingServer(false);
    }
  };

  const handleRemoveServer = async (serverId: string) => {
    try {
      setError(null);
      await tauri.removeMcpServer(serverId);
      await loadServers();
    } catch (err) {
      setError(String(err));
    }
  };

  const handleConnect = async (serverId: string) => {
    try {
      setError(null);
      await tauri.connectMcpServer(serverId);
      await loadServers();
    } catch (err) {
      setError(String(err));
    }
  };

  const handleDisconnect = async (serverId: string) => {
    try {
      setError(null);
      await tauri.disconnectMcpServer(serverId);
      setServerTools(prev => {
        const next = { ...prev };
        delete next[serverId];
        return next;
      });
      await loadServers();
    } catch (err) {
      setError(String(err));
    }
  };

  const handleTestConnection = async (serverId: string) => {
    try {
      setError(null);
      const toolCount = await tauri.testMcpServer(serverId);
      alert(`Connection successful! Found ${toolCount} tools.`);
    } catch (err) {
      setError(String(err));
    }
  };

  const handleStartOauth = async (serverId: string) => {
    try {
      setError(null);
      setOauthPending(serverId);
      const authUrl = await tauri.startMcpOauth(serverId);
      // Open the authorization URL in the default browser using Tauri shell
      await open(authUrl);
    } catch (err) {
      setError(String(err));
      setOauthPending(null);
    }
  };

  const handleCompleteOauth = async (code: string) => {
    if (!oauthPending) return;
    try {
      setError(null);
      await tauri.completeMcpOauth(oauthPending, code);
      setOauthPending(null);
      await loadServers();
    } catch (err) {
      setError(String(err));
    }
  };

  const handleExpandServer = async (serverId: string) => {
    if (expandedServer === serverId) {
      setExpandedServer(null);
      return;
    }

    setExpandedServer(serverId);
    const server = servers.find(s => s.id === serverId);
    if (server?.isConnected && !serverTools[serverId]) {
      try {
        const tools = await tauri.getMcpServerTools(serverId);
        setServerTools(prev => ({ ...prev, [serverId]: tools }));
      } catch (err) {
        setError(String(err));
      }
    }
  };

  return (
    <div className="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50">
      <div className="bg-surface rounded-lg shadow-xl w-full max-w-2xl max-h-[80vh] overflow-hidden flex flex-col">
        {/* Header */}
        <div className="px-6 py-4 border-b border-gray-700 flex items-center justify-between">
          <h2 className="text-xl font-semibold text-foreground">
            MCP Servers
          </h2>
          <button
            onClick={onClose}
            className="p-2 text-muted hover:text-foreground"
          >
            <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>

        {/* Error banner */}
        {error && (
          <div className="px-6 py-2 bg-red-900/50 text-red-200 text-sm">
            {error}
            <button onClick={() => setError(null)} className="ml-2 underline">dismiss</button>
          </div>
        )}

        {/* Content */}
        <div className="flex-1 overflow-y-auto p-6">
          {loading ? (
            <div className="text-center py-8 text-muted">
              Loading...
            </div>
          ) : (
            <>
              {/* Server list */}
              {servers.length === 0 ? (
                <p className="text-muted text-center py-8">
                  No MCP servers configured. Add one to get started.
                </p>
              ) : (
                <ul className="space-y-3 mb-6">
                  {servers.map(server => (
                    <li key={server.id} className="border border-gray-700 rounded-lg overflow-hidden">
                      <div
                        className="p-4 cursor-pointer hover:bg-elevated"
                        onClick={() => handleExpandServer(server.id)}
                      >
                        <div className="flex items-center justify-between">
                          <div className="flex items-center gap-3">
                            {/* Connection status indicator */}
                            <div className={`w-3 h-3 rounded-full ${server.isConnected ? 'bg-teal-500' : 'bg-gray-500'}`} />
                            <div>
                              <h3 className="font-medium text-foreground">
                                {server.name}
                              </h3>
                              <p className="text-sm text-muted">
                                {server.url}
                              </p>
                            </div>
                          </div>
                          <div className="flex items-center gap-2">
                            <span className="text-xs px-2 py-1 bg-elevated rounded text-gray-300">
                              {server.authType}
                            </span>
                            {server.isConnected && (
                              <span className="text-xs text-muted">
                                {server.toolCount} tools
                              </span>
                            )}
                            <svg
                              className={`w-4 h-4 transition-transform ${expandedServer === server.id ? 'rotate-180' : ''}`}
                              fill="none"
                              stroke="currentColor"
                              viewBox="0 0 24 24"
                            >
                              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 9l-7 7-7-7" />
                            </svg>
                          </div>
                        </div>
                      </div>

                      {/* Expanded content */}
                      {expandedServer === server.id && (
                        <div className="px-4 pb-4 border-t border-gray-700 pt-3">
                          {/* Actions */}
                          <div className="flex gap-2 mb-3">
                            {server.needsOauthLogin ? (
                              <button
                                onClick={() => handleStartOauth(server.id)}
                                className="px-3 py-1.5 text-sm bg-teal-600 hover:bg-teal-700 text-white rounded"
                              >
                                Login with OAuth
                              </button>
                            ) : server.isConnected ? (
                              <button
                                onClick={() => handleDisconnect(server.id)}
                                className="px-3 py-1.5 text-sm bg-elevated hover:bg-background text-gray-200 rounded"
                              >
                                Disconnect
                              </button>
                            ) : (
                              <button
                                onClick={() => handleConnect(server.id)}
                                className="px-3 py-1.5 text-sm bg-teal-600 hover:bg-teal-700 text-white rounded"
                              >
                                Connect
                              </button>
                            )}
                            <button
                              onClick={() => handleTestConnection(server.id)}
                              className="px-3 py-1.5 text-sm bg-elevated hover:bg-background text-gray-200 rounded"
                            >
                              Test
                            </button>
                            <button
                              onClick={() => handleRemoveServer(server.id)}
                              className="px-3 py-1.5 text-sm bg-red-900/50 hover:bg-red-900 text-red-200 rounded"
                            >
                              Remove
                            </button>
                          </div>

                          {/* Tools list */}
                          {server.isConnected && serverTools[server.id] && (
                            <div>
                              <h4 className="text-sm font-medium text-gray-300 mb-2">
                                Available Tools:
                              </h4>
                              <ul className="text-sm space-y-1">
                                {serverTools[server.id].map(tool => (
                                  <li key={tool.name} className="text-muted">
                                    <span className="font-mono">{tool.name}</span>
                                    {tool.description && (
                                      <span className="text-gray-500"> - {tool.description}</span>
                                    )}
                                  </li>
                                ))}
                              </ul>
                            </div>
                          )}
                        </div>
                      )}
                    </li>
                  ))}
                </ul>
              )}

              {/* OAuth code entry */}
              {oauthPending && (
                <OAuthCodeEntry
                  serverId={oauthPending}
                  onSubmit={handleCompleteOauth}
                  onCancel={() => setOauthPending(null)}
                />
              )}

              {/* Add server form */}
              {showAddForm ? (
                <div className="border border-gray-700 rounded-lg p-4">
                  <h3 className="font-medium text-foreground mb-4">Add MCP Server</h3>
                  <div className="space-y-4">
                    <div>
                      <label className="block text-sm font-medium text-gray-300 mb-1">
                        Name
                      </label>
                      <input
                        type="text"
                        value={formName}
                        onChange={e => setFormName(e.target.value)}
                        placeholder="My Server"
                        className="w-full px-3 py-2 border border-gray-600 rounded bg-elevated text-foreground"
                      />
                    </div>
                    <div>
                      <label className="block text-sm font-medium text-gray-300 mb-1">
                        URL
                      </label>
                      <input
                        type="text"
                        value={formUrl}
                        onChange={e => setFormUrl(e.target.value)}
                        placeholder="https://mcp-server.example.com"
                        className="w-full px-3 py-2 border border-gray-600 rounded bg-elevated text-foreground"
                      />
                    </div>
                    <p className="text-sm text-muted">
                      Authentication will be auto-detected. If the server requires OAuth,
                      you'll be prompted to login after adding it.
                    </p>

                    <div className="flex gap-2 pt-2">
                      <button
                        onClick={handleAddServer}
                        disabled={!formName || !formUrl || addingServer}
                        className="px-4 py-2 bg-teal-600 hover:bg-teal-700 disabled:bg-gray-600 text-white rounded font-medium"
                      >
                        {addingServer ? "Detecting auth..." : "Add Server"}
                      </button>
                      <button
                        onClick={() => setShowAddForm(false)}
                        className="px-4 py-2 bg-elevated hover:bg-background text-gray-200 rounded"
                      >
                        Cancel
                      </button>
                    </div>
                  </div>
                </div>
              ) : (
                <button
                  onClick={() => setShowAddForm(true)}
                  className="w-full py-3 border-2 border-dashed border-gray-600 rounded-lg text-muted hover:border-teal-500 hover:text-teal-400 transition-colors"
                >
                  + Add MCP Server
                </button>
              )}
            </>
          )}
        </div>
      </div>
    </div>
  );
}

// Component for entering OAuth authorization code
function OAuthCodeEntry({
  onSubmit,
  onCancel,
}: {
  serverId: string;
  onSubmit: (code: string) => void;
  onCancel: () => void;
}) {
  const [code, setCode] = useState("");

  return (
    <div className="mb-6 p-4 border border-teal-800 bg-teal-900/20 rounded-lg">
      <h4 className="font-medium text-foreground mb-2">
        Waiting for OAuth Login
      </h4>
      <p className="text-sm text-muted mb-3">
        A browser window should have opened for authentication.
        After authorizing, the app will automatically complete the login.
        If it doesn't work, you can paste the code manually:
      </p>
      <div className="flex gap-2">
        <input
          type="text"
          value={code}
          onChange={e => setCode(e.target.value)}
          placeholder="Paste authorization code"
          className="flex-1 px-3 py-2 border border-gray-600 rounded bg-elevated text-foreground"
        />
        <button
          onClick={() => onSubmit(code)}
          disabled={!code}
          className="px-4 py-2 bg-teal-600 hover:bg-teal-700 disabled:bg-gray-600 text-white rounded"
        >
          Submit
        </button>
        <button
          onClick={onCancel}
          className="px-4 py-2 bg-elevated hover:bg-background text-gray-200 rounded"
        >
          Cancel
        </button>
      </div>
    </div>
  );
}
