import type { DisplayContent } from "../../generated";
import { AudioPlayer } from "../AudioPlayer";
import { ImageViewer } from "../ImageViewer";
import { MarkdownText } from "./MarkdownText";
import { ToolCallBlock } from "./ToolCallBlock";
import { ToolResultBlock } from "./ToolResultBlock";
import { DocumentRefChip } from "./DocumentRefChip";
import { AssetBlock } from "./AssetBlock";

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
    return (
      <AssetBlock
        url={block.assetRef.url}
        mimeType={block.assetRef.mimeType}
      />
    );
  }

  if ("audio" in block) {
    return <AudioPlayer data={block.audio.data} mimeType={block.audio.mimeType} />;
  }

  if ("toolCall" in block) {
    return <ToolCallBlock name={block.toolCall.name} arguments={block.toolCall.arguments} />;
  }

  if ("toolResult" in block) {
    return <ToolResultBlock content={block.toolResult.content} />;
  }

  if ("documentRef" in block) {
    return (
      <DocumentRefChip
        id={block.documentRef.id}
        onClick={onDocumentClick}
      />
    );
  }

  return null;
}
