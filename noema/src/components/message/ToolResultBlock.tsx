import { useState } from "react";
import type { DisplayToolResultContent } from "../../types";
import { AudioPlayer } from "../AudioPlayer";
import { ImageViewer } from "../ImageViewer";
import { MarkdownText } from "./MarkdownText";

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

export function ToolResultBlock({ content }: { content: DisplayToolResultContent[] }) {
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
