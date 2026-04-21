<script lang="ts">
  import type { ConversationInfo } from '@simply/client';
  import type { ActivityId } from './ActivityBar.svelte';
  import ConversationsPanel from './ConversationsPanel.svelte';
  import DocumentsPanel from './DocumentsPanel.svelte';

  type Props = {
    active: ActivityId;
    conversations: ConversationInfo[];
    currentConversationId: string | null;
    onNewConversation: () => void;
    onSelectConversation: (id: string) => void;
    onRenameConversation: (id: string, name: string) => void;
    onDeleteConversation: (id: string) => void;
  };

  const props: Props = $props();
</script>

<div class="flex h-full w-64 flex-col border-r border-gray-700 bg-surface">
  {#if props.active === 'conversations'}
    <ConversationsPanel
      conversations={props.conversations}
      currentId={props.currentConversationId}
      onNew={props.onNewConversation}
      onSelect={props.onSelectConversation}
      onRename={props.onRenameConversation}
      onDelete={props.onDeleteConversation}
    />
  {:else}
    <DocumentsPanel />
  {/if}
</div>
