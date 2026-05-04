<script lang="ts">
  import { onDestroy } from 'svelte';
  import type { EntitiesBrowser } from '@simply/entity-ui';
  import { adminApi } from './adminApi';

  type Props = {
    browser: EntitiesBrowser;
  };

  let { browser }: Props = $props();

  let busy = $state<'export' | 'scan' | null>(null);
  let includeFrontmatterIdentity = $state(false);
  let status = $state('');
  let statusTimer: ReturnType<typeof setTimeout> | undefined;

  onDestroy(() => {
    clearTimeout(statusTimer);
  });

  function setStatus(message: string) {
    status = message;
    clearTimeout(statusTimer);
    statusTimer = setTimeout(() => {
      status = '';
    }, 5000);
  }

  async function refreshDocuments() {
    const selectedId = browser.selected?.id ?? null;
    await browser.load();
    if (selectedId) {
      await browser.selectEntity(selectedId);
    }
  }

  async function exportDocuments() {
    busy = 'export';
    try {
      const summary = await adminApi.exportVaultDocuments({
        entityIds: [],
        includeFrontmatterIdentity,
      });
      setStatus(`${summary.exportedFiles} files exported`);
      await refreshDocuments();
    } catch (e) {
      setStatus(e instanceof Error ? e.message : String(e));
    } finally {
      busy = null;
    }
  }

  async function scanVault() {
    busy = 'scan';
    try {
      const summary = await adminApi.scanVault();
      setStatus(`${summary.contentSnapshots} updates, ${summary.conflicts} conflicts`);
      await refreshDocuments();
    } catch (e) {
      setStatus(e instanceof Error ? e.message : String(e));
    } finally {
      busy = null;
    }
  }
</script>

<div class="shrink-0 border-b border-gray-700 bg-surface px-3 py-2">
  <div class="flex items-center gap-1.5">
    <button
      class="inline-flex h-8 w-8 items-center justify-center rounded border border-gray-700 text-muted transition-colors hover:border-teal-500 hover:text-teal-300 disabled:cursor-default disabled:opacity-40"
      disabled={busy !== null}
      onclick={exportDocuments}
      title="Export documents to vault"
      aria-label="Export documents to vault"
    >
      <svg class="h-4 w-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 3v12m0 0 4-4m-4 4-4-4" />
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 17v2a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2v-2" />
      </svg>
    </button>

    <button
      class="inline-flex h-8 w-8 items-center justify-center rounded border border-gray-700 text-muted transition-colors hover:border-teal-500 hover:text-teal-300 disabled:cursor-default disabled:opacity-40"
      disabled={busy !== null}
      onclick={scanVault}
      title="Scan vault"
      aria-label="Scan vault"
    >
      <svg class="h-4 w-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 12a9 9 0 1 1-2.64-6.36" />
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 3v6h-6" />
      </svg>
    </button>

    <label class="flex min-w-0 flex-1 items-center gap-2 text-xs text-muted" title="Include Noema-owned identity fields in exported Markdown frontmatter">
      <input
        type="checkbox"
        bind:checked={includeFrontmatterIdentity}
        class="h-3.5 w-3.5 shrink-0 accent-teal-600"
        disabled={busy !== null}
      />
      <span class="truncate">Frontmatter IDs</span>
    </label>
  </div>

  {#if status}
    <div class="mt-1 truncate text-[11px] text-muted" title={status}>{status}</div>
  {/if}
</div>
