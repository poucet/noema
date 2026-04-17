<script lang="ts">
  import { tick } from 'svelte';
  import { chatStore } from '../../lib/stores/chat.svelte';
  import MessageBubble from './MessageBubble.svelte';

  let container: HTMLElement;

  // Auto-scroll to bottom when messages change
  $effect(() => {
    // Track both messages and streamingMessage
    const _ = chatStore.messages.length;
    const __ = chatStore.streamingMessage;
    tick().then(() => {
      if (container) {
        container.scrollTop = container.scrollHeight;
      }
    });
  });
</script>

<div bind:this={container} class="flex-1 overflow-y-auto px-4 py-4 space-y-3">
  {#each chatStore.messages as msg, i}
    <MessageBubble message={msg} />
  {/each}

  {#if chatStore.streamingMessage}
    <MessageBubble message={chatStore.streamingMessage} streaming />
  {/if}

  {#if chatStore.isLoading && !chatStore.streamingMessage}
    <div class="flex gap-1 px-3 py-2">
      <span class="w-2 h-2 bg-muted/40 rounded-full animate-bounce" style="animation-delay: 0ms"></span>
      <span class="w-2 h-2 bg-muted/40 rounded-full animate-bounce" style="animation-delay: 150ms"></span>
      <span class="w-2 h-2 bg-muted/40 rounded-full animate-bounce" style="animation-delay: 300ms"></span>
    </div>
  {/if}
</div>
