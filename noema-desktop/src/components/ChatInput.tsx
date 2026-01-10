import { useState, useRef, useEffect, useCallback } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { readFile } from "@tauri-apps/plugin-fs";
import { AttachmentPreview } from "./AttachmentPreview";
import { isSupportedAttachmentType } from "../mime_types";
import type { Attachment, DocumentInfoResponse, InputContentBlock, ToolConfig } from "../generated";
import * as tauri from "../tauri";

export type VoiceStatus = "disabled" | "enabled" | "listening" | "transcribing" | "buffering";

interface MentionState {
  isActive: boolean;
  query: string;
  startPosition: number;
  selectedIndex: number;
}

// Subset of InputContentBlock that can appear in the editor (text and documentRef)
type EditorBlock = Extract<InputContentBlock, { type: "text" } | { type: "documentRef" }>;

interface ChatInputProps {
  onSend: (content: InputContentBlock[], toolConfig?: ToolConfig) => void;
  disabled?: boolean;
  voiceAvailable?: boolean;
  voiceStatus?: VoiceStatus;
  voiceBufferedCount?: number;
  onToggleVoice?: () => void;
  pendingFork?: boolean;
  prefilledText?: string;
  onCancelFork?: () => void;
  /** Whether tools are enabled (controlled by parent) */
  toolsEnabled?: boolean;
  /** Callback when tools toggle is clicked */
  onToggleTools?: () => void;
}

