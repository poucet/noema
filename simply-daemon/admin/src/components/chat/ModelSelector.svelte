<script lang="ts">
  import { chatStore } from '../../lib/stores/chat.svelte';

  let open = $state(false);
  let search = $state('');

  const filtered = $derived(
    chatStore.models.filter(m => {
      if (!search) return true;
      const q = search.toLowerCase();
      const name = m.definition.displayName || m.definition.id;
      return name.toLowerCase().includes(q) || m.id.provider.toLowerCase().includes(q);
    })
  );

  function displayName(model: typeof chatStore.models[0]): string {
    return model.definition.displayName || model.definition.id;
  }

  function select(modelId: string) {
    chatStore.setModel(modelId);
    open = false;
    search = '';
  }
</script>

<div class="relative">
  <button
    class="text-xs px-2 py-1 rounded bg-white/5 border border-border text-muted hover:text-fg hover:border-accent/50 truncate max-w-48"
    onclick={() => open = !open}
  >
    {chatStore.currentModelId?.split('/').pop() || 'Select model'}
  </button>

  {#if open}
    <!-- backdrop -->
    <div class="fixed inset-0 z-10" onclick={() => { open = false; search = ''; }}></div>

    <div class="absolute right-0 top-full mt-1 z-20 w-72 bg-bg border border-border rounded-lg shadow-xl overflow-hidden">
      <input
        class="w-full px-3 py-2 bg-transparent border-b border-border text-sm text-fg outline-none placeholder:text-muted/50"
        placeholder="Search models…"
        bind:value={search}
        autofocus
      />
      <div class="max-h-64 overflow-y-auto">
        {#each filtered as model}
          {@const fullId = `${model.id.provider}/${model.id.model}`}
          <button
            class="w-full text-left px-3 py-2 text-xs hover:bg-white/5 flex items-center justify-between
                   {fullId === chatStore.currentModelId ? 'text-accent' : 'text-muted'}"
            onclick={() => select(fullId)}
          >
            <span class="truncate">{displayName(model)}</span>
            <span class="text-muted/40 shrink-0 ml-2">{model.id.provider}</span>
          </button>
        {:else}
          <div class="px-3 py-4 text-xs text-muted/50 text-center">No models found</div>
        {/each}
      </div>
    </div>
  {/if}
</div>
