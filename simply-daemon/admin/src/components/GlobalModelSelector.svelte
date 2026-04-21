<script lang="ts">
  import { onMount } from 'svelte';
  import { getTransport, modelApi, type ModelInfo } from '@simply/client';

  let models = $state<ModelInfo[]>([]);
  let currentModelId = $state('');
  let open = $state(false);
  let search = $state('');

  onMount(async () => {
    try {
      const t = getTransport();
      const api = modelApi(t);
      models = await api.listModels();
      currentModelId = await api.defaultModelId();
    } catch (e) {
      console.error('[model-selector] load failed:', e);
    }
  });

  const textModels = $derived(models.filter(m => m.definition.capabilities.includes('Text')));

  const grouped = $derived(() => {
    const q = search.toLowerCase();
    const filtered = q
      ? textModels.filter(m => {
          const name = m.definition.displayName || m.definition.id;
          return name.toLowerCase().includes(q) || m.id.provider.toLowerCase().includes(q);
        })
      : textModels;

    const groups = new Map<string, ModelInfo[]>();
    for (const m of filtered) {
      const list = groups.get(m.id.provider) || [];
      list.push(m);
      groups.set(m.id.provider, list);
    }
    return groups;
  });

  function displayName(model: ModelInfo): string {
    return model.definition.displayName || model.definition.id;
  }

  function currentDisplayName(): string {
    if (!currentModelId) return 'No model';
    const model = models.find(m => `${m.id.provider}/${m.id.model}` === currentModelId);
    if (model) return displayName(model);
    return currentModelId.split('/').pop() || currentModelId;
  }

  async function select(modelId: string) {
    try {
      const t = getTransport();
      await modelApi(t).setDefaultModel(modelId);
      currentModelId = modelId;
    } catch (e) {
      console.error('[model-selector] set failed:', e);
    }
    open = false;
    search = '';
  }

  function badges(model: ModelInfo): string[] {
    const caps = model.definition.capabilities;
    const b: string[] = [];
    if (caps.includes('Vision')) b.push('👁');
    if (caps.includes('Tools')) b.push('🔧');
    if (caps.includes('Thinking')) b.push('💭');
    return b;
  }
</script>

<div class="relative">
  <button
    class="flex items-center gap-1 px-2 py-0.5 rounded bg-white/5 border border-border
           text-xs text-muted hover:text-fg hover:border-accent/50 transition-colors"
    onclick={() => open = !open}
  >
    <span class="truncate max-w-36">{currentDisplayName()}</span>
    <svg class="w-2.5 h-2.5 shrink-0 opacity-50" fill="none" viewBox="0 0 24 24" stroke="currentColor">
      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" />
    </svg>
  </button>

  {#if open}
    <div class="fixed inset-0 z-10" onclick={() => { open = false; search = ''; }}></div>

    <div class="absolute right-0 top-full mt-1 z-20 w-80 bg-bg border border-border rounded-lg shadow-xl overflow-hidden">
      <input
        class="w-full px-3 py-2 bg-transparent border-b border-border text-sm text-fg outline-none placeholder:text-muted/50"
        placeholder="Search models…"
        bind:value={search}
        autofocus
      />
      <div class="max-h-80 overflow-y-auto">
        {#each [...grouped().entries()] as [provider, providerModels]}
          <div class="px-3 pt-2 pb-1 text-[10px] uppercase tracking-wider text-muted/40 font-medium">
            {provider}
          </div>
          {#each providerModels as model}
            {@const fullId = `${model.id.provider}/${model.id.model}`}
            {@const isActive = fullId === currentModelId}
            <button
              class="w-full text-left px-3 py-1.5 text-xs hover:bg-white/5 flex items-center gap-2
                     {isActive ? 'text-accent bg-accent/5' : 'text-fg/80'}"
              onclick={() => select(fullId)}
            >
              {#if isActive}
                <span class="w-1.5 h-1.5 rounded-full bg-accent shrink-0"></span>
              {:else}
                <span class="w-1.5 h-1.5 shrink-0"></span>
              {/if}
              <span class="truncate flex-1">{displayName(model)}</span>
              {#each badges(model) as badge}
                <span class="text-[9px]">{badge}</span>
              {/each}
            </button>
          {/each}
        {:else}
          <div class="px-3 py-4 text-xs text-muted/50 text-center">No models available</div>
        {/each}
      </div>
    </div>
  {/if}
</div>
