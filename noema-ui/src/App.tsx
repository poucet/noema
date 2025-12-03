import { useState, useEffect, useRef } from "react";
import { MessageBubble } from "./components/MessageBubble";
import { ChatInput } from "./components/ChatInput";
import { Sidebar } from "./components/Sidebar";
import { ModelSelector } from "./components/ModelSelector";
import { McpSettings } from "./components/McpSettings";
import type { DisplayMessage, ModelInfo, ConversationInfo, Attachment } from "./types";
import * as tauri from "./tauri";
import { useVoiceInput } from "./hooks/useVoiceInput";

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
  const [showMcpSettings, setShowMcpSettings] = useState(false);

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
        setError(String(err));
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
        onOpenMcpSettings={() => setShowMcpSettings(true)}
      />

      {/* MCP Settings Modal */}
      {showMcpSettings && (
        <McpSettings onClose={() => setShowMcpSettings(false)} />
      )}

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
        <div ref={messagesContainerRef} className="flex-1 overflow-y-auto p-4">
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
        <ChatInput
          onSend={handleSendMessage}
          disabled={isLoading}
          voiceAvailable={voice.isAvailable}
          voiceStatus={voice.status}
          onToggleVoice={voice.toggle}
        />
      </div>
    </div>
  );
}

export default App;
