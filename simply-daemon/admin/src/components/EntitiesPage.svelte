<script lang="ts">
  import { onMount, onDestroy, tick } from 'svelte';
  import { api } from '../lib/api';
  import type { EntitySummary, EntityContent, ChildEntity } from '@simply/client';
  import DocumentEditor from './DocumentEditor.svelte';
  import MarkdownView from './MarkdownView.svelte';

  // Default kind for the "+ New" action. Other kinds (tabbed docs, todos,
  // prompts, ...) are produced by import skills or future kind-specific flows;
  // the admin UI deliberately doesn't make users pick one up front.
  const DEFAULT_NEW_KIND = 'document::note';

  const TYPE_FILTERS = [
    { id: '', label: 'All entities' },
    { id: 'document::', label: 'Documents' },
  ] as const;

  function autofocus(node: HTMLElement) {
    tick().then(() => node.focus());
  }

  // ── State ───────────────────────────────────────────────────────────────
  let entities = $state<EntitySummary[]>([]);
  let searchQuery = $state('');
  let typeFilter = $state<string>('document::');

  let selectedEntity = $state<EntitySummary | null>(null);
  // For tabbed docs: the tab tree we've loaded for the currently selected doc.
  let selectedTabs = $state<ChildEntity[]>([]);
  // The entity we're actually viewing content for — self for flat docs, or a tab.
  let viewEntity = $state<EntitySummary | null>(null);
  let viewContent = $state<EntityContent | null>(null);

  let loading = $state(true);
  let creatingEntity = $state(false);
  let newEntityTitle = $state('');
  let creatingTab = $state(false);
  let newTabTitle = $state('');

  let viewMode = $state<'render' | 'edit'>('render');

  // ── Lifecycle ──────────────────────────────────────────────────────────
  let handleUnload: (() => void) | null = null;

  onMount(() => {
    loadEntities();
    handleUnload = () => flushCurrent();
    window.addEventListener('beforeunload', handleUnload);
  });

  onDestroy(() => {
    if (typeof window !== 'undefined' && handleUnload) {
      window.removeEventListener('beforeunload', handleUnload);
    }
    flushCurrent();
  });

  function flushCurrent() {
    if (viewEntity) {
      api.flushEntityEmbedding(viewEntity.id).catch(() => {});
    }
  }

  // ── Loading ────────────────────────────────────────────────────────────
  async function loadEntities() {
    loading = true;
    try {
      const prefix = typeFilter || undefined;
      entities = searchQuery
        ? await api.searchEntities(searchQuery, prefix)
        : await api.listEntities(prefix);
    } catch (e) {
      console.error('Failed to load entities:', e);
    }
    loading = false;
  }

  async function selectEntity(e: EntitySummary) {
    flushCurrent();
    selectedEntity = e;
    selectedTabs = [];
    viewEntity = null;
    viewContent = null;
    creatingTab = false;
    viewMode = 'render';

    const hasChildren = (e.childCounts['structure::contained_in'] ?? 0) > 0;

    if (hasChildren) {
      try {
        selectedTabs = await api.listChildren(e.id, 'structure::contained_in');
        if (selectedTabs.length > 0) {
          await openChild(selectedTabs[0].summary);
        }
      } catch (err) {
        console.error('Failed to load tabs:', err);
      }
    } else if (e.hasContent) {
      viewEntity = e;
      await loadContent(e.id);
    } else {
      // No content and no tabs — show metadata only. Opening edit starts fresh.
      viewEntity = e;
    }
  }

  async function openChild(child: EntitySummary) {
    flushCurrent();
    viewEntity = child;
    viewContent = null;
    viewMode = 'render';
    await loadContent(child.id);
  }

  async function loadContent(id: string) {
    try {
      viewContent = await api.getEntityContent(id);
    } catch (e) {
      console.error('Failed to load content:', e);
      viewContent = null;
    }
  }

  // ── Mutations ──────────────────────────────────────────────────────────
  async function createEntity() {
    if (!newEntityTitle.trim()) return;
    try {
      const created = await api.createEntity({
        kind: DEFAULT_NEW_KIND,
        title: newEntityTitle.trim(),
        content: '',
        origin: null,
        referencedAssets: [],
      });
      newEntityTitle = '';
      creatingEntity = false;
      await loadEntities();
      await selectEntity(created);
    } catch (e) {
      console.error('Failed to create entity:', e);
    }
  }

  async function deleteEntity(id: string) {
    if (!confirm('Delete this entity and all its children?')) return;
    try {
      await api.deleteEntity(id);
      if (selectedEntity?.id === id) {
        selectedEntity = null;
        selectedTabs = [];
        viewEntity = null;
        viewContent = null;
      }
      await loadEntities();
    } catch (e) {
      console.error('Failed to delete entity:', e);
    }
  }

  async function saveContent(content: string) {
    if (!viewEntity) return;
    try {
      await api.updateEntityContent(viewEntity.id, {
        content,
        referencedAssets: viewContent?.referencedAssets ?? [],
      });
      // Local update so the editor doesn't blink.
      if (viewContent) viewContent = { ...viewContent, contentMarkdown: content };
    } catch (e) {
      console.error('Failed to save content:', e);
    }
  }

  async function createTab() {
    if (!selectedEntity || !newTabTitle.trim()) return;
    try {
      const tab = await api.createEntity({
        kind: 'document::tab',
        title: newTabTitle.trim(),
        content: '',
        origin: null,
        referencedAssets: [],
      });
      await api.addChild({
        parentId: selectedEntity.id,
        childId: tab.id,
        relation: 'structure::contained_in',
        position: selectedTabs.length,
      });
      newTabTitle = '';
      creatingTab = false;
      selectedTabs = await api.listChildren(selectedEntity.id, 'structure::contained_in');
      await openChild(tab);
    } catch (e) {
      console.error('Failed to create tab:', e);
    }
  }

  async function deleteTab(tabId: string) {
    if (!selectedEntity) return;
    if (!confirm('Delete this tab?')) return;
    try {
      await api.deleteEntity(tabId);
      selectedTabs = await api.listChildren(selectedEntity.id, 'structure::contained_in');
      if (viewEntity?.id === tabId) {
        viewEntity = selectedTabs[0]?.summary ?? null;
        viewContent = null;
        if (viewEntity) await loadContent(viewEntity.id);
      }
    } catch (e) {
      console.error('Failed to delete tab:', e);
    }
  }

  // ── Filters / search ───────────────────────────────────────────────────
  let searchTimeout: ReturnType<typeof setTimeout>;
  function onSearchInput(e: Event) {
    searchQuery = (e.target as HTMLInputElement).value;
    clearTimeout(searchTimeout);
    searchTimeout = setTimeout(loadEntities, 300);
  }

  function setTypeFilter(id: string) {
    typeFilter = id;
    loadEntities();
  }

  // ── Helpers ────────────────────────────────────────────────────────────
  function displayKind(kind: string): string {
    if (kind.startsWith('document::')) return kind.slice('document::'.length);
    return kind;
  }

  function formatDate(ts: number): string {
    return new Date(ts).toLocaleDateString(undefined, {
      month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit',
    });
  }

  const isTabbedSelected = $derived(
    !!selectedEntity && selectedTabs.length > 0
  );

  const viewContentMarkdown = $derived(viewContent?.contentMarkdown ?? '');
