export type ActivityId = "conversations" | "documents";

interface ActivityConfig {
  id: ActivityId;
  label: string;
  icon: React.ReactNode;
}

const ACTIVITIES: ActivityConfig[] = [
  {
    id: "conversations",
    label: "Conversations",
    icon: (
      <svg className="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path
          strokeLinecap="round"
          strokeLinejoin="round"
          strokeWidth={2}
          d="M8 12h.01M12 12h.01M16 12h.01M21 12c0 4.418-4.03 8-9 8a9.863 9.863 0 01-4.255-.949L3 20l1.395-3.72C3.512 15.042 3 13.574 3 12c0-4.418 4.03-8 9-8s9 3.582 9 8z"
        />
      </svg>
    ),
  },
  {
    id: "documents",
    label: "Documents",
    icon: (
      <svg className="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path
          strokeLinecap="round"
          strokeLinejoin="round"
          strokeWidth={2}
          d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z"
        />
      </svg>
    ),
  },
];

interface ActivityButtonProps {
  activity: ActivityConfig;
  isActive: boolean;
  onClick: () => void;
}

function ActivityButton({ activity, isActive, onClick }: ActivityButtonProps) {
  return (
    <button
      onClick={onClick}
      className={`w-12 h-12 flex items-center justify-center transition-colors relative ${
        isActive ? "text-foreground" : "text-muted hover:text-gray-300"
      }`}
      title={activity.label}
    >
      {isActive && (
        <div className="absolute left-0 top-2 bottom-2 w-0.5 bg-teal-500" />
      )}
      {activity.icon}
    </button>
  );
}

interface ActivityBarProps {
  activeActivity: ActivityId;
  onActivityChange: (activity: ActivityId) => void;
  onOpenSettings: () => void;
}

export function ActivityBar({ activeActivity, onActivityChange, onOpenSettings }: ActivityBarProps) {
  return (
    <div className="w-12 bg-background border-r border-gray-700 flex flex-col items-center py-2">
      {/* Activity buttons at top */}
      <div className="flex flex-col items-center">
        {ACTIVITIES.map((activity) => (
          <ActivityButton
            key={activity.id}
            activity={activity}
            isActive={activity.id === activeActivity}
            onClick={() => onActivityChange(activity.id)}
          />
        ))}
      </div>

      {/* Spacer */}
      <div className="flex-1" />

      {/* Settings button at bottom */}
      <button
        onClick={onOpenSettings}
        className="w-12 h-12 flex items-center justify-center text-muted hover:text-gray-300 transition-colors"
        title="Settings"
      >
        <svg className="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth={2}
            d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z"
          />
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth={2}
            d="M15 12a3 3 0 11-6 0 3 3 0 016 0z"
          />
        </svg>
      </button>
    </div>
  );
}
