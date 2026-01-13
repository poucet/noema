import { useState, useEffect, useRef } from "react";
import type { ViewInfo } from "../tauri";

interface ViewSelectorProps {
  views: ViewInfo[];
  currentViewId: string | null;
  onSwitchView: (viewId: string) => void;
}

export function ViewSelector({ views, currentViewId, onSwitchView }: ViewSelectorProps) {
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

  // Don't show if only one view (main view)
  if (views.length <= 1) {
    return null;
  }

  const currentView = views.find(v => v.id === currentViewId) || views.find(v => v.isMain);
  const viewName = currentView?.name || (currentView?.isMain ? "Main" : "Fork");

  return (
    <div ref={dropdownRef} className="relative">
      <button
        onClick={() => setIsOpen(!isOpen)}
        className="flex items-center gap-2 px-3 py-1.5 bg-purple-900/30 hover:bg-purple-900/50 border border-purple-500/50 rounded-lg text-sm transition-colors"
        title="Switch view"
      >
        {/* Fork icon */}
        <svg className="w-4 h-4 text-purple-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <circle cx="6" cy="6" r="2" strokeWidth={2} />
          <circle cx="6" cy="18" r="2" strokeWidth={2} />
          <circle cx="18" cy="12" r="2" strokeWidth={2} />
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 8v8M6 8c0 2 2 4 6 4h4" />
        </svg>
        <span className="text-purple-200">{viewName}</span>
        <span className="text-purple-400 text-xs">({views.length})</span>
        {/* Dropdown arrow */}
        <svg className={`w-3 h-3 text-purple-400 transition-transform ${isOpen ? "rotate-180" : ""}`} fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 9l-7 7-7-7" />
        </svg>
      </button>

      {isOpen && (
        <div className="absolute top-full left-0 mt-1 w-64 bg-elevated border border-gray-600 rounded-lg shadow-lg z-50 overflow-hidden">
          <div className="text-xs text-muted px-3 py-2 border-b border-gray-700">
            Views ({views.length})
          </div>
          <ul className="max-h-64 overflow-y-auto">
            {views.map((view) => {
              const isActive = view.id === currentViewId || (currentViewId === null && view.isMain);
              const displayName = view.name || (view.isMain ? "Main" : `Fork ${view.id.slice(0, 8)}`);

              return (
                <li key={view.id}>
                  <button
                    onClick={() => {
                      onSwitchView(view.id);
                      setIsOpen(false);
                    }}
                    className={`w-full text-left px-3 py-2 flex items-center gap-2 ${
                      isActive
                        ? "bg-purple-600/30 text-purple-100"
                        : "hover:bg-gray-700/50 text-foreground"
                    }`}
                  >
                    {view.isMain ? (
                      <svg className="w-4 h-4 text-teal-400 shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M3 12l2-2m0 0l7-7 7 7M5 10v10a1 1 0 001 1h3m10-11l2 2m-2-2v10a1 1 0 01-1 1h-3m-6 0a1 1 0 001-1v-4a1 1 0 011-1h2a1 1 0 011 1v4a1 1 0 001 1m-6 0h6" />
                      </svg>
                    ) : (
                      <svg className="w-4 h-4 text-purple-400 shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <circle cx="6" cy="6" r="2" strokeWidth={2} />
                        <circle cx="6" cy="18" r="2" strokeWidth={2} />
                        <circle cx="18" cy="12" r="2" strokeWidth={2} />
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 8v8M6 8c0 2 2 4 6 4h4" />
                      </svg>
                    )}
                    <span className="truncate">{displayName}</span>
                    {isActive && (
                      <svg className="w-4 h-4 text-purple-400 ml-auto shrink-0" fill="currentColor" viewBox="0 0 20 20">
                        <path fillRule="evenodd" d="M16.707 5.293a1 1 0 010 1.414l-8 8a1 1 0 01-1.414 0l-4-4a1 1 0 011.414-1.414L8 12.586l7.293-7.293a1 1 0 011.414 0z" clipRule="evenodd" />
                      </svg>
                    )}
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
