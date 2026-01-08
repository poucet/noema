import { useState, useEffect, useRef, useMemo } from "react";
import type { ModelInfo } from "../types";

interface ModelSelectorProps {
  models: ModelInfo[];
  currentModel: string;
  favoriteModels: string[];
  onSelectModel: (modelId: string, provider: string) => void;
  onToggleFavorite: (modelId: string) => void;
}

// Format context window size for display
function formatContextWindow(tokens: number | null): string {
  if (tokens === null) return "";
  if (tokens >= 1000000) return `${(tokens / 1000000).toFixed(1)}M`;
  if (tokens >= 1000) return `${Math.round(tokens / 1000)}K`;
  return String(tokens);
}

// Capability icon components
const TextIcon = () => (
  <svg className="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M8 10h.01M12 10h.01M16 10h.01M9 16H5a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v8a2 2 0 01-2 2h-5l-5 5v-5z" />
  </svg>
);

const VisionIcon = () => (
  <svg className="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z" />
  </svg>
);

const EmbeddingIcon = () => (
  <svg className="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M7 21a4 4 0 01-4-4V5a2 2 0 012-2h4a2 2 0 012 2v12a4 4 0 01-4 4zm0 0h12a2 2 0 002-2v-4a2 2 0 00-2-2h-2.343M11 7.343l1.657-1.657a2 2 0 012.828 0l2.829 2.829a2 2 0 010 2.828l-8.486 8.485M7 17h.01" />
  </svg>
);

// Get capability info
function getCapabilities(capabilities: string[]): { key: string; label: string; Icon: React.FC }[] {
  const result: { key: string; label: string; Icon: React.FC }[] = [];
  if (capabilities.includes("Text")) {
    result.push({ key: "text", label: "Text/Chat", Icon: TextIcon });
  }
  if (capabilities.includes("Image")) {
    result.push({ key: "image", label: "Vision", Icon: VisionIcon });
  }
  if (capabilities.includes("Embedding")) {
    result.push({ key: "embedding", label: "Embedding", Icon: EmbeddingIcon });
  }
  return result;
}

