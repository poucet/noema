import { useState, useEffect, useRef } from "react";
import type { ModelInfo } from "../types";

interface ModelSelectorProps {
  models: ModelInfo[];
  currentModel: string;
  favoriteModels: string[];
  onSelectModel: (modelId: string, provider: string) => void;
  onToggleFavorite: (modelId: string) => void;
}

export function ModelSelector({
  models,
  currentModel,
  favoriteModels,
  onSelectModel,
  onToggleFavorite,
}: ModelSelectorProps) {
  const [isOpen, setIsOpen] = useState(false);
  const dropdownRef = useRef<HTMLDivElement>(null);

  // Debug logging
  console.log("[ModelSelector] Rendering with props:", {
    modelsCount: models.length,
    currentModel,
    favoriteModels,
    hasFavoriteModels: favoriteModels?.length > 0,
    hasOnToggleFavorite: typeof onToggleFavorite === 'function',
  });

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

  // Separate favorites from other models
  const favoriteModelsList = models.filter((m) =>
    favoriteModels.includes(getFullModelId(m))
  );

  // Group non-favorite models by provider
  const groupedModels = models
    .filter((m) => !favoriteModels.includes(getFullModelId(m)))
    .reduce((acc, model) => {
      const provider = model.provider;
      if (!acc[provider]) acc[provider] = [];
      acc[provider].push(model);
      return acc;
    }, {} as Record<string, ModelInfo[]>);

  const handleToggleFavorite = (e: React.MouseEvent, model: ModelInfo) => {
    e.stopPropagation();
    const fullId = getFullModelId(model);
    console.log("[ModelSelector] Toggling favorite:", fullId);
    onToggleFavorite(fullId);
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

  const ModelRow = ({ model, isFavorite }: { model: ModelInfo; isFavorite: boolean }) => (
    <div className="flex items-center justify-between border-b border-gray-700">
      <button
        onClick={() => {
          onSelectModel(model.id, model.provider);
          setIsOpen(false);
        }}
        className="flex-1 text-left px-3 py-2 text-sm text-gray-300 hover:bg-elevated transition-colors"
      >
        {model.displayName}
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
          <div className="absolute top-full right-0 mt-1 w-72 bg-surface border border-gray-700 rounded-lg shadow-lg z-50 max-h-96 overflow-y-auto">
            {/* Debug info */}
            {console.log("[ModelSelector] Dropdown open, rendering models:", {
              favoriteModelsList: favoriteModelsList.map(m => getFullModelId(m)),
              groupedProviders: Object.keys(groupedModels),
              totalModels: models.length,
            })}
            {/* Favorites section */}
            {favoriteModelsList.length > 0 && (
              <div>
                <div className="px-3 py-2 text-xs font-semibold text-yellow-400 uppercase bg-background flex items-center gap-1">
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
                <div className="px-3 py-2 text-xs font-semibold text-muted uppercase bg-background">
                  {provider}
                </div>
                {providerModels.map((model) => (
                  <ModelRow key={getFullModelId(model)} model={model} isFavorite={false} />
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
