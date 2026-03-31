import { useState } from "react";
import type { DisplayMessage, DisplayContent } from "../types";
import { AlternatesSelector } from "./message/AlternatesSelector";
import { ContentBlock } from "./message/ContentBlock";
import { EditIcon } from "./message/EditIcon";
import { ForkIcon } from "./message/ForkIcon";
import { RegenerateIcon } from "./message/RegenerateIcon";

// Extract raw markdown text from content blocks
function extractRawMarkdown(content: DisplayContent[]): string {
  return content
    .map((block) => {
      if ("text" in block) return block.text;
      if ("documentRef" in block) return `[@doc:${block.documentRef.id}]`;
      if ("toolCall" in block) return `[Tool: ${block.toolCall.name}]`;
      if ("toolResult" in block) {
        const textParts = block.toolResult.content
          .filter((c): c is { text: string } => "text" in c)
          .map((c) => c.text);
        return textParts.length > 0 ? `[Result: ${textParts.join("\n")}]` : "[Result]";
      }
      return "";
    })
    .filter(Boolean)
    .join("\n\n");
}

// Copy icon component
const CopyIcon = () => (
  <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
    <path
      strokeLinecap="round"
      strokeLinejoin="round"
      strokeWidth={2}
      d="M8 16H6a2 2 0 01-2-2V6a2 2 0 012-2h8a2 2 0 012 2v2m-6 12h8a2 2 0 002-2v-8a2 2 0 00-2-2h-8a2 2 0 00-2 2v8a2 2 0 002 2z"
    />
  </svg>
);

// Check icon for copy feedback
const CheckIcon = () => (
  <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 13l4 4L19 7" />
  </svg>
);

interface MessageBubbleProps {
  message: DisplayMessage;
  onDocumentClick?: (docId: string) => void;
  onSwitchAlternate?: (turnId: string, spanId: string) => void;
  // Fork handler: turnId, role, and optionally the user's text (for user messages)
  onFork?: (turnId: string, role: "user" | "assistant", userText?: string) => void;
  // Regenerate handler: creates new span at turn with fresh LLM response
  onRegenerate?: (turnId: string) => void;
  // Edit handler: opens edit modal with current message text
  onEdit?: (turnId: string, currentText: string) => void;
}

