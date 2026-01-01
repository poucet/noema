import { useState, useEffect, useRef } from "react";
import { MessageBubble } from "./components/MessageBubble";
import { ChatInput } from "./components/ChatInput";
import { ActivityBar, type ActivityId } from "./components/ActivityBar";
import { SidePanel } from "./components/SidePanel";
import { ModelSelector } from "./components/ModelSelector";
import { FavoriteModelChips } from "./components/FavoriteModelChips";
import { Settings } from "./components/Settings";
import { DocumentPanel } from "./components/DocumentPanel";
import type { DisplayMessage, ModelInfo, ConversationInfo, Attachment } from "./types";
import type { ReferencedDocument } from "./components/ChatInput";
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
  const [currentModelId, setCurrentModelId] = useState(""); // Full model ID (provider/model)
  const [favoriteModels, setFavoriteModels] = useState<string[]>([]);
  const [selectedModelsForComparison, setSelectedModelsForComparison] = useState<string[]>([]);
  // Parallel execution state
  const [isParallelMode, setIsParallelMode] = useState(false);
  const [parallelStreaming, setParallelStreaming] = useState<Map<string, DisplayMessage>>(new Map());
  const [parallelAlternates, setParallelAlternates] = useState<tauri.ParallelAlternateInfo[]>([]);
  // Completed parallel responses (preserved after streaming ends for display)
  const [completedParallelResponses, setCompletedParallelResponses] = useState<Map<string, DisplayMessage[]>>(new Map());
  // Ref to store parallel responses immediately (bypasses React batching)
  const parallelResponsesRef = useRef<Map<string, DisplayMessage[]>>(new Map());
  // Selected tab for parallel comparison view
  const [selectedComparisonTab, setSelectedComparisonTab] = useState<string | null>(null);
  const [isInitialized, setIsInitialized] = useState(false);
  const [activeActivity, setActiveActivity] = useState<ActivityId>("conversations");
  const [showSettings, setShowSettings] = useState(false);
  const [availableUsers, setAvailableUsers] = useState<string[]>([]);
  const [activeDocumentId, setActiveDocumentId] = useState<string | null>(null);
  // Document selected in the documents activity (shown in main panel)
  const [selectedDocumentId, setSelectedDocumentId] = useState<string | null>(null);

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

        // Load messages with alternates info for span awareness
        const msgs = await tauri.getMessagesWithAlternates();
        setMessages(msgs);

        // Load models in background
        tauri.listModels().then(setModels).catch(console.error);

        // Load favorite models
        tauri.getFavoriteModels().then(setFavoriteModels).catch(console.error);

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

    // Parallel execution events
    tauri.onParallelStreamingMessage(({ modelId, message }) => {
      setIsParallelMode(true);
      setIsLoading(true);
      setParallelStreaming((prev) => new Map(prev).set(modelId, message));
    }).then((unlisten) => unlisteners.push(unlisten));

    tauri.onParallelModelComplete(({ modelId, messages: modelMessages }) => {
      // Store complete messages in ref immediately (bypasses React batching)
      parallelResponsesRef.current.set(modelId, modelMessages);
      // Also update streaming map with final message for display during streaming
      if (modelMessages.length > 0) {
        setParallelStreaming((prev) => new Map(prev).set(modelId, modelMessages[modelMessages.length - 1]));
      }
    }).then((unlisten) => unlisteners.push(unlisten));

    tauri.onParallelComplete(({ alternates }) => {
      // Skip if ref is empty (duplicate event from React StrictMode)
      if (parallelResponsesRef.current.size === 0) {
        return;
      }

      setParallelAlternates(alternates);
      setIsLoading(false);
      // Copy from ref (which has complete data) to state for display
      setCompletedParallelResponses(new Map(parallelResponsesRef.current));
      // Set initial tab to first model
      const firstModel = Array.from(parallelResponsesRef.current.keys())[0];
      if (firstModel) {
        setSelectedComparisonTab(firstModel);
      }
      // Clear streaming and ref
      setParallelStreaming(new Map());
      parallelResponsesRef.current = new Map();
      setIsParallelMode(false);
      // Refresh messages to get the final state (will have first response)
      tauri.getMessages().then(setMessages).catch(console.error);
      // Refresh conversations
      tauri.listConversations().then(setConversations).catch(console.error);
    }).then((unlisten) => unlisteners.push(unlisten));

    tauri.onParallelModelError(({ modelId, error: modelError }) => {
      appLog.error(`Parallel model error: ${modelId}`, modelError);
      setError(`${modelId}: ${modelError}`);
    }).then((unlisten) => unlisteners.push(unlisten));

    // Voice events are now handled by the useVoiceInput hook

    return () => {
      unlisteners.forEach((unlisten) => unlisten());
    };
  }, []);

  const handleSendMessage = async (
    message: string,
    attachments: Attachment[] = [],
    referencedDocuments?: ReferencedDocument[]
  ) => {
    try {
      setError(null);

      // Check if we have multiple models selected for parallel comparison
      if (selectedModelsForComparison.length >= 2) {
        // Clear any previous comparison results
        setCompletedParallelResponses(new Map());
        parallelResponsesRef.current = new Map();
        // Use parallel send - note: attachments/documents not yet supported for parallel
        await tauri.sendParallelMessage(message, selectedModelsForComparison);
        // Clear selection after sending
        setSelectedModelsForComparison([]);
      } else if (referencedDocuments && referencedDocuments.length > 0) {
        // Send with document context for RAG
        await tauri.sendMessageWithDocuments(message, attachments, referencedDocuments);
      } else if (attachments.length > 0) {
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
      setCurrentModelId(`${provider}/${modelId}`);
    } catch (err) {
      appLog.error("Select model error", String(err));
      setError(String(err));
    }
  };

  const handleToggleFavorite = async (modelId: string) => {
    try {
      const updatedFavorites = await tauri.toggleFavoriteModel(modelId);
      setFavoriteModels(updatedFavorites);
    } catch (err) {
      appLog.error("Toggle favorite error", String(err));
      setError(String(err));
    }
  };

  const handleToggleModelForComparison = (modelId: string) => {
    setSelectedModelsForComparison((prev) =>
      prev.includes(modelId)
        ? prev.filter((id) => id !== modelId)
        : [...prev, modelId]
    );
  };

  const handleSendToMultipleModels = () => {
    // The "Send to N models" button is a hint - actual sending happens via the main input
    // This could be used to focus the input or show a tooltip
    appLog.info("Send to multiple models - use the chat input to send");
  };

  const handleSwitchAlternate = async (spanSetId: string, spanId: string) => {
    try {
      // Update the selected span in the database
      await tauri.setSelectedSpan(spanSetId, spanId);
      // Reload messages to get the updated content
      const msgs = await tauri.getMessagesWithAlternates();
      setMessages(msgs);
    } catch (err) {
      appLog.error("Switch alternate error", String(err));
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
      tauri.getFavoriteModels().then(setFavoriteModels).catch(console.error);
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
      {/* Activity Bar */}
      <ActivityBar
        activeActivity={activeActivity}
        onActivityChange={setActiveActivity}
        onOpenSettings={() => setShowSettings(true)}
      />

      {/* Side Panel */}
      <SidePanel
        activeActivity={activeActivity}
        conversations={conversations}
        currentConversationId={currentConversationId}
        onNewConversation={handleNewConversation}
        onSelectConversation={handleSelectConversation}
        onDeleteConversation={handleDeleteConversation}
        onRenameConversation={handleRenameConversation}
        selectedDocumentId={selectedDocumentId}
        onSelectDocument={setSelectedDocumentId}
      />

      {/* Settings Modal */}
      {showSettings && (
        <Settings onClose={() => setShowSettings(false)} />
      )}

      {/* Document Panel */}
      {activeDocumentId && (
        <DocumentPanel
          documentId={activeDocumentId}
          onClose={() => setActiveDocumentId(null)}
        />
      )}

      {/* Main content area */}
      <div className="flex-1 flex flex-col min-w-0">
        {/* Top bar */}
        <div className="border-b border-gray-700 px-4 py-3 flex items-center justify-between bg-background">
          <h1 className="text-lg font-semibold text-foreground">
            Noema
          </h1>
          {activeActivity === "conversations" && (
            <ModelSelector
              models={models}
              currentModel={currentModel}
              favoriteModels={favoriteModels}
              onSelectModel={handleSelectModel}
              onToggleFavorite={handleToggleFavorite}
            />
          )}
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

        {/* Content based on activity */}
        {activeActivity === "conversations" ? (
          <>
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
                      <MessageBubble
                        key={i}
                        message={msg}
                        onDocumentClick={setActiveDocumentId}
                        onSwitchAlternate={handleSwitchAlternate}
                      />
                    ))}
                    {streamingMessage && !isParallelMode && (
                      <MessageBubble
                        message={streamingMessage}
                        onDocumentClick={setActiveDocumentId}
                      />
                    )}
                    {/* Parallel streaming view - shows all models' responses while streaming */}
                    {isParallelMode && parallelStreaming.size > 0 && (
                      <div className="mb-4 space-y-2">
                        <div className="text-xs text-muted mb-2">Comparing {parallelStreaming.size} models...</div>
                        <div className="grid grid-cols-1 md:grid-cols-2 gap-2">
                          {Array.from(parallelStreaming.entries()).map(([modelId, msg]) => (
                            <div key={modelId} className="bg-surface rounded-lg p-3 border border-gray-700">
                              <div className="text-xs text-teal-400 mb-2 font-medium">
                                {modelId.split('/').pop()}
                              </div>
                              <MessageBubble
                                message={msg}
                                onDocumentClick={setActiveDocumentId}
                              />
                            </div>
                          ))}
                        </div>
                      </div>
                    )}
                    {/* Completed parallel responses - tabbed comparison after all models finish */}
                    {!isParallelMode && completedParallelResponses.size > 0 && (
                      <div className="mb-4">
                        {/* Tab bar */}
                        <div className="flex items-center gap-1 mb-3 border-b border-gray-700">
                          {Array.from(completedParallelResponses.keys()).map((modelId) => (
                            <button
                              key={modelId}
                              onClick={() => setSelectedComparisonTab(modelId)}
                              className={`px-3 py-2 text-sm font-medium transition-colors border-b-2 -mb-px ${
                                selectedComparisonTab === modelId
                                  ? "text-teal-400 border-teal-400"
                                  : "text-muted border-transparent hover:text-foreground hover:border-gray-500"
                              }`}
                            >
                              {modelId.split('/').pop()}
                            </button>
                          ))}
                          <div className="flex-1" />
                          <button
                            onClick={() => {
                              setCompletedParallelResponses(new Map());
                              setSelectedComparisonTab(null);
                            }}
                            className="px-2 py-1 text-xs text-muted hover:text-foreground"
                          >
                            Dismiss
                          </button>
                        </div>
                        {/* Tab content */}
                        {selectedComparisonTab && completedParallelResponses.get(selectedComparisonTab) && (
                          <div className="bg-surface rounded-lg p-3 border border-gray-700">
                            {completedParallelResponses.get(selectedComparisonTab)!.map((msg, idx) => (
                              <MessageBubble
                                key={idx}
                                message={msg}
                                onDocumentClick={setActiveDocumentId}
                              />
                            ))}
                          </div>
                        )}
                      </div>
                    )}
                    {isLoading && !streamingMessage && !isParallelMode && (
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

            {/* Favorite model chips for parallel comparison */}
            <FavoriteModelChips
              models={models}
              favoriteModelIds={favoriteModels}
              selectedModelIds={selectedModelsForComparison}
              currentModelId={currentModelId}
              onToggleSelection={handleToggleModelForComparison}
              onSendToModels={handleSendToMultipleModels}
            />

            {/* Input area */}
            <ChatInput
              onSend={handleSendMessage}
              disabled={isLoading}
              voiceAvailable={voice.isAvailable}
              voiceStatus={voice.status}
              voiceBufferedCount={voice.bufferedCount}
              onToggleVoice={voice.toggle}
            />
          </>
        ) : activeActivity === "documents" ? (
          // Documents activity - show document viewer in main panel
          selectedDocumentId ? (
            <div className="flex-1 overflow-hidden">
              <DocumentPanel
                documentId={selectedDocumentId}
                onClose={() => setSelectedDocumentId(null)}
                embedded={true}
              />
            </div>
          ) : (
            <div className="flex-1 flex items-center justify-center">
              <div className="text-center p-8">
                <svg
                  className="w-16 h-16 text-muted mx-auto mb-4"
                  fill="none"
                  stroke="currentColor"
                  viewBox="0 0 24 24"
                >
                  <path
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    strokeWidth={1.5}
                    d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z"
                  />
                </svg>
                <p className="text-muted text-lg mb-2">Select a document</p>
                <p className="text-muted text-sm">
                  Choose a document from the sidebar to view its contents
                </p>
              </div>
            </div>
          )
        ) : null}
      </div>
    </div>
  );
}

export default App;
