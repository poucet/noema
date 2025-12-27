import { useState } from "react";
import { McpSettingsContent } from "./McpSettingsContent";
import { ApiKeySettings } from "./ApiKeySettings";

type TabId = "mcp" | "apikeys";

interface SettingsProps {
  onClose: () => void;
  initialTab?: TabId;
}

interface TabConfig {
  id: TabId;
  label: string;
}

const tabs: TabConfig[] = [
  { id: "mcp", label: "MCP Servers" },
  { id: "apikeys", label: "API Keys" },
];

export function Settings({ onClose, initialTab = "mcp" }: SettingsProps) {
  const [activeTab, setActiveTab] = useState<TabId>(initialTab);

  return (
    <div className="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50">
      <div className="bg-surface rounded-lg shadow-xl w-full max-w-2xl max-h-[80vh] overflow-hidden flex flex-col">
        {/* Header with tabs */}
        <div className="border-b border-gray-700">
          <div className="px-6 py-4 flex items-center justify-between">
            <h2 className="text-xl font-semibold text-foreground">Settings</h2>
            <button
              onClick={onClose}
              className="p-2 text-muted hover:text-foreground"
            >
              <svg
                className="w-5 h-5"
                fill="none"
                stroke="currentColor"
                viewBox="0 0 24 24"
              >
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth={2}
                  d="M6 18L18 6M6 6l12 12"
                />
              </svg>
            </button>
          </div>

          {/* Tab bar */}
          <div className="px-6 flex gap-1">
            {tabs.map((tab) => (
              <button
                key={tab.id}
                onClick={() => setActiveTab(tab.id)}
                className={`px-4 py-2 text-sm font-medium rounded-t-lg transition-colors ${
                  activeTab === tab.id
                    ? "bg-elevated text-foreground border-b-2 border-teal-500"
                    : "text-muted hover:text-foreground hover:bg-elevated/50"
                }`}
              >
                {tab.label}
              </button>
            ))}
          </div>
        </div>

        {/* Tab content */}
        <div className="flex-1 overflow-y-auto p-6">
          {activeTab === "mcp" && <McpSettingsContent />}
          {activeTab === "apikeys" && <ApiKeySettings />}
        </div>
      </div>
    </div>
  );
}
