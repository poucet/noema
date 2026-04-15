<script lang="ts">
  import { onMount } from 'svelte';

  interface RouteInfo {
    method: string;
    path: string;
    name: string;
    group: string;
    description: string | null;
    schema: any | null;
  }

  interface FieldDef {
    name: string;
    kind: 'string' | 'number' | 'boolean' | 'select' | 'json';
    required: boolean;
    description?: string;
    options?: string[];
    isPathParam: boolean;
  }

  let method = $state('GET');
  let path = $state('');
  let body = $state('');
  let response = $state('');
  let status = $state('');
  let loading = $state(false);
  let rawMode = $state(false);

  let routes = $state<RouteInfo[]>([]);
  let activeRoute = $state<RouteInfo | null>(null);
  let formFields = $state<Record<string, string>>({});

  let grouped = $derived(groupByService(routes));
  let fields = $derived(activeRoute ? buildFieldDefs(activeRoute) : []);
  let hasForm = $derived(!rawMode && fields.length > 0 && (method === 'POST' || method === 'PUT' || fields.some(f => f.isPathParam)));

  function groupByService(routes: RouteInfo[]): [string, RouteInfo[]][] {
    const groups: Record<string, RouteInfo[]> = {};
    for (const r of routes) {
      (groups[r.group] ??= []).push(r);
    }
    return Object.entries(groups);
  }

  function shortName(name: string): string {
    return name.split('.').pop() ?? name;
  }

  function resolveRef(schema: any, defs: any): any {
    if (!schema) return schema;
    if (schema.$ref) {
      const name = schema.$ref.replace(/^#\/\$defs\//, '').replace(/^#\/definitions\//, '');
      return resolveRef(defs?.[name], defs);
    }
    // Option<T> → anyOf with null
    if (schema.anyOf) {
      const nonNull = schema.anyOf.filter((s: any) => s.type !== 'null');
      if (nonNull.length === 1) return resolveRef(nonNull[0], defs);
    }
    return schema;
  }

  function classifyProp(resolved: any): { kind: FieldDef['kind']; options?: string[] } {
    if (resolved.enum) return { kind: 'select', options: resolved.enum.map(String) };
    if (resolved.type === 'boolean') return { kind: 'boolean' };
    if (resolved.type === 'number' || resolved.type === 'integer') return { kind: 'number' };
    if (resolved.type === 'array' || resolved.type === 'object') return { kind: 'json' };
    if (resolved.type === 'string') return { kind: 'string' };
    // Complex schemas (oneOf, allOf without null) → json
    if (resolved.oneOf || resolved.allOf || resolved.anyOf) return { kind: 'json' };
    return { kind: 'string' };
  }

  function buildFieldDefs(route: RouteInfo): FieldDef[] {
    const defs: FieldDef[] = [];
    const pathParams = [...route.path.matchAll(/\{(\w+)\}/g)].map(m => m[1]);

    for (const name of pathParams) {
      defs.push({ name, kind: 'string', required: true, isPathParam: true });
    }

    if (!route.schema?.properties) return defs;
    const schemaDefs = route.schema.$defs ?? route.schema.definitions ?? {};
    const required = new Set<string>(route.schema.required ?? []);

    for (const [name, rawProp] of Object.entries<any>(route.schema.properties)) {
      if (pathParams.includes(name)) continue;
      const resolved = resolveRef(rawProp, schemaDefs);
      const { kind, options } = classifyProp(resolved);
      defs.push({
        name,
        kind,
        required: required.has(name),
        description: resolved.description ?? rawProp.description,
        options,
        isPathParam: false,
      });
    }
    return defs;
  }

  function buildResolvedPath(): string {
    let p = activeRoute?.path ?? path;
    for (const f of fields) {
      if (f.isPathParam && formFields[f.name]) {
        p = p.replace(`{${f.name}}`, encodeURIComponent(formFields[f.name]));
      }
    }
    return p;
  }

  function buildRequestBody(): string {
    const obj: Record<string, any> = {};
    for (const field of fields) {
      if (field.isPathParam) continue;
      const val = formFields[field.name] ?? '';
      if (!val && !field.required) continue;

      if (field.kind === 'boolean') obj[field.name] = val === 'true';
      else if (field.kind === 'number') obj[field.name] = Number(val) || 0;
      else if (field.kind === 'json') {
        try { obj[field.name] = JSON.parse(val); } catch { obj[field.name] = val; }
      } else {
        obj[field.name] = val;
      }
    }
    return JSON.stringify(obj);
  }

  function loadRoute(r: RouteInfo) {
    activeRoute = r;
    method = r.method === 'STREAM' ? 'GET' : r.method;
    path = r.path;
    formFields = {};
    rawMode = false;
    body = '';
  }

  async function send() {
    loading = true;
    response = '';
    status = '';
    try {
      const url = hasForm ? buildResolvedPath() : path;
      const reqBody = hasForm ? buildRequestBody() : body;
      const init: RequestInit = { method };
      if ((method === 'POST' || method === 'PUT') && reqBody && reqBody !== '{}') {
        init.headers = { 'Content-Type': 'application/json' };
        init.body = reqBody;
      }
      const res = await fetch(url, init);
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

  onMount(async () => {
    try {
      const res = await fetch('/admin/api/routes');
      routes = await res.json();
    } catch {
      // endpoint unavailable
    }
  });
</script>

<div class="flex h-[calc(100vh-3rem)]">
  <!-- Routes sidebar -->
  <div class="w-60 shrink-0 border-r border-border bg-surface overflow-y-auto">
    {#each grouped as [group, groupRoutes]}
      <div class="px-3 py-1.5 text-xs text-muted uppercase tracking-wider font-medium border-t border-border first:border-t-0">
        {group}
      </div>
      {#each groupRoutes as r}
        <button
          class="w-full text-left px-3 py-1.5 text-sm hover:bg-elevated transition-colors text-fg"
          class:bg-elevated={activeRoute === r}
          onclick={() => loadRoute(r)}
          title={r.description ?? ''}
        >
          <span class="text-xs font-mono text-accent mr-1">{r.method === 'STREAM' ? 'WS' : r.method}</span>
          {shortName(r.name)}
        </button>
      {/each}
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
        readonly={hasForm}
      />
      <button
        class="px-4 py-2 bg-accent text-white rounded text-sm font-medium hover:bg-accent-hover disabled:opacity-50"
        onclick={send}
        disabled={loading}
      >
        {loading ? '...' : 'Send'}
      </button>
    </div>

    <!-- Structured form -->
    {#if hasForm}
      <div class="border border-border rounded bg-bg p-3">
        <div class="flex items-center justify-between mb-2">
          <span class="text-xs text-muted uppercase tracking-wider font-medium">Parameters</span>
          <button
            class="text-xs text-accent hover:underline"
            onclick={() => { rawMode = true; body = buildRequestBody(); path = buildResolvedPath(); }}
          >raw JSON</button>
        </div>
        <div class="grid gap-2">
          {#each fields as field}
            <div class="flex items-start gap-2">
              <span class="text-xs text-muted w-36 shrink-0 pt-2 text-right font-mono" title={field.description ?? ''}>
                {field.name}{#if field.required}<span class="text-danger">*</span>{/if}
                {#if field.isPathParam}<span class="text-accent ml-1">path</span>{/if}
              </span>

              {#if field.kind === 'boolean'}
                <select
                  class="flex-1 px-2 py-1.5 bg-surface border border-border rounded text-sm text-fg font-mono focus:outline-none focus:border-accent"
                  value={formFields[field.name] ?? ''}
                  onchange={(e) => formFields[field.name] = (e.target as HTMLSelectElement).value}
                >
                  <option value="">—</option>
                  <option value="true">true</option>
                  <option value="false">false</option>
                </select>
              {:else if field.kind === 'select'}
                <select
                  class="flex-1 px-2 py-1.5 bg-surface border border-border rounded text-sm text-fg font-mono focus:outline-none focus:border-accent"
                  value={formFields[field.name] ?? ''}
                  onchange={(e) => formFields[field.name] = (e.target as HTMLSelectElement).value}
                >
                  <option value="">—</option>
                  {#each field.options ?? [] as opt}
                    <option value={opt}>{opt}</option>
                  {/each}
                </select>
              {:else if field.kind === 'json'}
                <textarea
                  class="flex-1 px-2 py-1.5 bg-surface border border-border rounded text-sm text-fg font-mono focus:outline-none focus:border-accent resize-y"
                  rows="2"
                  placeholder={field.kind}
                  value={formFields[field.name] ?? ''}
                  oninput={(e) => formFields[field.name] = (e.target as HTMLTextAreaElement).value}
                ></textarea>
              {:else}
                <input
                  type={field.kind === 'number' ? 'number' : 'text'}
                  class="flex-1 px-2 py-1.5 bg-surface border border-border rounded text-sm text-fg font-mono focus:outline-none focus:border-accent"
                  placeholder={field.description ?? field.kind}
                  value={formFields[field.name] ?? ''}
                  oninput={(e) => formFields[field.name] = (e.target as HTMLInputElement).value}
                  onkeydown={(e) => { if (e.key === 'Enter') send(); }}
                />
              {/if}
            </div>
          {/each}
        </div>
      </div>

    <!-- Raw body fallback -->
    {:else if method === 'POST' || method === 'PUT'}
      <div>
        <div class="flex items-center justify-between mb-1">
          <span class="text-xs text-muted">Request Body (JSON)</span>
          {#if activeRoute?.schema}
            <button
              class="text-xs text-accent hover:underline"
              onclick={() => rawMode = false}
            >form view</button>
          {/if}
        </div>
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
