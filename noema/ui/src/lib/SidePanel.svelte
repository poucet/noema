<script lang="ts">
  import type { ActivityId } from './ActivityBar.svelte';

  type Props = { active: ActivityId };
  const { active }: Props = $props();
</script>

<div class="flex h-full w-64 flex-col border-r border-gray-700 bg-surface">
  {#if active === 'conversations'}
    <div class="border-b border-gray-700 p-4">
      <button
        class="w-full rounded-lg bg-teal-600 px-4 py-2 font-medium text-white transition-colors hover:bg-teal-700"
      >
        New conversation
      </button>
    </div>
    <div class="flex-1 overflow-y-auto">
      <p class="p-4 text-center text-sm text-muted">No conversations yet.</p>
    </div>
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
