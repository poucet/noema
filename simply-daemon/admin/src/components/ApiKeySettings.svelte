<script lang="ts">
  import { onMount } from 'svelte';
  import { api, PROVIDERS } from '../lib/api';

  let keys = $state<string[]>([]);
  let editing = $state<string | null>(null);
  let keyValue = $state('');
  let error = $state<string | null>(null);

  onMount(async () => {
    try {
      const status = await api.getSetupStatus();
      keys = status.api_keys;
    } catch (e) {
      console.error('[settings] load keys:', e);
      error = `${e}`;
    }
  });

  async function saveKey(provider: string) {
    if (!keyValue.trim()) return;
    try {
      await api.setApiKey(provider, keyValue.trim());
      keyValue = '';
      editing = null;
      const status = await api.getSetupStatus();
      keys = status.api_keys;
    } catch (e) {
      console.error('[settings] save key:', e);
      error = `${e}`;
    }
  }

  async function removeKey(provider: string) {
    try {
      await api.removeApiKey(provider);
      const status = await api.getSetupStatus();
      keys = status.api_keys;
    } catch (e) {
      console.error('[settings] remove key:', e);
      error = `${e}`;
    }
  }
</script>

<div class="space-y-4">
  <h2 class="text-lg font-medium">API Keys</h2>

  {#if error}
    <div class="px-3 py-2 bg-red-900/30 text-red-300 text-xs rounded">
      {error}
      <button class="ml-2 underline" onclick={() => error = null}>dismiss</button>
    </div>
  {/if}

  <div class="grid gap-2">
    {#each PROVIDERS as provider}
      {@const hasKey = keys.includes(provider.id)}
      <div class="flex items-center gap-3 border border-border rounded-lg p-3">
        <div class="flex-1 min-w-0">
          <div class="text-sm font-medium">{provider.name}</div>
          <div class="text-xs {hasKey ? 'text-green-400' : 'text-muted/50'}">
            {hasKey ? 'Configured' : 'Not set'}
          </div>
        </div>

        {#if editing === provider.id}
          <input
            class="flex-1 bg-white/5 border border-border rounded px-2 py-1 text-xs text-fg outline-none font-mono"
            placeholder={provider.placeholder}
            bind:value={keyValue}
            onkeydown={(e) => { if (e.key === 'Enter') saveKey(provider.id); if (e.key === 'Escape') editing = null; }}
            autofocus
          />
          <button
            class="text-xs px-2 py-1 rounded bg-accent/20 text-accent hover:bg-accent/30"
            onclick={() => saveKey(provider.id)}
          >Save</button>
          <button
            class="text-xs px-2 py-1 rounded bg-white/5 text-muted hover:text-fg"
            onclick={() => editing = null}
          >Cancel</button>
        {:else}
          <a
            href={provider.url}
            target="_blank"
            class="text-xs text-accent/50 hover:text-accent"
          >Get key</a>
          <button
            class="text-xs px-2 py-1 rounded bg-white/5 text-muted hover:text-fg"
            onclick={() => { editing = provider.id; keyValue = ''; }}
          >{hasKey ? 'Update' : 'Set'}</button>
          {#if hasKey}
            <button
              class="text-xs px-2 py-1 rounded bg-red-900/20 text-red-400 hover:bg-red-900/40"
              onclick={() => removeKey(provider.id)}
            >Remove</button>
          {/if}
        {/if}
      </div>
    {/each}
  </div>
</div>
