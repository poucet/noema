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
    return (
      <div className="bg-purple-900/50 text-purple-200 px-3 py-2 rounded-lg text-sm">
        <span className="font-semibold">Tool Call:</span> {block.toolCall.name}
      </div>
    );
  }

  if ("toolResult" in block) {
    return (
      <div className="bg-teal-900/50 text-teal-200 px-3 py-2 rounded-lg text-sm">
        <span className="font-semibold">Tool Result:</span>
        <div className="mt-1">
          {block.toolResult.content.map((content, i) => (
            <div key={i}>{renderToolResultContent(content)}</div>
          ))}
        </div>
      </div>
    );
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
