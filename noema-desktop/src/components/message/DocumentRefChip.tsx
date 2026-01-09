interface DocumentRefChipProps {
  id: string;
  title: string;
  onClick?: (docId: string) => void;
}

export function DocumentRefChip({ id, title, onClick }: DocumentRefChipProps) {
  return (
    <button
      onClick={() => onClick?.(id)}
      className="inline-flex items-center gap-1.5 px-2.5 py-1 my-1 rounded-full bg-purple-600/30 hover:bg-purple-600/50 text-purple-200 text-sm transition-colors cursor-pointer border border-purple-500/30"
      title={`View document: ${title}`}
    >
      <svg
        className="w-3.5 h-3.5"
        fill="none"
        stroke="currentColor"
        viewBox="0 0 24 24"
      >
        <path
          strokeLinecap="round"
          strokeLinejoin="round"
          strokeWidth={2}
          d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z"
        />
      </svg>
      <span className="max-w-[200px] truncate">{title}</span>
    </button>
  );
}
