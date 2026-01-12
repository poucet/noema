import { AudioPlayer } from "../AudioPlayer";
import { AssetImage } from "./AssetImage";

interface AssetBlockProps {
  url: string;
  mimeType: string;
  filename?: string | null;
}

export function AssetBlock({ url, mimeType, filename }: AssetBlockProps) {
  if (mimeType.startsWith("audio/")) {
    return <AudioPlayer src={url} mimeType={mimeType} />;
  }

  // Default to image for image/* mime types
  return <AssetImage src={url} alt={filename || "Image"} />;
}
