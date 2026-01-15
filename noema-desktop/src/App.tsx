import { useState, useEffect, useRef } from "react";
import { MessageBubble } from "./components/MessageBubble";
import { ChatInput } from "./components/ChatInput";
import { ActivityBar, type ActivityId } from "./components/ActivityBar";
import { SidePanel } from "./components/SidePanel";
import { ModelSelector } from "./components/ModelSelector";
import { FavoriteModelChips } from "./components/FavoriteModelChips";
import { Settings } from "./components/Settings";
import { DocumentPanel } from "./components/DocumentPanel";
import { ViewSelector } from "./components/ViewSelector";
import { EditMessageModal } from "./components/EditMessageModal";
import type { DisplayMessage, ModelInfo, ConversationInfo, InputContentBlock, ToolConfig } from "./generated";
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
  const [parallelSpanSetId, setParallelSpanSetId] = useState<string>("");
  // Completed parallel responses (preserved after streaming ends for display)
  const [completedParallelResponses, setCompletedParallelResponses] = useState<Map<string, DisplayMessage[]>>(new Map());
  // Ref to store parallel responses immediately (bypasses React batching)
  const parallelResponsesRef = useRef<Map<string, DisplayMessage[]>>(new Map());
  // Selected tab for parallel comparison view (value currently unused but may be needed)
  const [_selectedComparisonTab, setSelectedComparisonTab] = useState<string | null>(null);
  const [isInitialized, setIsInitialized] = useState(false);
  const [activeActivity, setActiveActivity] = useState<ActivityId>("conversations");
  const [showSettings, setShowSettings] = useState(false);
  const [availableUsers, setAvailableUsers] = useState<string[]>([]);
  const [activeDocumentId, setActiveDocumentId] = useState<string | null>(null);
  // Document selected in the documents activity (shown in main panel)
  const [selectedDocumentId, setSelectedDocumentId] = useState<string | null>(null);
  // View management for conversation forks
  const [views, setViews] = useState<tauri.ViewInfo[]>([]);
  const [currentViewId, setCurrentViewId] = useState<string | null>(null);
  // Prefilled input text (used when forking from a user message to let them edit and resend)
  const [prefilledInput, setPrefilledInput] = useState<string>("");
  // Tools enabled state - controls whether MCP tools are sent to the model
  const [toolsEnabled, setToolsEnabled] = useState<boolean>(true);
  // Conversation privacy state - warns before using cloud models with private conversations
  const [isConversationPrivate, setIsConversationPrivate] = useState<boolean>(false);
  // Privacy warning dialog state
  const [privacyWarning, setPrivacyWarning] = useState<{
    show: boolean;
    pendingContent: InputContentBlock[];
    pendingToolConfig?: ToolConfig;
  }>({ show: false, pendingContent: [] });
  // Edit message modal state
  const [editingMessage, setEditingMessage] = useState<{
    turnId: string;
    text: string;
  } | null>(null);

  const messagesEndRef = useRef<HTMLDivElement>(null);
  const messagesContainerRef = useRef<HTMLDivElement>(null);

  // Voice input hook - handles browser audio capture and Whisper transcription
  const handleVoiceTranscription = (text: string) => {
    // Voice transcriptions use current tools state
    const toolConfig: ToolConfig = { enabled: toolsEnabled, serverIds: null, toolNames: null };
    handleSendMessage([{ type: "text", text }], toolConfig);
  };

  const handleVoiceError = (err: string) => {
    setError(`Voice error: ${err}`);
  };

  const handleToggleTools = () => {
    setToolsEnabled((prev) => !prev);
  };

  const handleTogglePrivate = async () => {
    if (!currentConversationId) return;
    const newPrivate = !isConversationPrivate;
    try {
      await tauri.setConversationPrivate(currentConversationId, newPrivate);
      setIsConversationPrivate(newPrivate);
      // Refresh conversation list to show updated privacy status
      const convos = await tauri.listConversations();
      setConversations(convos);
    } catch (err) {
      appLog.error("Toggle private error", String(err));
      setError(String(err));
    }
  };

  // Check if current model is private (local) by looking at capabilities
  const isCurrentModelPrivate = (): boolean => {
    const currentModelObj = models.find(
      (m) => m.displayName === currentModel || m.id === currentModel
    );
    return currentModelObj?.capabilities.includes("Private") ?? false;
  };

  // Check if current model supports vision (image input)
  const currentModelHasVision = (): boolean => {
    const currentModelObj = models.find(
      (m) => m.displayName === currentModel || m.id === currentModel
    );
    return currentModelObj?.capabilities.includes("Vision") ?? true;
  };

  // Check if current model supports audio input
  const currentModelHasAudioInput = (): boolean => {
    const currentModelObj = models.find(
      (m) => m.displayName === currentModel || m.id === currentModel
    );
    return currentModelObj?.capabilities.includes("AudioInput") ?? true;
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

        // Pick the most recent conversation, or create a new one
        let convId: string;
        if (convos.length > 0) {
          convId = convos[0].id;
        } else {
          convId = await tauri.newConversation();
        }
        setCurrentConversationId(convId);

        // Load privacy status for current conversation
        const isPrivate = await tauri.getConversationPrivate(convId);
        setIsConversationPrivate(isPrivate);

        // Load messages for this conversation
        const msgs = await tauri.loadConversation(convId);
        setMessages(Array.isArray(msgs) ? msgs : []);

        // Load views for this conversation
        const convViews = await tauri.listConversationViews(convId);
        setViews(convViews);
        const viewId = await tauri.getCurrentViewId(convId);
        setCurrentViewId(viewId);

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

    tauri.onUserMessage(({ conversationId, message: msg }) => {
      // Only update if this event is for the current conversation
      setCurrentConversationId((currentId) => {
        if (currentId === conversationId) {
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
        }
        return currentId;
      });
    }).then((unlisten) => unlisteners.push(unlisten));

    tauri.onStreamingMessage(({ conversationId, message: msg }) => {
      // Only update if this event is for the current conversation
      setCurrentConversationId((currentId) => {
        if (currentId === conversationId) {
          setStreamingMessage(msg);
        }
        return currentId;
      });
    }).then((unlisten) => unlisteners.push(unlisten));

    tauri.onMessageComplete(({ conversationId }) => {
      // Only update if this event is for the current conversation
      setCurrentConversationId((currentId) => {
        if (currentId === conversationId) {
          // Reload messages from storage to get alternates info
          tauri.loadConversation(conversationId).then((msgs) => {
            setMessages(Array.isArray(msgs) ? msgs : []);
          }).catch(console.error);
          setStreamingMessage(null);
          setIsLoading(false);
        }
        return currentId;
      });
      // Refresh conversations
      tauri.listConversations().then(setConversations).catch(console.error);
    }).then((unlisten) => unlisteners.push(unlisten));

    tauri.onError(({ conversationId, error }) => {
      appLog.error("Backend error received", error);
      setCurrentConversationId((currentId) => {
        if (currentId === conversationId) {
          setError(error);
          setIsLoading(false);
          setStreamingMessage(null);
        }
        return currentId;
      });
    }).then((unlisten) => unlisteners.push(unlisten));

    tauri.onModelChanged(({ conversationId, model }) => {
      setCurrentConversationId((currentId) => {
        if (currentId === conversationId) {
          setCurrentModel(model);
        }
        return currentId;
      });
    }).then((unlisten) => unlisteners.push(unlisten));

    tauri.onHistoryCleared(({ conversationId }) => {
      setCurrentConversationId((currentId) => {
        if (currentId === conversationId) {
          setMessages([]);
        }
        return currentId;
      });
    }).then((unlisten) => unlisteners.push(unlisten));

    // Handle truncation (used during regeneration)
    tauri.onTruncated(({ conversationId, turnId }) => {
      setCurrentConversationId((currentId) => {
        if (currentId === conversationId) {
          if (turnId === null) {
            // Full clear
            setMessages([]);
          } else {
            // Truncate to before the specified turn - remove messages with this turnId and after
            setMessages((prev) => {
              const turnIndex = prev.findIndex((msg) => msg.turnId === turnId);
              if (turnIndex >= 0) {
                return prev.slice(0, turnIndex);
              }
              return prev;
            });
          }
          setIsLoading(true);
        }
        return currentId;
      });
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

    tauri.onParallelComplete(({ spanSetId, alternates }) => {
      // Skip if ref is empty (duplicate event from React StrictMode)
      if (parallelResponsesRef.current.size === 0) {
        return;
      }

      setParallelSpanSetId(spanSetId);
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

  const handleSendMessage = async (content: InputContentBlock[], toolConfig?: ToolConfig, skipPrivacyCheck?: boolean) => {
    try {
      setError(null);

      // Check if we need to show privacy warning (private conversation + cloud model)
      if (!skipPrivacyCheck && isConversationPrivate && !isCurrentModelPrivate()) {
        setPrivacyWarning({
          show: true,
          pendingContent: content,
          pendingToolConfig: toolConfig,
        });
        return;
      }

      // Clear any prefilled text after sending
      if (prefilledInput) {
        setPrefilledInput("");
      }

      // Check if we have multiple models selected for parallel comparison
      if (selectedModelsForComparison.length >= 2) {
        // Clear any previous comparison results
        setCompletedParallelResponses(new Map());
        parallelResponsesRef.current = new Map();
        // TODO: Parallel comparison mode - spawn multiple engines with different models
        // For now, just send to primary model
        await tauri.sendMessage(currentConversationId, content, toolConfig);
        // Clear selection after sending
        setSelectedModelsForComparison([]);
      } else {
        await tauri.sendMessage(currentConversationId, content, toolConfig);
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
      setIsConversationPrivate(false); // New conversations start as non-private
      const convos = await tauri.listConversations();
      setConversations(convos);
    } catch (err) {
      appLog.error("New conversation error", String(err));
      setError(String(err));
    }
  };

  const handleSelectConversation = async (id: string) => {
    try {
      const msgs = await tauri.loadConversation(id);
      setCurrentConversationId(id);
      setMessages(Array.isArray(msgs) ? msgs : []);
      // Load privacy status for this conversation
      const isPrivate = await tauri.getConversationPrivate(id);
      setIsConversationPrivate(isPrivate);
      // Load views for this conversation
      const convViews = await tauri.listConversationViews(id);
      setViews(convViews);
      const viewId = await tauri.getCurrentViewId(id);
      setCurrentViewId(viewId);
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
          const msgs = await tauri.loadConversation(otherConversation.id);
          setCurrentConversationId(otherConversation.id);
          setMessages(Array.isArray(msgs) ? msgs : []);
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

  const handleSwitchAlternate = async (turnId: string, spanId: string) => {
    appLog.info(`handleSwitchAlternate called: turnId=${turnId}, spanId=${spanId}`);
    try {
      await tauri.selectSpan(currentConversationId, turnId, spanId);
      // Reload messages to reflect the new selection
      const msgs = await tauri.getMessages();
      setMessages(msgs);
    } catch (err) {
      appLog.error("Select span error", String(err));
      setError(String(err));
    }
  };

  // Fork handler - creates a new view forked at the given turn
  const handleFork = async (turnId: string, role: "user" | "assistant", userText?: string) => {
    appLog.info(`Forking at turn: turnId=${turnId}, role=${role}`);
    try {
      setError(null);
      const newView = await tauri.forkConversation(currentConversationId, turnId);
      appLog.info(`Created fork: viewId=${newView.id}`);
      // Switch to the new forked view
      const msgs = await tauri.switchView(currentConversationId, newView.id);
      setMessages(msgs);
      setCurrentViewId(newView.id);
      // Refresh views list
      const convViews = await tauri.listConversationViews(currentConversationId);
      setViews(convViews);
      // For user messages, prefill the input with their original text so they can edit and resend
      if (role === "user" && userText) {
        setPrefilledInput(userText);
      }
    } catch (err) {
      appLog.error("Fork error", String(err));
      setError(String(err));
    }
  };

  // Switch to a different view in the current conversation
  const handleSwitchView = async (viewId: string) => {
    appLog.info(`Switching to view: ${viewId}`);
    try {
      setError(null);
      const msgs = await tauri.switchView(currentConversationId, viewId);
      setMessages(msgs);
      setCurrentViewId(viewId);
    } catch (err) {
      appLog.error("Switch view error", String(err));
      setError(String(err));
    }
  };

  // Clear prefilled input (used after forking when user wants to cancel the edit)
  const handleClearPrefill = () => {
    setPrefilledInput("");
  };

  // Regenerate response at a specific turn
  const handleRegenerate = async (turnId: string) => {
    try {
      setError(null);
      setIsLoading(true);
      await tauri.regenerateResponse(currentConversationId, turnId);
    } catch (err) {
      appLog.error("Regenerate error", String(err));
      setError(String(err));
      setIsLoading(false);
    }
  };

  // Open edit modal for a user message
  const handleEdit = (turnId: string, currentText: string) => {
    setEditingMessage({ turnId, text: currentText });
  };

  // Submit edited message - creates a fork with new content and triggers AI response
  const handleEditSubmit = async (newText: string) => {
    if (!editingMessage) return;

    try {
      setError(null);
      setIsLoading(true);

      const content: InputContentBlock[] = [{ type: "text", text: newText }];
      const toolConfig: ToolConfig = { enabled: toolsEnabled, serverIds: null, toolNames: null };
      const result = await tauri.editMessage(currentConversationId, editingMessage.turnId, content, toolConfig);

      // Update state with the new view and messages
      setMessages(result.messages);
      setCurrentViewId(result.view.id);

      // Refresh views list to include the new fork
      const convViews = await tauri.listConversationViews(currentConversationId);
      setViews(convViews);

      // Close the modal (AI response will stream in via events)
      setEditingMessage(null);
    } catch (err) {
      appLog.error("Edit message error", String(err));
      setIsLoading(false);
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

      // Pick the most recent conversation, or create a new one
      let convId: string;
      if (convos.length > 0) {
        convId = convos[0].id;
      } else {
        convId = await tauri.newConversation();
      }
      setCurrentConversationId(convId);
      const msgs = await tauri.loadConversation(convId);
      setMessages(Array.isArray(msgs) ? msgs : []);
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

      {/* Privacy Warning Dialog */}
      {privacyWarning.show && (
        <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
          <div className="bg-surface rounded-lg p-6 max-w-md mx-4 shadow-xl border border-gray-700">
            <div className="flex items-center gap-3 mb-4">
              <div className="bg-amber-900/50 p-2 rounded-full">
                <svg className="w-6 h-6 text-amber-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
                </svg>
              </div>
              <h2 className="text-lg font-semibold text-foreground">Privacy Warning</h2>
            </div>
            <p className="text-muted mb-6">
              This conversation is marked as <span className="text-amber-300 font-medium">private</span>, but you're using a <span className="text-blue-300 font-medium">cloud model</span>. Your message will be sent to an external provider.
            </p>
            <div className="flex flex-col gap-2">
              <button
                onClick={() => {
                  // Send anyway
                  handleSendMessage(privacyWarning.pendingContent, privacyWarning.pendingToolConfig, true);
                  setPrivacyWarning({ show: false, pendingContent: [] });
                }}
                className="w-full px-4 py-2 bg-amber-600 hover:bg-amber-700 text-white rounded-lg font-medium transition-colors"
              >
                Send Anyway
              </button>
              <button
                onClick={() => setPrivacyWarning({ show: false, pendingContent: [] })}
                className="w-full px-4 py-2 bg-gray-700 hover:bg-gray-600 text-gray-200 rounded-lg font-medium transition-colors"
              >
                Cancel
              </button>
            </div>
            <p className="text-xs text-muted mt-4 text-center">
              Tip: Switch to a local model (marked with a green lock) to keep your data private.
            </p>
          </div>
        </div>
      )}

      {/* Edit Message Modal */}
      {editingMessage && (
        <EditMessageModal
          initialText={editingMessage.text}
          onSubmit={handleEditSubmit}
          onCancel={() => setEditingMessage(null)}
        />
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
          <div className="flex items-center gap-3">
            <h1 className="text-lg font-semibold text-foreground">
              Noema
            </h1>
            {/* Privacy toggle for current conversation */}
            {activeActivity === "conversations" && (
              <button
                onClick={handleTogglePrivate}
                className={`flex items-center gap-1.5 px-2 py-1 rounded-lg text-xs transition-colors ${
                  isConversationPrivate
                    ? "bg-amber-900/50 text-amber-300 hover:bg-amber-900/70"
                    : "bg-gray-700/50 text-gray-400 hover:bg-gray-700/70"
                }`}
                title={isConversationPrivate
                  ? "Private: Will warn before using cloud models"
                  : "Not private: Click to mark as private"
                }
              >
                {isConversationPrivate ? (
                  <svg className="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z" />
                  </svg>
                ) : (
                  <svg className="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M8 11V7a4 4 0 118 0m-4 8v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2z" />
                  </svg>
                )}
                <span>{isConversationPrivate ? "Private" : "Not Private"}</span>
              </button>
            )}
            {/* View selector - shown when conversation has multiple views (forks) */}
            {activeActivity === "conversations" && views.length > 1 && (
              <ViewSelector
                views={views}
                currentViewId={currentViewId}
                onSwitchView={handleSwitchView}
              />
            )}
          </div>
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
                        onFork={handleFork}
                        onRegenerate={handleRegenerate}
                        onEdit={handleEdit}
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
                    {/* Completed parallel responses - side-by-side comparison after all models finish */}
                    {!isParallelMode && completedParallelResponses.size > 0 && (
                      <div className="mb-4">
                        <div className="text-sm text-muted mb-2">
                          Compare responses and pick one:
                        </div>
                        {/* Side-by-side grid of all responses */}
                        <div className="grid gap-4" style={{ gridTemplateColumns: `repeat(${completedParallelResponses.size}, minmax(0, 1fr))` }}>
                          {Array.from(completedParallelResponses.entries()).map(([modelId, messages], index) => {
                            const alternate = parallelAlternates.find(a => a.modelId === modelId);
                            const isSelected = alternate?.isSelected ?? (index === 0);
                            return (
                              <div
                                key={modelId}
                                className={`bg-surface rounded-lg overflow-hidden ${
                                  isSelected
                                    ? "border-2 border-teal-500"
                                    : "border border-gray-700"
                                }`}
                              >
                                {/* Model header with pick button */}
                                <div className={`flex items-center justify-between px-3 py-2 border-b ${
                                  isSelected ? "bg-teal-900/30 border-teal-600" : "bg-elevated border-gray-700"
                                }`}>
                                  <span className="text-sm font-medium text-foreground flex items-center gap-1">
                                    {isSelected && (
                                      <svg className="w-4 h-4 text-teal-400" fill="currentColor" viewBox="0 0 20 20">
                                        <path fillRule="evenodd" d="M16.707 5.293a1 1 0 010 1.414l-8 8a1 1 0 01-1.414 0l-4-4a1 1 0 011.414-1.414L8 12.586l7.293-7.293a1 1 0 011.414 0z" clipRule="evenodd" />
                                      </svg>
                                    )}
                                    {modelId.split('/').pop()}
                                    {isSelected && <span className="text-xs text-teal-400 ml-1">(current)</span>}
                                  </span>
                                  <button
                                    onClick={async () => {
                                      // Find the span_id for this model from parallelAlternates
                                      const alt = parallelAlternates.find(a => a.modelId === modelId);
                                      appLog.info(`Use this clicked: modelId=${modelId}, parallelSpanSetId=${parallelSpanSetId}, alternate=${JSON.stringify(alt)}, all=${JSON.stringify(parallelAlternates)}`);
                                      // NOTE: Span selection is pending re-implementation in new UCM model
                                      appLog.warn("Span selection (setSelectedSpan) not yet implemented in new UCM model");
                                      // Clear comparison view (selection doesn't persist yet)
                                      setCompletedParallelResponses(new Map());
                                      setSelectedComparisonTab(null);
                                      setParallelSpanSetId("");
                                      setParallelAlternates([]);
                                      // Reload messages
                                      const msgs = await tauri.getMessages();
                                      setMessages(msgs);
                                    }}
                                    className={`px-2 py-1 text-xs rounded transition-colors ${
                                      isSelected
                                        ? "bg-teal-700 text-teal-100 hover:bg-teal-600"
                                        : "bg-teal-600 text-white hover:bg-teal-500"
                                    }`}
                                  >
                                    {isSelected ? "Keep this" : "Use this"}
                                  </button>
                                </div>
                                {/* Response content */}
                                <div className="p-3 max-h-96 overflow-y-auto">
                                  {messages.map((msg, idx) => (
                                    <MessageBubble
                                      key={idx}
                                      message={msg}
                                      onDocumentClick={setActiveDocumentId}
                                    />
                                  ))}
                                </div>
                              </div>
                            );
                          })}
                        </div>
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
              prefilledText={prefilledInput}
              onClearPrefill={handleClearPrefill}
              toolsEnabled={toolsEnabled}
              onToggleTools={handleToggleTools}
              modelHasVision={currentModelHasVision()}
              modelHasAudioInput={currentModelHasAudioInput()}
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
