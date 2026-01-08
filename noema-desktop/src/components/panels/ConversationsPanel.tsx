import { useState } from "react";
import type { ConversationInfo } from "../../types";

interface ConversationsPanelProps {
  conversations: ConversationInfo[];
  currentConversationId: string;
  onNewConversation: () => void;
  onSelectConversation: (id: string) => void;
  onDeleteConversation: (id: string) => void;
  onRenameConversation: (id: string, name: string) => void;
  pendingFork?: boolean;
}

function formatDate(timestamp: number): string {
  // Backend sends unix seconds, JavaScript expects milliseconds
  const date = new Date(timestamp * 1000);
  const now = new Date();
  const diffDays = Math.floor(
    (now.getTime() - date.getTime()) / (1000 * 60 * 60 * 24)
  );

  if (diffDays === 0) return "Today";
  if (diffDays === 1) return "Yesterday";
  if (diffDays < 7) return `${diffDays} days ago`;
  return date.toLocaleDateString();
}

export function ConversationsPanel({
  conversations,
  currentConversationId,
  onNewConversation,
  onSelectConversation,
  onDeleteConversation,
  onRenameConversation,
  pendingFork,
}: ConversationsPanelProps) {
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editName, setEditName] = useState("");

  const startRename = (conv: ConversationInfo) => {
    setEditingId(conv.id);
    setEditName(conv.name || "");
  };

  const submitRename = () => {
    if (editingId) {
      onRenameConversation(editingId, editName);
      setEditingId(null);
    }
  };

  return (
    <>
      {/* Header */}
      <div className="p-4 border-b border-gray-700">
        <button
          onClick={onNewConversation}
          className="w-full px-4 py-2 bg-teal-600 hover:bg-teal-700 text-white rounded-lg font-medium transition-colors"
        >
          + New Chat
        </button>
      </div>

      {/* Conversations list */}
      <div className="flex-1 overflow-y-auto">
        {conversations.length === 0 ? (
          <p className="text-muted text-center p-4 text-sm">
            No conversations yet
          </p>
        ) : (
          <ul className="py-2">
            {/* Show draft fork entry when pending */}
            {pendingFork && (
              <li className="px-2">
                <div className="p-3 rounded-lg bg-purple-900/30 border border-purple-500/50 border-dashed">
                  <div className="flex items-center gap-2">
                    <svg className="w-4 h-4 text-purple-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <circle cx="6" cy="6" r="2" strokeWidth={2} />
                      <circle cx="6" cy="18" r="2" strokeWidth={2} />
                      <circle cx="18" cy="12" r="2" strokeWidth={2} />
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 8v8M6 8c0 2 2 4 6 4h4" />
                    </svg>
                    <span className="text-sm text-purple-300 font-medium">New Fork</span>
                  </div>
                  <span className="text-xs text-purple-400/70">Send a message to create</span>
                </div>
              </li>
            )}
            {conversations.map((conv) => {
              const isCurrent = conv.id === currentConversationId;
              const displayName =
                conv.name || `Chat (${conv.messageCount} messages)`;

              return (
                <li key={conv.id} className="px-2">
                  {editingId === conv.id ? (
                    <div className="p-2">
                      <input
                        type="text"
                        value={editName}
                        onChange={(e) => setEditName(e.target.value)}
                        onBlur={submitRename}
                        onKeyDown={(e) => {
                          if (e.key === "Enter") submitRename();
                          if (e.key === "Escape") setEditingId(null);
                        }}
                        className="w-full px-2 py-1 text-sm border border-gray-600 rounded bg-elevated text-foreground"
                        autoFocus
                      />
                    </div>
                  ) : (
                    <button
                      onClick={() => onSelectConversation(conv.id)}
                      className={`w-full text-left p-3 rounded-lg transition-colors group ${
                        isCurrent
                          ? "bg-teal-900/50 text-teal-100"
                          : "hover:bg-elevated text-gray-300"
                      }`}
                    >
                      <div className="flex items-center justify-between">
                        <span className="truncate font-medium text-sm">
                          {displayName}
                        </span>
                        <div className="opacity-0 group-hover:opacity-100 flex gap-1 flex-shrink-0">
                          <button
                            onClick={(e) => {
                              e.stopPropagation();
                              startRename(conv);
                            }}
                            className="p-1 text-muted hover:text-foreground"
                            title="Rename"
                          >
                            <svg
                              className="w-4 h-4"
                              fill="none"
                              stroke="currentColor"
                              viewBox="0 0 24 24"
                            >
                              <path
                                strokeLinecap="round"
                                strokeLinejoin="round"
                                strokeWidth={2}
                                d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z"
                              />
                            </svg>
                          </button>
                          <button
                            onClick={(e) => {
                              e.stopPropagation();
                              onDeleteConversation(conv.id);
                            }}
                            className="p-1 text-muted hover:text-red-500"
                            title="Delete"
                          >
                            <svg
                              className="w-4 h-4"
                              fill="none"
                              stroke="currentColor"
                              viewBox="0 0 24 24"
                            >
                              <path
                                strokeLinecap="round"
                                strokeLinejoin="round"
                                strokeWidth={2}
                                d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"
                              />
                            </svg>
                          </button>
                        </div>
                      </div>
                      <span className="text-xs text-muted">
                        {formatDate(Number(conv.updatedAt))}
                      </span>
                    </button>
                  )}
                </li>
              );
            })}
          </ul>
        )}
      </div>
    </>
  );
}
