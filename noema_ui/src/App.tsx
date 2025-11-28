import { useState, useEffect, useRef } from "react";
import { MessageBubble } from "./components/MessageBubble";
import { ChatInput } from "./components/ChatInput";
import { Sidebar } from "./components/Sidebar";
import { ModelSelector } from "./components/ModelSelector";
import type { DisplayMessage, ModelInfo, ConversationInfo } from "./types";
import * as tauri from "./tauri";

function App() {
  const [messages, setMessages] = useState<DisplayMessage[]>([]);
  const [streamingMessage, setStreamingMessage] = useState<DisplayMessage | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [conversations, setConversations] = useState<ConversationInfo[]>([]);
  const [currentConversationId, setCurrentConversationId] = useState("");
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [currentModel, setCurrentModel] = useState("");
  const [isInitialized, setIsInitialized] = useState(false);

  const messagesEndRef = useRef<HTMLDivElement>(null);

  // Auto-scroll to bottom
  const scrollToBottom = () => {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  };

  useEffect(() => {
    scrollToBottom();
  }, [messages, streamingMessage]);

  // Initialize app
  useEffect(() => {
    async function init() {
      try {
        const modelName = await tauri.initApp();
        setCurrentModel(modelName);

        const convos = await tauri.listConversations();
        setConversations(convos);

        const convId = await tauri.getCurrentConversationId();
        setCurrentConversationId(convId);

        const msgs = await tauri.getMessages();
        setMessages(msgs);

        // Load models in background
        tauri.listModels().then(setModels).catch(console.error);

        setIsInitialized(true);
      } catch (err) {
        setError(String(err));
      }
    }
    init();
  }, []);

  // Set up event listeners
  useEffect(() => {
    const unlisteners: (() => void)[] = [];

    tauri.onUserMessage((msg) => {
      setMessages((prev) => [...prev, msg]);
      setIsLoading(true);
    }).then((unlisten) => unlisteners.push(unlisten));

    tauri.onStreamingMessage((msg) => {
      setStreamingMessage(msg);
    }).then((unlisten) => unlisteners.push(unlisten));

    tauri.onMessageComplete((msgs) => {
      setMessages(msgs);
      setStreamingMessage(null);
      setIsLoading(false);
      // Refresh conversations
      tauri.listConversations().then(setConversations).catch(console.error);
    }).then((unlisten) => unlisteners.push(unlisten));

    tauri.onError((err) => {
      setError(err);
      setIsLoading(false);
      setStreamingMessage(null);
    }).then((unlisten) => unlisteners.push(unlisten));

    tauri.onModelChanged((name) => {
      setCurrentModel(name);
    }).then((unlisten) => unlisteners.push(unlisten));

    tauri.onHistoryCleared(() => {
      setMessages([]);
    }).then((unlisten) => unlisteners.push(unlisten));

    return () => {
      unlisteners.forEach((unlisten) => unlisten());
    };
  }, []);

  const handleSendMessage = async (message: string) => {
    try {
      setError(null);
      await tauri.sendMessage(message);
    } catch (err) {
      setError(String(err));
    }
  };

  const handleNewConversation = async () => {
    try {
      const id = await tauri.newConversation();
      setCurrentConversationId(id);
      setMessages([]);
      const convos = await tauri.listConversations();
      setConversations(convos);
    } catch (err) {
      setError(String(err));
    }
  };

  const handleSelectConversation = async (id: string) => {
    try {
      const msgs = await tauri.switchConversation(id);
      setCurrentConversationId(id);
      setMessages(msgs);
    } catch (err) {
      setError(String(err));
    }
  };

  const handleDeleteConversation = async (id: string) => {
    try {
      await tauri.deleteConversation(id);
      const convos = await tauri.listConversations();
      setConversations(convos);
    } catch (err) {
      setError(String(err));
    }
  };

  const handleRenameConversation = async (id: string, name: string) => {
    try {
      await tauri.renameConversation(id, name);
      const convos = await tauri.listConversations();
      setConversations(convos);
    } catch (err) {
      setError(String(err));
    }
  };

  const handleSelectModel = async (modelId: string, provider: string) => {
    try {
      await tauri.setModel(modelId, provider);
    } catch (err) {
      setError(String(err));
    }
  };

  const handleClearHistory = async () => {
    try {
      await tauri.clearHistory();
    } catch (err) {
      setError(String(err));
    }
  };

  if (!isInitialized) {
    return (
      <div className="h-screen flex items-center justify-center bg-white dark:bg-gray-900">
        <div className="text-center">
          <div className="animate-spin w-8 h-8 border-4 border-blue-500 border-t-transparent rounded-full mx-auto mb-4"></div>
          <p className="text-gray-600 dark:text-gray-400">Initializing...</p>
        </div>
      </div>
    );
  }

  return (
    <div className="h-screen flex bg-white dark:bg-gray-900">
      {/* Sidebar */}
      <Sidebar
        conversations={conversations}
        currentConversationId={currentConversationId}
        onNewConversation={handleNewConversation}
        onSelectConversation={handleSelectConversation}
        onDeleteConversation={handleDeleteConversation}
        onRenameConversation={handleRenameConversation}
      />

      {/* Main chat area */}
      <div className="flex-1 flex flex-col min-w-0">
        {/* Top bar */}
        <div className="border-b border-gray-200 dark:border-gray-700 px-4 py-3 flex items-center justify-between bg-white dark:bg-gray-900">
          <h1 className="text-lg font-semibold text-gray-800 dark:text-white">
            Noema
          </h1>
          <ModelSelector
            models={models}
            currentModel={currentModel}
            onSelectModel={handleSelectModel}
            onClearHistory={handleClearHistory}
          />
        </div>

        {/* Error banner */}
        {error && (
          <div className="bg-red-100 dark:bg-red-900 text-red-800 dark:text-red-200 px-4 py-2 flex items-center justify-between">
            <span className="text-sm">{error}</span>
            <button
              onClick={() => setError(null)}
              className="text-red-600 dark:text-red-300 hover:text-red-800 dark:hover:text-red-100"
            >
              <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
              </svg>
            </button>
          </div>
        )}

        {/* Messages area */}
        <div className="flex-1 overflow-y-auto p-4">
          <div className="max-w-4xl mx-auto">
            {messages.length === 0 && !streamingMessage ? (
              <div className="text-center py-20">
                <h2 className="text-2xl font-semibold text-gray-800 dark:text-white mb-2">
                  Welcome to Noema
                </h2>
                <p className="text-gray-600 dark:text-gray-400">
                  Start a conversation by typing a message below.
                </p>
                <p className="text-gray-500 dark:text-gray-500 text-sm mt-4">
                  Supports Markdown and LaTeX math: $E = mc^2$
                </p>
              </div>
            ) : (
              <>
                {messages.map((msg, i) => (
                  <MessageBubble key={i} message={msg} />
                ))}
                {streamingMessage && (
                  <MessageBubble message={streamingMessage} />
                )}
                {isLoading && !streamingMessage && (
                  <div className="flex justify-start mb-4">
                    <div className="bg-gray-100 dark:bg-gray-800 px-4 py-3 rounded-2xl">
                      <div className="flex gap-1">
                        <div className="w-2 h-2 bg-gray-400 rounded-full animate-bounce" style={{ animationDelay: "0ms" }}></div>
                        <div className="w-2 h-2 bg-gray-400 rounded-full animate-bounce" style={{ animationDelay: "150ms" }}></div>
                        <div className="w-2 h-2 bg-gray-400 rounded-full animate-bounce" style={{ animationDelay: "300ms" }}></div>
                      </div>
                    </div>
                  </div>
                )}
              </>
            )}
            <div ref={messagesEndRef} />
          </div>
        </div>

        {/* Input area */}
        <ChatInput onSend={handleSendMessage} disabled={isLoading} />
      </div>
    </div>
  );
}

export default App;
