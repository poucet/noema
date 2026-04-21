<script lang="ts">
  import ChatPage from '../chat/ChatPage.svelte';
  import EntitiesPage from '../EntitiesPage.svelte';
  import SettingsModal from './SettingsModal.svelte';

  type View = 'chat' | 'documents';
  let view: View = $state('chat');
  let settingsOpen = $state(false);

  type IconItem = { id: View; label: string; icon: string };
  const nav: IconItem[] = [
    { id: 'chat', label: 'Chat', icon: '💬' },
    { id: 'documents', label: 'Documents', icon: '📄' },
  ];
</script>

<div class="h-screen flex bg-bg text-fg overflow-hidden">
  <!-- Activity bar -->
  <nav class="w-14 shrink-0 bg-black/30 border-r border-border flex flex-col items-center py-2">
    {#each nav as item}
      <button
        class="w-10 h-10 mb-1 flex items-center justify-center rounded text-lg hover:bg-white/10"
        class:bg-white={view === item.id}
        class:bg-opacity-10={view === item.id}
        class:text-accent={view === item.id}
        title={item.label}
        aria-label={item.label}
        onclick={() => (view = item.id)}
      >
        {item.icon}
      </button>
    {/each}

    <div class="mt-auto">
      <button
        class="w-10 h-10 flex items-center justify-center rounded text-lg hover:bg-white/10"
        title="Settings"
        aria-label="Settings"
        onclick={() => (settingsOpen = true)}
      >⚙</button>
    </div>
  </nav>

  <!-- Main view -->
  <main class="flex-1 min-w-0">
    {#if view === 'chat'}
      <ChatPage />
    {:else if view === 'documents'}
      <EntitiesPage />
    {/if}
  </main>
</div>

<SettingsModal open={settingsOpen} onClose={() => (settingsOpen = false)} />
