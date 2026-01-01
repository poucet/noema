import { useState } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import remarkMath from "remark-math";
import rehypeKatex from "rehype-katex";
import type { DisplayMessage, DisplayContent, DisplayToolResultContent, AlternateInfo } from "../types";
import { AudioPlayer } from "./AudioPlayer";
import { ImageViewer } from "./ImageViewer";

interface MessageBubbleProps {
  message: DisplayMessage;
  onDocumentClick?: (docId: string) => void;
  onSwitchAlternate?: (spanSetId: string, spanId: string) => void;
}

interface MarkdownTextProps {
  text: string;
  onDocumentClick?: (docId: string) => void;
}

function MarkdownText({ text, onDocumentClick }: MarkdownTextProps) {
  return (
    <ReactMarkdown
      remarkPlugins={[remarkGfm, remarkMath]}
      rehypePlugins={[rehypeKatex]}
      components={{
        a({ href, children }) {
          // Check for noema://doc/ links (document references)
          if (href?.startsWith('noema://doc/')) {
            const docId = href.replace('noema://doc/', '');
            return (
              <button
                onClick={() => onDocumentClick?.(docId)}
                className="text-teal-400 hover:text-teal-300 underline inline-flex items-center gap-1"
              >
                <svg className="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
                </svg>
                {children}
              </button>
            );
          }
          // Regular external links - open in browser
          return (
            <a
              href={href}
              target="_blank"
              rel="noopener noreferrer"
              className="text-teal-400 hover:text-teal-300 underline"
            >
              {children}
            </a>
          );
        },
        code(props) {
          const { children, className } = props;
          const isInline = !className;
          return isInline ? (
            <code className="bg-elevated text-gray-100 px-1 py-0.5 rounded text-sm">
              {children}
            </code>
          ) : (
            <code className={className}>{children}</code>
          );
        },
        pre(props) {
          return (
            <pre className="bg-background text-gray-100 p-3 rounded-lg overflow-x-auto text-sm">
              {props.children}
            </pre>
          );
        },
      }}
    >
      {text}
    </ReactMarkdown>
  );
}

function renderToolResultContent(content: DisplayToolResultContent): React.ReactNode {
  if ("text" in content) {
    return <MarkdownText text={content.text} />;
  }
  if ("image" in content) {
    return (
      <ImageViewer data={content.image.data} mimeType={content.image.mimeType} alt="Tool result" />
    );
  }
  if ("audio" in content) {
    return (
      <AudioPlayer data={content.audio.data} mimeType={content.audio.mimeType} />
    );
  }
  return null;
}

function getToolResultSummary(content: DisplayToolResultContent[]): string {
  if (content.length === 0) return "Empty result";
  const first = content[0];
  if ("text" in first) {
    const text = first.text;
    if (text.length <= 60) return text;
    return text.slice(0, 60) + "...";
  }
  if ("image" in first) return "[Image]";
  if ("audio" in first) return "[Audio]";
  return "[Result]";
}

function ToolCallBlock({ name, arguments: args }: { name: string; arguments: unknown }) {
  const [expanded, setExpanded] = useState(false);
  const argsString = args && typeof args === "object"
    ? JSON.stringify(args, null, 2)
    : String(args ?? "");
  const shortArgs = args && typeof args === "object"
    ? JSON.stringify(args)
    : String(args ?? "");
  const shortDisplay = shortArgs.length > 60 ? shortArgs.slice(0, 60) + "..." : shortArgs;

  return (
    <div className="bg-purple-900/50 text-purple-200 px-3 py-2 rounded-lg text-sm">
      <button
        onClick={() => setExpanded(!expanded)}
        className="w-full text-left flex items-center gap-2"
      >
        <span className="text-purple-400">{expanded ? "▼" : "▶"}</span>
        <span className="font-semibold">{name}</span>
        {!expanded && shortDisplay && (
          <span className="text-purple-300/70 text-xs truncate flex-1">{shortDisplay}</span>
        )}
      </button>
      {expanded && argsString && (
        <pre className="mt-2 text-xs bg-purple-950/50 p-2 rounded overflow-x-auto whitespace-pre-wrap">
          {argsString}
        </pre>
      )}
    </div>
  );
}

