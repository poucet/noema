<script lang="ts">
  import { onMount } from 'svelte';
  import { getTransport } from '../lib/transport';
  import { mcpApi, oAuthApi, documentApi } from '../lib/generated/api';
  import type { McpServerInfo, McpTool } from '../lib/generated/types';

  const t = getTransport();
  const mcp = mcpApi(t);
  const oauth = oAuthApi(t);
  const docs = documentApi(t);

  let gdocsServer = $state<McpServerInfo | null>(null);
  let serverTools = $state<McpTool[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let success = $state<string | null>(null);

  // Search / list
  let query = $state('');
  let results = $state<{ id: string; name: string; modified_time?: string }[]>([]);
  let searching = $state(false);

  // Import
  let importing = $state<string | null>(null);
  let importProgress = $state('');

  onMount(async () => {
    await findGdocsServer();
    loading = false;
    // Auto-list if connected
    if (gdocsServer?.isConnected) {
      await listDocs();
    }
  });

  async function findGdocsServer() {
    try {
      const servers = await mcp.listMcpServers();
      // Find by checking tools for gdocs_list, or by name pattern
      for (const s of servers) {
        if (s.name.toLowerCase().includes('gdoc') ||
            s.name.toLowerCase().includes('google') ||
            s.url.includes('gdocs')) {
          gdocsServer = s;
          if (s.isConnected) {
            try {
              serverTools = await mcp.getMcpServerTools(s.id);
            } catch { /* tools not available yet */ }
          }
          console.log('[gdocs] found server:', s.id, s.name, s.isConnected ? 'connected' : 'disconnected',
            'tools:', serverTools.map(t => t.name));
          return;
        }
      }
    } catch (e) {
      console.error('[gdocs] find server:', e);
    }
  }

  function findToolName(baseName: string): string | null {
    // Try exact match, then with underscores, then partial match
    const tool = serverTools.find(t =>
      t.name === baseName ||
      t.name === baseName.replace(/-/g, '_') ||
      t.name.includes(baseName.replace('gdocs_', ''))
    );
    return tool?.name ?? null;
  }

  async function reauth() {
    if (!gdocsServer) return;
    try {
      const info = await oauth.startOauth(gdocsServer.id);
      window.open(info.authUrl, '_blank');
      error = null;
      // Poll for completion
      const interval = setInterval(async () => {
        await findGdocsServer();
        if (gdocsServer?.isConnected) {
          clearInterval(interval);
          await listDocs();
        }
      }, 3000);
      setTimeout(() => clearInterval(interval), 120_000);
    } catch (e) {
      console.error('[gdocs] reauth:', e);
      error = `Re-auth failed: ${e}`;
    }
  }

  async function connectServer() {
    if (!gdocsServer) return;
    try {
      await mcp.connectMcpServer(gdocsServer.id);
      await findGdocsServer();
      if (gdocsServer?.isConnected) await listDocs();
    } catch (e) {
      console.error('[gdocs] connect:', e);
      error = `Connect failed: ${e}`;
    }
  }

  async function listDocs() {
    if (!gdocsServer?.isConnected) return;
    searching = true;
    error = null;

    const toolName = findToolName('gdocs_list') ?? 'gdocs_list';
    console.log('[gdocs] calling tool:', toolName, 'query:', query || '(none)');

    try {
      console.log('[gdocs] available tools on server:', serverTools.map(t => t.name));
      const result = await mcp.callToolDirect({
        name: toolName,
        arguments: query ? { query, limit: 20 } : { limit: 20 },
      });
      console.log('[gdocs] callToolDirect result:', result);

      const content = result.content as any[];
      const text = content?.[0]?.text ?? content?.[0]?.Text?.text ?? '';

      if (result.isError) {
        throw new Error(text || 'Tool returned an error');
      }

      if (text) {
        const parsed = JSON.parse(text);
        results = Array.isArray(parsed) ? parsed : [];
      } else {
        results = [];
      }
      console.log('[gdocs] results:', results.length);
    } catch (e) {
      console.error('[gdocs] list:', e);
      error = `List failed: ${e}`;
    }
    searching = false;
  }

  async function importDoc(docId: string, name: string) {
    if (!gdocsServer?.isConnected) return;
    importing = docId;
    error = null;
    success = null;

    const toolName = findToolName('gdocs_extract') ?? 'gdocs_extract';

    try {
      const result = await mcp.callToolDirect({
        name: toolName,
        arguments: { doc_id: docId },
      });

      const content = result.content as any[];
      if (result.isError || !content?.[0]?.text) {
        throw new Error(content?.[0]?.text || 'Extract failed');
      }

      const extracted = JSON.parse(content[0].text) as {
        doc_id: string;
        title: string;
        tabs: { title: string; content_markdown: string; tab_index: number }[];
        images: { object_id: string; data_base64: string; mime_type: string }[];
      };

      console.log('[gdocs] extracted:', extracted.title, extracted.tabs.length, 'tabs');

      const doc = await docs.createDocument({
        title: extracted.title,
        documentType: 'knowledge',
        content: null,
        sourceId: docId,
      });

      for (const tab of extracted.tabs) {
        await docs.createTab(doc.id, {
          title: tab.title,
          content: tab.content_markdown,
          parentTabId: null,
          tabIndex: tab.tab_index,
        });
      }

      success = `Imported "${extracted.title}" with ${extracted.tabs.length} tab(s)`;
    } catch (e) {
      console.error('[gdocs] import:', e);
      error = `Import failed: ${e}`;
    }
    importing = null;
  }
</script>

<div class="space-y-4">
  <h2 class="text-lg font-medium">Google Docs Import</h2>

  {#if error}
    <div class="px-3 py-2 bg-red-900/30 text-red-300 text-xs rounded">
      {error}
      <button class="ml-2 underline" onclick={() => error = null}>dismiss</button>
    </div>
  {/if}
  {#if success}
    <div class="px-3 py-2 bg-green-900/30 text-green-300 text-xs rounded">{success}</div>
  {/if}

  {#if loading}
    <div class="text-sm text-muted">Loading...</div>
  {:else if !gdocsServer}
    <div class="border border-border rounded-lg p-4 text-sm text-muted">
      <p>No Google Docs MCP server found.</p>
      <p class="mt-2 text-xs">Add one in <a href="/settings" class="text-accent hover:underline">Settings → MCP Servers</a>. The server name should contain "google" or "gdocs".</p>
    </div>
  {:else if !gdocsServer.isConnected}
    <div class="border border-border rounded-lg p-4 text-sm">
      <p class="text-muted">Google Docs server "{gdocsServer.name}" found but not connected.</p>
      {#if gdocsServer.needsOauthLogin}
        <p class="text-xs text-muted/50 mt-1">OAuth login required — connect from <a href="/settings" class="text-accent hover:underline">Settings</a>.</p>
      {:else}
        <button
          class="mt-2 text-xs px-3 py-1 rounded bg-accent/20 text-accent hover:bg-accent/30"
          onclick={connectServer}
        >Connect</button>
      {/if}
    </div>
  {:else}
    <!-- Re-auth link if needed -->
    {#if error?.includes('scope') || error?.includes('auth') || error?.includes('403') || error?.includes('Forbidden')}
      <div class="border border-yellow-800 bg-yellow-900/20 rounded-lg p-3 text-sm">
        <p class="text-yellow-300">OAuth token may have insufficient permissions.</p>
        <button
          class="mt-2 text-xs px-3 py-1 rounded bg-accent/20 text-accent hover:bg-accent/30"
          onclick={reauth}
        >Re-authenticate with Google</button>
      </div>
    {/if}

    <!-- Search bar -->
    <div class="flex gap-2">
      <input
        class="flex-1 bg-white/5 border border-border rounded px-3 py-2 text-sm text-fg outline-none placeholder:text-muted/50"
        placeholder="Filter docs…"
        bind:value={query}
        onkeydown={(e) => { if (e.key === 'Enter') listDocs(); }}
      />
      <button
        class="px-4 py-2 bg-white/5 border border-border text-sm text-muted rounded hover:text-fg disabled:opacity-30"
        disabled={searching}
        onclick={listDocs}
      >{searching ? 'Loading…' : 'Refresh'}</button>
    </div>

    <!-- Results -->
    {#if results.length > 0}
      <div class="border border-border rounded-lg overflow-hidden">
        {#each results as doc}
          <div class="flex items-center gap-3 px-3 py-2 border-b border-border/50 last:border-0 hover:bg-white/5">
            <div class="flex-1 min-w-0">
              <div class="text-sm truncate">{doc.name}</div>
              {#if doc.modified_time}
                <div class="text-xs text-muted/50">{new Date(doc.modified_time).toLocaleDateString()}</div>
              {/if}
            </div>
            <button
              class="text-xs px-3 py-1 rounded bg-accent/20 text-accent hover:bg-accent/30 shrink-0 disabled:opacity-30"
              disabled={importing === doc.id}
              onclick={() => importDoc(doc.id, doc.name)}
            >{importing === doc.id ? 'Importing…' : 'Import'}</button>
          </div>
        {/each}
      </div>
    {:else if !searching}
      <div class="text-xs text-muted/50 text-center py-4">
        No documents found. Make sure the Google Docs server is authenticated.
      </div>
    {/if}
  {/if}
</div>