export function MessageBubble({ message, onDocumentClick, onSwitchAlternate, onFork, onRegenerate, onEdit }: MessageBubbleProps) {
  const isUser = message.role === "user";
  const isSystem = message.role === "system";
  const hasAlternates = message.alternates && message.alternates.length > 1;

  // Local preview state - which alternate we're viewing (null = show saved selection)
  const [previewSpanId, setPreviewSpanId] = useState<string | null>(null);
  const [previewContent, setPreviewContent] = useState<DisplayContent[] | null>(null);
  const [isLoadingPreview, setIsLoadingPreview] = useState(false);

  // Copy feedback state
  const [justCopied, setJustCopied] = useState(false);

  // Handle preview - fetch content for the alternate
  const handlePreview = async (spanId: string) => {
    // If clicking the saved selection, clear preview
    const savedSpanId = message.alternates?.find(a => a.isSelected)?.spanId;
    if (spanId === savedSpanId) {
      setPreviewSpanId(null);
      setPreviewContent(null);
      return;
    }

    // Otherwise, fetch and preview the alternate
    setPreviewSpanId(spanId);
    setIsLoadingPreview(true);
    try {
      // Dynamic import to avoid circular dependency
      const { getSpanMessages } = await import("../tauri");
      const msgs = await getSpanMessages(spanId);
      if (msgs.length > 0) {
        setPreviewContent(msgs[0].content);
      }
    } catch (err) {
      console.error("Failed to load preview:", err);
    } finally {
      setIsLoadingPreview(false);
    }
  };

  // Handle confirm - commit the selection to database
  const handleConfirmSelection = async (turnId: string, spanId: string) => {
    if (onSwitchAlternate) {
      await onSwitchAlternate(turnId, spanId);
    }
    // Clear preview state after confirming
    setPreviewSpanId(null);
    setPreviewContent(null);
  };

  // Handle fork click
  const handleForkClick = () => {
    if (!onFork || !message.turnId) return;

    if (isUser) {
      // For user messages: extract text and pass to fork handler
      const userText = message.content
        .filter((c): c is { text: string } => "text" in c)
        .map((c) => c.text)
        .join("\n");
      onFork(message.turnId, "user", userText);
    } else {
      // For assistant messages: just pass the turnId
      onFork(message.turnId, "assistant");
    }
  };

  // Handle edit click (user messages only)
  const handleEditClick = () => {
    if (!onEdit || !message.turnId || !isUser) return;
    const currentText = message.content
      .filter((c): c is { text: string } => "text" in c)
      .map((c) => c.text)
      .join("\n");
    onEdit(message.turnId, currentText);
  };

  // Handle copy raw markdown
  const handleCopyRawMarkdown = async () => {
    const markdown = extractRawMarkdown(contentToShow);
    try {
      await navigator.clipboard.writeText(markdown);
      setJustCopied(true);
      setTimeout(() => setJustCopied(false), 2000);
    } catch (err) {
      console.error("Failed to copy:", err);
    }
  };

  // Determine which content to show
  const contentToShow = previewContent || message.content || [];

  // Can fork if we have a turnId and fork handler
  const canFork = onFork && message.turnId && !isSystem;

  // Can regenerate if we have a turnId and regenerate handler (assistant messages only)
  const canRegenerate = onRegenerate && message.turnId && !isUser && !isSystem;

  // Can edit if we have a turnId and edit handler (user messages only)
  const canEdit = onEdit && message.turnId && isUser;

  return (
    <div
      className={`flex ${isUser ? "justify-end" : "justify-start"} mb-4 group`}
    >
      <div
        className={`max-w-[85%] px-4 py-3 rounded-2xl relative ${isUser ? "bg-teal-600 text-white" : isSystem ? "bg-amber-500/20 text-amber-100" : "bg-surface text-foreground"}`}
      >
        {/* Show alternates selector for assistant messages with multiple responses */}
        {hasAlternates && message.turnId && !isUser && (
          <AlternatesSelector
            alternates={message.alternates!}
            turnId={message.turnId}
            previewSpanId={previewSpanId}
            onPreview={handlePreview}
            onConfirmSelection={handleConfirmSelection}
          />
        )}
        {/* Message content */}
        <div className="prose prose-sm prose-invert max-w-none">
          {isLoadingPreview ? (
            <div className="flex items-center gap-2 text-muted">
              <div className="w-4 h-4 border-2 border-gray-400 border-t-transparent rounded-full animate-spin" />
              Loading preview...
            </div>
          ) : (
            contentToShow.map((block, i) => (
              <ContentBlock key={i} block={block} onDocumentClick={onDocumentClick} />
            ))
          )}
        </div>
        {/* Action buttons for user messages - inside bubble */}
        {isUser && (canFork || canEdit) && (
          <div className="absolute bottom-1 right-1 flex gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity">
            {canEdit && (
              <button
                onClick={handleEditClick}
                className="p-1 rounded text-teal-200 hover:text-white hover:bg-teal-500"
                title="Edit message"
              >
                <EditIcon />
              </button>
            )}
            {canFork && (
              <button
                onClick={handleForkClick}
                className="p-1 rounded text-teal-200 hover:text-white hover:bg-teal-500"
                title="Fork with this message"
              >
                <ForkIcon />
              </button>
            )}
          </div>
        )}
      </div>
      {/* Action buttons for assistant messages - positioned outside bubble on right */}
      {!isUser && !isSystem && (
        <div className="flex flex-col justify-end ml-1 opacity-0 group-hover:opacity-100 transition-opacity">
          <button
            onClick={handleCopyRawMarkdown}
            className={`p-1.5 rounded transition-colors ${
              justCopied
                ? "text-green-500"
                : "text-gray-500 hover:text-gray-300 hover:bg-gray-700/50"
            }`}
            title={justCopied ? "Copied!" : "Copy"}
          >
            {justCopied ? <CheckIcon /> : <CopyIcon />}
          </button>
          {canRegenerate && (
            <button
              onClick={() => onRegenerate!(message.turnId!)}
              className="p-1.5 rounded text-gray-500 hover:text-gray-300 hover:bg-gray-700/50 transition-colors"
              title="Regenerate response"
            >
              <RegenerateIcon className="w-4 h-4" />
            </button>
          )}
          {canFork && (
            <button
              onClick={handleForkClick}
              className="p-1.5 rounded text-gray-500 hover:text-gray-300 hover:bg-gray-700/50 transition-colors"
              title="Fork from here"
            >
              <ForkIcon />
            </button>
          )}
        </div>
      )}
    </div>
  );
}