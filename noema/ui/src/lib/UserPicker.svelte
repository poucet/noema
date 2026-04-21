<script lang="ts">
  import { onMount } from 'svelte';
  import { getTransport, getCurrentUser, setCurrentUser } from '@simply/client';

  // Admin endpoint — lives at /admin/api/users, outside the generated API
  // surface. We just hit it directly via the transport rather than pulling in
  // admin's local api.ts facade.
  interface UserInfo {
    id: string;
    email: string | null;
  }

  let users = $state<UserInfo[]>([]);
  let currentId = $state<string | null>(null);
  let open = $state(false);

  onMount(async () => {
    currentId = getCurrentUser();
    try {
      users = await getTransport().rpc<UserInfo[]>(
        'admin.list_users',
        'GET',
        '/admin/api/users',
      );
    } catch (e) {
      console.error('[user-picker] failed to load users', e);
    }
  });

  const currentUser = $derived(users.find((u) => u.id === currentId));

  function label(u: UserInfo | undefined): string {
    if (!u) return currentId?.slice(0, 8) ?? 'anonymous';
    return u.email ?? `anon · ${u.id.slice(0, 8)}`;
  }

  function select(id: string) {
    if (id === currentId) {
      open = false;
      return;
    }
    setCurrentUser(id);
    // Reload so all stores (chat, documents) re-fetch under the new identity.
    window.location.reload();
  }
</script>

<div class="relative">
  <button
    class="flex items-center gap-1 rounded border border-gray-700 bg-elevated px-2 py-0.5 text-xs text-muted transition-colors hover:border-teal-500/50 hover:text-foreground"
    onclick={() => (open = !open)}
  >
    <span class="max-w-48 truncate">{label(currentUser)}</span>
    <svg class="h-2.5 w-2.5 shrink-0 opacity-50" fill="none" viewBox="0 0 24 24" stroke="currentColor">
      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" />
    </svg>
  </button>

  {#if open}
    <div
      class="fixed inset-0 z-10"
      role="button"
      tabindex="-1"
      aria-label="Close user picker"
      onclick={() => (open = false)}
      onkeydown={(e) => {
        if (e.key === 'Escape') open = false;
      }}
    ></div>

    <div
      class="absolute right-0 top-full z-20 mt-1 w-64 overflow-hidden rounded-lg border border-gray-700 bg-surface shadow-xl"
    >
      <div class="max-h-80 overflow-y-auto">
        {#if users.length === 0}
          <div class="px-3 py-4 text-center text-xs text-muted">No users</div>
        {:else}
          {#each users as user (user.id)}
            {@const isActive = user.id === currentId}
            <button
              class="flex w-full items-center gap-2 px-3 py-2 text-left text-xs hover:bg-elevated {isActive
                ? 'bg-teal-900/30 text-teal-300'
                : 'text-gray-300'}"
              onclick={() => select(user.id)}
            >
              {#if isActive}
                <span class="h-1.5 w-1.5 shrink-0 rounded-full bg-teal-400"></span>
              {:else}
                <span class="h-1.5 w-1.5 shrink-0"></span>
              {/if}
              <span class="flex-1 truncate">{label(user)}</span>
              <span class="text-[10px] text-muted/60">{user.id.slice(0, 8)}</span>
            </button>
          {/each}
        {/if}
      </div>
    </div>
  {/if}
</div>
