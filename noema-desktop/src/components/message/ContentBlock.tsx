import type { DisplayContent } from "../../types";
import { AudioPlayer } from "../AudioPlayer";
import { ImageViewer } from "../ImageViewer";
import { MarkdownText } from "./MarkdownText";
import { ToolCallBlock } from "./ToolCallBlock";
import { ToolResultBlock } from "./ToolResultBlock";
import { AssetImage } from "./AssetImage";

interface ContentBlockProps {
  block: DisplayContent;
  onDocumentClick?: (docId: string) => void;
}

export function ContentBlock({ block, onDocumentClick }: ContentBlockProps) {
  if ("text" in block) {
    return <MarkdownText text={block.text} onDocumentClick={onDocumentClick} />;
  }

  if ("image" in block) {
    return (
      <ImageViewer data={block.image.data} mimeType={block.image.mimeType} alt="Message attachment" />
    );
  }

  if ("assetRef" in block) {
    // Load image from asset protocol - assets are served at noema-asset://localhost/{assetId}
    const assetUrl = `noema-asset://localhost/${block.assetRef.assetId}?mime_type=${encodeURIComponent(block.assetRef.mimeType)}`;
    return (
      <AssetImage
        src={assetUrl}
        alt={block.assetRef.filename || "Image"}
      />
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

  if ("documentRef" in block) {
    // Render document reference as a clickable chip
    return (
      <button
        onClick={() => onDocumentClick?.(block.documentRef.id)}
        className="inline-flex items-center gap-1.5 px-2.5 py-1 my-1 rounded-full bg-purple-600/30 hover:bg-purple-600/50 text-purple-200 text-sm transition-colors cursor-pointer border border-purple-500/30"
        title={`View document: ${block.documentRef.title}`}
      >
        <svg
          className="w-3.5 h-3.5"
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth={2}
            d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z"
          />
        </svg>
        <span className="max-w-[200px] truncate">{block.documentRef.title}</span>
      </button>
    );
  }

  return null;
}
