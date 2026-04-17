<script lang="ts">
  import { onMount } from 'svelte';
  import { getTransport } from '../lib/transport';
  import { mcpApi, oAuthApi } from '../lib/generated/api';
  import type { McpServerInfo, McpTool } from '../lib/generated/types';

  const t = getTransport();
  const mcp = mcpApi(t);
  const oauth = oAuthApi(t);

  let servers = $state<McpServerInfo[]>([]);
  let selectedTools = $state<McpTool[]>([]);
  let selectedServerId = $state<string | null>(null);
  let error = $state<string | null>(null);
  let loading = $state(false);

  // Add server form
  let showAdd = $state(false);
  let addName = $state('');
  let addUrl = $state('');
  let addAuthType = $state('none');

  onMount(() => refresh());

  async function refresh() {
    loading = true;
    try {
      servers = await mcp.listMcpServers();
    } catch (e) {
      console.error('[mcp]', e);
      error = `${e}`;
    }
    loading = false;
  }

  async function addServer() {
    if (!addName.trim() || !addUrl.trim()) return;
    try {
      await mcp.addMcpServer({
        id: addName.toLowerCase().replace(/\s+/g, '-'),
        name: addName,
        url: addUrl,
        authType: addAuthType,
        authToken: null,
        clientId: null,
        clientSecret: null,
        scopes: null,
      });
      addName = '';
      addUrl = '';
      addAuthType = 'none';
      showAdd = false;
      await refresh();
    } catch (e) {
      console.error('[mcp] add failed:', e);
      error = `Add failed: ${e}`;
    }
  }

  async function removeServer(id: string) {
    try {
      await mcp.removeMcpServer(id);
      if (selectedServerId === id) { selectedServerId = null; selectedTools = []; }
      await refresh();
    } catch (e) {
      console.error('[mcp] remove failed:', e);
      error = `Remove failed: ${e}`;
    }
  }

  async function connectServer(id: string) {
    try {
      await mcp.connectMcpServer(id);
      await refresh();
    } catch (e) {
      console.error('[mcp] connect failed:', e);
      error = `Connect failed: ${e}`;
    }
  }

  async function disconnectServer(id: string) {
    try {
      await mcp.disconnectMcpServer(id);
      await refresh();
    } catch (e) {
      console.error('[mcp] disconnect failed:', e);
      error = `Disconnect failed: ${e}`;
    }
  }

  async function startOauth(id: string) {
    try {
      const info = await oauth.startOauth(id);
      window.open(info.authUrl, '_blank');
      // Poll for completion
      const interval = setInterval(async () => {
        await refresh();
        const srv = servers.find(s => s.id === id);
        if (srv && !srv.needsOauthLogin) clearInterval(interval);
      }, 3000);
      setTimeout(() => clearInterval(interval), 120_000);
    } catch (e) {
      console.error('[mcp] oauth failed:', e);
      error = `OAuth failed: ${e}`;
    }
  }

  async function showTools(id: string) {
    if (selectedServerId === id) { selectedServerId = null; selectedTools = []; return; }
    try {
      selectedTools = await mcp.getMcpServerTools(id);
      selectedServerId = id;
    } catch (e) {
      console.error('[mcp] tools failed:', e);
      error = `Failed to load tools: ${e}`;
    }
  }

  function statusColor(s: McpServerInfo): string {
    if (s.isConnected) return 'text-green-400';
    if (s.status.startsWith('retrying')) return 'text-yellow-400';
    return 'text-muted/50';
  }

  function statusLabel(s: McpServerInfo): string {
    if (s.isConnected) return `connected (${s.toolCount} tools)`;
    if (s.needsOauthLogin) return 'needs OAuth login';
    return s.status;
  }
</script>

