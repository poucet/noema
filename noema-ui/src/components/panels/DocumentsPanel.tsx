export function DocumentsPanel() {
  return (
    <>
      <div className="p-4 border-b border-gray-700">
        <h2 className="text-lg font-semibold text-foreground">Documents</h2>
      </div>
      <div className="flex-1 overflow-y-auto flex items-center justify-center">
        <div className="text-center p-4">
          <svg
            className="w-12 h-12 text-muted mx-auto mb-3"
            fill="none"
            stroke="currentColor"
            viewBox="0 0 24 24"
          >
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              strokeWidth={1.5}
              d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z"
            />
          </svg>
          <p className="text-muted text-sm">Documents feature coming soon</p>
        </div>
      </div>
    </>
  );
}
