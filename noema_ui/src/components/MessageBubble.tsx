import ReactMarkdown from "react-markdown";
import remarkMath from "remark-math";
import rehypeKatex from "rehype-katex";
import type { DisplayMessage, DisplayContent, DisplayToolResultContent } from "../types";

interface MessageBubbleProps {
  message: DisplayMessage;
}

function renderToolResultContent(content: DisplayToolResultContent): React.ReactNode {
  if ("text" in content) {
    return <span>{content.text}</span>;
  }
  if ("image" in content) {
    return (
      <img
        src={`data:${content.image.mimeType};base64,${content.image.data}`}
        alt="Tool result"
        className="max-w-full rounded"
      />
    );
  }
  if ("audio" in content) {
    return (
      <audio controls className="w-full">
        <source
          src={`data:${content.audio.mimeType};base64,${content.audio.data}`}
          type={content.audio.mimeType}
        />
      </audio>
    );
  }
  return null;
}

function ContentBlock({ block }: { block: DisplayContent }) {
  if ("text" in block) {
    return (
      <ReactMarkdown
        remarkPlugins={[remarkMath]}
        rehypePlugins={[rehypeKatex]}
        components={{
          code(props) {
            const { children, className } = props;
            const isInline = !className;
            return isInline ? (
              <code className="bg-gray-800 text-gray-100 px-1 py-0.5 rounded text-sm">
                {children}
              </code>
            ) : (
              <code className={className}>{children}</code>
            );
          },
          pre(props) {
            return (
              <pre className="bg-gray-900 text-gray-100 p-3 rounded-lg overflow-x-auto text-sm">
                {props.children}
              </pre>
            );
          },
        }}
      >
        {block.text}
      </ReactMarkdown>
    );
  }

  if ("image" in block) {
    return (
      <img
        src={`data:${block.image.mimeType};base64,${block.image.data}`}
        alt="Message attachment"
        className="max-w-full rounded-lg"
      />
    );
  }

  if ("audio" in block) {
    return (
      <audio controls className="w-full">
        <source
          src={`data:${block.audio.mimeType};base64,${block.audio.data}`}
          type={block.audio.mimeType}
        />
      </audio>
    );
  }

  if ("toolCall" in block) {
    return (
      <div className="bg-purple-100 dark:bg-purple-900 text-purple-800 dark:text-purple-200 px-3 py-2 rounded-lg text-sm">
        <span className="font-semibold">Tool Call:</span> {block.toolCall.name}
      </div>
    );
  }

  if ("toolResult" in block) {
    return (
      <div className="bg-green-100 dark:bg-green-900 text-green-800 dark:text-green-200 px-3 py-2 rounded-lg text-sm">
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
            ? "bg-blue-500 text-white"
            : isSystem
            ? "bg-yellow-100 dark:bg-yellow-900 text-yellow-900 dark:text-yellow-100"
            : "bg-gray-100 dark:bg-gray-800 text-gray-900 dark:text-gray-100"
        }`}
      >
        <div className="prose prose-sm dark:prose-invert max-w-none">
          {message.content.map((block, i) => (
            <ContentBlock key={i} block={block} />
          ))}
        </div>
      </div>
    </div>
  );
}
