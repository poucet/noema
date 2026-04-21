<script lang="ts">
  import { documentsStore } from './stores/documents.svelte';

  let creating = $state(false);
  let newTitle = $state('');

  function formatDate(ts: number): string {
    return new Date(ts).toLocaleDateString(undefined, {
      month: 'short',
      day: 'numeric',
      hour: '2-digit',
      minute: '2-digit',
    });
  }

  async function submitCreate() {
    const title = newTitle.trim();
    if (!title) return;
    await documentsStore.create(title);
    newTitle = '';
    creating = false;
  }
</script>

<div class="border-b border-gray-700 p-3">
  <input
    type="text"
    placeholder="Search documents…"
    value={documentsStore.searchQuery}
    oninput={(e) => documentsStore.setSearch((e.target as HTMLInputElement).value)}
    class="w-full rounded border border-gray-700 bg-background px-2.5 py-1.5 text-sm text-foreground placeholder-muted focus:border-teal-500 focus:outline-none"
  />
</div>

<div class="flex-1 overflow-y-auto">
  {#if documentsStore.isLoading && documentsStore.documents.length === 0}
    <p class="p-4 text-center text-sm text-muted">Loading…</p>
  {:else if documentsStore.documents.length === 0}
    <p class="p-4 text-center text-sm text-muted">No documents yet</p>
  {:else}
    {#each documentsStore.documents as doc (doc.id)}
      {@const isSelected = documentsStore.selected?.id === doc.id}
      <div
        class="group relative border-b border-gray-700 transition-colors {isSelected
          ? 'bg-elevated'
          : 'hover:bg-elevated'}"
      >
        <button
          class="block w-full px-3 py-2 pr-6 text-left"
          onclick={() => documentsStore.select(doc.id)}
        >
          <div class="truncate text-sm font-medium text-foreground">{doc.title}</div>
          <div class="flex justify-between text-xs text-muted">
            <span>{doc.tabCount} tab{doc.tabCount === 1 ? '' : 's'}</span>
            <span>{formatDate(doc.updatedAt)}</span>
          </div>
        </button>
        <button
          class="absolute right-2 top-2 text-xs text-muted opacity-0 transition-opacity hover:text-red-500 group-hover:opacity-100"
          title="Delete"
          onclick={() => {
            if (confirm('Delete this document?')) documentsStore.remove(doc.id);
          }}
        >
          ×
        </button>
      </div>
    {/each}
  {/if}
</div>

<div class="border-t border-gray-700 p-3">
  {#if creating}
    <!-- svelte-ignore a11y_autofocus -->
    <input
      type="text"
      bind:value={newTitle}
      placeholder="Document name — Enter to create"
      autofocus
      onkeydown={(e) => {
        if (e.key === 'Enter') {
          e.preventDefault();
          submitCreate();
        } else if (e.key === 'Escape') {
          creating = false;
          newTitle = '';
        }
      }}
      onblur={() => {
        if (newTitle.trim()) submitCreate();
        else creating = false;
      }}
      class="w-full rounded border border-gray-700 bg-background px-2.5 py-1.5 text-sm text-foreground placeholder-muted focus:border-teal-500 focus:outline-none"
    />
  {:else}
    <button
      class="w-full rounded border border-dashed border-gray-700 px-2.5 py-1.5 text-sm text-muted transition-colors hover:border-teal-500 hover:text-teal-400"
      onclick={() => (creating = true)}
    >
      + New document
    </button>
  {/if}
</div>
