<script lang="ts">
  import type { ConversationInfo } from '@simply/client';
  import type { ActivityId } from './ActivityBar.svelte';
  import ConversationsPanel from './ConversationsPanel.svelte';

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
    <div class="border-b border-gray-700 p-4">
      <button
        class="w-full rounded-lg bg-teal-600 px-4 py-2 font-medium text-white transition-colors hover:bg-teal-700"
      >
        New document
      </button>
    </div>
    <div class="flex-1 overflow-y-auto">
      <p class="p-4 text-center text-sm text-muted">No documents yet.</p>
    </div>
  {/if}
</div>
