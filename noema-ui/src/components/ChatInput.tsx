import { useState, useRef, useEffect, useCallback } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { readFile } from "@tauri-apps/plugin-fs";
import { AttachmentPreview } from "./AttachmentPreview";
import { isSupportedAttachmentType, type Attachment } from "../types";

export type VoiceStatus = "disabled" | "enabled" | "listening" | "transcribing" | "buffering";

interface ChatInputProps {
  onSend: (message: string, attachments: Attachment[]) => void;
  disabled?: boolean;
  voiceAvailable?: boolean;
  voiceStatus?: VoiceStatus;
  voiceBufferedCount?: number;
  onToggleVoice?: () => void;
}

// Get MIME type from file extension
function getMimeType(filePath: string): string | null {
  const ext = filePath.split(".").pop()?.toLowerCase();
  const mimeTypes: Record<string, string> = {
    // Images
    png: "image/png",
    jpg: "image/jpeg",
    jpeg: "image/jpeg",
    gif: "image/gif",
    webp: "image/webp",
    // Audio
    mp3: "audio/mpeg",
    m4a: "audio/mp4",
    wav: "audio/wav",
    webm: "audio/webm",
    ogg: "audio/ogg",
    // Text
    txt: "text/plain",
    md: "text/markdown",
    markdown: "text/markdown",
    // Documents
    pdf: "application/pdf",
  };
  return ext ? mimeTypes[ext] || null : null;
}

async function fileToAttachment(file: File): Promise<Attachment | null> {
  if (!isSupportedAttachmentType(file.type)) {
    return null;
  }

  return new Promise((resolve) => {
    const reader = new FileReader();
    reader.onload = () => {
      const result = reader.result as string;
      // Remove the data URL prefix (e.g., "data:image/png;base64,")
      const base64 = result.split(",")[1];
      resolve({
        data: base64,
        mimeType: file.type,
      });
    };
    reader.onerror = () => resolve(null);
    reader.readAsDataURL(file);
  });
}

// Convert file path to attachment using Tauri's fs plugin
async function filePathToAttachment(filePath: string): Promise<Attachment | null> {
  const mimeType = getMimeType(filePath);
  if (!mimeType || !isSupportedAttachmentType(mimeType)) {
    console.log("Unsupported file type:", filePath, mimeType);
    return null;
  }

  try {
    const contents = await readFile(filePath);
    // Convert Uint8Array to base64
    const base64 = btoa(
      Array.from(contents)
        .map((byte) => String.fromCharCode(byte))
        .join("")
    );
    return { data: base64, mimeType };
  } catch (err) {
    console.error("Failed to read file:", filePath, err);
    return null;
  }
}

