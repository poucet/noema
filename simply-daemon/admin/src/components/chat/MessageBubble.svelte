<script lang="ts">
  import type { DisplayMessage } from '../../lib/stores/chat.svelte';
  import type { ResolvedContent } from '@simply/client';

  interface Props {
    message: DisplayMessage;
    streaming?: boolean;
  }
  let { message, streaming = false }: Props = $props();

  const isUser = $derived(message.role === 'user');
  const isAssistant = $derived(message.role === 'assistant');
</script>

<div class="flex {isUser ? 'justify-end' : 'justify-start'}">
  <div class="max-w-[80%] rounded-lg px-3 py-2 text-sm
              {isUser ? 'bg-accent/20 text-fg' : 'bg-white/5 text-fg'}
              {streaming ? 'border border-accent/30' : ''}">
    {#each message.content as block}
      {@render contentBlock(block)}
    {/each}
  </div>
</div>

{#snippet contentBlock(block: ResolvedContent)}
  {#if 'text' in block && block.type === 'text'}
    <div class="whitespace-pre-wrap break-words">{block.text}</div>
  {:else if 'type' in block && block.type === 'asset'}
    {#if block.mime_type?.startsWith('image/')}
      <img
        src="/api/blob/{block.blob_hash}"
        alt="asset"
        class="max-w-full rounded mt-1"
      />
    {:else}
      <div class="text-xs text-muted italic">Asset: {block.mime_type}</div>
    {/if}
  {:else if 'type' in block && block.type === 'tool_call'}
    <div class="text-xs mt-1 p-2 bg-black/20 rounded font-mono">
      <span class="text-accent">⚡ {block.name}</span>
      <pre class="text-muted/70 overflow-x-auto mt-1">{JSON.stringify(block.arguments, null, 2)}</pre>
    </div>
  {:else if 'type' in block && block.type === 'tool_result'}
    <div class="text-xs mt-1 p-2 bg-black/20 rounded font-mono">
      <span class="text-green-400">✓ Result</span>
      {#each block.content as rc}
        {#if 'text' in rc}
          <pre class="text-muted/70 overflow-x-auto mt-1">{rc.text}</pre>
        {/if}
      {/each}
    </div>
  {:else if 'type' in block && block.type === 'document'}
    <span class="inline-block px-2 py-0.5 bg-accent/10 text-accent text-xs rounded">
      📄 doc:{block.document_id}
    </span>
  {:else}
    <div class="text-xs text-muted italic">Unknown content type</div>
  {/if}
{/snippet}
