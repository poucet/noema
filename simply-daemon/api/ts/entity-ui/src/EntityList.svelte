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

  function displayKind(kind: string): string {
    return kind.startsWith('document::') ? kind.slice('document::'.length) : kind;
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
          class="relative w-full text-left px-3 py-2 text-sm border-b border-border hover:bg-elevated transition-colors group cursor-pointer"
          class:bg-elevated={selectedId === entity.id}
          role="button"
          tabindex="0"
          onclick={() => onselect(entity.id)}
          onkeydown={(e) => { if (e.key === 'Enter') onselect(entity.id); }}
        >
          <div class="font-medium text-fg truncate pr-5">{entity.title ?? '(untitled)'}</div>
          <div class="text-xs text-muted flex justify-between">
            <span class="uppercase tracking-wide">{displayKind(entity.kind)}</span>
            <span>{formatDate(entity.updatedAt)}</span>
          </div>
          {#if showOwner}
            <div class="text-[11px] text-muted/70 truncate">
              {entity.ownerEmail ?? (entity.userId ? `anon · ${entity.userId.slice(0, 8)}` : 'no owner')}
            </div>
          {/if}
          <button
            class="absolute right-2 top-2 text-xs text-muted hover:text-danger opacity-0 group-hover:opacity-100"
            onclick={(e) => { e.stopPropagation(); ondelete(entity.id); }}
            aria-label="Delete"
          >
            &times;
          </button>
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
