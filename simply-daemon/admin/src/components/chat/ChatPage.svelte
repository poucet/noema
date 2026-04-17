<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { chatStore } from '../../lib/stores/chat.svelte';
  import ConversationList from './ConversationList.svelte';
  import MessageList from './MessageList.svelte';
  import ChatInput from './ChatInput.svelte';

  let handleUnload: (() => void) | null = null;

  onMount(() => {
    chatStore.init();
    handleUnload = () => chatStore.cleanup();
    window.addEventListener('beforeunload', handleUnload);
  });

  onDestroy(() => {
    if (typeof window !== 'undefined' && handleUnload) {
      window.removeEventListener('beforeunload', handleUnload);
    }
    chatStore.cleanup();
  });
</script>

<div class="flex h-[calc(100vh-3rem)] overflow-hidden">
  <!-- Sidebar: conversation list -->
  <div class="w-64 shrink-0 border-r border-border flex flex-col">
    <div class="p-3 border-b border-border flex items-center justify-between">
      <span class="text-sm font-medium text-muted">Conversations</span>
      <button
        class="text-xs px-2 py-1 rounded bg-accent/20 text-accent hover:bg-accent/30"
        onclick={() => chatStore.newConversation()}
      >+ New</button>
    </div>
    <ConversationList />
  </div>

  <!-- Main: messages + input -->
  <div class="flex-1 flex flex-col min-w-0">
    {#if chatStore.currentConversationId}
      <!-- Header -->
      <div class="px-4 py-2 border-b border-border flex items-center justify-between text-sm">
        <span class="text-muted truncate">
          {chatStore.currentConversation?.name || 'Untitled'}
        </span>
      </div>

      <!-- Messages -->
      <MessageList />

      <!-- Error banner -->
      {#if chatStore.error}
        <div class="px-4 py-2 bg-red-900/30 text-red-300 text-xs border-t border-red-800">
          {chatStore.error}
          <button class="ml-2 underline" onclick={() => chatStore.error = null}>dismiss</button>
        </div>
      {/if}

      <!-- Input -->
      <ChatInput />
    {:else}
      <div class="flex-1 flex items-center justify-center text-muted text-sm">
        Select a conversation or create a new one
      </div>
    {/if}
  </div>
</div>
