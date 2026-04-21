<script lang="ts">
  import { onMount } from 'svelte';
  import { getTransport, oAuthApi } from '@simply/client';

  const oauth = oAuthApi(getTransport());

  type ProviderInfo = {
    id: string;
    displayName: string;
    authorizationUrl: string;
    tokenUrl: string;
    userinfoUrl: string | null;
    clientId: string;
    hasClientSecret: boolean;
    clientSecretSuffix: string;
    registeredScopes: string[];
  };

  type ProviderPreset = {
    id: string;
    displayName: string;
    authorizationUrl: string;
    tokenUrl: string;
    userinfoUrl: string;
    consoleUrl: string;
    steps: string[];
    redirectUriNote?: string;
  };

  // Walkthrough presets for common providers.
  const PRESETS: ProviderPreset[] = [
    {
      id: 'google',
      displayName: 'Google',
      authorizationUrl: 'https://accounts.google.com/o/oauth2/v2/auth',
      tokenUrl: 'https://oauth2.googleapis.com/token',
      userinfoUrl: 'https://www.googleapis.com/oauth2/v2/userinfo',
      consoleUrl: 'https://console.cloud.google.com/apis/credentials',
      steps: [
        'Open Google Cloud Console → APIs & Services → Credentials.',
        'Click "Create Credentials" → "OAuth client ID".',
        'Application type: "Web application".',
        'Add your redirect URI (shown below) under "Authorized redirect URIs".',
        'Enable the APIs your skills need (e.g., Drive API, Docs API) under "Enabled APIs".',
        'Copy the Client ID and Client Secret and paste them here.',
      ],
    },
    {
      id: 'github',
      displayName: 'GitHub',
      authorizationUrl: 'https://github.com/login/oauth/authorize',
      tokenUrl: 'https://github.com/login/oauth/access_token',
      userinfoUrl: 'https://api.github.com/user',
      consoleUrl: 'https://github.com/settings/developers',
      steps: [
        'Open GitHub → Settings → Developer settings → OAuth Apps.',
        'Click "New OAuth App".',
        'Set the "Authorization callback URL" to the redirect URI shown below.',
        'After creation, click "Generate a new client secret".',
        'Copy the Client ID and Client Secret and paste them here.',
      ],
    },
    {
      id: 'notion',
      displayName: 'Notion',
      authorizationUrl: 'https://api.notion.com/v1/oauth/authorize',
      tokenUrl: 'https://api.notion.com/v1/oauth/token',
      userinfoUrl: '',
      consoleUrl: 'https://www.notion.so/my-integrations',
      steps: [
        'Open Notion → My Integrations.',
        'Click "New integration" → choose "Public integration".',
        'Under "Redirect URIs", add the redirect URI shown below.',
        'Copy the OAuth client ID and OAuth client secret.',
      ],
    },
    {
      id: 'slack',
      displayName: 'Slack',
      authorizationUrl: 'https://slack.com/oauth/v2/authorize',
      tokenUrl: 'https://slack.com/api/oauth.v2.access',
      userinfoUrl: 'https://slack.com/api/users.identity',
      consoleUrl: 'https://api.slack.com/apps',
      steps: [
        'Open api.slack.com/apps → "Create New App" → "From scratch".',
        'In "OAuth & Permissions", add the redirect URI shown below.',
        'Under "Basic Information", copy the Client ID and Client Secret.',
      ],
    },
  ];

  let providers = $state<ProviderInfo[]>([]);
  let redirectUri = $state('');
  let error = $state<string | null>(null);
  let loading = $state(false);

  // Form state
  let editing = $state<string | null>(null);        // provider_id being edited
  let selectedPreset = $state<string>('');          // for new provider
  let showCustom = $state(false);
  let formId = $state('');
  let formDisplayName = $state('');
  let formAuthUrl = $state('');
  let formTokenUrl = $state('');
  let formUserinfoUrl = $state('');
  let formClientId = $state('');
  let formClientSecret = $state('');
  let editingSecretSuffix = $state('');             // last 4 chars of existing secret (display only)
  let expandedGuide = $state<string | null>(null);  // preset id whose guide is open

  onMount(() => {
    redirectUri = `${window.location.origin}/auth/mcp/callback`;
    refresh();
  });

  async function refresh() {
    loading = true;
    try {
      providers = await (oauth as any).listOauthProviders();
    } catch (e) {
      console.error('[oauth-providers]', e);
      error = `${e}`;
    }
    loading = false;
  }

  function resetForm() {
    editing = null;
    selectedPreset = '';
    showCustom = false;
    formId = '';
    formDisplayName = '';
    formAuthUrl = '';
    formTokenUrl = '';
    formUserinfoUrl = '';
    formClientId = '';
    formClientSecret = '';
    editingSecretSuffix = '';
  }

  function selectPreset(presetId: string) {
    const p = PRESETS.find(x => x.id === presetId);
    if (!p) return;
    selectedPreset = presetId;
    showCustom = false;
    formId = p.id;
    formDisplayName = p.displayName;
    formAuthUrl = p.authorizationUrl;
    formTokenUrl = p.tokenUrl;
    formUserinfoUrl = p.userinfoUrl;
    expandedGuide = presetId;
  }

  function startEdit(p: ProviderInfo) {
    editing = p.id;
    selectedPreset = '';
    showCustom = false;
    formId = p.id;
    formDisplayName = p.displayName;
    formAuthUrl = p.authorizationUrl;
    formTokenUrl = p.tokenUrl;
    formUserinfoUrl = p.userinfoUrl || '';
    formClientId = p.clientId;
    formClientSecret = '';  // never prefill — user must re-enter to change
    editingSecretSuffix = p.hasClientSecret ? p.clientSecretSuffix : '';
  }

  async function save() {
    if (!formId.trim()) { error = 'Provider ID is required'; return; }
    if (!formAuthUrl.trim() || !formTokenUrl.trim()) {
      error = 'Authorization URL and token URL are required';
      return;
    }
    try {
      await oauth.registerOauthProvider(formId.trim(), {
        displayName: formDisplayName || formId,
        authorizationUrl: formAuthUrl,
        tokenUrl: formTokenUrl,
        userinfoUrl: formUserinfoUrl || null,
        clientId: formClientId,
        clientSecret: formClientSecret || null,
        scopes: [],
      });
      resetForm();
      await refresh();
    } catch (e) {
      console.error('[oauth-providers] save failed:', e);
      error = `Save failed: ${e}`;
    }
  }

  async function removeProvider(id: string) {
    if (!confirm(`Remove provider "${id}"? This won't delete already-issued tokens but new auth flows will fail.`)) return;
    try {
      await (oauth as any).removeOauthProvider(id);
      await refresh();
    } catch (e) {
      error = `Remove failed: ${e}`;
    }
  }

  async function copyRedirect() {
    try {
      await navigator.clipboard.writeText(redirectUri);
    } catch {}
  }

  const existingIds = $derived(new Set(providers.map(p => p.id)));
  const availablePresets = $derived(PRESETS.filter(p => !existingIds.has(p.id)));
