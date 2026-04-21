<script lang="ts">
  import { getBaseUrl } from '@simply/client';
  import type { DisplayMessage } from './stores/chat.svelte';
  import MarkdownView from './MarkdownView.svelte';

  type Props = { message: DisplayMessage };
  const { message }: Props = $props();

  // Asset/blob URLs are same-origin when the daemon serves the page. In the
  // Tauri webview the origin is `tauri://localhost`, so we need an absolute
  // URL pointing at the daemon (see MarkdownView for the markdown-side
  // equivalent).
  function blobUrl(hash: string): string {
    return `${getBaseUrl()}/api/blob/${hash}`;
  }

  // Pretty-print a value as indented JSON when it looks like an object/array;
  // otherwise return the string form. Matches the terminal-log style tool
  // blocks get in the old React UI.
  function formatArgs(value: unknown): string {
    if (value == null) return '';
    if (typeof value === 'string') return value;
    try {
      return JSON.stringify(value, null, 2);
    } catch {
      return String(value);
    }
  }
</script>

<div class="flex w-full {message.role === 'user' ? 'justify-end' : 'justify-start'}">
  <div
    class="max-w-[85%] space-y-2 rounded-lg px-4 py-3 {message.role === 'user'
      ? 'bg-teal-800 text-foreground'
      : 'bg-surface text-foreground'}"
  >
    {#each message.content as block, i (i)}
      {#if block.type === 'text'}
        <MarkdownView content={block.text} />
      {:else if block.type === 'tool_call'}
        <div class="rounded border border-gray-700 bg-elevated p-2 font-mono text-xs">
          <div class="mb-1 text-teal-400">🔧 {block.name}</div>
          {#if block.arguments !== undefined && block.arguments !== null}
            <pre class="whitespace-pre-wrap break-all text-gray-300">{formatArgs(block.arguments)}</pre>
          {/if}
        </div>
      {:else if block.type === 'tool_result'}
        <div class="rounded border border-gray-700 bg-elevated p-2 font-mono text-xs">
          <div class="mb-1 text-teal-400">↪ tool result</div>
          {#each block.content as part, j (j)}
            {#if part.type === 'text'}
              <pre class="whitespace-pre-wrap break-all text-gray-200">{part.text}</pre>
            {:else if part.type === 'image'}
              <img
                src={`data:${part.mime_type};base64,${part.data}`}
                alt="tool result"
                class="max-w-full rounded"
              />
            {:else if part.type === 'audio'}
              <audio controls src={`data:${part.mime_type};base64,${part.data}`}></audio>
            {/if}
          {/each}
        </div>
      {:else if block.type === 'document'}
        <div class="rounded border border-gray-700 bg-elevated px-2 py-1 text-xs text-teal-300">
          📄 document {block.document_id}
        </div>
      {:else if block.type === 'asset'}
        {#if block.mime_type.startsWith('image/')}
          <img src={blobUrl(block.blob_hash)} alt="asset" class="max-w-full rounded" />
        {:else if block.mime_type.startsWith('audio/')}
          <audio controls src={blobUrl(block.blob_hash)}></audio>
        {:else}
          <div class="text-xs text-muted">[asset: {block.mime_type}]</div>
        {/if}
      {/if}
    {/each}
  </div>
</div>
