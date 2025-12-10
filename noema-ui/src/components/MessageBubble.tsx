import { useState } from "react";
import ReactMarkdown from "react-markdown";
import remarkMath from "remark-math";
import rehypeKatex from "rehype-katex";
import type { DisplayMessage, DisplayContent, DisplayToolResultContent } from "../types";
import { AudioPlayer } from "./AudioPlayer";
import { ImageViewer } from "./ImageViewer";

interface MessageBubbleProps {
  message: DisplayMessage;
}

function MarkdownText({ text }: { text: string }) {
  return (
    <ReactMarkdown
      remarkPlugins={[remarkMath]}
      rehypePlugins={[rehypeKatex]}
      components={{
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

function ContentBlock({ block }: { block: DisplayContent }) {
  if ("text" in block) {
    return <MarkdownText text={block.text} />;
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

export function MessageBubble({ message }: MessageBubbleProps) {
  const isUser = message.role === "user";
  const isSystem = message.role === "system";

  return (
    <div
      className={`flex ${isUser ? "justify-end" : "justify-start"} mb-4`}
    >
      <div
        className={`max-w-[80%] px-4 py-3 rounded-2xl ${
          isUser
            ? "bg-teal-600 text-white"
            : isSystem
            ? "bg-amber-500/20 text-amber-100"
            : "bg-surface text-foreground"
        }`}
      >
        <div className="prose prose-sm prose-invert max-w-none">
          {message.content.map((block, i) => (
            <ContentBlock key={i} block={block} />
          ))}
        </div>
      </div>
    </div>
  );
}
