<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { api, type DocumentInfo, type DocumentDetail, type TabInfo } from '../lib/api';
  import DocumentEditor from './DocumentEditor.svelte';

  function autofocus(node: HTMLElement) {
    tick().then(() => node.focus());
  }

  // State
  let documents = $state<DocumentInfo[]>([]);
  let searchQuery = $state('');
  let selectedDoc = $state<DocumentDetail | null>(null);
  let selectedTab = $state<TabInfo | null>(null);
  let loading = $state(true);
  let creatingDoc = $state(false);
  let creatingTab = $state(false);
  let newDocTitle = $state('');
  let newTabTitle = $state('');

  onMount(() => loadDocuments());

  async function loadDocuments() {
    loading = true;
    try {
      documents = searchQuery
        ? await api.searchDocuments(searchQuery)
        : await api.listDocuments();
    } catch (e) {
      console.error('Failed to load documents:', e);
    }
    loading = false;
  }

  async function selectDocument(id: string) {
    try {
      const doc = await api.getDocument(id);
      selectedDoc = doc;
      selectedTab = doc.tabs.length > 0 ? doc.tabs[0] : null;
    } catch (e) {
      console.error('Failed to load document:', e);
    }
  }

  async function reloadDocument() {
    if (!selectedDoc) return;
    const doc = await api.getDocument(selectedDoc.id);
    const prevTabId = selectedTab?.id;
    selectedDoc = doc;
    selectedTab = doc.tabs.find(t => t.id === prevTabId) ?? doc.tabs[0] ?? null;
  }

  async function createDocument() {
    if (!newDocTitle.trim()) return;
    try {
      // Pass empty string as content so backend creates an initial tab
      const doc = await api.createDocument(newDocTitle.trim(), '');
      newDocTitle = '';
      creatingDoc = false;
      await loadDocuments();
      await selectDocument(doc.id);
    } catch (e) {
      console.error('Failed to create document:', e);
    }
  }

  async function deleteDocument(id: string) {
    if (!confirm('Delete this document?')) return;
    try {
      await api.deleteDocument(id);
      if (selectedDoc?.id === id) {
        selectedDoc = null;
        selectedTab = null;
      }
      await loadDocuments();
    } catch (e) {
      console.error('Failed to delete document:', e);
    }
  }

  async function saveTab(content: string) {
    if (!selectedTab) return;
    try {
      await api.updateTab(selectedTab.id, content);
    } catch (e) {
      console.error('Failed to save tab:', e);
    }
  }

  async function createTab() {
    if (!selectedDoc || !newTabTitle.trim()) return;
    try {
      const tab = await api.createTab(selectedDoc.id, newTabTitle.trim());
      newTabTitle = '';
      creatingTab = false;
      await reloadDocument();
      // Select the newly created tab
      selectedTab = selectedDoc!.tabs.find(t => t.id === tab.id) ?? selectedTab;
    } catch (e) {
      console.error('Failed to create tab:', e);
    }
  }

  async function deleteTab(tabId: string) {
    if (!selectedDoc) return;
    if (!confirm('Delete this tab?')) return;
    try {
      await api.deleteTab(tabId);
      await reloadDocument();
    } catch (e) {
      console.error('Failed to delete tab:', e);
    }
  }

  function formatDate(ts: number): string {
    return new Date(ts * 1000).toLocaleDateString(undefined, {
      month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit',
    });
  }

  let searchTimeout: ReturnType<typeof setTimeout>;
  function onSearchInput(e: Event) {
    searchQuery = (e.target as HTMLInputElement).value;
    clearTimeout(searchTimeout);
    searchTimeout = setTimeout(loadDocuments, 300);
  }
</script>

