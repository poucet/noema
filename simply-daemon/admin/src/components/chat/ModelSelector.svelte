<script lang="ts">
  import { chatStore } from '../../lib/stores/chat.svelte';
  import type { ModelInfo } from '@simply/client';

  let open = $state(false);
  let search = $state('');

  // Group models by provider, filter to text-capable only
  const grouped = $derived(() => {
    const textModels = chatStore.models.filter(m =>
      m.definition.capabilities.includes('Text')
    );
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
    const id = chatStore.currentModelId;
    if (!id) return 'Select model';
    const model = chatStore.models.find(m => `${m.id.provider}/${m.id.model}` === id);
    if (model) return displayName(model);
    return id.split('/').pop() || id;
  }

  function select(modelId: string) {
    chatStore.setModel(modelId);
    open = false;
    search = '';
  }

  function badges(model: ModelInfo): string[] {
    const caps = model.definition.capabilities;
    const b: string[] = [];
    if (caps.includes('Vision')) b.push('vision');
    if (caps.includes('Tools')) b.push('tools');
    if (caps.includes('Thinking')) b.push('thinking');
    return b;
  }
</script>

<div class="relative">
  <button
    class="flex items-center gap-1.5 px-2.5 py-1 rounded-md bg-white/5 border border-border
           text-sm text-muted hover:text-fg hover:border-accent/50 transition-colors"
    onclick={() => open = !open}
  >
    <span class="truncate max-w-52">{currentDisplayName()}</span>
    <svg class="w-3 h-3 shrink-0 opacity-50" fill="none" viewBox="0 0 24 24" stroke="currentColor">
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
        {#each [...grouped().entries()] as [provider, models]}
          <div class="px-3 pt-2 pb-1 text-[10px] uppercase tracking-wider text-muted/40 font-medium">
            {provider}
          </div>
          {#each models as model}
            {@const fullId = `${model.id.provider}/${model.id.model}`}
            {@const isActive = fullId === chatStore.currentModelId}
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
                <span class="text-[9px] px-1 py-0.5 rounded bg-white/5 text-muted/50">{badge}</span>
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