</script>

<div class="flex h-[calc(100vh-3rem)]">
  <!-- Sidebar: entity list -->
  <div class="w-64 shrink-0 border-r border-border flex flex-col bg-surface">
    <div class="p-3 border-b border-border space-y-2">
      <input
        type="text"
        placeholder="Search entities..."
        value={searchQuery}
        oninput={onSearchInput}
        class="w-full px-2.5 py-1.5 text-sm bg-bg border border-border rounded text-fg placeholder:text-muted focus:outline-none focus:border-accent"
      />
      <div class="flex gap-1 text-xs">
        {#each TYPE_FILTERS as filter}
          <button
            class="px-2 py-0.5 rounded transition-colors"
            class:bg-accent={typeFilter === filter.id}
            class:text-bg={typeFilter === filter.id}
            class:text-muted={typeFilter !== filter.id}
            onclick={() => setTypeFilter(filter.id)}
          >{filter.label}</button>
        {/each}
      </div>
    </div>

    <div class="flex-1 overflow-y-auto">
      {#if loading}
        <p class="p-3 text-sm text-muted">Loading...</p>
      {:else if entities.length === 0}
        <p class="p-3 text-sm text-muted italic">No entities</p>
      {:else}
        {#each entities as entity}
          <div
            class="relative w-full text-left px-3 py-2 text-sm border-b border-border hover:bg-elevated transition-colors group cursor-pointer"
            class:bg-elevated={selectedEntity?.id === entity.id}
            role="button"
            tabindex="0"
            onclick={() => selectEntity(entity)}
            onkeydown={(e) => { if (e.key === 'Enter') selectEntity(entity); }}
          >
            <div class="font-medium text-fg truncate pr-5">{entity.title ?? '(untitled)'}</div>
            <div class="text-xs text-muted flex justify-between">
              <span class="uppercase tracking-wide">{displayKind(entity.kind)}</span>
              <span>{formatDate(entity.updatedAt)}</span>
            </div>
            <div class="text-[11px] text-muted/70 truncate">
              {entity.ownerEmail ?? (entity.userId ? `anon · ${entity.userId.slice(0, 8)}` : 'no owner')}
            </div>
            <button
              class="absolute right-2 top-2 text-xs text-muted hover:text-danger opacity-0 group-hover:opacity-100"
              onclick={(e) => { e.stopPropagation(); deleteEntity(entity.id); }}
            >
              &times;
            </button>
          </div>
        {/each}
      {/if}
    </div>

    <div class="p-3 border-t border-border space-y-2">
      {#if creatingEntity}
        <form onsubmit={(e) => { e.preventDefault(); createEntity(); }}>
          <input
            type="text"
            bind:value={newEntityTitle}
            placeholder="Note title — Enter to create"
            use:autofocus
            onkeydown={(e) => { if (e.key === 'Escape') creatingEntity = false; }}
            class="w-full px-2.5 py-1.5 text-sm bg-bg border border-border rounded text-fg placeholder:text-muted focus:outline-none focus:border-accent"
          />
        </form>
      {:else}
        <button
          class="w-full text-sm px-2.5 py-1.5 border border-dashed border-border rounded text-muted hover:text-accent hover:border-accent transition-colors"
          onclick={() => creatingEntity = true}
        >
          + New note
        </button>
      {/if}
    </div>
  </div>

  {#if selectedEntity}
    {#if isTabbedSelected}
      <!-- Tab sidebar for tabbed docs -->
      <div class="w-56 shrink-0 border-r border-border flex flex-col bg-surface">
        <div class="p-2 border-b border-border flex items-center justify-between">
          <span class="text-xs font-medium text-muted truncate">{selectedEntity.title ?? '(untitled)'}</span>
          {#if creatingTab}
            <form class="flex-1 ml-2" onsubmit={(e) => { e.preventDefault(); createTab(); }}>
              <input
                type="text"
                bind:value={newTabTitle}
                placeholder="Tab name"
                use:autofocus
                onkeydown={(e) => { if (e.key === 'Escape') creatingTab = false; }}
                class="w-full px-1.5 py-0.5 text-xs bg-bg border border-border rounded text-fg placeholder:text-muted focus:outline-none focus:border-accent"
              />
            </form>
          {:else}
            <button
              class="text-xs text-muted hover:text-accent px-1"
              onclick={() => creatingTab = true}
            >+</button>
          {/if}
        </div>

        <div class="flex-1 overflow-y-auto">
          {#each selectedTabs as child (child.summary.id)}
            {@const isActive = viewEntity?.id === child.summary.id}
            <button
              class="w-full text-left py-1.5 pr-2 text-xs hover:bg-elevated transition-colors group flex items-center gap-1 pl-2
                     {isActive ? 'bg-elevated text-fg' : 'text-muted'}"
              onclick={() => openChild(child.summary)}
            >
              <span class="shrink-0">📄</span>
              <span class="truncate flex-1">{child.summary.title ?? '(untitled)'}</span>
              {#if selectedTabs.length > 1}
                <span
                  class="text-muted hover:text-danger opacity-0 group-hover:opacity-100 shrink-0"
                  onclick={(e) => { e.stopPropagation(); deleteTab(child.summary.id); }}
                >&times;</span>
              {/if}
            </button>
          {/each}
        </div>
      </div>
    {/if}

    <!-- Editor / Rendered view -->
    <div class="flex-1 flex flex-col min-w-0">
      {#if viewEntity}
        <div class="flex items-center justify-between gap-2 px-3 py-1.5 border-b border-border text-xs">
          <span class="text-muted">
            {displayKind(viewEntity.kind)}
            {#if viewEntity.origin}
              · <span class="text-muted/60">{viewEntity.origin}</span>
            {/if}
          </span>
          <div class="flex items-center gap-1">
            <button
              class="px-2 py-0.5 rounded transition-colors"
              class:bg-accent={viewMode === 'render'}
              class:text-bg={viewMode === 'render'}
              class:text-muted={viewMode !== 'render'}
              onclick={() => viewMode = 'render'}
            >Rendered</button>
            <button
              class="px-2 py-0.5 rounded transition-colors"
              class:bg-accent={viewMode === 'edit'}
              class:text-bg={viewMode === 'edit'}
              class:text-muted={viewMode !== 'edit'}
              onclick={() => viewMode = 'edit'}
            >Edit</button>
          </div>
        </div>
        {#if viewMode === 'edit'}
          {#key viewEntity.id}
            <DocumentEditor content={viewContentMarkdown} onsave={saveContent} />
          {/key}
        {:else}
          <div class="flex-1 overflow-auto p-6">
            <MarkdownView content={viewContentMarkdown} />
          </div>
        {/if}
      {:else if isTabbedSelected}
        <div class="flex items-center justify-center h-full text-muted text-sm">
          No tabs yet — click + to create one
        </div>
      {/if}
    </div>
  {:else}
    <div class="flex-1 flex items-center justify-center text-muted text-sm">
      Select an entity or create a new one
    </div>
  {/if}
</div>
