// Fork icon component - git branch style
export function ForkIcon({ className = "w-3.5 h-3.5" }: { className?: string }) {
  return (
    <svg className={className} fill="none" stroke="currentColor" viewBox="0 0 24 24">
      {/* Git branch/fork icon: vertical line with a branch splitting off */}
      <circle cx="6" cy="6" r="2" strokeWidth={2} />
      <circle cx="6" cy="18" r="2" strokeWidth={2} />
      <circle cx="18" cy="12" r="2" strokeWidth={2} />
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 8v8M6 8c0 2 2 4 6 4h4" />
    </svg>
  );
}
