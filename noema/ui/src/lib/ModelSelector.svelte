<script lang="ts">
  import type { ModelInfo } from '@simply/client';
  import { chatStore } from './stores/chat.svelte';

  let open = $state(false);
  let search = $state('');

  // Providers occasionally surface the same model id via multiple paths
  // (aliases, short + long names). De-dupe by full id so the list — and the
  // Svelte keyed each — stays unique.
  const textModels = $derived.by(() => {
    const seen = new Set<string>();
    const result: ModelInfo[] = [];
    for (const m of chatStore.models) {
      if (!m.definition.capabilities.includes('Text')) continue;
      const id = `${m.id.provider}/${m.id.model}`;
      if (seen.has(id)) continue;
      seen.add(id);
      result.push(m);
    }
    return result;
  });

  const grouped = $derived(() => {
    const q = search.toLowerCase();
    const filtered = q
      ? textModels.filter((m) => {
          const name = m.definition.displayName ?? m.definition.id;
          return (
            name.toLowerCase().includes(q) ||
            m.id.provider.toLowerCase().includes(q)
          );
        })
      : textModels;

    const groups = new Map<string, ModelInfo[]>();
    for (const m of filtered) {
      const list = groups.get(m.id.provider) ?? [];
      list.push(m);
      groups.set(m.id.provider, list);
    }
    return groups;
  });

  function displayName(model: ModelInfo): string {
    return model.definition.displayName ?? model.definition.id;
  }

  function currentDisplayName(): string {
    const id = chatStore.currentModelId;
    if (!id) return 'No model';
    const model = chatStore.models.find(
      (m) => `${m.id.provider}/${m.id.model}` === id,
    );
    if (model) return displayName(model);
    return id.split('/').pop() ?? id;
  }

  async function select(modelId: string) {
    await chatStore.setModel(modelId);
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

  // Disabled state: switching the model is session-scoped, so we need an
  // active session to have anything to switch.
  const disabled = $derived(chatStore.currentSessionId == null);
</script>

<div class="relative">
  <button
    class="flex items-center gap-1 rounded border border-gray-700 bg-elevated px-2 py-0.5 text-xs text-muted transition-colors hover:border-teal-500/50 hover:text-foreground disabled:cursor-not-allowed disabled:opacity-50"
    {disabled}
    onclick={() => (open = !open)}
  >
    <span class="max-w-48 truncate">{currentDisplayName()}</span>
    <svg class="h-2.5 w-2.5 shrink-0 opacity-50" fill="none" viewBox="0 0 24 24" stroke="currentColor">
      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" />
    </svg>
  </button>

  {#if open}
    <!-- Backdrop that swallows outside clicks. Role+keydown make it a11y-valid. -->
    <div
      class="fixed inset-0 z-10"
      role="button"
      tabindex="-1"
      aria-label="Close model picker"
      onclick={() => {
        open = false;
        search = '';
      }}
      onkeydown={(e) => {
        if (e.key === 'Escape') {
          open = false;
          search = '';
        }
      }}
    ></div>

    <div
      class="absolute right-0 top-full z-20 mt-1 w-80 overflow-hidden rounded-lg border border-gray-700 bg-surface shadow-xl"
    >
      <!-- svelte-ignore a11y_autofocus -->
      <input
        class="w-full border-b border-gray-700 bg-transparent px-3 py-2 text-sm text-foreground outline-none placeholder:text-muted/50"
        placeholder="Search models…"
        bind:value={search}
        autofocus
      />
      <div class="max-h-80 overflow-y-auto">
        {#each [...grouped().entries()] as [provider, providerModels] (provider)}
          <div
            class="px-3 pb-1 pt-2 text-[10px] font-medium uppercase tracking-wider text-muted/50"
          >
            {provider}
          </div>
          {#each providerModels as model (model.id.provider + model.id.model)}
            {@const fullId = `${model.id.provider}/${model.id.model}`}
            {@const isActive = fullId === chatStore.currentModelId}
            <button
              class="flex w-full items-center gap-2 px-3 py-1.5 text-left text-xs hover:bg-elevated {isActive
                ? 'bg-teal-900/30 text-teal-300'
                : 'text-gray-300'}"
              onclick={() => select(fullId)}
            >
              {#if isActive}
                <span class="h-1.5 w-1.5 shrink-0 rounded-full bg-teal-400"></span>
              {:else}
                <span class="h-1.5 w-1.5 shrink-0"></span>
              {/if}
              <span class="flex-1 truncate">{displayName(model)}</span>
              {#each badges(model) as badge, i (i)}
                <span class="text-[11px]">{badge}</span>
              {/each}
            </button>
          {/each}
        {:else}
          <div class="px-3 py-4 text-center text-xs text-muted">No models available</div>
        {/each}
      </div>
    </div>
  {/if}
</div>
