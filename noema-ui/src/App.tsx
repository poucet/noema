import { useState, useEffect, useRef } from "react";
import { MessageBubble } from "./components/MessageBubble";
import { ChatInput } from "./components/ChatInput";
import { Sidebar } from "./components/Sidebar";
import { ModelSelector } from "./components/ModelSelector";
import { Settings } from "./components/Settings";
import type { DisplayMessage, ModelInfo, ConversationInfo, Attachment } from "./types";
import * as tauri from "./tauri";
import { useVoiceInput } from "./hooks/useVoiceInput";
import { appLog } from "./utils/log";

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
  const [showSettings, setShowSettings] = useState(false);
  const [availableUsers, setAvailableUsers] = useState<string[]>([]);

  const messagesEndRef = useRef<HTMLDivElement>(null);
  const messagesContainerRef = useRef<HTMLDivElement>(null);

  // Voice input hook - handles browser audio capture and Whisper transcription
  const handleVoiceTranscription = (text: string) => {
    handleSendMessage(text);
  };

  const handleVoiceError = (err: string) => {
    setError(`Voice error: ${err}`);
  };

  const voice = useVoiceInput({
    onTranscription: handleVoiceTranscription,
    onError: handleVoiceError,
  });

  // Auto-scroll to bottom when new messages arrive
  const prevMessagesLengthRef = useRef(0);

  useEffect(() => {
    // Only auto-scroll if messages were added (not on initial load or conversation switch)
    if (messages.length > prevMessagesLengthRef.current || streamingMessage) {
      if (messagesContainerRef.current) {
        messagesContainerRef.current.scrollTop = messagesContainerRef.current.scrollHeight;
      }
    }
    prevMessagesLengthRef.current = messages.length;
  }, [messages, streamingMessage]);

  // Initialize app
  useEffect(() => {
    async function init() {
      try {
        const modelName = await tauri.initApp();
        // Empty string means this is a duplicate init call (React StrictMode)
        // The real init is still running, so just bail out
        if (!modelName) {
          console.log("Duplicate init call, skipping");
          return;
        }
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
        const errorMsg = String(err);
        // Check if multiple users exist and selection is needed
        if (errorMsg.includes("MULTIPLE_USERS:")) {
          const emails = errorMsg.split("MULTIPLE_USERS:")[1].split(",");
          setAvailableUsers(emails);
        } else {
          appLog.error("Init error", errorMsg);
          setError(errorMsg);
        }
      }
    }
    init();
  }, []);

  // Set up event listeners
  useEffect(() => {
    const unlisteners: (() => void)[] = [];

    tauri.onUserMessage((msg) => {
      setMessages((prev) => {
        // Avoid adding duplicate user messages (can happen with rapid voice input)
        // Check if the last message is already this user message
        if (prev.length > 0) {
          const lastMsg = prev[prev.length - 1];
          if (lastMsg.role === "user" && msg.role === "user") {
            const lastText = lastMsg.content.find((c) => "text" in c);
            const newText = msg.content.find((c) => "text" in c);
            if (lastText && newText && "text" in lastText && "text" in newText) {
              if (lastText.text === newText.text) {
                return prev; // Skip duplicate
              }
            }
          }
        }
        return [...prev, msg];
      });
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
      appLog.error("Backend error received", err);
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

    // Voice events are now handled by the useVoiceInput hook

    return () => {
      unlisteners.forEach((unlisten) => unlisten());
    };
  }, []);

  const handleSendMessage = async (message: string, attachments: Attachment[] = []) => {
    try {
      setError(null);
      if (attachments.length > 0) {
        await tauri.sendMessageWithAttachments(message, attachments);
      } else {
        await tauri.sendMessage(message);
      }
    } catch (err) {
      appLog.error("Send message error", String(err));
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
      appLog.error("New conversation error", String(err));
      setError(String(err));
    }
  };

  const handleSelectConversation = async (id: string) => {
    try {
      const msgs = await tauri.switchConversation(id);
      setCurrentConversationId(id);
      setMessages(msgs);
    } catch (err) {
      appLog.error("Select conversation error", String(err));
      setError(String(err));
    }
  };

  const handleDeleteConversation = async (id: string) => {
    try {
      const isCurrentConversation = id === currentConversationId;

      if (isCurrentConversation) {
        // If deleting current conversation, switch to another one first or create new
        const otherConversation = conversations.find((c) => c.id !== id);
        if (otherConversation) {
          // Switch to another existing conversation
          const msgs = await tauri.switchConversation(otherConversation.id);
          setCurrentConversationId(otherConversation.id);
          setMessages(msgs);
        } else {
          // No other conversations, create a new one
          const newId = await tauri.newConversation();
          setCurrentConversationId(newId);
          setMessages([]);
        }
      }

      // Now delete the conversation
      await tauri.deleteConversation(id);
      const convos = await tauri.listConversations();
      setConversations(convos);
    } catch (err) {
      appLog.error("Delete conversation error", String(err));
      setError(String(err));
    }
  };

  const handleRenameConversation = async (id: string, name: string) => {
    try {
      await tauri.renameConversation(id, name);
      const convos = await tauri.listConversations();
      setConversations(convos);
    } catch (err) {
      appLog.error("Rename conversation error", String(err));
      setError(String(err));
    }
  };

  const handleSelectModel = async (modelId: string, provider: string) => {
    try {
      await tauri.setModel(modelId, provider);
    } catch (err) {
      appLog.error("Select model error", String(err));
      setError(String(err));
    }
  };


  // Retry initialization after setup
  const retryInit = async () => {
    setError(null);
    setAvailableUsers([]);
    try {
      const modelName = await tauri.initApp();
      if (!modelName) return; // Duplicate call

      setCurrentModel(modelName);
      const convos = await tauri.listConversations();
      setConversations(convos);
      const convId = await tauri.getCurrentConversationId();
      setCurrentConversationId(convId);
      const msgs = await tauri.getMessages();
      setMessages(msgs);
      tauri.listModels().then(setModels).catch(console.error);
      setIsInitialized(true);
    } catch (err) {
      const errorMsg = String(err);
      if (errorMsg.includes("MULTIPLE_USERS:")) {
        const emails = errorMsg.split("MULTIPLE_USERS:")[1].split(",");
        setAvailableUsers(emails);
      } else {
        appLog.error("RetryInit error", errorMsg);
        setError(errorMsg);
      }
    }
  };

  // User selection screen (when multiple users exist)
  if (availableUsers.length > 0) {
    const handleUserSelect = async (email: string) => {
      try {
        await tauri.setUserEmail(email);
        await retryInit();
      } catch (err) {
        appLog.error("User select error", String(err));
        setError(String(err));
      }
    };

    return (
      <div className="h-screen flex items-center justify-center bg-background">
        <div className="bg-surface p-8 rounded-lg shadow-lg max-w-md w-full mx-4">
          <h1 className="text-2xl font-bold text-foreground mb-2">Select Account</h1>
          <p className="text-muted mb-6">
            Multiple accounts found. Choose which one to use:
          </p>
          <div className="space-y-2">
            {availableUsers.map((email) => (
              <button
                key={email}
                onClick={() => handleUserSelect(email)}
                className="w-full px-4 py-3 bg-elevated hover:bg-surface border border-gray-600 rounded-lg text-foreground text-left transition-colors"
              >
                {email}
              </button>
            ))}
          </div>
          {error && (
            <p className="text-red-400 text-sm mt-4">{error}</p>
          )}
        </div>
      </div>
    );
  }

  if (!isInitialized) {
    return (
      <div className="h-screen flex items-center justify-center bg-background">
        <div className="text-center">
          <div className="animate-spin w-8 h-8 border-4 border-teal-500 border-t-transparent rounded-full mx-auto mb-4"></div>
          <p className="text-muted">Initializing...</p>
          {error && (
            <p className="text-red-400 text-sm mt-4 max-w-md mx-auto">{error}</p>
          )}
        </div>
      </div>
    );
  }

  return (
    <div className="h-screen flex bg-background">
      {/* Sidebar */}
      <Sidebar
        conversations={conversations}
        currentConversationId={currentConversationId}
        onNewConversation={handleNewConversation}
        onSelectConversation={handleSelectConversation}
        onDeleteConversation={handleDeleteConversation}
        onRenameConversation={handleRenameConversation}
        onOpenSettings={() => setShowSettings(true)}
      />

      {/* Settings Modal */}
      {showSettings && (
        <Settings onClose={() => setShowSettings(false)} />
      )}

      {/* Main chat area */}
      <div className="flex-1 flex flex-col min-w-0">
        {/* Top bar */}
        <div className="border-b border-gray-700 px-4 py-3 flex items-center justify-between bg-background">
          <h1 className="text-lg font-semibold text-foreground">
            Noema
          </h1>
          <ModelSelector
            models={models}
            currentModel={currentModel}
            onSelectModel={handleSelectModel}
          />
        </div>

        {/* Error banner */}
        {error && (
          <div className="bg-red-900/50 text-red-200 px-4 py-2 flex items-center justify-between">
            <span className="text-sm">{error}</span>
            <button
              onClick={() => setError(null)}
              className="text-red-300 hover:text-red-100"
            >
              <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
              </svg>
            </button>
          </div>
        )}

        {/* Messages area */}
        <div ref={messagesContainerRef} className="flex-1 overflow-y-auto p-4">
          <div className="max-w-4xl mx-auto">
            {messages.length === 0 && !streamingMessage ? (
              <div className="text-center py-20">
                <h2 className="text-2xl font-semibold text-foreground mb-2">
                  Welcome to Noema
                </h2>
                <p className="text-muted">
                  Start a conversation by typing a message below.
                </p>
                <p className="text-muted text-sm mt-4">
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
                    <div className="bg-surface px-4 py-3 rounded-2xl">
                      <div className="flex gap-1">
                        <div className="w-2 h-2 bg-muted rounded-full animate-bounce" style={{ animationDelay: "0ms" }}></div>
                        <div className="w-2 h-2 bg-muted rounded-full animate-bounce" style={{ animationDelay: "150ms" }}></div>
                        <div className="w-2 h-2 bg-muted rounded-full animate-bounce" style={{ animationDelay: "300ms" }}></div>
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
        <ChatInput
          onSend={handleSendMessage}
          disabled={isLoading}
          voiceAvailable={voice.isAvailable}
          voiceStatus={voice.status}
          voiceBufferedCount={voice.bufferedCount}
          onToggleVoice={voice.toggle}
        />
      </div>
    </div>
  );
}

export default App;
