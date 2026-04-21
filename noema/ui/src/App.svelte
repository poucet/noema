<script lang="ts">
  import { onMount } from 'svelte';
  import { getTransport, getCurrentUser, setCurrentUser, coreApi } from '@simply/client';
  import ActivityBar, { type ActivityId } from './lib/ActivityBar.svelte';
  import SidePanel from './lib/SidePanel.svelte';
  import ChatView from './lib/ChatView.svelte';
  import DocumentView from './lib/DocumentView.svelte';
  import ModelSelector from './lib/ModelSelector.svelte';
  import UserPicker from './lib/UserPicker.svelte';
  import { chatStore } from './lib/stores/chat.svelte';
  import { documentsBrowser } from './lib/stores/documents.svelte';

  // Active activity persists across reloads — otherwise a UserPicker switch
  // (which full-page-reloads) always snaps back to conversations.
  const ACTIVE_KEY = 'noema-active-activity';
  function loadActive(): ActivityId {
    const stored = localStorage.getItem(ACTIVE_KEY);
    return stored === 'documents' ? 'documents' : 'conversations';
  }
  let active = $state<ActivityId>(loadActive());
  $effect(() => {
    localStorage.setItem(ACTIVE_KEY, active);
  });
  let showSettings = $state(false);

  let daemonStatus = $state<'checking' | 'ok' | 'error'>('checking');
  let daemonVersion = $state<string | null>(null);
  let daemonError = $state<string | null>(null);

  async function syncAdminUserId() {
    // Respect an explicit UserPicker choice across reloads — only seed the
    // user id from the Tauri host on first boot (when localStorage is empty).
    if (getCurrentUser()) return;
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
    // Documents load lazily on first sidebar mount (EntitySidebar handles it).
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
        {#if active === 'conversations'}
          {chatStore.currentConversation?.name ?? 'Noema'}
        {:else}
          {documentsBrowser().selected?.title ?? 'Noema'}
        {/if}
      </h1>

      <div class="flex items-center gap-2">
        {#if daemonStatus === 'ok'}
          {#if active === 'conversations'}
            <ModelSelector />
          {:else}
            <span class="rounded bg-elevated px-2 py-1 text-xs text-teal-300">
              daemon v{daemonVersion}
            </span>
          {/if}
          <UserPicker />
        {:else if daemonStatus === 'error'}
          <span class="rounded bg-red-900/50 px-2 py-1 text-xs text-red-200" title={daemonError}>
            daemon unreachable
          </span>
        {/if}
      </div>
    </div>

    {#if active === 'conversations'}
      <ChatView />
    {:else}
      <DocumentView />
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
