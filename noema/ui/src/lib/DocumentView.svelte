<script lang="ts">
  import type { TabInfo } from '@simply/client';
  import { documentsStore } from './stores/documents.svelte';
  import MarkdownView from './MarkdownView.svelte';

  // Flattened tree view: nest tabs under `parentTabId`, render depth as
  // indent so the sidebar tree isn't mis-hierarchical for nested notes.
  interface TabNode {
    tab: TabInfo;
    children: TabNode[];
    depth: number;
  }

  function buildTabTree(tabs: TabInfo[]): TabNode[] {
    const byId = new Map<string, TabNode>();
    const roots: TabNode[] = [];
    for (const tab of tabs) byId.set(tab.id, { tab, children: [], depth: 0 });
    for (const tab of tabs) {
      const node = byId.get(tab.id)!;
      const parentId = tab.parentTabId;
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

  const tabTree = $derived(
    documentsStore.selected ? flattenTree(buildTabTree(documentsStore.selected.tabs)) : [],
  );
</script>

{#if !documentsStore.selected}
  <div class="flex flex-1 items-center justify-center">
    <p class="text-muted">Select a document or create a new one.</p>
  </div>
{:else}
  <div class="flex min-h-0 flex-1">
    <!-- Tab list -->
    <div class="flex w-56 shrink-0 flex-col border-r border-gray-700 bg-surface">
      <div class="border-b border-gray-700 px-3 py-2">
        <div class="truncate text-xs font-medium text-muted">
          {documentsStore.selected.title}
        </div>
      </div>
      <div class="flex-1 overflow-y-auto">
        {#if tabTree.length === 0}
          <p class="p-3 text-xs italic text-muted">No tabs</p>
        {:else}
          {#each tabTree as node (node.tab.id)}
            {@const isActive = documentsStore.selectedTab?.id === node.tab.id}
            <button
              class="flex w-full items-center gap-1 py-1.5 pr-2 text-left text-xs transition-colors {isActive
                ? 'bg-elevated text-foreground'
                : 'text-muted hover:bg-elevated'}"
              style="padding-left: {8 + node.depth * 16}px"
              onclick={() => documentsStore.setTab(node.tab)}
            >
              {#if node.tab.icon}
                <span class="shrink-0">{node.tab.icon}</span>
              {:else}
                <span class="shrink-0">{node.children.length > 0 ? '📁' : '📄'}</span>
              {/if}
              <span class="flex-1 truncate">{node.tab.title}</span>
            </button>
          {/each}
        {/if}
      </div>
    </div>

    <!-- Tab content -->
    <div class="flex min-w-0 flex-1 flex-col">
      {#if documentsStore.selectedTab}
        <div class="min-h-0 flex-1 overflow-auto p-6">
          <MarkdownView content={documentsStore.selectedTab.contentMarkdown ?? ''} />
        </div>
      {:else}
        <div class="flex flex-1 items-center justify-center">
          <p class="text-sm text-muted">This document has no tabs yet.</p>
        </div>
      {/if}
    </div>
  </div>
{/if}
