import type { ContentBlock, InputContent, ResolvedContent, ResolvedMessage } from '@simply/client';

export type EntityResolvedContent = {
  type: 'entity';
  entity_id: string;
  resolved: ContentBlock | null;
};

export type EntityRefResolvedContent = {
  type: 'entity_ref';
  entity_id: string;
};

export type AssetRefResolvedContent = {
  type: 'asset_ref';
  asset_id: string;
  mime_type: string;
};

export type ChatDisplayContent =
  | ResolvedContent
  | InputContent
  | EntityResolvedContent
  | EntityRefResolvedContent
  | AssetRefResolvedContent;

export interface ChatDisplayMessage {
  role: 'user' | 'assistant' | 'system';
  content: ChatDisplayContent[];
  turnId?: string;
}

export function normalizeInputContent(content: InputContent[] | string): InputContent[] {
  const inputContent = typeof content === 'string'
    ? [{ type: 'text' as const, text: content.trim() }]
    : content;

  return inputContent.filter((item) => item.type !== 'text' || item.text.trim());
}

export function toChatDisplayMessage(message: ResolvedMessage): ChatDisplayMessage {
  return {
    role: message.role as 'user' | 'assistant' | 'system',
    content: message.content as ChatDisplayContent[],
    turnId: (message as { turn_id?: string; turnId?: string }).turn_id ?? message.turnId,
  };
}