<div class="flex h-[calc(100vh-3rem)]">
  <!-- Sidebar: document list -->
  <div class="w-64 shrink-0 border-r border-border flex flex-col bg-surface">
    <div class="p-3 border-b border-border">
      <input
        type="text"
        placeholder="Search documents..."
        value={searchQuery}
        oninput={onSearchInput}
        class="w-full px-2.5 py-1.5 text-sm bg-bg border border-border rounded text-fg placeholder:text-muted focus:outline-none focus:border-accent"
      />
    </div>

    <div class="flex-1 overflow-y-auto">
      {#if loading}
        <p class="p-3 text-sm text-muted">Loading...</p>
      {:else if documents.length === 0}
        <p class="p-3 text-sm text-muted italic">No documents</p>
      {:else}
        {#each documents as doc}
          <div
            class="relative w-full text-left px-3 py-2 text-sm border-b border-border hover:bg-elevated transition-colors group cursor-pointer"
            class:bg-elevated={selectedDoc?.id === doc.id}
            role="button"
            tabindex="0"
            onclick={() => selectDocument(doc.id)}
            onkeydown={(e) => { if (e.key === 'Enter') selectDocument(doc.id); }}
          >
            <div class="font-medium text-fg truncate pr-5">{doc.title}</div>
            <div class="text-xs text-muted flex justify-between">
              <span>{doc.tab_count} tab{doc.tab_count !== 1 ? 's' : ''}</span>
              <span>{formatDate(doc.updated_at)}</span>
            </div>
            <button
              class="absolute right-2 top-2 text-xs text-muted hover:text-danger opacity-0 group-hover:opacity-100"
              onclick={(e) => { e.stopPropagation(); deleteDocument(doc.id); }}
            >
              &times;
            </button>
          </div>
        {/each}
      {/if}
    </div>

    <div class="p-3 border-t border-border">
      {#if creatingDoc}
        <form onsubmit={(e) => { e.preventDefault(); createDocument(); }}>
          <!-- svelte-ignore binding_property_non_reactive -->
          <input
            type="text"
            bind:value={newDocTitle}
            placeholder="Document name — Enter to create"
            use:autofocus
            onkeydown={(e) => { if (e.key === 'Escape') creatingDoc = false; }}
            class="w-full px-2.5 py-1.5 text-sm bg-bg border border-border rounded text-fg placeholder:text-muted focus:outline-none focus:border-accent"
          />
        </form>
      {:else}
        <button
          class="w-full text-sm px-2.5 py-1.5 border border-dashed border-border rounded text-muted hover:text-accent hover:border-accent transition-colors"
          onclick={() => creatingDoc = true}
        >
          + New document
        </button>
      {/if}
    </div>
  </div>

  <!-- Main: tabs + editor -->
  <div class="flex-1 flex flex-col min-w-0">
    {#if selectedDoc}
      <!-- Tab bar -->
      <div class="flex items-center border-b border-border bg-surface px-2 gap-0.5 shrink-0">
        {#each selectedDoc.tabs as tab (tab.id)}
          <button
            class="px-3 py-1.5 text-sm border-b-2 transition-colors group relative"
            class:border-accent={selectedTab?.id === tab.id}
            class:text-fg={selectedTab?.id === tab.id}
            class:border-transparent={selectedTab?.id !== tab.id}
            class:text-muted={selectedTab?.id !== tab.id}
            class:hover:text-fg={selectedTab?.id !== tab.id}
            onclick={() => { selectedTab = tab; }}
          >
            {tab.title}
            {#if selectedDoc.tabs.length > 1}
              <span
                class="ml-1.5 text-xs text-muted hover:text-danger opacity-0 group-hover:opacity-100"
                onclick={(e) => { e.stopPropagation(); deleteTab(tab.id); }}
              >&times;</span>
            {/if}
          </button>
        {/each}

        {#if creatingTab}
          <form class="flex items-center" onsubmit={(e) => { e.preventDefault(); createTab(); }}>
            <!-- svelte-ignore binding_property_non_reactive -->
            <input
              type="text"
              bind:value={newTabTitle}
              placeholder="Tab name"
              use:autofocus
              onkeydown={(e) => { if (e.key === 'Escape') creatingTab = false; }}
              class="px-2 py-1 text-sm bg-bg border border-border rounded text-fg placeholder:text-muted focus:outline-none focus:border-accent w-32"
            />
          </form>
        {:else}
          <button
            class="px-2 py-1.5 text-sm text-muted hover:text-accent"
            onclick={() => creatingTab = true}
          >+</button>
        {/if}

        <div class="flex-1"></div>
        <span class="text-xs text-muted pr-2">{selectedDoc.title}</span>
      </div>

      <!-- Editor -->
      <div class="flex-1 min-h-0">
        {#if selectedTab}
          {#key selectedTab.id}
            <DocumentEditor content={selectedTab.content_markdown ?? ''} onsave={saveTab} />
          {/key}
        {:else}
          <div class="flex items-center justify-center h-full text-muted text-sm">
            No tabs yet — click + to create one
          </div>
        {/if}
      </div>
    {:else}
      <div class="flex items-center justify-center h-full text-muted text-sm">
        Select a document or create a new one
      </div>
    {/if}
  </div>
</div>
