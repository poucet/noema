// Types matching the Rust backend

export interface ModelInfo {
  id: string;
  displayName: string;
  provider: string;
}

export interface ConversationInfo {
  id: string;
  name: string | null;
  messageCount: number;
  createdAt: number;
  updatedAt: number;
}

// Note: Rust uses #[serde(rename_all = "camelCase")] so variant names are lowercase
export type DisplayContent =
  | { text: string }
  | { image: { data: string; mimeType: string } }
  | { audio: { data: string; mimeType: string } }
  | { toolCall: { name: string; id: string } }
  | { toolResult: { id: string; content: DisplayToolResultContent[] } };

export type DisplayToolResultContent =
  | { text: string }
  | { image: { data: string; mimeType: string } }
  | { audio: { data: string; mimeType: string } };

export interface DisplayMessage {
  role: "user" | "assistant" | "system";
  content: DisplayContent[];
}

// Helper to extract text from content
export function getTextContent(content: DisplayContent[]): string {
  return content
    .map((block) => {
      if ("text" in block) return block.text;
      if ("toolCall" in block) return `[Tool: ${block.toolCall.name}]`;
      if ("toolResult" in block) {
        return block.toolResult.content
          .map((c) => ("text" in c ? c.text : "[Media]"))
          .join("");
      }
      return "";
    })
    .join("");
}

// Check if content has media
export function hasMedia(content: DisplayContent[]): boolean {
  return content.some(
    (block) => "image" in block || "audio" in block
  );
}
