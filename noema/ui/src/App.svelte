<script lang="ts">
  import { onMount } from 'svelte';
  import { getTransport, getCurrentUser, setCurrentUser, coreApi } from '@simply/client';
  import ActivityBar, { type ActivityId } from './lib/ActivityBar.svelte';
  import SidePanel from './lib/SidePanel.svelte';
  import ChatView from './lib/ChatView.svelte';
  import DocumentView from './lib/DocumentView.svelte';
  import ModelSelector from './lib/ModelSelector.svelte';
  import UserPicker from './lib/UserPicker.svelte';
  import SettingsModal from './lib/SettingsModal.svelte';
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

  interface BootUser {
    id: string;
    email: string | null;
  }

  async function resolveBootUser(): Promise<BootUser[]> {
    try {
      return await getTransport().rpc<BootUser[]>(
        'admin.list_users',
        'GET',
        '/admin/api/users',
      );
    } catch (e) {
      console.warn('[noema] list_users failed', e);
      return [];
    }
  }

  async function tauriAdminUserId(): Promise<string | null> {
    const internals = (window as any).__TAURI_INTERNALS__;
    if (!internals || typeof internals.invoke !== 'function') return null;
    try {
      const id = await internals.invoke('admin_user_id');
      return typeof id === 'string' && id.length > 0 ? id : null;
    } catch (e) {
      console.warn('[noema] admin_user_id invoke failed', e);
      return null;
    }
  }

  /**
   * Pick the user id that should drive this session — on every boot, not just
   * the first one. Covers three regressions we've hit:
   *   1. Empty localStorage on fresh install → fall back to the Tauri host's
   *      bootstrap user (email from settings.toml).
   *   2. Stored id no longer in the DB (db nuke+reimport) → clear + refall.
   *   3. Tauri host is unavailable → users[0] so we at least have *some*
   *      scope instead of anonymous (which hides data in conversations /
   *      documents stores).
   */
  async function syncAdminUserId() {
    const users = await resolveBootUser();
    const stored = getCurrentUser();
    if (stored && users.some((u) => u.id === stored)) return;

    const fromTauri = await tauriAdminUserId();
    if (fromTauri && users.some((u) => u.id === fromTauri)) {
      setCurrentUser(fromTauri);
      return;
    }
    if (users.length > 0) {
      setCurrentUser(users[0].id);
      return;
    }
    setCurrentUser(null);
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

  <SettingsModal open={showSettings} onClose={() => (showSettings = false)} />
</div>
