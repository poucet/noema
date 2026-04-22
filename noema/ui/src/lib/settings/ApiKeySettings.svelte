<script lang="ts">
  import { onMount } from 'svelte';
  import { adminApi, PROVIDERS } from '../adminApi';

  let keys = $state<string[]>([]);
  let editing = $state<string | null>(null);
  let keyValue = $state('');
  let error = $state<string | null>(null);

  async function refresh() {
    const status = await adminApi.getSetupStatus();
    keys = status.api_keys;
  }

  onMount(async () => {
    try {
      await refresh();
    } catch (e) {
      console.error('[settings] load keys:', e);
      error = `${e}`;
    }
  });

  async function saveKey(provider: string) {
    if (!keyValue.trim()) return;
    try {
      await adminApi.setApiKey(provider, keyValue.trim());
      keyValue = '';
      editing = null;
      await refresh();
    } catch (e) {
      console.error('[settings] save key:', e);
      error = `${e}`;
    }
  }

  async function removeKey(provider: string) {
    try {
      await adminApi.removeApiKey(provider);
      await refresh();
    } catch (e) {
      console.error('[settings] remove key:', e);
      error = `${e}`;
    }
  }
</script>

<div class="space-y-4">
  <h2 class="text-lg font-medium text-foreground">API Keys</h2>

  {#if error}
    <div class="rounded bg-red-900/30 px-3 py-2 text-xs text-red-300">
      {error}
      <button class="ml-2 underline" onclick={() => (error = null)}>dismiss</button>
    </div>
  {/if}

  <div class="grid gap-2">
    {#each PROVIDERS as provider (provider.id)}
      {@const hasKey = keys.includes(provider.id)}
      <div class="flex items-center gap-3 rounded-lg border border-gray-700 p-3">
        <div class="min-w-0 flex-1">
          <div class="text-sm font-medium text-foreground">{provider.name}</div>
          <div class="text-xs {hasKey ? 'text-green-400' : 'text-muted'}">
            {hasKey ? 'Configured' : 'Not set'}
          </div>
        </div>

        {#if editing === provider.id}
          <!-- svelte-ignore a11y_autofocus -->
          <input
            class="flex-1 rounded border border-gray-700 bg-elevated px-2 py-1 font-mono text-xs text-foreground outline-none focus:border-teal-500"
            placeholder={provider.placeholder}
            bind:value={keyValue}
            onkeydown={(e) => {
              if (e.key === 'Enter') saveKey(provider.id);
              if (e.key === 'Escape') editing = null;
            }}
            autofocus
          />
          <button
            class="rounded bg-teal-700/40 px-2 py-1 text-xs text-teal-200 hover:bg-teal-700/60"
            onclick={() => saveKey(provider.id)}
          >
            Save
          </button>
          <button
            class="rounded bg-elevated px-2 py-1 text-xs text-muted hover:text-foreground"
            onclick={() => (editing = null)}
          >
            Cancel
          </button>
        {:else}
          <a
            href={provider.url}
            target="_blank"
            rel="noopener noreferrer"
            class="text-xs text-teal-400/60 hover:text-teal-300"
          >
            Get key
          </a>
          <button
            class="rounded bg-elevated px-2 py-1 text-xs text-muted hover:text-foreground"
            onclick={() => {
              editing = provider.id;
              keyValue = '';
            }}
          >
            {hasKey ? 'Update' : 'Set'}
          </button>
          {#if hasKey}
            <button
              class="rounded bg-red-900/20 px-2 py-1 text-xs text-red-400 hover:bg-red-900/40"
              onclick={() => removeKey(provider.id)}
            >
              Remove
            </button>
          {/if}
        {/if}
      </div>
    {/each}
  </div>
</div>
