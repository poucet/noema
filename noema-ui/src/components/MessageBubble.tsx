import { useState } from "react";
import type { DisplayMessage, DisplayContent } from "../types";
import { AlternatesSelector } from "./message/AlternatesSelector";
import { ContentBlock } from "./message/ContentBlock";
import { ForkIcon } from "./message/ForkIcon";

interface MessageBubbleProps {
  message: DisplayMessage;
  onDocumentClick?: (docId: string) => void;
  onSwitchAlternate?: (spanSetId: string, spanId: string) => void;
  // Fork handler: spanId, role, and optionally the user's text (for user messages)
  onFork?: (spanId: string, role: "user" | "assistant", userText?: string) => void;
}

export function MessageBubble({ message, onDocumentClick, onSwitchAlternate, onFork }: MessageBubbleProps) {
  const isUser = message.role === "user";
  const isSystem = message.role === "system";
  const hasAlternates = message.alternates && message.alternates.length > 1;

  // Local preview state - which alternate we're viewing (null = show saved selection)
  const [previewSpanId, setPreviewSpanId] = useState<string | null>(null);
  const [previewContent, setPreviewContent] = useState<DisplayContent[] | null>(null);
  const [isLoadingPreview, setIsLoadingPreview] = useState(false);

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
  const handleConfirmSelection = async (spanSetId: string, spanId: string) => {
    if (onSwitchAlternate) {
      await onSwitchAlternate(spanSetId, spanId);
    }
    // Clear preview state after confirming
    setPreviewSpanId(null);
    setPreviewContent(null);
  };

  // Handle fork click
  const handleForkClick = () => {
    if (!onFork || !message.spanId) return;

    if (isUser) {
      // For user messages: extract text and pass to fork handler
      const userText = message.content
        .filter((c): c is { text: string } => "text" in c)
        .map((c) => c.text)
        .join("\n");
      onFork(message.spanId, "user", userText);
    } else {
      // For assistant messages: just pass the spanId
      onFork(message.spanId, "assistant");
    }
  };

  // Determine which content to show
  const contentToShow = previewContent || message.content;

  // Can fork if we have a spanId and fork handler
  const canFork = onFork && message.spanId && !isSystem;

  return (
    <div
      className={`flex ${isUser ? "justify-end" : "justify-start"} mb-4 group`}
    >
      <div
        className={`max-w-[85%] px-4 py-3 rounded-2xl relative ${isUser ? "bg-teal-600 text-white" : isSystem ? "bg-amber-500/20 text-amber-100" : "bg-surface text-foreground"}`}
      >
        {/* Show alternates selector for assistant messages with multiple responses */}
        {hasAlternates && message.spanSetId && (
          <AlternatesSelector
            alternates={message.alternates!}
            spanSetId={message.spanSetId}
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
        {/* Fork button - shown at bottom right of every message on hover */}
        {canFork && (
          <button
            onClick={handleForkClick}
            className={`absolute bottom-1 right-1 p-1 rounded opacity-0 group-hover:opacity-100 transition-opacity ${isUser ? "text-teal-200 hover:text-white hover:bg-teal-500" : "text-muted hover:text-purple-400 hover:bg-purple-900/30"}`}
            title={isUser ? "Fork with this message" : "Fork from this response"}
          >
            <ForkIcon />
          </button>
        )}
      </div>
    </div>
  );
}