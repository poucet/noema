<script lang="ts">
  import GeneralSettings from '../GeneralSettings.svelte';
  import ApiKeySettings from '../ApiKeySettings.svelte';
  import OAuthProviders from '../OAuthProviders.svelte';
  import McpSettings from '../McpSettings.svelte';

  type Tab = 'general' | 'keys' | 'oauth' | 'mcp';

  type Props = {
    open: boolean;
    onClose: () => void;
  };
  const { open, onClose }: Props = $props();

  let active: Tab = $state('general');

  const tabs: { id: Tab; label: string }[] = [
    { id: 'general', label: 'General' },
    { id: 'keys', label: 'API Keys' },
    { id: 'oauth', label: 'OAuth' },
    { id: 'mcp', label: 'MCP Servers' },
  ];
</script>

{#if open}
  <div
    class="fixed inset-0 z-50 bg-black/60 flex items-center justify-center p-6"
    role="presentation"
    onclick={(e) => { if (e.target === e.currentTarget) onClose(); }}
    onkeydown={(e) => { if (e.key === 'Escape') onClose(); }}
  >
    <div class="bg-bg border border-border rounded-lg shadow-xl w-full max-w-3xl h-[80vh] flex overflow-hidden">
      <!-- Tab rail -->
      <aside class="w-40 shrink-0 border-r border-border py-3 flex flex-col">
        <div class="px-3 pb-2 text-xs uppercase tracking-wide text-muted">Settings</div>
        {#each tabs as tab}
          <button
            class="text-left px-3 py-2 text-sm hover:bg-white/5"
            class:text-accent={active === tab.id}
            class:bg-white={active === tab.id}
            class:bg-opacity-5={active === tab.id}
            onclick={() => (active = tab.id)}
          >
            {tab.label}
          </button>
        {/each}
        <div class="mt-auto px-3 pt-2">
          <button class="text-xs text-muted hover:text-fg" onclick={onClose}>Close</button>
        </div>
      </aside>

      <!-- Content -->
      <section class="flex-1 overflow-y-auto p-6 space-y-6">
        {#if active === 'general'}
          <GeneralSettings />
        {:else if active === 'keys'}
          <ApiKeySettings />
        {:else if active === 'oauth'}
          <OAuthProviders />
        {:else if active === 'mcp'}
          <McpSettings />
        {/if}
      </section>
    </div>
  </div>
{/if}
