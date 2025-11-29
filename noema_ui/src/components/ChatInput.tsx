import { useState, useRef, useEffect } from "react";

export type VoiceStatus = "disabled" | "enabled" | "listening" | "transcribing";

interface ChatInputProps {
  onSend: (message: string) => void;
  disabled?: boolean;
  voiceAvailable?: boolean;
  voiceStatus?: VoiceStatus;
  onToggleVoice?: () => void;
}

export function ChatInput({
  onSend,
  disabled = false,
  voiceAvailable = false,
  voiceStatus = "disabled",
  onToggleVoice,
}: ChatInputProps) {
  const [message, setMessage] = useState("");
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  // Auto-resize textarea
  useEffect(() => {
    const textarea = textareaRef.current;
    if (textarea) {
      textarea.style.height = "auto";
      textarea.style.height = `${Math.min(textarea.scrollHeight, 200)}px`;
    }
  }, [message]);

  const handleSubmit = () => {
    const trimmed = message.trim();
    if (trimmed && !disabled) {
      onSend(trimmed);
      setMessage("");
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSubmit();
    }
  };

  const getVoiceButtonClass = () => {
    const base = "px-4 py-3 rounded-2xl transition-colors";
    switch (voiceStatus) {
      case "listening":
        return `${base} bg-red-500 hover:bg-red-600 text-white animate-pulse`;
      case "transcribing":
        return `${base} bg-yellow-500 hover:bg-yellow-600 text-white`;
      case "enabled":
        return `${base} bg-green-500 hover:bg-green-600 text-white`;
      default:
        return `${base} bg-gray-200 dark:bg-gray-700 hover:bg-gray-300 dark:hover:bg-gray-600 text-gray-700 dark:text-gray-300`;
    }
  };

  return (
    <div className="border-t border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-900 p-4">
      <div className="flex gap-3 items-end max-w-4xl mx-auto">
        {voiceAvailable && (
          <button
            type="button"
            onClick={onToggleVoice}
            disabled={disabled}
            className={getVoiceButtonClass()}
            title={
              voiceStatus === "disabled"
                ? "Enable voice input"
                : voiceStatus === "listening"
                ? "Listening..."
                : voiceStatus === "transcribing"
                ? "Transcribing..."
                : "Voice enabled (click to disable)"
            }
          >
            <svg
              className="w-5 h-5"
              fill="none"
              stroke="currentColor"
              viewBox="0 0 24 24"
            >
              {voiceStatus === "disabled" ? (
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
        )}
        <textarea
          ref={textareaRef}
          value={message}
          onChange={(e) => setMessage(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder={
            voiceStatus === "listening"
              ? "Listening... speak now"
              : voiceStatus === "transcribing"
              ? "Transcribing..."
              : "Type a message..."
          }
          disabled={disabled}
          rows={1}
          className="flex-1 px-4 py-3 border border-gray-300 dark:border-gray-600 rounded-2xl resize-none focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent bg-gray-50 dark:bg-gray-800 text-gray-900 dark:text-white placeholder-gray-500 disabled:opacity-50"
        />
        <button
          type="button"
          onClick={handleSubmit}
          disabled={disabled || !message.trim()}
          className="px-4 py-3 bg-blue-500 hover:bg-blue-600 disabled:bg-gray-300 disabled:cursor-not-allowed text-white rounded-2xl transition-colors"
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
  );
}
