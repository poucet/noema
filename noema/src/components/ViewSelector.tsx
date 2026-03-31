import { useState, useEffect, useRef } from "react";
import type { ForkInfo } from "../tauri";

interface ViewSelectorProps {
  forks: ForkInfo[];
  onSwitchToFork: (conversationId: string) => void;
}

export function ViewSelector({ forks, onSwitchToFork }: ViewSelectorProps) {
  const [isOpen, setIsOpen] = useState(false);
  const dropdownRef = useRef<HTMLDivElement>(null);

  // Close dropdown when clicking outside
  useEffect(() => {
    const handleClickOutside = (event: MouseEvent) => {
      if (dropdownRef.current && !dropdownRef.current.contains(event.target as Node)) {
        setIsOpen(false);
      }
    };
    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, []);

  // Don't show if no forks
  if (forks.length === 0) {
    return null;
  }

  return (
    <div ref={dropdownRef} className="relative">
      <button
        onClick={() => setIsOpen(!isOpen)}
        className="flex items-center gap-2 px-3 py-1.5 bg-purple-900/30 hover:bg-purple-900/50 border border-purple-500/50 rounded-lg text-sm transition-colors"
        title="View forks"
      >
        {/* Fork icon */}
        <svg className="w-4 h-4 text-purple-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <circle cx="6" cy="6" r="2" strokeWidth={2} />
          <circle cx="6" cy="18" r="2" strokeWidth={2} />
          <circle cx="18" cy="12" r="2" strokeWidth={2} />
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 8v8M6 8c0 2 2 4 6 4h4" />
        </svg>
        <span className="text-purple-200">Forks</span>
        <span className="text-purple-400 text-xs">({forks.length})</span>
        {/* Dropdown arrow */}
        <svg className={`w-3 h-3 text-purple-400 transition-transform ${isOpen ? "rotate-180" : ""}`} fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 9l-7 7-7-7" />
        </svg>
      </button>

      {isOpen && (
        <div className="absolute top-full left-0 mt-1 w-64 bg-elevated border border-gray-600 rounded-lg shadow-lg z-50 overflow-hidden">
          <div className="text-xs text-muted px-3 py-2 border-b border-gray-700">
            Forked conversations ({forks.length})
          </div>
          <ul className="max-h-64 overflow-y-auto">
            {forks.map((fork) => {
              const displayName = `Fork (${fork.turnCount} turns)`;
              const createdDate = new Date(fork.createdAt * 1000).toLocaleDateString();

              return (
                <li key={fork.conversationId}>
                  <button
                    onClick={() => {
                      onSwitchToFork(fork.conversationId);
                      setIsOpen(false);
                    }}
                    className="w-full text-left px-3 py-2 flex items-center gap-2 hover:bg-gray-700/50 text-foreground"
                  >
                    <svg className="w-4 h-4 text-purple-400 shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <circle cx="6" cy="6" r="2" strokeWidth={2} />
                      <circle cx="6" cy="18" r="2" strokeWidth={2} />
                      <circle cx="18" cy="12" r="2" strokeWidth={2} />
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 8v8M6 8c0 2 2 4 6 4h4" />
                    </svg>
                    <div className="flex-1 min-w-0">
                      <span className="truncate block">{displayName}</span>
                      <span className="text-xs text-muted">{createdDate}</span>
                    </div>
                  </button>
                </li>
              );
            })}
          </ul>
        </div>
      )}
    </div>
  );
}
