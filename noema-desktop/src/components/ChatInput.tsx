import { useState, useRef, useEffect, useCallback } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { readFile } from "@tauri-apps/plugin-fs";
import { AttachmentPreview } from "./AttachmentPreview";
import { isSupportedAttachmentType} from "../mime_types";
import type { Attachment, DocumentInfoResponse, InputContentBlock } from "../generated";
import * as tauri from "../tauri";

export type VoiceStatus = "disabled" | "enabled" | "listening" | "transcribing" | "buffering";

interface MentionState {
  isActive: boolean;
  query: string;
  startPosition: number;
  selectedIndex: number;
}

// Referenced document for chip display
interface ReferencedDocument {
  id: string;
  title: string;
}

interface ChatInputProps {
  onSend: (content: InputContentBlock[]) => void;
  disabled?: boolean;
  voiceAvailable?: boolean;
  voiceStatus?: VoiceStatus;
  voiceBufferedCount?: number;
  onToggleVoice?: () => void;
  // Fork mode: when set, shows indicator and prefills input
  pendingFork?: boolean;
  prefilledText?: string;
  onCancelFork?: () => void;
}

// Parse message text with embedded [title](noema://doc/id) links into content blocks
function parseMessageToContentBlocks(text: string): InputContentBlock[] {
  const blocks: InputContentBlock[] = [];
  // Match markdown links with noema://doc/ URLs
  const docLinkRegex = /\[([^\]]+)\]\(noema:\/\/doc\/([^)]+)\)/g;

  let lastIndex = 0;
  let match;

  while ((match = docLinkRegex.exec(text)) !== null) {
    // Add text before this match
    if (match.index > lastIndex) {
      const textBefore = text.slice(lastIndex, match.index);
      if (textBefore) {
        blocks.push({ type: "text", text: textBefore });
      }
    }

    // Add document reference
    const title = match[1];
    const id = match[2];
    blocks.push({ type: "documentRef", id, title });

    lastIndex = match.index + match[0].length;
  }

  // Add remaining text after last match
  if (lastIndex < text.length) {
    const textAfter = text.slice(lastIndex);
    if (textAfter) {
      blocks.push({ type: "text", text: textAfter });
    }
  }

  return blocks;
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
  pendingFork = false,
  prefilledText = "",
  onCancelFork,
}: ChatInputProps) {
  const [message, setMessage] = useState("");
  const [attachments, setAttachments] = useState<Attachment[]>([]);
  const [isDragOver, setIsDragOver] = useState(false);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);

  // When prefilledText changes (fork from user message), update the input
  useEffect(() => {
    if (prefilledText) {
      setMessage(prefilledText);
      // Focus the textarea
      setTimeout(() => {
        textareaRef.current?.focus();
      }, 0);
    }
  }, [prefilledText]);

  // @ mention autocomplete state
  const [mentionState, setMentionState] = useState<MentionState>({
    isActive: false,
    query: "",
    startPosition: 0,
    selectedIndex: 0,
  });
  const [mentionResults, setMentionResults] = useState<DocumentInfoResponse[]>([]);
  const mentionDebounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  // Track documents referenced via @ mentions for RAG
  const [referencedDocs, setReferencedDocs] = useState<ReferencedDocument[]>([]);

  // Auto-resize textarea
  useEffect(() => {
    const textarea = textareaRef.current;
    if (textarea) {
      textarea.style.height = "auto";
      textarea.style.height = `${Math.min(textarea.scrollHeight, 200)}px`;
    }
  }, [message]);

  // Search for documents when mention query changes
  useEffect(() => {
    if (!mentionState.isActive) {
      setMentionResults([]);
      return;
    }

    // Debounce the search (shorter delay for empty query to feel more responsive)
    if (mentionDebounceRef.current) {
      clearTimeout(mentionDebounceRef.current);
    }

    const delay = mentionState.query.length === 0 ? 50 : 150;
    mentionDebounceRef.current = setTimeout(async () => {
      try {
        const results = await tauri.searchDocuments(mentionState.query, 5);
        setMentionResults(results);
        setMentionState(prev => ({ ...prev, selectedIndex: 0 }));
      } catch (err) {
        console.error("Failed to search documents:", err);
        setMentionResults([]);
      }
    }, delay);

    return () => {
      if (mentionDebounceRef.current) {
        clearTimeout(mentionDebounceRef.current);
      }
    };
  }, [mentionState.isActive, mentionState.query]);

  // Handle message changes to detect @ mentions
  const handleMessageChange = useCallback((e: React.ChangeEvent<HTMLTextAreaElement>) => {
    const newValue = e.target.value;
    const cursorPos = e.target.selectionStart;
    setMessage(newValue);

    // Check if we should activate or update mention mode
    const textBeforeCursor = newValue.substring(0, cursorPos);
    const atIndex = textBeforeCursor.lastIndexOf("@");

    if (atIndex >= 0) {
      // Check if @ is at start or preceded by whitespace
      const charBefore = atIndex > 0 ? textBeforeCursor[atIndex - 1] : " ";
      if (charBefore === " " || charBefore === "\n" || atIndex === 0) {
        const query = textBeforeCursor.substring(atIndex + 1);
        // Only activate if query doesn't contain whitespace (still typing the mention)
        if (!query.includes(" ") && !query.includes("\n")) {
          setMentionState({
            isActive: true,
            query,
            startPosition: atIndex,
            selectedIndex: 0,
          });
          return;
        }
      }
    }

    // Deactivate mention mode if conditions not met
    if (mentionState.isActive) {
      setMentionState(prev => ({ ...prev, isActive: false }));
    }
  }, [mentionState.isActive]);

  // Insert a document mention as a markdown link inline in the text
  const insertMention = useCallback((doc: DocumentInfoResponse) => {
    // Replace the @query with a markdown link
    const beforeMention = message.substring(0, mentionState.startPosition);
    const afterMention = message.substring(
      mentionState.startPosition + mentionState.query.length + 1
    );
    // Insert markdown link: [title](noema://doc/id)
    const docLink = `[${doc.title}](noema://doc/${doc.id})`;
    const newMessage = beforeMention + docLink + afterMention;
    const newCursorPos = beforeMention.length + docLink.length;
    setMessage(newMessage);
    setMentionState({ isActive: false, query: "", startPosition: 0, selectedIndex: 0 });
    setMentionResults([]);

    // Also track in referencedDocs for the chip display (backwards compat + visual indicator)
    setReferencedDocs(prev => {
      if (prev.some(d => d.id === doc.id)) {
        return prev;
      }
      return [...prev, { id: doc.id, title: doc.title }];
    });

    // Focus and position cursor after the inserted link
    setTimeout(() => {
      if (textareaRef.current) {
        textareaRef.current.focus();
        textareaRef.current.setSelectionRange(newCursorPos, newCursorPos);
      }
    }, 0);
  }, [message, mentionState.startPosition, mentionState.query]);

  // Remove a referenced document
  const removeReferencedDoc = useCallback((docId: string) => {
    setReferencedDocs(prev => prev.filter(d => d.id !== docId));
  }, []);

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
      // Build content blocks array preserving inline positions
      const contentBlocks: InputContentBlock[] = [];

      // Parse message text to extract inline document references
      if (trimmed) {
        const textBlocks = parseMessageToContentBlocks(trimmed);
        contentBlocks.push(...textBlocks);
      }

      // Add attachments as image/audio blocks
      for (const attachment of attachments) {
        if (attachment.mimeType.startsWith("image/")) {
          contentBlocks.push({ type: "image", data: attachment.data, mimeType: attachment.mimeType });
        } else if (attachment.mimeType.startsWith("audio/")) {
          contentBlocks.push({ type: "audio", data: attachment.data, mimeType: attachment.mimeType });
        }
      }

      if (contentBlocks.length > 0) {
        onSend(contentBlocks);
        setMessage("");
        setAttachments([]);
        setReferencedDocs([]);
      }
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    // Handle mention navigation
    if (mentionState.isActive && mentionResults.length > 0) {
      if (e.key === "ArrowDown") {
        e.preventDefault();
        setMentionState(prev => ({
          ...prev,
          selectedIndex: Math.min(prev.selectedIndex + 1, mentionResults.length - 1),
        }));
        return;
      }
      if (e.key === "ArrowUp") {
        e.preventDefault();
        setMentionState(prev => ({
          ...prev,
          selectedIndex: Math.max(prev.selectedIndex - 1, 0),
        }));
        return;
      }
      if (e.key === "Enter" || e.key === "Tab") {
        e.preventDefault();
        insertMention(mentionResults[mentionState.selectedIndex]);
        return;
      }
      if (e.key === "Escape") {
        e.preventDefault();
        setMentionState({ isActive: false, query: "", startPosition: 0, selectedIndex: 0 });
        return;
      }
    }

    // Normal submit on Enter
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

      {/* Referenced documents chips */}
      {referencedDocs.length > 0 && (
        <div className="px-4 pt-2 max-w-4xl mx-auto">
          <div className="flex flex-wrap gap-2">
            {referencedDocs.map(doc => (
              <span
                key={doc.id}
                className="inline-flex items-center gap-1 px-2 py-1 bg-teal-900/50 text-teal-300 rounded-full text-sm"
              >
                <svg className="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
                </svg>
                {doc.title}
                <button
                  onClick={() => removeReferencedDoc(doc.id)}
                  className="ml-1 hover:text-red-400"
                >
                  <svg className="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                  </svg>
                </button>
              </span>
            ))}
          </div>
        </div>
      )}

      {/* Input area */}
      <div className="p-4 relative">
        {/* Mention autocomplete dropdown */}
        {mentionState.isActive && mentionResults.length > 0 && (
          <div className="absolute bottom-full left-4 right-4 mb-2 max-w-4xl mx-auto">
            <div className="bg-elevated border border-gray-600 rounded-lg shadow-lg overflow-hidden">
              <div className="text-xs text-muted px-3 py-2 border-b border-gray-700">
                Documents
              </div>
              <ul className="max-h-48 overflow-y-auto">
                {mentionResults.map((doc, index) => (
                  <li key={doc.id}>
                    <button
                      type="button"
                      onClick={() => insertMention(doc)}
                      className={`w-full text-left px-3 py-2 flex items-center gap-2 ${
                        index === mentionState.selectedIndex
                          ? "bg-teal-600/30 text-teal-100"
                          : "hover:bg-gray-700/50 text-foreground"
                      }`}
                    >
                      <svg className="w-4 h-4 text-teal-400 shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
                      </svg>
                      <span className="truncate">{doc.title}</span>
                    </button>
                  </li>
                ))}
              </ul>
            </div>
          </div>
        )}

        <div className="flex gap-3 items-end max-w-4xl mx-auto">
          {/* Cancel fork button */}
          {pendingFork && onCancelFork && (
            <button
              type="button"
              onClick={onCancelFork}
              className="px-4 py-3 bg-gray-600 hover:bg-gray-500 text-white rounded-2xl transition-colors"
              title="Cancel fork"
            >
              <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
              </svg>
            </button>
          )}
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
            onChange={handleMessageChange}
            onKeyDown={handleKeyDown}
            onPaste={handlePaste}
            placeholder={
              voiceStatus === "listening"
                ? "Listening... speak now"
                : voiceStatus === "transcribing"
                ? "Transcribing..."
                : voiceStatus === "buffering"
                ? `${voiceBufferedCount} message${voiceBufferedCount !== 1 ? 's' : ''} queued while thinking...`
                : attachments.length > 0 || referencedDocs.length > 0
                ? "Add a message..."
                : "Type a message, @ to reference docs..."
            }
            disabled={disabled}
            rows={1}
            className="flex-1 px-4 py-3 border border-gray-600 rounded-2xl resize-none focus:outline-none focus:ring-2 focus:ring-teal-500 focus:border-transparent bg-surface text-foreground placeholder-muted disabled:opacity-50 overflow-hidden"
          />
          <button
            type="button"
            onClick={handleSubmit}
            disabled={disabled || (!message.trim() && attachments.length === 0 && referencedDocs.length === 0)}
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
