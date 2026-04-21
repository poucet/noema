<script lang="ts">
  // Minimal shell — verifies the scaffold boots, the dark theme is wired, and
  // the daemon is reachable via @simply/client. Real components (ActivityBar,
  // Sidebar, ChatInput, MessageBubble, etc.) land in follow-up commits.

  import { onMount } from 'svelte';
  import { getTransport, coreApi } from '@simply/client';

  let status = $state<'checking' | 'ok' | 'error'>('checking');
  let version = $state<string | null>(null);
  let errorMessage = $state<string | null>(null);

  onMount(async () => {
    try {
      const t = getTransport();
      const core = coreApi(t);
      version = await core.version();
      status = 'ok';
    } catch (e) {
      status = 'error';
      errorMessage = e instanceof Error ? e.message : String(e);
    }
  });
</script>

<div class="flex h-full items-center justify-center">
  <div class="max-w-md rounded-lg bg-surface p-8 shadow-lg">
    <h1 class="mb-2 text-2xl font-semibold text-teal-400">Noema</h1>
    <p class="mb-6 text-sm text-muted">Svelte shell — daemon connectivity check.</p>

    {#if status === 'checking'}
      <p class="text-muted">Contacting daemon…</p>
    {:else if status === 'ok'}
      <p class="text-foreground">
        Daemon reachable.
        <span class="ml-2 rounded bg-elevated px-2 py-1 text-xs text-teal-300">
          v{version}
        </span>
      </p>
    {:else}
      <p class="text-amber-500">Daemon unreachable.</p>
      <pre class="mt-2 whitespace-pre-wrap break-words text-xs text-muted">{errorMessage}</pre>
    {/if}
  </div>
</div>
