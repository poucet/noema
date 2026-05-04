<script lang="ts">
  import { getBaseUrl } from '@simply/client';
  import MarkdownView from './MarkdownView.svelte';
  import EntityRefChip from './EntityRefChip.svelte';
  import type { ChatDisplayMessage } from './chatContent';

  type Props = {
    message: ChatDisplayMessage;
    streaming?: boolean;
    wrapperClass?: string;
    baseBubbleClass?: string;
    userBubbleClass?: string;
    assistantBubbleClass?: string;
    systemBubbleClass?: string;
    streamingBubbleClass?: string;
    entityChipClass?: string;
  };

  let {
    message,
    streaming = false,
    wrapperClass = '',
    baseBubbleClass = 'max-w-[85%] space-y-2 rounded-lg px-4 py-3 text-sm',
    userBubbleClass = 'bg-teal-800 text-foreground',
    assistantBubbleClass = 'bg-surface text-foreground',
    systemBubbleClass = 'bg-elevated text-muted',
    streamingBubbleClass = 'border border-teal-500/40',
    entityChipClass = '',
  }: Props = $props();

  const alignClass = $derived(message.role === 'user' ? 'justify-end' : 'justify-start');
  const roleClass = $derived(
    message.role === 'user'
      ? userBubbleClass
      : message.role === 'system'
        ? systemBubbleClass
        : assistantBubbleClass,
  );
  const bubbleClass = $derived(`${baseBubbleClass} ${roleClass} ${streaming ? streamingBubbleClass : ''}`);

  function apiUrl(path: string): string {
    return `${getBaseUrl()}${path}`;
  }

  function blobUrl(hash: string): string {
    return apiUrl(`/api/blob/${hash}`);
  }

  function assetUrl(assetId: string): string {
    return apiUrl(`/api/asset/${assetId}`);
  }

  function formatArgs(value: unknown): string {
    if (value == null) return '';
    if (typeof value === 'string') return value;
    try {
      return JSON.stringify(value, null, 2);
    } catch {
      return String(value);
    }
  }

  function inputDataUrl(block: { data: string; mimeType?: string; mime_type?: string }): string {
    return `data:${block.mimeType ?? block.mime_type ?? 'application/octet-stream'};base64,${block.data}`;
  }
</script>

<div class="flex w-full {alignClass} {wrapperClass}">
  <div class={bubbleClass}>
    {#each message.content as block, i (i)}
      {#if block.type === 'text'}
        <MarkdownView content={block.text} />
      {:else if block.type === 'entityRef'}
        <EntityRefChip entityId={block.id} chipClass={entityChipClass} />
      {:else if block.type === 'entity'}
        <EntityRefChip entityId={block.entity_id} chipClass={entityChipClass} />
      {:else if block.type === 'entity_ref'}
        <EntityRefChip entityId={block.entity_id} chipClass={entityChipClass} />
      {:else if block.type === 'document'}
        <EntityRefChip entityId={block.document_id} chipClass={entityChipClass} />
      {:else if block.type === 'tool_call'}
        <div class="rounded border border-gray-700 bg-black/20 p-2 font-mono text-xs">
          <div class="mb-1 text-teal-400">Tool call: {block.name}</div>
          {#if block.arguments !== undefined && block.arguments !== null}
            <pre class="whitespace-pre-wrap break-all text-gray-300">{formatArgs(block.arguments)}</pre>
          {/if}
        </div>
      {:else if block.type === 'tool_result'}
        <div class="rounded border border-gray-700 bg-black/20 p-2 font-mono text-xs">
          <div class="mb-1 text-teal-400">Tool result</div>
          {#each block.content as part, j (j)}
            {#if part.type === 'text'}
              <pre class="whitespace-pre-wrap break-all text-gray-200">{part.text}</pre>
            {:else if part.type === 'image'}
              <img
                src={inputDataUrl(part)}
                alt="tool result"
                class="max-w-full rounded"
              />
            {:else if part.type === 'audio'}
              <audio controls src={inputDataUrl(part)}></audio>
            {/if}
          {/each}
        </div>
      {:else if block.type === 'asset'}
        {#if block.mime_type.startsWith('image/')}
          <img src={blobUrl(block.blob_hash)} alt="asset" class="max-w-full rounded" />
        {:else if block.mime_type.startsWith('audio/')}
          <audio controls src={blobUrl(block.blob_hash)}></audio>
        {:else}
          <div class="text-xs text-muted">[asset: {block.mime_type}]</div>
        {/if}
      {:else if block.type === 'assetRef'}
        {#if block.mimeType.startsWith('image/')}
          <img src={assetUrl(block.assetId)} alt="asset" class="max-w-full rounded" />
        {:else if block.mimeType.startsWith('audio/')}
          <audio controls src={assetUrl(block.assetId)}></audio>
        {:else}
          <div class="text-xs text-muted">[asset: {block.mimeType}]</div>
        {/if}
      {:else if block.type === 'asset_ref'}
        {#if block.mime_type.startsWith('image/')}
          <img src={assetUrl(block.asset_id)} alt="asset" class="max-w-full rounded" />
        {:else if block.mime_type.startsWith('audio/')}
          <audio controls src={assetUrl(block.asset_id)}></audio>
        {:else}
          <div class="text-xs text-muted">[asset: {block.mime_type}]</div>
        {/if}
      {:else if block.type === 'image'}
        <img
          src={inputDataUrl(block)}
          alt="uploaded"
          class="max-w-full rounded"
        />
      {:else if block.type === 'audio'}
        <audio controls src={inputDataUrl(block)}></audio>
      {:else}
        <div class="text-xs text-muted">Unknown content type</div>
      {/if}
    {/each}
  </div>
</div>
