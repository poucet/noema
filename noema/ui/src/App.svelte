<script lang="ts">
  import { onMount } from 'svelte';
  import {
    getTransport,
    setCurrentUser,
    coreApi,
    conversationApi,
    type ConversationInfo,
  } from '@simply/client';
  import ActivityBar, { type ActivityId } from './lib/ActivityBar.svelte';
  import SidePanel from './lib/SidePanel.svelte';

  const t = getTransport();
  const core = coreApi(t);
  const conversations$api = conversationApi(t);

  let active = $state<ActivityId>('conversations');
  let showSettings = $state(false);

  let daemonStatus = $state<'checking' | 'ok' | 'error'>('checking');
  let daemonVersion = $state<string | null>(null);
  let daemonError = $state<string | null>(null);

  let conversations = $state<ConversationInfo[]>([]);
  let currentConversationId = $state<string | null>(null);

  async function refreshConversations() {
    conversations = await conversations$api.listConversations();
  }

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
      daemonVersion = await core.version();
      daemonStatus = 'ok';
    } catch (e) {
      daemonStatus = 'error';
      daemonError = e instanceof Error ? e.message : String(e);
      return;
    }
    try {
      await refreshConversations();
      if (conversations.length > 0) currentConversationId = conversations[0].id;
    } catch (e) {
      console.error('[noema] failed to load conversations', e);
    }
  });

  async function handleNewConversation() {
    const id = await conversations$api.createConversation(null);
    await refreshConversations();
    currentConversationId = id;
  }

  function handleSelectConversation(id: string) {
    currentConversationId = id;
  }

  async function handleRenameConversation(id: string, name: string) {
    await conversations$api.renameConversation(id, name);
    await refreshConversations();
  }

  async function handleDeleteConversation(id: string) {
    await conversations$api.deleteConversation(id);
    if (currentConversationId === id) currentConversationId = null;
    await refreshConversations();
  }
</script>

<div class="flex h-screen bg-background">
  <ActivityBar
    {active}
    onChange={(id) => (active = id)}
    onOpenSettings={() => (showSettings = true)}
  />

  <SidePanel
    {active}
    {conversations}
    {currentConversationId}
    onNewConversation={handleNewConversation}
    onSelectConversation={handleSelectConversation}
    onRenameConversation={handleRenameConversation}
    onDeleteConversation={handleDeleteConversation}
  />

  <div class="flex min-w-0 flex-1 flex-col">
    <div class="flex items-center justify-between border-b border-gray-700 bg-background px-4 py-3">
      <h1 class="text-lg font-semibold text-foreground">Noema</h1>

      {#if daemonStatus === 'ok'}
        <span class="rounded bg-elevated px-2 py-1 text-xs text-teal-300">
          daemon v{daemonVersion}
        </span>
      {:else if daemonStatus === 'error'}
        <span class="rounded bg-red-900/50 px-2 py-1 text-xs text-red-200" title={daemonError}>
          daemon unreachable
        </span>
      {/if}
    </div>

    <div class="flex flex-1 items-center justify-center">
      {#if active === 'conversations'}
        {#if currentConversationId}
          <p class="text-muted">Chat for {currentConversationId} lands here.</p>
        {:else}
          <p class="text-muted">Select or create a conversation.</p>
        {/if}
      {:else}
        <p class="text-muted">Document view lands here.</p>
      {/if}
    </div>
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
