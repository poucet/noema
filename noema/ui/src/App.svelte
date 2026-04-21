<script lang="ts">
  import { onMount } from 'svelte';
  import { getTransport, coreApi } from '@simply/client';
  import ActivityBar, { type ActivityId } from './lib/ActivityBar.svelte';
  import SidePanel from './lib/SidePanel.svelte';

  let active = $state<ActivityId>('conversations');
  let showSettings = $state(false);

  let daemonStatus = $state<'checking' | 'ok' | 'error'>('checking');
  let daemonVersion = $state<string | null>(null);
  let daemonError = $state<string | null>(null);

  onMount(async () => {
    try {
      daemonVersion = await coreApi(getTransport()).version();
      daemonStatus = 'ok';
    } catch (e) {
      daemonStatus = 'error';
      daemonError = e instanceof Error ? e.message : String(e);
    }
  });
</script>

<div class="flex h-screen bg-background">
  <ActivityBar
    {active}
    onChange={(id) => (active = id)}
    onOpenSettings={() => (showSettings = true)}
  />

  <SidePanel {active} />

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
      <p class="text-muted">
        {active === 'conversations' ? 'Chat area lands here.' : 'Document view lands here.'}
      </p>
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
