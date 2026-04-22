<script lang="ts">
  import { onMount } from 'svelte';
  import { getTransport, modelApi, type ModelInfo } from '@simply/client';
  import { adminApi } from '../adminApi';

  const t = getTransport();
  const model = modelApi(t);

  // Empty string, not null — `bind:value` with a nullable field can leave
  // the input blank even after a successful fetch depending on Svelte's
  // coercion path; keeping it `string` avoids that whole class of edge case.
  let email = $state('');
  let defaultModel = $state('');
  let models = $state<ModelInfo[]>([]);
  let saving = $state(false);
  let error = $state<string | null>(null);
  let success = $state<string | null>(null);

  // Providers occasionally surface the same model id via multiple paths
  // (aliases, short + long names). De-dupe so the keyed `{#each}` doesn't
  // explode — same fix lives in ModelSelector.
  const uniqueModels = $derived.by(() => {
    const seen = new Set<string>();
    const out: ModelInfo[] = [];
    for (const m of models) {
      const id = `${m.id.provider}/${m.id.model}`;
      if (seen.has(id)) continue;
      seen.add(id);
      out.push(m);
    }
    return out;
  });

  // Load the three independently so a single failure (e.g. a model provider
  // timing out) doesn't leave the email field blank.
  onMount(() => {
    adminApi.getSettings()
      .then((s) => {
        console.log('[settings] raw response:', s);
        email = s.user_email ?? '';
        console.log('[settings] email set to:', email);
      })
      .catch((e) => {
        console.error('[settings] getSettings failed:', e);
        error = `${e}`;
      });
    model.listModels()
      .then((m) => (models = m))
      .catch((e) => console.error('[settings] listModels failed:', e));
    model.defaultModelId()
      .then((id) => (defaultModel = id))
      .catch((e) => console.error('[settings] defaultModelId failed:', e));
  });

  async function saveEmail() {
    if (!email.trim()) return;
    saving = true;
    try {
      await adminApi.updateSettings({ user_email: email.trim() });
      success = 'Email saved';
      setTimeout(() => (success = null), 2000);
    } catch (e) {
      console.error('[settings] save email:', e);
      error = `${e}`;
    }
    saving = false;
  }

  async function setDefault(modelId: string) {
    try {
      await model.setDefaultModel(modelId);
      defaultModel = modelId;
      success = 'Default model updated';
      setTimeout(() => (success = null), 2000);
    } catch (e) {
      console.error('[settings] set model:', e);
      error = `${e}`;
    }
  }
</script>

<div class="space-y-4">
  <h2 class="text-lg font-medium text-foreground">General</h2>

  {#if error}
    <div class="rounded bg-red-900/30 px-3 py-2 text-xs text-red-300">
      {error}
      <button class="ml-2 underline" onclick={() => (error = null)}>dismiss</button>
    </div>
  {/if}
  {#if success}
    <div class="rounded bg-green-900/30 px-3 py-2 text-xs text-green-300">{success}</div>
  {/if}

  <div class="rounded-lg border border-gray-700 p-3">
    <label for="admin-email" class="mb-2 block text-sm font-medium text-foreground">Admin Email</label>
    <div class="flex gap-2">
      <input
        id="admin-email"
        class="flex-1 rounded border border-gray-700 bg-elevated px-3 py-2 text-sm text-foreground outline-none focus:border-teal-500"
        bind:value={email}
        placeholder="you@example.com"
        onkeydown={(e) => {
          if (e.key === 'Enter') saveEmail();
        }}
      />
      <button
        class="rounded bg-teal-600 px-4 py-2 text-sm font-medium text-white hover:bg-teal-700 disabled:opacity-30"
        disabled={saving}
        onclick={saveEmail}
      >
        Save
      </button>
    </div>
  </div>

  <div class="rounded-lg border border-gray-700 p-3">
    <label for="default-model" class="mb-2 block text-sm font-medium text-foreground">
      Default Model
    </label>
    <select
      id="default-model"
      class="w-full rounded border border-gray-700 bg-elevated px-3 py-2 text-sm text-foreground outline-none focus:border-teal-500"
      value={defaultModel}
      onchange={(e) => setDefault((e.target as HTMLSelectElement).value)}
    >
      <option value="">Select a model</option>
      {#each uniqueModels as m (`${m.id.provider}/${m.id.model}`)}
        {@const fullId = `${m.id.provider}/${m.id.model}`}
        <option value={fullId}>
          {m.definition.displayName ?? m.definition.id} ({m.id.provider})
        </option>
      {/each}
    </select>
    {#if defaultModel}
      <div class="mt-1 text-xs text-muted">Current: {defaultModel}</div>
    {/if}
  </div>
</div>
