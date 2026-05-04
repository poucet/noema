<script lang="ts">
  import { tick } from 'svelte';
  import { chatStore } from './stores/chat.svelte';
  import MessageBubble from './MessageBubble.svelte';
  import ChatInput from './ChatInput.svelte';

  let scrollEl: HTMLDivElement | undefined = $state();
  let prevMessageCount = 0;

  // Auto-scroll to bottom when a new message (or streaming chunk) arrives.
  // Tick first so the DOM reflects the latest state before we measure.
  $effect(() => {
    const count = chatStore.messages.length + (chatStore.streamingMessage ? 1 : 0);
    const shouldScroll = count !== prevMessageCount || chatStore.streamingMessage;
    prevMessageCount = count;
    if (!shouldScroll || !scrollEl) return;
    tick().then(() => {
      if (scrollEl) scrollEl.scrollTop = scrollEl.scrollHeight;
    });
  });
</script>

<div class="flex min-h-0 flex-1 flex-col">
  <div bind:this={scrollEl} class="min-h-0 flex-1 space-y-4 overflow-y-auto p-6">
    {#if chatStore.currentConversationId == null}
      <div class="flex h-full items-center justify-center">
        <p class="text-muted">Select or create a conversation to start chatting.</p>
      </div>
    {:else if chatStore.messages.length === 0 && !chatStore.streamingMessage}
      <div class="flex h-full items-center justify-center">
        <p class="text-muted">Send a message to kick off the conversation.</p>
      </div>
    {:else}
      {#each chatStore.messages as msg, i (msg.turnId ?? i)}
        <MessageBubble message={msg} />
      {/each}
      {#if chatStore.streamingMessage}
        <MessageBubble message={chatStore.streamingMessage} />
      {/if}
    {/if}

    {#if chatStore.error}
      <div class="rounded bg-red-900/50 px-3 py-2 text-sm text-red-200">
        {chatStore.error}
      </div>
    {/if}
  </div>

  <ChatInput
    onSend={(content) => chatStore.sendMessage(content)}
    disabled={chatStore.currentConversationId == null}
    isLoading={chatStore.isLoading}
  />
</div>
