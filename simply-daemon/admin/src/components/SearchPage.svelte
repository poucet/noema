<script lang="ts">
  import { onMount } from 'svelte';
  import { api, type SearchHit, type EmbeddingQueueStatus, type ReindexStatus } from '../lib/api';

  let query = $state('');
  let documentType = $state('');
  let topK = $state(10);
  let results = $state<SearchHit[]>([]);
  let searching = $state(false);
  let queueStatus = $state<EmbeddingQueueStatus | null>(null);
  let reindexResult = $state<ReindexStatus | null>(null);
  let error = $state('');

  onMount(() => {
    refreshStatus();
    const interval = setInterval(refreshStatus, 5000);
    return () => clearInterval(interval);
  });

  async function refreshStatus() {
    try {
      queueStatus = await api.getQueueStatus();
    } catch (e) {
      console.error('Failed to get queue status:', e);
    }
  }

  async function doSearch() {
    if (!query.trim()) return;
    searching = true;
    error = '';
    try {
      results = await api.search(query, documentType || undefined, topK);
    } catch (e) {
      error = String(e);
      results = [];
    }
    searching = false;
  }

  async function doReindex() {
    if (!confirm('Re-embed all documents? This will delete existing vectors and re-index everything.')) return;
    try {
      reindexResult = await api.reindex();
      await refreshStatus();
    } catch (e) {
      error = String(e);
    }
  }

  function scoreColor(score: number): string {
    if (score > 0.8) return 'text-green-400';
    if (score > 0.5) return 'text-yellow-400';
    return 'text-muted';
  }
</script>

<div class="flex flex-col h-[calc(100vh-3rem)]">
  <!-- Header: search form + status -->
  <div class="border-b border-border bg-surface p-4">
    <div class="flex gap-4 items-end max-w-4xl">
      <div class="flex-1">
        <label class="block text-xs text-muted mb-1">Query</label>
        <form onsubmit={(e) => { e.preventDefault(); doSearch(); }}>
          <input
            type="text"
            bind:value={query}
            placeholder="Search documents..."
            class="w-full px-3 py-2 text-sm bg-bg border border-border rounded text-fg placeholder:text-muted focus:outline-none focus:border-accent"
          />
        </form>
      </div>
      <div class="w-40">
        <label class="block text-xs text-muted mb-1">Type filter</label>
        <select
          bind:value={documentType}
          class="w-full px-3 py-2 text-sm bg-bg border border-border rounded text-fg focus:outline-none focus:border-accent"
        >
          <option value="">All types</option>
          <option value="document">document</option>
          <option value="note">note</option>
          <option value="todo">todo</option>
          <option value="knowledge">knowledge</option>
          <option value="context">context</option>
          <option value="system_prompt">system_prompt</option>
        </select>
      </div>
      <div class="w-20">
        <label class="block text-xs text-muted mb-1">Top K</label>
        <input
          type="number"
          bind:value={topK}
          min="1"
          max="50"
          class="w-full px-3 py-2 text-sm bg-bg border border-border rounded text-fg focus:outline-none focus:border-accent"
        />
      </div>
      <button
        onclick={doSearch}
        disabled={searching || !query.trim()}
        class="px-4 py-2 text-sm bg-accent text-white rounded hover:opacity-90 disabled:opacity-50"
      >
        {searching ? 'Searching...' : 'Search'}
      </button>
    </div>

    <!-- Queue status bar -->
    <div class="flex items-center gap-6 mt-3 text-xs text-muted max-w-4xl">
      {#if queueStatus}
        <span>Pending: <span class="text-fg">{queueStatus.pending}</span></span>
        <span>Processing: <span class="text-fg">{queueStatus.processing}</span></span>
        <span>Completed: <span class="text-fg">{queueStatus.completed}</span></span>
        {#if queueStatus.failed > 0}
          <span>Failed: <span class="text-danger">{queueStatus.failed}</span></span>
        {/if}
      {/if}
      <div class="flex-1"></div>
      <button
        onclick={doReindex}
        class="text-xs text-muted hover:text-accent transition-colors"
      >
        Reindex all
      </button>
    </div>

    {#if reindexResult}
      <div class="mt-2 text-xs text-accent">{reindexResult.message}</div>
    {/if}
  </div>

  <!-- Results -->
  <div class="flex-1 overflow-y-auto p-4">
    {#if error}
      <div class="text-sm text-danger mb-4">{error}</div>
    {/if}

    {#if results.length === 0 && !searching}
      <div class="text-sm text-muted italic">
        {query ? 'No results found.' : 'Enter a query to search embedded documents.'}
      </div>
    {:else}
      <div class="space-y-3 max-w-4xl">
        {#each results as hit, i}
          <div class="border border-border rounded bg-surface p-4">
            <div class="flex items-center gap-2 mb-2">
              <span class="text-xs text-muted">#{i + 1}</span>
              <span class="font-medium text-sm text-fg">{hit.document_title}</span>
              <span class="text-xs px-1.5 py-0.5 bg-elevated rounded text-muted">{hit.document_type}</span>
              <div class="flex-1"></div>
              <span class="text-xs {scoreColor(hit.score)}">{(hit.score * 100).toFixed(1)}%</span>
            </div>
            <pre class="text-xs text-muted whitespace-pre-wrap bg-bg rounded p-3 max-h-48 overflow-y-auto">{hit.chunk_text}</pre>
            <div class="text-xs text-muted mt-2">
              chunk {hit.chunk_index} &middot; doc {hit.document_id.slice(0, 8)}... &middot; tab {hit.tab_id.slice(0, 8)}...
            </div>
          </div>
        {/each}
      </div>
    {/if}
  </div>
</div>