<div class="space-y-4">
  <div class="flex items-center justify-between">
    <h2 class="text-lg font-medium">MCP Servers</h2>
    <button
      class="text-xs px-3 py-1 rounded bg-accent/20 text-accent hover:bg-accent/30"
      onclick={() => showAdd = !showAdd}
    >{showAdd ? 'Cancel' : '+ Add Server'}</button>
  </div>

  {#if error}
    <div class="px-3 py-2 bg-red-900/30 text-red-300 text-xs rounded">
      {error}
      <button class="ml-2 underline" onclick={() => error = null}>dismiss</button>
    </div>
  {/if}

  {#if showAdd}
    <div class="border border-border rounded-lg p-4 space-y-3">
      <input
        class="w-full bg-white/5 border border-border rounded px-3 py-2 text-sm text-fg outline-none"
        placeholder="Server name"
        bind:value={addName}
      />
      <input
        class="w-full bg-white/5 border border-border rounded px-3 py-2 text-sm text-fg outline-none"
        placeholder="URL (e.g. https://mcp.example.com)"
        bind:value={addUrl}
      />
      <div class="flex gap-2 items-center">
        <select
          class="bg-white/5 border border-border rounded px-3 py-2 text-sm text-fg outline-none"
          bind:value={addAuthType}
        >
          <option value="none">No Auth</option>
          <option value="oauth">OAuth</option>
          <option value="bearer">Bearer Token</option>
        </select>
        <button
          class="px-4 py-2 bg-accent text-bg text-sm font-medium rounded hover:bg-accent/80"
          onclick={addServer}
        >Add</button>
      </div>
    </div>
  {/if}

  {#if loading && servers.length === 0}
    <div class="text-sm text-muted">Loading...</div>
  {/if}

  <div class="space-y-2">
    {#each servers as srv}
      <div class="border border-border rounded-lg p-3">
        <div class="flex items-center gap-3">
          <div class="flex-1 min-w-0">
            <div class="text-sm font-medium truncate">{srv.name}</div>
            <div class="text-xs text-muted/50 truncate">{srv.url}</div>
            <div class="text-xs {statusColor(srv)} mt-0.5">{statusLabel(srv)}</div>
          </div>
          <div class="flex gap-1 shrink-0">
            {#if srv.needsOauthLogin}
              <button
                class="text-xs px-2 py-1 rounded bg-accent/20 text-accent hover:bg-accent/30"
                onclick={() => startOauth(srv.id)}
              >Login</button>
            {:else if srv.isConnected}
              <button
                class="text-xs px-2 py-1 rounded bg-white/5 text-muted hover:text-fg"
                onclick={() => showTools(srv.id)}
              >{selectedServerId === srv.id ? 'Hide' : 'Tools'}</button>
              <button
                class="text-xs px-2 py-1 rounded bg-white/5 text-muted hover:text-fg"
                onclick={() => disconnectServer(srv.id)}
              >Disconnect</button>
            {:else}
              <button
                class="text-xs px-2 py-1 rounded bg-accent/20 text-accent hover:bg-accent/30"
                onclick={() => connectServer(srv.id)}
              >Connect</button>
            {/if}
            <button
              class="text-xs px-2 py-1 rounded bg-red-900/20 text-red-400 hover:bg-red-900/40"
              onclick={() => removeServer(srv.id)}
            >Remove</button>
          </div>
        </div>

        {#if selectedServerId === srv.id && selectedTools.length > 0}
          <div class="mt-3 border-t border-border/50 pt-2">
            <div class="text-xs text-muted mb-1">Tools ({selectedTools.length})</div>
            <div class="grid gap-1">
              {#each selectedTools as tool}
                <div class="text-xs p-2 bg-black/20 rounded">
                  <span class="text-accent font-mono">{tool.name}</span>
                  {#if tool.description}
                    <span class="text-muted/70 ml-2">{tool.description}</span>
                  {/if}
                </div>
              {/each}
            </div>
          </div>
        {/if}
      </div>
    {:else}
      {#if !loading}
        <div class="text-sm text-muted/50 text-center py-4">No MCP servers configured</div>
      {/if}
    {/each}
  </div>
</div>