function ToolResultBlock({ content }: { content: DisplayToolResultContent[] }) {
  const [expanded, setExpanded] = useState(false);
  const summary = getToolResultSummary(content);

  return (
    <div className="bg-teal-900/50 text-teal-200 px-3 py-2 rounded-lg text-sm">
      <button
        onClick={() => setExpanded(!expanded)}
        className="w-full text-left flex items-center gap-2"
      >
        <span className="text-teal-400">{expanded ? "▼" : "▶"}</span>
        <span className="font-semibold">Result</span>
        {!expanded && (
          <span className="text-teal-300/70 text-xs truncate flex-1">{summary}</span>
        )}
      </button>
      {expanded && (
        <div className="mt-2">
          {content.map((c, i) => (
            <div key={i}>{renderToolResultContent(c)}</div>
          ))}
        </div>
      )}
    </div>
  );
}

interface ContentBlockProps {
  block: DisplayContent;
  onDocumentClick?: (docId: string) => void;
}

function ContentBlock({ block, onDocumentClick }: ContentBlockProps) {
  if ("text" in block) {
    return <MarkdownText text={block.text} onDocumentClick={onDocumentClick} />;
  }

  if ("image" in block) {
    return (
      <ImageViewer data={block.image.data} mimeType={block.image.mimeType} alt="Message attachment" />
    );
  }

  if ("audio" in block) {
    return (
      <AudioPlayer data={block.audio.data} mimeType={block.audio.mimeType} />
    );
  }

  if ("toolCall" in block) {
    return <ToolCallBlock name={block.toolCall.name} arguments={block.toolCall.arguments} />;
  }

  if ("toolResult" in block) {
    return <ToolResultBlock content={block.toolResult.content} />;
  }

  return null;
}

// Alternates selector component for assistant messages with multiple model responses
// Separates "viewing" (preview) from "selecting" (committing to database)
function AlternatesSelector({
  alternates,
  spanSetId,
  previewSpanId,
  onPreview,
  onConfirmSelection,
}: {
  alternates: AlternateInfo[];
  spanSetId: string;
  previewSpanId: string | null;
  onPreview: (spanId: string) => void;
  onConfirmSelection?: (spanSetId: string, spanId: string) => void;
}) {
  // Find which is the saved selection and which is being previewed
  const savedSelection = alternates.find(a => a.isSelected);
  const currentlyViewing = previewSpanId
    ? alternates.find(a => a.spanId === previewSpanId)
    : savedSelection;
  const isPreviewingDifferent = previewSpanId && previewSpanId !== savedSelection?.spanId;

  return (
    <div className="flex flex-wrap items-center gap-2 mb-3 pb-2 border-b border-gray-600">
      {alternates.map((alt) => {
        const isViewing = currentlyViewing?.spanId === alt.spanId;
        const isSaved = alt.isSelected;
        return (
          <button
            key={alt.spanId}
            onClick={() => onPreview(alt.spanId)}
            className={`px-3 py-1.5 text-xs rounded-md transition-all whitespace-nowrap flex items-center gap-1.5 border ${
              isViewing
                ? "bg-teal-600 text-white font-semibold border-teal-500 shadow-sm"
                : "bg-elevated text-foreground hover:bg-surface hover:border-gray-500 cursor-pointer border-gray-600"
            }`}
            title={isSaved ? `${alt.modelId || "Model"} (saved)` : `Preview ${alt.modelId || "Model"}`}
          >
            {isSaved && (
              <svg className="w-3 h-3 text-teal-300" fill="currentColor" viewBox="0 0 20 20">
                <path fillRule="evenodd" d="M16.707 5.293a1 1 0 010 1.414l-8 8a1 1 0 01-1.414 0l-4-4a1 1 0 011.414-1.414L8 12.586l7.293-7.293a1 1 0 011.414 0z" clipRule="evenodd" />
              </svg>
            )}
            {alt.modelDisplayName || alt.modelId?.split("/").pop() || "Model"}
          </button>
        );
      })}
      {/* Confirm selection icon button - only show when previewing a different response */}
      {isPreviewingDifferent && onConfirmSelection && (
        <button
          onClick={() => onConfirmSelection(spanSetId, previewSpanId!)}
          className="p-1.5 bg-teal-600 text-white rounded hover:bg-teal-500 transition-colors"
          title="Use this response"
        >
          <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 13l4 4L19 7" />
          </svg>
        </button>
      )}
    </div>
  );
}

export function MessageBubble({ message, onDocumentClick, onSwitchAlternate }: MessageBubbleProps) {
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

  // Determine which content to show
  const contentToShow = previewContent || message.content;

  return (
    <div
      className={`flex ${isUser ? "justify-end" : "justify-start"} mb-4`}
    >
      <div
        className={`max-w-[85%] px-4 py-3 rounded-2xl ${
          isUser
            ? "bg-teal-600 text-white"
            : isSystem
            ? "bg-amber-500/20 text-amber-100"
            : "bg-surface text-foreground"
        }`}
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
      </div>
    </div>
  );
}