export function ModelSelector({
  models,
  currentModel,
  favoriteModels,
  onSelectModel,
  onToggleFavorite,
}: ModelSelectorProps) {
  const [isOpen, setIsOpen] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const dropdownRef = useRef<HTMLDivElement>(null);
  const searchInputRef = useRef<HTMLInputElement>(null);

  // Focus search input when dropdown opens
  useEffect(() => {
    if (isOpen && searchInputRef.current) {
      searchInputRef.current.focus();
    }
    if (!isOpen) {
      setSearchQuery("");
    }
  }, [isOpen]);

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

  // Helper to get full model ID
  const getFullModelId = (model: ModelInfo) => `${model.provider}/${model.id}`;

  // Find the current model object for display
  const currentModelObj = useMemo(() => {
    return models.find((m) => m.displayName === currentModel || m.id === currentModel);
  }, [models, currentModel]);

  // Filter models by search query
  const filteredModels = useMemo(() => {
    if (!searchQuery.trim()) return models;
    const query = searchQuery.toLowerCase();
    return models.filter(
      (m) =>
        m.displayName.toLowerCase().includes(query) ||
        m.id.toLowerCase().includes(query) ||
        m.provider.toLowerCase().includes(query)
    );
  }, [models, searchQuery]);

  // Separate favorites from other models
  const favoriteModelsList = filteredModels.filter((m) =>
    favoriteModels.includes(getFullModelId(m))
  );

  // Group non-favorite models by provider
  const groupedModels = filteredModels
    .filter((m) => !favoriteModels.includes(getFullModelId(m)))
    .reduce((acc, model) => {
      const provider = model.provider;
      if (!acc[provider]) acc[provider] = [];
      acc[provider].push(model);
      return acc;
    }, {} as Record<string, ModelInfo[]>);

  const handleToggleFavorite = (e: React.MouseEvent, model: ModelInfo) => {
    e.stopPropagation();
    onToggleFavorite(getFullModelId(model));
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") {
      setIsOpen(false);
    }
  };

  const StarIcon = ({ filled }: { filled: boolean }) => (
    <svg
      className={`w-4 h-4 ${filled ? "text-yellow-400 fill-yellow-400" : "text-gray-400 hover:text-yellow-300"}`}
      fill={filled ? "currentColor" : "none"}
      stroke="currentColor"
      viewBox="0 0 24 24"
    >
      <path
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth={2}
        d="M11.049 2.927c.3-.921 1.603-.921 1.902 0l1.519 4.674a1 1 0 00.95.69h4.915c.969 0 1.371 1.24.588 1.81l-3.976 2.888a1 1 0 00-.363 1.118l1.518 4.674c.3.922-.755 1.688-1.538 1.118l-3.976-2.888a1 1 0 00-1.176 0l-3.976 2.888c-.783.57-1.838-.197-1.538-1.118l1.518-4.674a1 1 0 00-.363-1.118l-3.976-2.888c-.784-.57-.38-1.81.588-1.81h4.914a1 1 0 00.951-.69l1.519-4.674z"
      />
    </svg>
  );

  const ModelRow = ({ model, isFavorite }: { model: ModelInfo; isFavorite: boolean }) => {
    const contextWindow = formatContextWindow(model.contextWindow);
    const capabilities = getCapabilities(model.capabilities);

    return (
      <div className="flex items-center justify-between border-b border-gray-700 hover:bg-elevated transition-colors">
        <button
          onClick={() => {
            onSelectModel(model.id, model.provider);
            setIsOpen(false);
          }}
          className="flex-1 text-left px-3 py-2"
        >
          <div className="flex items-center justify-between">
            <div className="flex-1 min-w-0">
              <div className="text-sm text-gray-300 truncate">{model.displayName}</div>
              <div className="text-xs text-gray-500 truncate">{model.id}</div>
            </div>
            <div className="flex items-center gap-2 ml-2 shrink-0">
              {/* Capabilities */}
              <div className="flex gap-1">
                {capabilities.map(({ key, label, Icon }) => (
                  <span
                    key={key}
                    title={label}
                    className="w-5 h-5 flex items-center justify-center text-gray-400 hover:text-gray-300"
                  >
                    <Icon />
                  </span>
                ))}
              </div>
              {/* Context window */}
              {contextWindow && (
                <span
                  title={`Context window: ${model.contextWindow?.toLocaleString()} tokens`}
                  className="text-[10px] text-gray-500 bg-gray-800 px-1.5 py-0.5 rounded"
                >
                  {contextWindow}
                </span>
              )}
            </div>
          </div>
        </button>
        <button
          onClick={(e) => handleToggleFavorite(e, model)}
          className="px-3 py-2 hover:bg-yellow-900 transition-colors"
          title={isFavorite ? "Remove from favorites" : "Add to favorites"}
        >
          <span className="text-lg">{isFavorite ? "★" : "☆"}</span>
        </button>
      </div>
    );
  };

  return (
    <div className="flex items-center gap-4">
      {/* Model dropdown */}
      <div className="relative" ref={dropdownRef}>
        <button
          onClick={() => setIsOpen(!isOpen)}
          className="flex items-center gap-2 px-3 py-2 bg-surface hover:bg-elevated rounded-lg text-sm font-medium text-muted transition-colors"
        >
          <div className="flex flex-col items-end">
            <span>{currentModelObj?.displayName || currentModel || "Select Model"}</span>
            {currentModelObj && currentModelObj.displayName !== currentModelObj.id && (
              <span className="text-xs text-gray-500">{currentModelObj.id}</span>
            )}
          </div>
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
          <div
            className="absolute top-full right-0 mt-1 w-96 bg-surface border border-gray-700 rounded-lg shadow-lg z-50 max-h-112 flex flex-col"
            onKeyDown={handleKeyDown}
          >
            {/* Search input */}
            <div className="p-2 border-b border-gray-700">
              <input
                ref={searchInputRef}
                type="text"
                placeholder="Search models..."
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
                className="w-full px-3 py-2 text-sm bg-background border border-gray-600 rounded-md text-gray-300 placeholder-gray-500 focus:outline-none focus:border-teal-500"
              />
            </div>

            {/* Scrollable model list */}
            <div className="overflow-y-auto overflow-x-hidden flex-1">
              {/* Favorites section */}
              {favoriteModelsList.length > 0 && (
                <div>
                  <div className="px-3 py-2 text-xs font-semibold text-yellow-400 uppercase bg-background flex items-center gap-1 sticky top-0">
                    <StarIcon filled={true} />
                    Favorites
                  </div>
                  {favoriteModelsList.map((model) => (
                    <ModelRow key={getFullModelId(model)} model={model} isFavorite={true} />
                  ))}
                </div>
              )}

              {/* Other models grouped by provider */}
              {Object.entries(groupedModels).map(([provider, providerModels]) => (
                <div key={provider}>
                  <div className="px-3 py-2 text-xs font-semibold text-muted uppercase bg-background sticky top-0">
                    {provider}
                  </div>
                  {providerModels.map((model) => (
                    <ModelRow key={getFullModelId(model)} model={model} isFavorite={false} />
                  ))}
                </div>
              ))}

              {filteredModels.length === 0 && searchQuery && (
                <div className="px-3 py-4 text-sm text-muted text-center">
                  No models match "{searchQuery}"
                </div>
              )}

              {models.length === 0 && (
                <div className="px-3 py-4 text-sm text-muted text-center">
                  Loading models...
                </div>
              )}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
