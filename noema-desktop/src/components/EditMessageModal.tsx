import { useState, useEffect, useRef } from "react";

interface EditMessageModalProps {
  initialText: string;
  onSubmit: (newText: string) => void;
  onCancel: () => void;
}

export function EditMessageModal({ initialText, onSubmit, onCancel }: EditMessageModalProps) {
  const [text, setText] = useState(initialText);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  // Focus and select all text on mount
  useEffect(() => {
    if (textareaRef.current) {
      textareaRef.current.focus();
      textareaRef.current.select();
    }
  }, []);

  // Auto-resize textarea
  useEffect(() => {
    if (textareaRef.current) {
      textareaRef.current.style.height = "auto";
      textareaRef.current.style.height = `${Math.min(textareaRef.current.scrollHeight, 300)}px`;
    }
  }, [text]);

  const handleSubmit = () => {
    const trimmed = text.trim();
    if (trimmed && trimmed !== initialText.trim()) {
      onSubmit(trimmed);
    } else {
      onCancel();
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      handleSubmit();
    } else if (e.key === "Escape") {
      e.preventDefault();
      onCancel();
    }
  };

  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
      <div className="bg-surface rounded-lg p-6 max-w-2xl w-full mx-4 shadow-xl border border-gray-700">
        <h2 className="text-lg font-semibold text-foreground mb-4">Edit Message</h2>
        <p className="text-sm text-muted mb-4">
          Edit your message and submit to create a new branch with the updated content.
        </p>
        <textarea
          ref={textareaRef}
          value={text}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={handleKeyDown}
          className="w-full bg-elevated border border-gray-600 rounded-lg p-3 text-foreground resize-none focus:outline-none focus:border-teal-500 min-h-[100px]"
          placeholder="Enter your message..."
        />
        <div className="flex justify-end gap-2 mt-4">
          <button
            onClick={onCancel}
            className="px-4 py-2 bg-gray-700 hover:bg-gray-600 text-gray-200 rounded-lg font-medium transition-colors"
          >
            Cancel
          </button>
          <button
            onClick={handleSubmit}
            disabled={!text.trim() || text.trim() === initialText.trim()}
            className="px-4 py-2 bg-teal-600 hover:bg-teal-500 disabled:bg-gray-600 disabled:cursor-not-allowed text-white rounded-lg font-medium transition-colors"
          >
            Submit Edit
          </button>
        </div>
        <p className="text-xs text-muted mt-3 text-center">
          Press <kbd className="px-1 py-0.5 bg-gray-700 rounded text-xs">Cmd+Enter</kbd> to submit, <kbd className="px-1 py-0.5 bg-gray-700 rounded text-xs">Esc</kbd> to cancel
        </p>
      </div>
    </div>
  );
}
