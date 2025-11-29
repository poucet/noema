import { useCallback } from "react";

interface ImageViewerProps {
  data: string; // base64 encoded image data
  mimeType: string;
  alt?: string;
  className?: string;
}

export function ImageViewer({ data, mimeType, alt = "Image", className = "" }: ImageViewerProps) {
  const dataUrl = `data:${mimeType};base64,${data}`;

  const handleDownload = useCallback(() => {
    // Create a temporary link and trigger download
    const link = document.createElement("a");
    link.href = dataUrl;
    // Generate filename from mime type
    const extension = mimeType.split("/")[1] || "bin";
    const timestamp = new Date().toISOString().replace(/[:.]/g, "-");
    link.download = `image-${timestamp}.${extension}`;
    document.body.appendChild(link);
    link.click();
    document.body.removeChild(link);
  }, [dataUrl, mimeType]);

  return (
    <div className="relative group">
      <img
        src={dataUrl}
        alt={alt}
        className={`max-w-full rounded-lg ${className}`}
      />
      {/* Download overlay on hover */}
      <div className="absolute top-2 right-2 opacity-0 group-hover:opacity-100 transition-opacity">
        <button
          onClick={handleDownload}
          className="p-2 bg-black/50 hover:bg-black/70 rounded-lg text-white transition-colors"
          title="Download image"
        >
          <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              strokeWidth={2}
              d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4"
            />
          </svg>
        </button>
      </div>
      {/* Right-click hint */}
      <div className="absolute bottom-2 right-2 opacity-0 group-hover:opacity-100 transition-opacity">
        <span className="text-xs text-white bg-black/50 px-2 py-1 rounded">
          Right-click to save
        </span>
      </div>
    </div>
  );
}
