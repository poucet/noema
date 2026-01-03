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

  return null;
}
