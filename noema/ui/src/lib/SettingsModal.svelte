<script lang="ts">
  import GeneralSettings from './settings/GeneralSettings.svelte';
  import ApiKeySettings from './settings/ApiKeySettings.svelte';

  type Tab = 'general' | 'keys';

  type Props = {
    open: boolean;
    onClose: () => void;
  };
  const { open, onClose }: Props = $props();

  let active = $state<Tab>('general');

  const tabs: { id: Tab; label: string }[] = [
    { id: 'general', label: 'General' },
    { id: 'keys', label: 'API Keys' },
  ];
</script>

{#if open}
  <div
    class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-6"
    role="presentation"
    onclick={(e) => {
      if (e.target === e.currentTarget) onClose();
    }}
    onkeydown={(e) => {
      if (e.key === 'Escape') onClose();
    }}
  >
    <div
      class="flex h-[80vh] w-full max-w-3xl overflow-hidden rounded-lg border border-gray-700 bg-background shadow-xl"
    >
      <aside class="flex w-40 shrink-0 flex-col border-r border-gray-700 py-3">
        <div class="px-3 pb-2 text-xs uppercase tracking-wide text-muted">Settings</div>
        {#each tabs as tab (tab.id)}
          <button
            class="px-3 py-2 text-left text-sm transition-colors {active === tab.id
              ? 'bg-elevated text-teal-300'
              : 'text-muted hover:bg-elevated hover:text-foreground'}"
            onclick={() => (active = tab.id)}
          >
            {tab.label}
          </button>
        {/each}
        <div class="mt-auto px-3 pt-2">
          <button class="text-xs text-muted hover:text-foreground" onclick={onClose}>Close</button>
        </div>
      </aside>

      <section class="flex-1 space-y-6 overflow-y-auto p-6">
        {#if active === 'general'}
          <GeneralSettings />
        {:else if active === 'keys'}
          <ApiKeySettings />
        {/if}
      </section>
    </div>
  </div>
{/if}
