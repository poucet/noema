import { useState, useEffect, useRef } from "react";
import type { ModelInfo } from "../types";

interface ModelSelectorProps {
  models: ModelInfo[];
  currentModel: string;
  onSelectModel: (modelId: string, provider: string) => void;
}

export function ModelSelector({
  models,
  currentModel,
  onSelectModel,
}: ModelSelectorProps) {
  const [isOpen, setIsOpen] = useState(false);
  const dropdownRef = useRef<HTMLDivElement>(null);

  // Close dropdown when clicking outside
  useEffect(() => {
    const handleClickOutside = (e: MouseEvent) => {
      if (dropdownRef.current && !dropdownRef.current.contains(e.target as Node)) {
        setIsOpen(false);
      }
    };
    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, []);

  // Group models by provider
  const groupedModels = models.reduce((acc, model) => {
    const provider = model.provider;
    if (!acc[provider]) acc[provider] = [];
    acc[provider].push(model);
    return acc;
  }, {} as Record<string, ModelInfo[]>);

  return (
    <div className="flex items-center gap-4">
      {/* Model dropdown */}
      <div className="relative" ref={dropdownRef}>
        <button
          onClick={() => setIsOpen(!isOpen)}
          className="flex items-center gap-2 px-3 py-2 bg-surface hover:bg-elevated rounded-lg text-sm font-medium text-muted transition-colors"
        >
          <span>{currentModel || "Select Model"}</span>
          <svg
            className={`w-4 h-4 transition-transform ${isOpen ? "rotate-180" : ""}`}
            fill="none"
            stroke="currentColor"
            viewBox="0 0 24 24"
          >
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              strokeWidth={2}
              d="M19 9l-7 7-7-7"
            />
          </svg>
        </button>

        {isOpen && (
          <div className="absolute top-full left-0 mt-1 w-64 bg-surface border border-gray-700 rounded-lg shadow-lg z-50 max-h-96 overflow-y-auto">
            {Object.entries(groupedModels).map(([provider, providerModels]) => (
              <div key={provider}>
                <div className="px-3 py-2 text-xs font-semibold text-muted uppercase bg-background">
                  {provider}
                </div>
                {providerModels.map((model) => (
                  <button
                    key={model.id}
                    onClick={() => {
                      onSelectModel(model.id, model.provider);
                      setIsOpen(false);
                    }}
                    className="w-full text-left px-3 py-2 text-sm text-gray-300 hover:bg-elevated transition-colors"
                  >
                    {model.displayName}
                  </button>
                ))}
              </div>
            ))}
            {models.length === 0 && (
              <div className="px-3 py-4 text-sm text-muted text-center">
                Loading models...
              </div>
            )}
          </div>
        )}
      </div>

    </div>
  );
}
