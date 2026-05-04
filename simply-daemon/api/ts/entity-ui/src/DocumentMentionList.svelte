<script lang="ts">
  import type { EntitySummary } from '@simply/client';
  import type { DocumentMentionController } from './documentMentions.svelte';

  type Props = {
    mentions: DocumentMentionController;
    onSelect: (doc: EntitySummary) => void;
    panelClass?: string;
    itemClass?: string;
    selectedItemClass?: string;
    iconClass?: string;
  };

  let {
    mentions,
    onSelect,
    panelClass = 'absolute bottom-full left-0 right-0 z-20 mb-2 overflow-hidden rounded-lg border border-gray-700 bg-surface shadow-xl',
    itemClass = 'text-muted hover:bg-elevated hover:text-foreground',
    selectedItemClass = 'bg-elevated text-foreground',
    iconClass = 'text-teal-400',
  }: Props = $props();
</script>

<div class={panelClass}>
  {#if mentions.loading && mentions.matches.length === 0}
    <div class="px-3 py-2 text-sm text-muted">Loading...</div>
  {:else if mentions.matches.length === 0}
    <div class="px-3 py-2 text-sm text-muted">No matching documents</div>
  {:else}
    {#each mentions.matches as doc, i (doc.id)}
      <button
        type="button"
        class="flex w-full items-center gap-2 px-3 py-2 text-left text-sm transition-colors {i === mentions.selectedIndex ? selectedItemClass : itemClass}"
        onmouseenter={() => mentions.setSelectedIndex(i)}
        onmousedown={(e) => {
          e.preventDefault();
          onSelect(doc);
        }}
      >
        <svg class="h-4 w-4 shrink-0 {iconClass}" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12h6m-6 4h6m2 5H7a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5.586a1 1 0 0 1 .707.293l5.414 5.414a1 1 0 0 1 .293.707V19a2 2 0 0 1-2 2z" />
        </svg>
        <span class="min-w-0 flex-1">
          <span class="block truncate">{doc.title ?? '(untitled)'}</span>
          <span class="block truncate text-[11px] text-muted">{doc.kind}</span>
        </span>
      </button>
    {/each}
  {/if}
</div>
