<!--
  Shared entity list sidebar. Admin uses this with showOwner=true to surface
  the owning user; Noema uses showOwner=false since it's always scoped to the
  current user.
-->
<script lang="ts">
  import type { EntitySummary } from '@simply/client';

  export type TypeFilter = { id: string; label: string };

  type Props = {
    entities: EntitySummary[];
    selectedId: string | null;
    loading: boolean;
    typeFilters?: TypeFilter[];
    activeFilter: string;
    showOwner?: boolean;
    searchQuery: string;
    creating: boolean;
    newEntityTitle: string;
    onsearch: (q: string) => void;
    onfilter: (id: string) => void;
    onselect: (id: string) => void;
    ondelete: (id: string) => void;
    onstartCreate: () => void;
    oncancelCreate: () => void;
    onsubmitCreate: (title: string) => void;
    ontitleInput: (value: string) => void;
  };

  let {
    entities,
    selectedId,
    loading,
    typeFilters = [
      { id: '', label: 'All' },
      { id: 'document::', label: 'Documents' },
    ],
    activeFilter,
    showOwner = false,
    searchQuery,
    creating,
    newEntityTitle,
    onsearch,
    onfilter,
    onselect,
    ondelete,
    onstartCreate,
    oncancelCreate,
    onsubmitCreate,
    ontitleInput,
  }: Props = $props();

  function autofocus(node: HTMLInputElement) {
    queueMicrotask(() => node.focus());
  }

  let searchTimeout: ReturnType<typeof setTimeout>;
  function onSearchInput(e: Event) {
    const v = (e.target as HTMLInputElement).value;
    clearTimeout(searchTimeout);
    searchTimeout = setTimeout(() => onsearch(v), 300);
  }

  function formatDate(ts: number): string {
    return new Date(ts).toLocaleDateString(undefined, {
      month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit',
    });
  }
</script>

<div class="w-64 shrink-0 border-r border-border flex flex-col bg-surface h-full min-h-0">
  <div class="p-3 border-b border-border space-y-2">
    <input
      type="text"
      placeholder="Search..."
      value={searchQuery}
      oninput={onSearchInput}
      class="w-full px-2.5 py-1.5 text-sm bg-bg border border-border rounded text-fg placeholder:text-muted focus:outline-none focus:border-accent"
    />
    {#if typeFilters.length > 1}
      <div class="flex gap-1 text-xs">
        {#each typeFilters as filter}
          <button
            class="px-2 py-0.5 rounded transition-colors"
            class:bg-accent={activeFilter === filter.id}
            class:text-bg={activeFilter === filter.id}
            class:text-muted={activeFilter !== filter.id}
            onclick={() => onfilter(filter.id)}
          >{filter.label}</button>
        {/each}
      </div>
    {/if}
  </div>

  <div class="flex-1 overflow-y-auto">
    {#if loading}
      <p class="p-3 text-sm text-muted">Loading...</p>
    {:else if entities.length === 0}
      <p class="p-3 text-sm text-muted italic">No entities</p>
    {:else}
      {#each entities as entity (entity.id)}
        <div
          class="w-full text-left px-3 py-2 text-sm border-b border-border hover:bg-elevated transition-colors group cursor-pointer"
          class:bg-elevated={selectedId === entity.id}
          role="button"
          tabindex="0"
          onclick={() => onselect(entity.id)}
          onkeydown={(e) => { if (e.key === 'Enter') onselect(entity.id); }}
        >
          <div class="flex items-center gap-2">
            <div class="font-medium text-fg truncate flex-1 min-w-0">{entity.title ?? '(untitled)'}</div>
            <button
              class="shrink-0 p-1 -m-1 rounded text-muted hover:text-danger hover:bg-danger/10 opacity-0 group-hover:opacity-100 transition"
              onclick={(e) => { e.stopPropagation(); ondelete(entity.id); }}
              aria-label="Delete"
            >
              <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="3 6 5 6 21 6"/><path d="M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6"/><path d="M10 11v6"/><path d="M14 11v6"/><path d="M9 6V4a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1v2"/></svg>
            </button>
          </div>
          <div class="text-xs text-muted">{formatDate(entity.updatedAt)}</div>
          {#if showOwner}
            <div class="text-[11px] text-muted/70 truncate">
              {entity.ownerEmail ?? (entity.userId ? `anon · ${entity.userId.slice(0, 8)}` : 'no owner')}
            </div>
          {/if}
        </div>
      {/each}
    {/if}
  </div>

  <div class="p-3 border-t border-border">
    {#if creating}
      <form
        onsubmit={(e) => {
          e.preventDefault();
          const t = newEntityTitle.trim();
          if (t) onsubmitCreate(t);
        }}
      >
        <input
          type="text"
          value={newEntityTitle}
          oninput={(e) => ontitleInput((e.target as HTMLInputElement).value)}
          placeholder="Note title — Enter to create"
          use:autofocus
          onkeydown={(e) => { if (e.key === 'Escape') oncancelCreate(); }}
          class="w-full px-2.5 py-1.5 text-sm bg-bg border border-border rounded text-fg placeholder:text-muted focus:outline-none focus:border-accent"
        />
      </form>
    {:else}
      <button
        class="w-full text-sm px-2.5 py-1.5 border border-dashed border-border rounded text-muted hover:text-accent hover:border-accent transition-colors"
        onclick={onstartCreate}
      >
        + New note
      </button>
    {/if}
  </div>
</div>
