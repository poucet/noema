<script lang="ts">
  import { onMount } from 'svelte';
  import { getTransport, setCurrentUser, coreApi } from '@simply/client';
  import ActivityBar, { type ActivityId } from './lib/ActivityBar.svelte';
  import SidePanel from './lib/SidePanel.svelte';
  import ChatView from './lib/ChatView.svelte';
  import { chatStore } from './lib/stores/chat.svelte';

  let active = $state<ActivityId>('conversations');
  let showSettings = $state(false);

  let daemonStatus = $state<'checking' | 'ok' | 'error'>('checking');
  let daemonVersion = $state<string | null>(null);
  let daemonError = $state<string | null>(null);

  async function syncAdminUserId() {
    const internals = (window as any).__TAURI_INTERNALS__;
    if (!internals || typeof internals.invoke !== 'function') return;
    try {
      const userId = await internals.invoke('admin_user_id');
      if (typeof userId === 'string' && userId.length > 0) setCurrentUser(userId);
    } catch (e) {
      console.warn('[noema] failed to fetch admin user id', e);
    }
  }

  onMount(async () => {
    await syncAdminUserId();
    try {
      daemonVersion = await coreApi(getTransport()).version();
      daemonStatus = 'ok';
    } catch (e) {
      daemonStatus = 'error';
      daemonError = e instanceof Error ? e.message : String(e);
      return;
    }
    await chatStore.init();

    // Auto-select the most recent conversation if any.
    if (chatStore.conversations.length > 0 && chatStore.currentConversationId == null) {
      await chatStore.selectConversation(chatStore.conversations[0].id);
    }
  });

  // Best-effort session teardown on window close — WS alone might miss it.
  $effect(() => {
    const handler = () => chatStore.cleanup();
    window.addEventListener('beforeunload', handler);
    return () => window.removeEventListener('beforeunload', handler);
  });
</script>

<div class="flex h-screen bg-background">
  <ActivityBar
    {active}
    onChange={(id) => (active = id)}
    onOpenSettings={() => (showSettings = true)}
  />

  <SidePanel
    {active}
    conversations={chatStore.conversations}
    currentConversationId={chatStore.currentConversationId}
    onNewConversation={() => chatStore.newConversation()}
    onSelectConversation={(id) => chatStore.selectConversation(id)}
    onRenameConversation={(id, name) => chatStore.renameConversation(id, name)}
    onDeleteConversation={(id) => chatStore.deleteConversation(id)}
  />

  <div class="flex min-w-0 flex-1 flex-col">
    <div class="flex items-center justify-between border-b border-gray-700 bg-background px-4 py-3">
      <h1 class="text-lg font-semibold text-foreground">
        {chatStore.currentConversation?.name ?? 'Noema'}
      </h1>

      {#if daemonStatus === 'ok'}
        <span class="rounded bg-elevated px-2 py-1 text-xs text-teal-300">
          {chatStore.currentModelId || `daemon v${daemonVersion}`}
        </span>
      {:else if daemonStatus === 'error'}
        <span class="rounded bg-red-900/50 px-2 py-1 text-xs text-red-200" title={daemonError}>
          daemon unreachable
        </span>
      {/if}
    </div>

    {#if active === 'conversations'}
      <ChatView />
    {:else}
      <div class="flex flex-1 items-center justify-center">
        <p class="text-muted">Document view lands here.</p>
      </div>
    {/if}
  </div>

  {#if showSettings}
    <div
      class="fixed inset-0 z-10 flex items-center justify-center bg-black/50"
      role="dialog"
      aria-modal="true"
    >
      <div class="w-96 rounded-lg bg-surface p-6 shadow-xl">
        <div class="mb-4 flex items-center justify-between">
          <h2 class="text-lg font-semibold">Settings</h2>
          <button
            class="text-muted hover:text-foreground"
            aria-label="Close settings"
            onclick={() => (showSettings = false)}
          >
            ×
          </button>
        </div>
        <p class="text-sm text-muted">Settings panel lands here.</p>
      </div>
    </div>
  {/if}
</div>
