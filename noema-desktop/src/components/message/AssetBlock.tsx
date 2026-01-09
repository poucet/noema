import { AudioPlayer } from "../AudioPlayer";
import { AssetImage } from "./AssetImage";

interface AssetBlockProps {
  assetId: string;
  mimeType: string;
  filename?: string | null;
}

export function AssetBlock({ assetId, mimeType, filename }: AssetBlockProps) {
  // Load asset from asset protocol - assets are served at noema-asset://localhost/{assetId}
  const assetUrl = `noema-asset://localhost/${assetId}?mime_type=${encodeURIComponent(mimeType)}`;

  if (mimeType.startsWith("audio/")) {
    return <AudioPlayer src={assetUrl} mimeType={mimeType} />;
  }

  // Default to image for image/* mime types
  return <AssetImage src={assetUrl} alt={filename || "Image"} />;
}
