<script lang="ts">
  import { onMount, onDestroy, tick } from 'svelte';
  import { api } from '../lib/api';
  import { getTransport, type DocumentInfo, type DocumentDetail, type TabInfo } from '@simply/client';
  import DocumentEditor from './DocumentEditor.svelte';
  import MarkdownView from './MarkdownView.svelte';

  const transport = getTransport();

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
  let viewMode = $state<'render' | 'edit'>('render');

  function flushCurrentTab() {
    if (selectedTab) {
      transport.rpc('document.flush_tab_embedding', 'POST', `/api/document/tab/${selectedTab.id}/flush`).catch(() => {});
    }
  }

  let handleUnload: (() => void) | null = null;

  onMount(() => {
    loadDocuments();
    handleUnload = () => flushCurrentTab();
    window.addEventListener('beforeunload', handleUnload);
  });

  onDestroy(() => {
    if (typeof window !== 'undefined' && handleUnload) {
      window.removeEventListener('beforeunload', handleUnload);
    }
    flushCurrentTab();
  });

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
    flushCurrentTab();
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

  function getTabCount(doc: any): number {
    return doc.tabCount ?? doc.tab_count ?? 0;
  }

  function getUpdatedAt(doc: any): number {
    return doc.updatedAt ?? doc.updated_at ?? 0;
  }

  function formatDate(ts: number): string {
    // Timestamps are already in milliseconds
    return new Date(ts).toLocaleDateString(undefined, {
      month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit',
    });
  }

  let searchTimeout: ReturnType<typeof setTimeout>;
  function onSearchInput(e: Event) {
    searchQuery = (e.target as HTMLInputElement).value;
    clearTimeout(searchTimeout);
    searchTimeout = setTimeout(loadDocuments, 300);
  }

  // Build tree structure from flat tab list
  interface TabNode {
    tab: TabInfo;
    children: TabNode[];
    depth: number;
  }

  function buildTabTree(tabs: TabInfo[]): TabNode[] {
    const byId = new Map<string, TabNode>();
    const roots: TabNode[] = [];

    // Create nodes
    for (const tab of tabs) {
      byId.set(tab.id, { tab, children: [], depth: 0 });
    }

    // Build tree
    for (const tab of tabs) {
      const node = byId.get(tab.id)!;
      const parentId = (tab as any).parent_tab_id ?? (tab as any).parentTabId;
      if (parentId && byId.has(parentId)) {
        const parent = byId.get(parentId)!;
        node.depth = parent.depth + 1;
        parent.children.push(node);
      } else {
        roots.push(node);
      }
    }

    return roots;
  }

  function flattenTree(nodes: TabNode[]): TabNode[] {
    const result: TabNode[] = [];
    function walk(list: TabNode[]) {
      for (const node of list) {
        result.push(node);
        walk(node.children);
      }
    }
    walk(nodes);
    return result;
  }

  const tabTree = $derived(selectedDoc ? flattenTree(buildTabTree(selectedDoc.tabs)) : []);
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
              <span>{getTabCount(doc)} tab{getTabCount(doc) !== 1 ? 's' : ''}</span>
              <span>{formatDate(getUpdatedAt(doc))}</span>
            </div>
            <div class="text-[11px] text-muted/70 truncate">
              {doc.ownerEmail ?? `anon · ${doc.userId.slice(0, 8)}`}
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

  {#if selectedDoc}
    <!-- Tab sidebar (left of editor) -->
    <div class="w-56 shrink-0 border-r border-border flex flex-col bg-surface">
      <div class="p-2 border-b border-border flex items-center justify-between">
        <span class="text-xs font-medium text-muted truncate">{selectedDoc.title}</span>
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
        {#each tabTree as node}
          {@const isActive = selectedTab?.id === node.tab.id}
          <button
            class="w-full text-left py-1.5 pr-2 text-xs hover:bg-elevated transition-colors group flex items-center gap-1
                   {isActive ? 'bg-elevated text-fg' : 'text-muted'}"
            style="padding-left: {8 + node.depth * 16}px"
            onclick={() => { flushCurrentTab(); selectedTab = node.tab; }}
          >
            {#if node.tab.icon}
              <span class="shrink-0">{node.tab.icon}</span>
            {:else if !/^[\p{Emoji_Presentation}\p{Extended_Pictographic}]/u.test(node.tab.title)}
              <span class="shrink-0">{node.children.length > 0 ? '📁' : '📄'}</span>
            {/if}
            <span class="truncate flex-1">{node.tab.title}</span>
            {#if selectedDoc!.tabs.length > 1}
              <span
                class="text-muted hover:text-danger opacity-0 group-hover:opacity-100 shrink-0"
                onclick={(e) => { e.stopPropagation(); deleteTab(node.tab.id); }}
              >&times;</span>
            {/if}
          </button>
        {/each}
      </div>
    </div>

    <!-- Editor / Rendered view -->
    <div class="flex-1 flex flex-col min-w-0">
      {#if selectedTab}
        {@const tabContent = (selectedTab as any).contentMarkdown ?? (selectedTab as any).content_markdown ?? ''}
        <div class="flex items-center justify-end gap-1 px-3 py-1.5 border-b border-border text-xs">
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
        {#if viewMode === 'edit'}
          {#key selectedTab.id}
            <DocumentEditor content={tabContent} onsave={saveTab} />
          {/key}
        {:else}
          <div class="flex-1 overflow-auto p-6">
            <MarkdownView content={tabContent} />
          </div>
        {/if}
      {:else}
        <div class="flex items-center justify-center h-full text-muted text-sm">
          No tabs yet — click + to create one
        </div>
      {/if}
    </div>
  {:else}
    <div class="flex-1 flex items-center justify-center text-muted text-sm">
      Select a document or create a new one
    </div>
  {/if}
</div>
