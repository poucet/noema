import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import remarkMath from "remark-math";
import rehypeKatex from "rehype-katex";

interface MarkdownTextProps {
  text: string;
  onDocumentClick?: (docId: string) => void;
}

export function MarkdownText({ text, onDocumentClick }: MarkdownTextProps) {
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