</script>

<section id="oauth-providers" class="space-y-4">
  <div>
    <h2 class="text-lg font-medium">OAuth Providers</h2>
    <p class="text-xs text-muted mt-1">
      OAuth client credentials for services your skills and MCP servers connect to.
      Credentials survive MCP server removal — they live in
      <code class="text-fg/80">~/.local/share/noema/oauth_providers.toml</code>.
    </p>
  </div>

  <div class="bg-white/5 border border-border rounded p-3 text-xs">
    <div class="flex items-center justify-between gap-2">
      <div>
        <div class="text-muted">Your OAuth redirect URI:</div>
        <code class="text-accent text-sm break-all">{redirectUri}</code>
      </div>
      <button
        class="shrink-0 px-2 py-1 rounded bg-white/10 text-fg hover:bg-white/20"
        onclick={copyRedirect}
      >Copy</button>
    </div>
    <div class="text-muted mt-1">Paste this into each provider's "authorized redirect URIs" field.</div>
  </div>

  {#if error}
    <div class="px-3 py-2 bg-red-900/30 text-red-300 text-xs rounded">
      {error}
      <button class="ml-2 underline" onclick={() => error = null}>dismiss</button>
    </div>
  {/if}

  <!-- Existing providers -->
  {#if providers.length > 0}
    <div class="space-y-2">
      {#each providers as p}
        <div class="border border-border rounded p-3">
          <div class="flex items-center gap-3">
            <div class="flex-1 min-w-0">
              <div class="text-sm font-medium">{p.displayName || p.id}</div>
              <div class="text-xs text-muted/70 truncate">{p.id}</div>
              <div class="text-xs mt-1">
                {#if p.clientId && p.hasClientSecret}
                  <span class="text-green-400">Configured</span>
                {:else if p.clientId}
                  <span class="text-yellow-400">Client ID set, secret missing</span>
                {:else}
                  <span class="text-red-400">Not configured</span>
                {/if}
                {#if p.registeredScopes.length > 0}
                  <span class="text-muted ml-2">· {p.registeredScopes.length} scope(s)</span>
                {/if}
              </div>
            </div>
            <div class="flex gap-1 shrink-0">
              <button
                class="text-xs px-2 py-1 rounded bg-white/5 text-muted hover:text-fg"
                onclick={() => startEdit(p)}
              >Edit</button>
              <button
                class="text-xs px-2 py-1 rounded bg-red-900/20 text-red-400 hover:bg-red-900/40"
                onclick={() => removeProvider(p.id)}
              >Remove</button>
            </div>
          </div>
        </div>
      {/each}
    </div>
  {:else if !loading}
    <div class="text-sm text-muted">No OAuth providers configured yet.</div>
  {/if}

  <!-- Add / Edit form -->
  {#if editing || selectedPreset || showCustom}
    <div class="border border-border rounded-lg p-4 space-y-3">
      <div class="flex items-center justify-between">
        <h3 class="text-sm font-medium">
          {editing ? `Edit ${editing}` : selectedPreset ? `Set up ${PRESETS.find(p => p.id === selectedPreset)?.displayName}` : 'Custom provider'}
        </h3>
        <button class="text-xs text-muted underline" onclick={resetForm}>Cancel</button>
      </div>

      {#if selectedPreset && expandedGuide === selectedPreset}
        {@const p = PRESETS.find(x => x.id === selectedPreset)}
        {#if p}
          <div class="bg-accent/5 border border-accent/20 rounded p-3 text-xs space-y-2">
            <div class="flex items-center justify-between">
              <div class="font-medium text-accent">Setup steps</div>
              <a href={p.consoleUrl} target="_blank" rel="noopener" class="text-accent underline">
                Open {p.displayName} console ↗
              </a>
            </div>
            <ol class="list-decimal list-inside space-y-1 text-fg/90">
              {#each p.steps as step}
                <li>{step}</li>
              {/each}
            </ol>
            {#if p.redirectUriNote}
              <div class="text-muted italic">{p.redirectUriNote}</div>
            {/if}
          </div>
        {/if}
      {/if}

      {#if !editing}
        <label class="block space-y-1">
          <span class="text-xs text-muted">Provider ID</span>
          <span class="block text-[11px] text-muted/70">Short unique key used internally (e.g. <code>google</code>). Skills and MCP servers reference this.</span>
          <input
            class="w-full bg-white/5 border border-border rounded px-3 py-2 text-sm text-fg outline-none"
            placeholder="google"
            bind:value={formId}
            readonly={!!selectedPreset}
          />
        </label>
      {/if}

      <label class="block space-y-1">
        <span class="text-xs text-muted">Display name</span>
        <span class="block text-[11px] text-muted/70">Shown in lists. Purely cosmetic.</span>
        <input
          class="w-full bg-white/5 border border-border rounded px-3 py-2 text-sm text-fg outline-none"
          placeholder="Google"
          bind:value={formDisplayName}
        />
      </label>

      <label class="block space-y-1">
        <span class="text-xs text-muted">Authorization URL</span>
        <span class="block text-[11px] text-muted/70">Where users are sent to log in and grant consent.</span>
        <input
          class="w-full bg-white/5 border border-border rounded px-3 py-2 text-sm text-fg outline-none"
          placeholder="https://…/authorize"
          bind:value={formAuthUrl}
        />
      </label>

      <label class="block space-y-1">
        <span class="text-xs text-muted">Token URL</span>
        <span class="block text-[11px] text-muted/70">Where the daemon exchanges the code for an access token.</span>
        <input
          class="w-full bg-white/5 border border-border rounded px-3 py-2 text-sm text-fg outline-none"
          placeholder="https://…/token"
          bind:value={formTokenUrl}
        />
      </label>

      <label class="block space-y-1">
        <span class="text-xs text-muted">Userinfo URL <span class="text-muted/70">(optional)</span></span>
        <span class="block text-[11px] text-muted/70">Used to read the user's email after login, to link accounts.</span>
        <input
          class="w-full bg-white/5 border border-border rounded px-3 py-2 text-sm text-fg outline-none"
          placeholder="https://…/userinfo"
          bind:value={formUserinfoUrl}
        />
      </label>

      <label class="block space-y-1">
        <span class="text-xs text-muted">Client ID</span>
        <span class="block text-[11px] text-muted/70">The public identifier issued by the provider when you created the OAuth app.</span>
        <input
          class="w-full bg-white/5 border border-border rounded px-3 py-2 text-sm text-fg outline-none"
          placeholder="xxxxxxxxxxxx.apps.googleusercontent.com"
          bind:value={formClientId}
        />
      </label>

      <label class="block space-y-1">
        <span class="text-xs text-muted">Client secret</span>
        <span class="block text-[11px] text-muted/70">
          The confidential secret from the provider.
          {#if editing && editingSecretSuffix}
            Currently stored: <code class="text-fg/70">••••{editingSecretSuffix}</code> — leave blank to keep.
          {:else if editing}
            Not set — enter one to save.
          {/if}
        </span>
        <input
          class="w-full bg-white/5 border border-border rounded px-3 py-2 text-sm text-fg outline-none"
          type="password"
          placeholder={editing && editingSecretSuffix ? `••••${editingSecretSuffix}` : 'Client secret'}
          bind:value={formClientSecret}
        />
      </label>
      <button
        class="w-full px-4 py-2 bg-accent text-bg text-sm font-medium rounded hover:bg-accent/80"
        onclick={save}
      >{editing ? 'Save' : 'Add provider'}</button>
    </div>
  {:else}
    <!-- Picker: presets + custom -->
    <div class="space-y-2">
      <div class="text-xs text-muted">Add a provider</div>
      <div class="flex flex-wrap gap-2">
        {#each availablePresets as p}
          <button
            class="text-xs px-3 py-2 rounded border border-border bg-white/5 hover:bg-white/10 hover:border-accent/40 text-fg"
            onclick={() => selectPreset(p.id)}
          >+ {p.displayName}</button>
        {/each}
        <button
          class="text-xs px-3 py-2 rounded border border-border bg-white/5 hover:bg-white/10 text-muted hover:text-fg"
          onclick={() => { resetForm(); showCustom = true; }}
        >+ Custom…</button>
      </div>
    </div>
  {/if}
</section>
