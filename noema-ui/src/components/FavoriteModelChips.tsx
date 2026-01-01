import type { ModelInfo } from "../types";

interface FavoriteModelChipsProps {
  models: ModelInfo[];
  favoriteModelIds: string[];
  selectedModelIds: string[];
  currentModelId: string;
  onToggleSelection: (modelId: string) => void;
  onSendToModels: () => void;
}

export function FavoriteModelChips({
  models,
  favoriteModelIds,
  selectedModelIds,
  currentModelId,
  onToggleSelection,
  onSendToModels,
}: FavoriteModelChipsProps) {
  // Get full model info for favorites, preserving the order from favoriteModelIds
  const favoriteModels = favoriteModelIds
    .map((favId) => models.find((m) => `${m.provider}/${m.id}` === favId))
    .filter((m): m is ModelInfo => m !== undefined);

  if (favoriteModels.length === 0) {
    return null;
  }

  const getFullModelId = (model: ModelInfo) => `${model.provider}/${model.id}`;
  const selectedCount = selectedModelIds.length;

  return (
    <div className="flex flex-wrap items-center gap-2 px-4 py-2 border-t border-gray-700 bg-background">
      <span className="text-xs text-muted mr-1">Compare:</span>
      {favoriteModels.map((model) => {
        const fullId = getFullModelId(model);
        const isSelected = selectedModelIds.includes(fullId);
        const isCurrent = fullId === currentModelId;

        return (
          <button
            key={fullId}
            onClick={() => onToggleSelection(fullId)}
            className={`
              px-3 py-1 text-xs rounded-full transition-colors
              ${isSelected
                ? "bg-teal-600 text-white"
                : isCurrent
                  ? "bg-surface text-foreground border border-teal-500"
                  : "bg-surface text-muted hover:bg-elevated"
              }
            `}
            title={isSelected ? "Remove from comparison" : "Add to comparison"}
          >
            {model.displayName}
            {isSelected && (
              <span className="ml-1.5 inline-flex items-center justify-center w-4 h-4 text-[10px] bg-teal-800 rounded-full">
                {selectedModelIds.indexOf(fullId) + 1}
              </span>
            )}
          </button>
        );
      })}

      {selectedCount >= 2 && (
        <button
          onClick={onSendToModels}
          className="ml-2 px-3 py-1 text-xs bg-teal-600 hover:bg-teal-700 text-white rounded-full transition-colors flex items-center gap-1"
        >
          <svg
            className="w-3 h-3"
            fill="none"
            stroke="currentColor"
            viewBox="0 0 24 24"
          >
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              strokeWidth={2}
              d="M8 7h12m0 0l-4-4m4 4l-4 4m0 6H4m0 0l4 4m-4-4l4-4"
            />
          </svg>
          Send to {selectedCount} models
        </button>
      )}

      {selectedCount === 1 && (
        <span className="ml-2 text-xs text-muted">
          Select 2+ models to compare
        </span>
      )}
    </div>
  );
}