// Get MIME type from file extension
function getMimeType(filePath: string): string | null {
  const ext = filePath.split(".").pop()?.toLowerCase();
  const mimeTypes: Record<string, string> = {
    png: "image/png",
    jpg: "image/jpeg",
    jpeg: "image/jpeg",
    gif: "image/gif",
    webp: "image/webp",
    mp3: "audio/mpeg",
    m4a: "audio/mp4",
    wav: "audio/wav",
    webm: "audio/webm",
    ogg: "audio/ogg",
    txt: "text/plain",
    md: "text/markdown",
    markdown: "text/markdown",
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

async function filePathToAttachment(filePath: string): Promise<Attachment | null> {
  const mimeType = getMimeType(filePath);
  if (!mimeType || !isSupportedAttachmentType(mimeType)) {
    console.log("Unsupported file type:", filePath, mimeType);
    return null;
  }

  try {
    const contents = await readFile(filePath);
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

// Check if blocks have any content
function hasContent(blocks: EditorBlock[]): boolean {
  return blocks.some((b) => {
    if (b.type === "text") return b.text.trim().length > 0;
    return true; // documentRef always counts as content
  });
}

// Get referenced documents from blocks
function getReferencedDocs(blocks: EditorBlock[]): { id: string; title: string }[] {
  return blocks.filter((b): b is EditorBlock & { type: "documentRef" } => b.type === "documentRef");
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
  toolsEnabled = true,
  onToggleTools,
}: ChatInputProps) {
  // Store content as structured blocks instead of a string
  const [blocks, setBlocks] = useState<EditorBlock[]>([{ type: "text", text: "" }]);
  const [attachments, setAttachments] = useState<Attachment[]>([]);
  const [isDragOver, setIsDragOver] = useState(false);
  const editorRef = useRef<HTMLDivElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);

  // When prefilledText changes (fork from user message), update the input
  useEffect(() => {
    if (prefilledText) {
      needsDomSyncRef.current = true;
      setBlocks([{ type: "text", text: prefilledText }]);
      setTimeout(() => {
        editorRef.current?.focus();
        // Move cursor to end
        const selection = window.getSelection();
        if (selection && editorRef.current) {
          selection.selectAllChildren(editorRef.current);
          selection.collapseToEnd();
        }
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

  // Track which text block and position within it the cursor is at
  const [cursorPosition, setCursorPosition] = useState<{ blockIndex: number; offset: number }>({
    blockIndex: 0,
    offset: 0,
  });

  // Flag to indicate when we need to force a DOM rebuild (e.g., after chip insert/remove)
  const needsDomSyncRef = useRef(true);

  // Search for documents when mention query changes
  useEffect(() => {
    if (!mentionState.isActive) {
      setMentionResults([]);
      return;
    }

    if (mentionDebounceRef.current) {
      clearTimeout(mentionDebounceRef.current);
    }

    const delay = mentionState.query.length === 0 ? 50 : 150;
    mentionDebounceRef.current = setTimeout(async () => {
      try {
        const results = await tauri.searchDocuments(mentionState.query, 5);
        setMentionResults(results);
        setMentionState((prev) => ({ ...prev, selectedIndex: 0 }));
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

  // Sync the DOM with our blocks state - only when explicitly needed
  useEffect(() => {
    if (!needsDomSyncRef.current) return;
    needsDomSyncRef.current = false;

    const editor = editorRef.current;
    if (!editor) return;

    // Build the expected DOM structure
    const fragment = document.createDocumentFragment();

    blocks.forEach((block, index) => {
      if (block.type === "text") {
        // Create a text span
        const span = document.createElement("span");
        span.setAttribute("data-block-type", "text");
        span.setAttribute("data-block-index", String(index));
        span.textContent = block.text || "\u200B"; // Zero-width space for empty text
        fragment.appendChild(span);
      } else {
        // Create a chip for document reference
        const chip = document.createElement("span");
        chip.setAttribute("data-block-type", "documentRef");
        chip.setAttribute("data-block-index", String(index));
        chip.setAttribute("data-doc-id", block.id);
        chip.contentEditable = "false";
        chip.className =
          "inline-flex items-center gap-1 px-2 py-0.5 mx-0.5 bg-teal-900/50 text-teal-300 rounded-full text-sm align-middle select-none";
        chip.innerHTML = `
          <svg class="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
          </svg>
          <span>${block.title}</span>
        `;
        fragment.appendChild(chip);
      }
    });

    editor.innerHTML = "";
    editor.appendChild(fragment);
  }, [blocks]);

  // Handle input in the contenteditable
  const handleInput = useCallback(() => {
    const editor = editorRef.current;
    if (!editor) return;

    // Parse the DOM back into blocks
    const newBlocks: EditorBlock[] = [];
    const children = Array.from(editor.childNodes);

    for (const child of children) {
      if (child.nodeType === Node.TEXT_NODE) {
        // Plain text node (shouldn't happen often with our structure, but handle it)
        const text = child.textContent || "";
        if (text && text !== "\u200B") {
          newBlocks.push({ type: "text", text });
        }
      } else if (child.nodeType === Node.ELEMENT_NODE) {
        const el = child as HTMLElement;
        const blockType = el.getAttribute("data-block-type");

        if (blockType === "text") {
          let text = el.textContent || "";
          // Remove zero-width spaces
          text = text.replace(/\u200B/g, "");
          newBlocks.push({ type: "text", text });
        } else if (blockType === "documentRef") {
          const docId = el.getAttribute("data-doc-id");
          const title = el.querySelector("span")?.textContent || "";
          if (docId) {
            newBlocks.push({ type: "documentRef", id: docId, title });
          }
        }
      }
    }

    // Ensure we always have at least one text block
    if (newBlocks.length === 0) {
      newBlocks.push({ type: "text", text: "" });
    }

    // Merge adjacent text blocks
    const mergedBlocks: EditorBlock[] = [];
    for (const block of newBlocks) {
      const last = mergedBlocks[mergedBlocks.length - 1];
      if (block.type === "text" && last?.type === "text") {
        last.text += block.text;
      } else {
        mergedBlocks.push(block);
      }
    }

    setBlocks(mergedBlocks);

    // Check for @ mention trigger
    const selection = window.getSelection();
    if (selection && selection.rangeCount > 0) {
      const range = selection.getRangeAt(0);
      const container = range.startContainer;

      if (container.nodeType === Node.TEXT_NODE) {
        const text = container.textContent || "";
        const offset = range.startOffset;
        const textBeforeCursor = text.substring(0, offset);
        const atIndex = textBeforeCursor.lastIndexOf("@");

        if (atIndex >= 0) {
          const charBefore = atIndex > 0 ? textBeforeCursor[atIndex - 1] : " ";
          if (charBefore === " " || charBefore === "\n" || atIndex === 0) {
            const query = textBeforeCursor.substring(atIndex + 1);
            if (!query.includes(" ") && !query.includes("\n")) {
              // Find which block this text node belongs to
              let blockIndex = 0;
              const parent = container.parentElement;
              if (parent) {
                const idx = parent.getAttribute("data-block-index");
                if (idx !== null) blockIndex = parseInt(idx, 10);
              }

              setMentionState({
                isActive: true,
                query,
                startPosition: atIndex,
                selectedIndex: 0,
              });
              setCursorPosition({ blockIndex, offset });
              return;
            }
          }
        }
      }

      // Deactivate mention if conditions not met
      if (mentionState.isActive) {
        setMentionState((prev) => ({ ...prev, isActive: false }));
      }
    }
  }, [mentionState.isActive]);

  // Insert a document reference at the current position
  const insertMention = useCallback(
    (doc: DocumentInfoResponse) => {
      const { blockIndex, offset } = cursorPosition;

      needsDomSyncRef.current = true;
      setBlocks((prevBlocks) => {
        const newBlocks: EditorBlock[] = [];

        for (let i = 0; i < prevBlocks.length; i++) {
          const block = prevBlocks[i];

          if (i === blockIndex && block.type === "text") {
            // Split this text block at the @ position
            const textBeforeAt = block.text.substring(0, mentionState.startPosition);
            const textAfterQuery = block.text.substring(offset);

            // Add text before @
            if (textBeforeAt) {
              newBlocks.push({ type: "text", text: textBeforeAt });
            }

            // Add the document reference
            newBlocks.push({ type: "documentRef", id: doc.id, title: doc.title });

            // Add text after the query (with a space for comfortable typing)
            newBlocks.push({ type: "text", text: " " + textAfterQuery });
          } else {
            newBlocks.push(block);
          }
        }

        return newBlocks;
      });

      setMentionState({ isActive: false, query: "", startPosition: 0, selectedIndex: 0 });
      setMentionResults([]);

      // Focus and position cursor after the chip
      setTimeout(() => {
        const editor = editorRef.current;
        if (editor) {
          editor.focus();
          // Find the text span after the chip and position cursor at start
          const textSpans = editor.querySelectorAll('[data-block-type="text"]');
          const lastTextSpan = textSpans[textSpans.length - 1];
          if (lastTextSpan && lastTextSpan.firstChild) {
            const selection = window.getSelection();
            const range = document.createRange();
            range.setStart(lastTextSpan.firstChild, 1); // After the space
            range.collapse(true);
            selection?.removeAllRanges();
            selection?.addRange(range);
          }
        }
      }, 0);
    },
    [cursorPosition, mentionState.startPosition]
  );

  // Remove a document reference by id
  const removeDocRef = useCallback((docId: string) => {
    needsDomSyncRef.current = true;
    setBlocks((prevBlocks) => {
      const newBlocks = prevBlocks.filter(
        (b) => !(b.type === "documentRef" && b.id === docId)
      );
      // Ensure at least one text block
      if (newBlocks.length === 0 || !newBlocks.some((b) => b.type === "text")) {
        newBlocks.push({ type: "text", text: "" });
      }
      // Merge adjacent text blocks
      const merged: EditorBlock[] = [];
      for (const block of newBlocks) {
        const last = merged[merged.length - 1];
        if (block.type === "text" && last?.type === "text") {
          last.text += block.text;
        } else {
          merged.push(block);
        }
      }
      return merged;
    });
  }, []);

  // Set up Tauri drag-drop event listener
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

  const handleSubmit = useCallback(() => {
    if (!hasContent(blocks) && attachments.length === 0) return;
    if (disabled) return;

    // Build content blocks: filter empty text blocks, trim text, and add attachments
    const contentBlocks: InputContentBlock[] = blocks
      .map((block) =>
        block.type === "text" ? { ...block, text: block.text.trim() } : block
      )
      .filter((block) => block.type !== "text" || block.text.length > 0);

    // Add attachments as image/audio blocks
    for (const attachment of attachments) {
      if (attachment.mimeType.startsWith("image/")) {
        contentBlocks.push({ type: "image", data: attachment.data, mimeType: attachment.mimeType });
      } else if (attachment.mimeType.startsWith("audio/")) {
        contentBlocks.push({ type: "audio", data: attachment.data, mimeType: attachment.mimeType });
      }
    }

    if (contentBlocks.length > 0) {
      // Build tool config based on current toggle state
      const toolConfig: ToolConfig = { enabled: toolsEnabled, serverIds: null, toolNames: null };
      onSend(contentBlocks, toolConfig);
      needsDomSyncRef.current = true;
      setBlocks([{ type: "text", text: "" }]);
      setAttachments([]);
    }
  }, [blocks, attachments, disabled, onSend, toolsEnabled]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      // Handle mention navigation
      if (mentionState.isActive && mentionResults.length > 0) {
        if (e.key === "ArrowDown") {
          e.preventDefault();
          setMentionState((prev) => ({
            ...prev,
            selectedIndex: Math.min(prev.selectedIndex + 1, mentionResults.length - 1),
          }));
          return;
        }
        if (e.key === "ArrowUp") {
          e.preventDefault();
          setMentionState((prev) => ({
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

      // Handle backspace on chip - check if we're right after a chip
      if (e.key === "Backspace") {
        const selection = window.getSelection();
        if (selection && selection.rangeCount > 0) {
          const range = selection.getRangeAt(0);
          if (range.collapsed) {
            const container = range.startContainer;
            const offset = range.startOffset;

            // If at start of a text node, check previous sibling
            if (offset === 0 || (container.textContent?.charAt(offset - 1) === "\u200B" && offset <= 1)) {
              const parent = container.parentElement;
              const prevSibling = parent?.previousElementSibling;
              if (prevSibling?.getAttribute("data-block-type") === "documentRef") {
                e.preventDefault();
                const docId = prevSibling.getAttribute("data-doc-id");
                if (docId) {
                  removeDocRef(docId);
                }
                return;
              }
            }
          }
        }
      }

      // Submit on Enter (without Shift)
      if (e.key === "Enter" && !e.shiftKey) {
        e.preventDefault();
        handleSubmit();
      }
    },
    [mentionState, mentionResults, insertMention, handleSubmit, removeDocRef]
  );

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
    if (e.dataTransfer.types.includes("Files")) {
      setIsDragOver(true);
    }
  }, []);

  const handleDragOver = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    if (e.dataTransfer.types.includes("Files")) {
      e.dataTransfer.dropEffect = "copy";
      setIsDragOver(true);
    }
  }, []);

  const handleDragLeave = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
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

      const files = e.dataTransfer.files;
      if (files.length > 0) {
        await processFiles(files);
      }
    },
    [processFiles]
  );

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

  const referencedDocs = getReferencedDocs(blocks);
  const isEmpty = !hasContent(blocks);

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
                      <svg
                        className="w-4 h-4 text-teal-400 shrink-0"
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
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth={2}
                  d="M6 18L18 6M6 6l12 12"
                />
              </svg>
            </button>
          )}
          {/* Tools toggle switch */}
          {onToggleTools && (
            <div
              className="flex items-center gap-2 px-2"
              title={toolsEnabled ? "MCP Tools enabled" : "MCP Tools disabled"}
            >
              {/* Wrench icon */}
              <svg className={`w-4 h-4 ${toolsEnabled ? "text-purple-400" : "text-gray-500"}`} fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth={2}
                  d="M14.7 6.3a1 1 0 000 1.4l1.6 1.6a1 1 0 001.4 0l3.77-3.77a6 6 0 01-7.94 7.94l-6.91 6.91a2.12 2.12 0 01-3-3l6.91-6.91a6 6 0 017.94-7.94l-3.76 3.76z"
                />
              </svg>
              {/* Toggle switch */}
              <button
                type="button"
                onClick={onToggleTools}
                disabled={disabled}
                className={`relative w-10 h-5 rounded-full transition-colors ${
                  toolsEnabled ? "bg-purple-600" : "bg-gray-600"
                } ${disabled ? "opacity-50 cursor-not-allowed" : "cursor-pointer"}`}
              >
                <span
                  className={`absolute top-0.5 left-0.5 w-4 h-4 bg-white rounded-full shadow transition-transform ${
                    toolsEnabled ? "translate-x-5" : "translate-x-0"
                  }`}
                />
              </button>
            </div>
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
                        ? `${voiceBufferedCount} message${voiceBufferedCount !== 1 ? "s" : ""} queued`
                        : "Voice enabled (click to disable)"
            }
          >
            <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={2}
                d="M19 11a7 7 0 01-7 7m0 0a7 7 0 01-7-7m7 7v4m0 0H8m4 0h4m-4-8a3 3 0 01-3-3V5a3 3 0 116 0v6a3 3 0 01-3 3z"
              />
            </svg>
          </button>

          {/* Rich text editor with inline chips */}
          <div className="flex-1 relative">
            <div
              ref={editorRef}
              contentEditable={!disabled}
              onInput={handleInput}
              onKeyDown={handleKeyDown}
              onPaste={handlePaste}
              className="min-h-12 max-h-[200px] overflow-y-auto px-4 py-3 border border-gray-600 rounded-2xl focus:outline-none focus:ring-2 focus:ring-teal-500 focus:border-transparent bg-surface text-foreground disabled:opacity-50"
              style={{ whiteSpace: "pre-wrap", wordBreak: "break-word" }}
              suppressContentEditableWarning
            />
            {/* Placeholder */}
            {isEmpty && (
              <div className="absolute left-4 top-3 text-muted pointer-events-none">
                {voiceStatus === "listening"
                  ? "Listening... speak now"
                  : voiceStatus === "transcribing"
                    ? "Transcribing..."
                    : voiceStatus === "buffering"
                      ? `${voiceBufferedCount} message${voiceBufferedCount !== 1 ? "s" : ""} queued while thinking...`
                      : attachments.length > 0 || referencedDocs.length > 0
                        ? "Add a message..."
                        : "Type a message, @ to reference docs..."}
              </div>
            )}
          </div>

          <button
            type="button"
            onClick={handleSubmit}
            disabled={disabled || (isEmpty && attachments.length === 0)}
            className="px-4 py-3 bg-teal-600 hover:bg-teal-700 disabled:bg-gray-600 disabled:cursor-not-allowed text-white rounded-2xl transition-colors"
          >
            <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
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
