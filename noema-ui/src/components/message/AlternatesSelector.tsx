import type { AlternateInfo } from "../../types";

// Alternates selector component for assistant messages with multiple model responses
// Separates "viewing" (preview) from "selecting" (committing to database)
export function AlternatesSelector({
  alternates,
  spanSetId,
  previewSpanId,
  onPreview,
  onConfirmSelection,
}: {
  alternates: AlternateInfo[];
  spanSetId: string;
  previewSpanId: string | null;
  onPreview: (spanId: string) => void;
  onConfirmSelection?: (spanSetId: string, spanId: string) => void;
}) {
  // Find which is the saved selection and which is being previewed
  const savedSelection = alternates.find(a => a.isSelected);
  const currentlyViewing = previewSpanId
    ? alternates.find(a => a.spanId === previewSpanId)
    : savedSelection;
  const isPreviewingDifferent = previewSpanId && previewSpanId !== savedSelection?.spanId;

  return (
    <div className="flex flex-wrap items-center gap-2 mb-3 pb-2 border-b border-gray-600">
      {alternates.map((alt) => {
        const isViewing = currentlyViewing?.spanId === alt.spanId;
        const isSaved = alt.isSelected;
        return (
          <button
            key={alt.spanId}
            onClick={() => onPreview(alt.spanId)}
            className={`px-3 py-1.5 text-xs rounded-md transition-all whitespace-nowrap flex items-center gap-1.5 border ${
              isViewing
                ? "bg-teal-600 text-white font-semibold border-teal-500 shadow-sm"
                : "bg-elevated text-foreground hover:bg-surface hover:border-gray-500 cursor-pointer border-gray-600"
            }`}
            title={isSaved ? `${alt.modelId || "Model"} (saved)` : `Preview ${alt.modelId || "Model"}`}
          >
            {isSaved && (
              <svg className="w-3 h-3 text-teal-300" fill="currentColor" viewBox="0 0 20 20">
                <path fillRule="evenodd" d="M16.707 5.293a1 1 0 010 1.414l-8 8a1 1 0 01-1.414 0l-4-4a1 1 0 011.414-1.414L8 12.586l7.293-7.293a1 1 0 011.414 0z" clipRule="evenodd" />
              </svg>
            )}
            {alt.modelDisplayName || alt.modelId?.split("/").pop() || "Model"}
          </button>
        );
      })}
      {/* Confirm selection icon button - only show when previewing a different response */}
      {isPreviewingDifferent && onConfirmSelection && (
        <button
          onClick={() => onConfirmSelection(spanSetId, previewSpanId!)}
          className="p-1.5 bg-teal-600 text-white rounded hover:bg-teal-500 transition-colors"
          title="Use this response"
        >
          <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 13l4 4L19 7" />
          </svg>
        </button>
      )}
    </div>
  );
}