export function ChatInput({
  onSend,
  disabled = false,
  voiceAvailable = false,
  voiceStatus = "disabled",
  voiceBufferedCount = 0,
  onToggleVoice,
}: ChatInputProps) {
  const [message, setMessage] = useState("");
  const [attachments, setAttachments] = useState<Attachment[]>([]);
  const [isDragOver, setIsDragOver] = useState(false);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);

  // Auto-resize textarea
  useEffect(() => {
    const textarea = textareaRef.current;
    if (textarea) {
      textarea.style.height = "auto";
      textarea.style.height = `${Math.min(textarea.scrollHeight, 200)}px`;
    }
  }, [message]);

  // Set up Tauri drag-drop event listener
  // Track last processed drop to avoid duplicates (Tauri bug: https://github.com/tauri-apps/tauri/issues/14134)
  const lastDropRef = useRef<{ paths: string[]; time: number } | null>(null);

  useEffect(() => {
    let unlisten: (() => void) | undefined;

    const setupDragDrop = async () => {
      try {
        const webview = getCurrentWebview();
        unlisten = await webview.onDragDropEvent(async (event) => {
          if (event.payload.type === "enter" || event.payload.type === "over") {
            setIsDragOver(true);
          } else if (event.payload.type === "leave") {
            setIsDragOver(false);
          } else if (event.payload.type === "drop") {
            setIsDragOver(false);
            const paths = event.payload.paths;

            // Deduplicate: skip if same paths were dropped within 500ms
            const now = Date.now();
            const lastDrop = lastDropRef.current;
            if (
              lastDrop &&
              now - lastDrop.time < 500 &&
              lastDrop.paths.length === paths.length &&
              lastDrop.paths.every((p, i) => p === paths[i])
            ) {
              console.log("Skipping duplicate drop event");
              return;
            }
            lastDropRef.current = { paths, time: now };

            console.log("Dropped files:", paths);

            const newAttachments: Attachment[] = [];
            for (const filePath of paths) {
              const attachment = await filePathToAttachment(filePath);
              if (attachment) {
                newAttachments.push(attachment);
              }
            }

            if (newAttachments.length > 0) {
              setAttachments((prev) => [...prev, ...newAttachments]);
            }
          }
        });
      } catch (err) {
        console.error("Failed to set up drag-drop listener:", err);
      }
    };

    setupDragDrop();

    return () => {
      if (unlisten) {
        unlisten();
      }
    };
  }, []);

  const handleSubmit = () => {
    const trimmed = message.trim();
    if ((trimmed || attachments.length > 0) && !disabled) {
      onSend(trimmed, attachments);
      setMessage("");
      setAttachments([]);
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSubmit();
    }
  };

  const handleRemoveAttachment = (index: number) => {
    setAttachments((prev) => prev.filter((_, i) => i !== index));
  };

  const processFiles = useCallback(async (files: FileList | File[]) => {
    const fileArray = Array.from(files);
    const newAttachments: Attachment[] = [];

    for (const file of fileArray) {
      const attachment = await fileToAttachment(file);
      if (attachment) {
        newAttachments.push(attachment);
      }
    }

    if (newAttachments.length > 0) {
      setAttachments((prev) => [...prev, ...newAttachments]);
    }
  }, []);

  const handleDragEnter = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    console.log("dragEnter", e.dataTransfer.types);
    // Check if files are being dragged
    if (e.dataTransfer.types.includes("Files")) {
      setIsDragOver(true);
    }
  }, []);

  const handleDragOver = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    // Check if files are being dragged
    if (e.dataTransfer.types.includes("Files")) {
      e.dataTransfer.dropEffect = "copy";
      setIsDragOver(true);
    }
  }, []);

  const handleDragLeave = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    // Only set isDragOver to false if we're leaving the container entirely
    const relatedTarget = e.relatedTarget as Node | null;
    if (!relatedTarget || !containerRef.current?.contains(relatedTarget)) {
      setIsDragOver(false);
    }
  }, []);

  const handleDrop = useCallback(
    async (e: React.DragEvent) => {
      e.preventDefault();
      e.stopPropagation();
      setIsDragOver(false);

      console.log("drop", e.dataTransfer.files.length, "files");
      const files = e.dataTransfer.files;
      if (files.length > 0) {
        for (let i = 0; i < files.length; i++) {
          console.log("file", i, files[i].name, files[i].type);
        }
        await processFiles(files);
      }
    },
    [processFiles]
  );

  // Handle paste events for images
  const handlePaste = useCallback(
    async (e: React.ClipboardEvent) => {
      const items = e.clipboardData.items;
      const files: File[] = [];

      for (const item of items) {
        if (item.kind === "file") {
          const file = item.getAsFile();
          if (file && isSupportedAttachmentType(file.type)) {
            files.push(file);
          }
        }
      }

      if (files.length > 0) {
        e.preventDefault();
        await processFiles(files);
      }
    },
    [processFiles]
  );

  const getVoiceButtonClass = () => {
    const base = "px-4 py-3 rounded-2xl transition-colors";
    switch (voiceStatus) {
      case "listening":
        return `${base} bg-red-500 hover:bg-red-600 text-white animate-pulse`;
      case "transcribing":
        return `${base} bg-amber-500 hover:bg-amber-600 text-white`;
      case "buffering":
        return `${base} bg-purple-500 hover:bg-purple-600 text-white`;
      case "enabled":
        return `${base} bg-teal-500 hover:bg-teal-600 text-white`;
      default:
        return `${base} bg-surface hover:bg-elevated text-muted`;
    }
  };

  return (
    <div
      ref={containerRef}
      className={`relative border-t border-gray-700 bg-background ${
        isDragOver ? "ring-2 ring-teal-500 ring-inset" : ""
      }`}
      onDragEnter={handleDragEnter}
      onDragOver={handleDragOver}
      onDragLeave={handleDragLeave}
      onDrop={handleDrop}
    >
      {/* Drag overlay */}
      {isDragOver && (
        <div className="absolute inset-0 bg-teal-500/20 flex items-center justify-center pointer-events-none z-10 rounded-lg border-2 border-dashed border-teal-500">
          <div className="bg-teal-600 text-white px-4 py-2 rounded-lg shadow-lg">
            Drop files to attach
          </div>
        </div>
      )}

      {/* Attachment preview */}
      <AttachmentPreview attachments={attachments} onRemove={handleRemoveAttachment} />

      {/* Input area */}
      <div className="p-4">
        <div className="flex gap-3 items-end max-w-4xl mx-auto">
          <button
            type="button"
            onClick={onToggleVoice}
            disabled={disabled || !voiceAvailable}
            className={getVoiceButtonClass()}
            title={
              !voiceAvailable
                ? "Voice input not available"
                : voiceStatus === "disabled"
                ? "Enable voice input"
                : voiceStatus === "listening"
                ? "Listening..."
                : voiceStatus === "transcribing"
                ? "Transcribing..."
                : voiceStatus === "buffering"
                ? `${voiceBufferedCount} message${voiceBufferedCount !== 1 ? 's' : ''} queued`
                : "Voice enabled (click to disable)"
            }
          >
            <svg
              className="w-5 h-5"
              fill="none"
              stroke="currentColor"
              viewBox="0 0 24 24"
            >
              {voiceStatus === "disabled" || !voiceAvailable ? (
                // Microphone off icon
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth={2}
                  d="M19 11a7 7 0 01-7 7m0 0a7 7 0 01-7-7m7 7v4m0 0H8m4 0h4m-4-8a3 3 0 01-3-3V5a3 3 0 116 0v6a3 3 0 01-3 3z"
                />
              ) : (
                // Microphone on icon
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth={2}
                  d="M19 11a7 7 0 01-7 7m0 0a7 7 0 01-7-7m7 7v4m0 0H8m4 0h4m-4-8a3 3 0 01-3-3V5a3 3 0 116 0v6a3 3 0 01-3 3z"
                />
              )}
            </svg>
          </button>
          <textarea
            ref={textareaRef}
            value={message}
            onChange={(e) => setMessage(e.target.value)}
            onKeyDown={handleKeyDown}
            onPaste={handlePaste}
            placeholder={
              voiceStatus === "listening"
                ? "Listening... speak now"
                : voiceStatus === "transcribing"
                ? "Transcribing..."
                : voiceStatus === "buffering"
                ? `${voiceBufferedCount} message${voiceBufferedCount !== 1 ? 's' : ''} queued while thinking...`
                : attachments.length > 0
                ? "Add a message or send attachments..."
                : "Type a message or drop files..."
            }
            disabled={disabled}
            rows={1}
            className="flex-1 px-4 py-3 border border-gray-600 rounded-2xl resize-none focus:outline-none focus:ring-2 focus:ring-teal-500 focus:border-transparent bg-surface text-foreground placeholder-muted disabled:opacity-50 overflow-hidden"
          />
          <button
            type="button"
            onClick={handleSubmit}
            disabled={disabled || (!message.trim() && attachments.length === 0)}
            className="px-4 py-3 bg-teal-600 hover:bg-teal-700 disabled:bg-gray-600 disabled:cursor-not-allowed text-white rounded-2xl transition-colors"
          >
            <svg
              className="w-5 h-5"
              fill="none"
              stroke="currentColor"
              viewBox="0 0 24 24"
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={2}
                d="M12 19l9 2-9-18-9 18 9-2zm0 0v-8"
              />
            </svg>
          </button>
        </div>
      </div>
    </div>
  );
}
