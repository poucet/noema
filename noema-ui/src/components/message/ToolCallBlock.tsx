import { useState } from "react";

export function ToolCallBlock({ name, arguments: args }: { name: string; arguments: unknown }) {
  const [expanded, setExpanded] = useState(false);
  const argsString = args && typeof args === "object"
    ? JSON.stringify(args, null, 2)
    : String(args ?? "");
  const shortArgs = args && typeof args === "object"
    ? JSON.stringify(args)
    : String(args ?? "");
  const shortDisplay = shortArgs.length > 60 ? shortArgs.slice(0, 60) + "..." : shortArgs;

  return (
    <div className="bg-purple-900/50 text-purple-200 px-3 py-2 rounded-lg text-sm">
      <button
        onClick={() => setExpanded(!expanded)}
        className="w-full text-left flex items-center gap-2"
      >
        <span className="text-purple-400">{expanded ? "▼" : "▶"}</span>
        <span className="font-semibold">{name}</span>
        {!expanded && shortDisplay && (
          <span className="text-purple-300/70 text-xs truncate flex-1">{shortDisplay}</span>
        )}
      </button>
      {expanded && argsString && (
        <pre className="mt-2 text-xs bg-purple-950/50 p-2 rounded overflow-x-auto whitespace-pre-wrap">
          {argsString}
        </pre>
      )}
    </div>
  );
}
