<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '../lib/api';
  import { getTransport, modelApi, type ModelInfo } from '@simply/client';

  const t = getTransport();
  const model = modelApi(t);

  let email = $state<string | null>(null);
  let defaultModel = $state('');
  let models = $state<ModelInfo[]>([]);
  let saving = $state(false);
  let error = $state<string | null>(null);
  let success = $state<string | null>(null);

  onMount(async () => {
    try {
      const [settings, allModels, currentDefault] = await Promise.all([
        api.getSettings(),
        model.listModels(),
        model.defaultModelId(),
      ]);
      email = settings.user_email;
      models = allModels;
      defaultModel = currentDefault;
    } catch (e) {
      console.error('[settings] load:', e);
      error = `${e}`;
    }
  });

  async function saveEmail() {
    if (!email?.trim()) return;
    saving = true;
    try {
      await api.updateSettings({ user_email: email.trim() });
      success = 'Email saved';
      setTimeout(() => success = null, 2000);
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
      setTimeout(() => success = null, 2000);
    } catch (e) {
      console.error('[settings] set model:', e);
      error = `${e}`;
    }
  }
</script>

<div class="space-y-4">
  <h2 class="text-lg font-medium">General</h2>

  {#if error}
    <div class="px-3 py-2 bg-red-900/30 text-red-300 text-xs rounded">
      {error}
      <button class="ml-2 underline" onclick={() => error = null}>dismiss</button>
    </div>
  {/if}
  {#if success}
    <div class="px-3 py-2 bg-green-900/30 text-green-300 text-xs rounded">{success}</div>
  {/if}

  <!-- Email -->
  <div class="border border-border rounded-lg p-3">
    <label class="text-sm font-medium block mb-2">Admin Email</label>
    <div class="flex gap-2">
      <input
        class="flex-1 bg-white/5 border border-border rounded px-3 py-2 text-sm text-fg outline-none"
        bind:value={email}
        placeholder="you@example.com"
        onkeydown={(e) => { if (e.key === 'Enter') saveEmail(); }}
      />
      <button
        class="px-4 py-2 bg-accent text-bg text-sm font-medium rounded hover:bg-accent/80 disabled:opacity-30"
        disabled={saving}
        onclick={saveEmail}
      >Save</button>
    </div>
  </div>

  <!-- Default model -->
  <div class="border border-border rounded-lg p-3">
    <label class="text-sm font-medium block mb-2">Default Model</label>
    <select
      class="w-full bg-white/5 border border-border rounded px-3 py-2 text-sm text-fg outline-none"
      value={defaultModel}
      onchange={(e) => setDefault((e.target as HTMLSelectElement).value)}
    >
      <option value="">Select a model</option>
      {#each models as m}
        {@const fullId = `${m.id.provider}/${m.id.model}`}
        <option value={fullId}>
          {m.definition.displayName || m.definition.id} ({m.id.provider})
        </option>
      {/each}
    </select>
    {#if defaultModel}
      <div class="text-xs text-muted/50 mt-1">Current: {defaultModel}</div>
    {/if}
  </div>
</div>
