<script lang="ts">
  let method = $state('GET');
  let path = $state('/api/document');
  let body = $state('');
  let response = $state('');
  let status = $state('');
  let loading = $state(false);

  const presets = [
    { label: 'List documents', method: 'GET', path: '/api/document' },
    { label: 'List sessions', method: 'GET', path: '/api/session' },
    { label: 'List models', method: 'GET', path: '/api/model' },
    { label: 'Default model', method: 'GET', path: '/api/model/default' },
    { label: 'Setup status', method: 'GET', path: '/admin/api/setup-status' },
    { label: 'Settings', method: 'GET', path: '/admin/api/settings' },
    { label: 'Users', method: 'GET', path: '/admin/api/users' },
    { label: 'Connections', method: 'GET', path: '/admin/api/connections' },
    { label: 'Health', method: 'GET', path: '/api/daemon/health' },
    { label: 'Create document', method: 'POST', path: '/api/document', body: '{"title":"Test","content":"# Hello"}' },
    { label: 'Create user', method: 'POST', path: '/admin/api/users', body: '{"email":"test@example.com"}' },
  ];

  async function send() {
    loading = true;
    response = '';
    status = '';
    try {
      const init: RequestInit = { method };
      if (body.trim() && (method === 'POST' || method === 'PUT')) {
        init.headers = { 'Content-Type': 'application/json' };
        init.body = body;
      }
      const res = await fetch(path, init);
      status = `${res.status} ${res.statusText}`;
      const text = await res.text();
      try {
        response = JSON.stringify(JSON.parse(text), null, 2);
      } catch {
        response = text;
      }
    } catch (e) {
      status = 'Error';
      response = String(e);
    }
    loading = false;
  }

  function loadPreset(p: typeof presets[0]) {
    method = p.method;
    path = p.path;
    body = (p as any).body ?? '';
  }
</script>

<div class="flex h-[calc(100vh-3rem)]">
  <!-- Presets sidebar -->
  <div class="w-52 shrink-0 border-r border-border bg-surface overflow-y-auto">
    <div class="p-2 text-xs text-muted uppercase tracking-wider font-medium">Presets</div>
    {#each presets as p}
      <button
        class="w-full text-left px-3 py-1.5 text-sm hover:bg-elevated transition-colors text-fg"
        onclick={() => loadPreset(p)}
      >
        <span class="text-xs font-mono text-accent mr-1">{p.method}</span>
        {p.label}
      </button>
    {/each}
  </div>

  <!-- Main -->
  <div class="flex-1 flex flex-col p-4 gap-3 min-w-0">
    <!-- Request bar -->
    <div class="flex gap-2">
      <select
        bind:value={method}
        class="px-2 py-2 bg-bg border border-border rounded text-sm text-fg font-mono"
      >
        <option>GET</option>
        <option>POST</option>
        <option>PUT</option>
        <option>DELETE</option>
      </select>
      <input
        type="text"
        bind:value={path}
        class="flex-1 px-3 py-2 bg-bg border border-border rounded text-sm text-fg font-mono focus:outline-none focus:border-accent"
        onkeydown={(e) => { if (e.key === 'Enter') send(); }}
      />
      <button
        class="px-4 py-2 bg-accent text-white rounded text-sm font-medium hover:bg-accent-hover disabled:opacity-50"
        onclick={send}
        disabled={loading}
      >
        {loading ? '...' : 'Send'}
      </button>
    </div>

    <!-- Body -->
    {#if method === 'POST' || method === 'PUT'}
      <div>
        <label class="text-xs text-muted mb-1 block">Request Body (JSON)</label>
        <textarea
          bind:value={body}
          rows="4"
          class="w-full px-3 py-2 bg-bg border border-border rounded text-sm text-fg font-mono focus:outline-none focus:border-accent resize-y"
          placeholder={'{ }'}
        ></textarea>
      </div>
    {/if}

    <!-- Response -->
    <div class="flex-1 flex flex-col min-h-0">
      {#if status}
        <div class="text-xs font-mono mb-1" class:text-accent={status.startsWith('2')} class:text-danger={!status.startsWith('2')}>
          {status}
        </div>
      {/if}
      <pre class="flex-1 overflow-auto bg-bg border border-border rounded p-3 text-sm font-mono text-fg whitespace-pre-wrap">{response || 'Response will appear here'}</pre>
    </div>
  </div>
</div>
