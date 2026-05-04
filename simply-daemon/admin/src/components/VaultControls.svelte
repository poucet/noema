<script lang="ts">
  import { onMount } from 'svelte';
  import {
    type VaultConflictInfo,
    type VaultConflictResolutionAction,
    type VaultExportSummary,
    type VaultScanSummary,
  } from '@simply/client';
  import { api } from '../lib/api';

  let includeFrontmatterIdentity = $state(false);
  let conflicts = $state<VaultConflictInfo[]>([]);
  let busy = $state<string | null>(null);
  let message = $state('');
  let bindEntityIds = $state<Record<string, string>>({});

  onMount(() => {
    void refreshConflicts();
  });

  function exportMessage(summary: VaultExportSummary): string {
    return `Exported ${summary.exportedEntities} entities to ${summary.exportedFiles} files; skipped ${summary.skippedEntities}.`;
  }

  function scanMessage(summary: VaultScanSummary): string {
    return `Scanned ${summary.scannedFiles} files; ${summary.contentSnapshots} content snapshots, ${summary.assetProjections} asset projections, ${summary.conflicts} conflicts.`;
  }

  async function refreshConflicts() {
    conflicts = await api.listVaultConflicts();
  }

  async function exportDocuments() {
    busy = 'export';
    try {
      message = exportMessage(
        await api.exportVaultDocuments({
          entityIds: [],
          includeFrontmatterIdentity,
        }),
      );
      await refreshConflicts();
    } catch (e) {
      message = e instanceof Error ? e.message : String(e);
    } finally {
      busy = null;
    }
  }

  async function scanVault() {
    busy = 'scan';
    try {
      message = scanMessage(await api.scanVault());
      await refreshConflicts();
    } catch (e) {
      message = e instanceof Error ? e.message : String(e);
    } finally {
      busy = null;
    }
  }

  async function resolveConflict(
    conflict: VaultConflictInfo,
    action: VaultConflictResolutionAction,
    entityId: string | null = null,
  ) {
    busy = conflict.id;
    try {
      await api.resolveVaultConflict({
        conflictId: conflict.id,
        action,
        entityId,
      });
      message = `Resolved ${conflict.path}.`;
      await refreshConflicts();
    } catch (e) {
      message = e instanceof Error ? e.message : String(e);
    } finally {
      busy = null;
    }
  }

  function bindTarget(conflict: VaultConflictInfo): string | null {
    return bindEntityIds[conflict.id]?.trim() || conflict.entityId || null;
  }
</script>

<div class="border-b border-border bg-surface px-3 py-2">
  <div class="flex flex-wrap items-center gap-2">
    <button
      class="btn btn-primary px-3 py-1.5 text-xs"
      disabled={busy !== null}
      onclick={exportDocuments}
      title="Export documents"
    >
      <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><path d="M7 10l5 5 5-5"/><path d="M12 15V3"/></svg>
      Export
    </button>
    <button
      class="btn btn-ghost px-3 py-1.5 text-xs"
      disabled={busy !== null}
      onclick={scanVault}
      title="Scan vault"
    >
      <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M21 12a9 9 0 1 1-2.64-6.36"/><path d="M21 3v6h-6"/></svg>
      Scan
    </button>
    <label class="mb-0 flex items-center gap-2 text-xs text-muted">
      <input type="checkbox" bind:checked={includeFrontmatterIdentity} class="h-3.5 w-3.5" />
      Frontmatter IDs
    </label>
    {#if message}
      <span class="text-xs text-muted">{message}</span>
    {/if}
  </div>

  {#if conflicts.length > 0}
    <div class="mt-2 overflow-x-auto">
      <table class="mb-0">
        <thead>
          <tr>
            <th>Path</th>
            <th>Reason</th>
            <th>Entity</th>
            <th>Resolve</th>
          </tr>
        </thead>
        <tbody>
          {#each conflicts as conflict (conflict.id)}
            <tr>
              <td class="mono text-xs">{conflict.path}</td>
              <td><span class="badge badge-info">{conflict.reason}</span></td>
              <td class="mono text-xs">{conflict.entityId ?? conflict.observedEntityId ?? 'unmanaged'}</td>
              <td>
                <div class="flex flex-wrap items-center gap-1.5">
                  {#if conflict.entityId}
                    <button class="btn btn-ghost px-2 py-1 text-xs" disabled={busy !== null} onclick={() => resolveConflict(conflict, 'restore_original_id', conflict.entityId)}>Restore ID</button>
                    <button class="btn btn-ghost px-2 py-1 text-xs" disabled={busy !== null} onclick={() => resolveConflict(conflict, 'accept_new_path', conflict.entityId)}>Accept path</button>
                  {/if}
                  <button class="btn btn-ghost px-2 py-1 text-xs" disabled={busy !== null} onclick={() => resolveConflict(conflict, 'fork_as_new_document')}>Fork</button>
                  <button class="btn btn-ghost px-2 py-1 text-xs" disabled={busy !== null} onclick={() => resolveConflict(conflict, 'ignore')}>Ignore</button>
                  <input
                    class="max-w-48 px-2 py-1 text-xs"
                    placeholder="Entity ID"
                    value={bindEntityIds[conflict.id] ?? ''}
                    oninput={(e) => {
                      bindEntityIds = {
                        ...bindEntityIds,
                        [conflict.id]: (e.currentTarget as HTMLInputElement).value,
                      };
                    }}
                  />
                  <button class="btn btn-ghost px-2 py-1 text-xs" disabled={busy !== null || !bindTarget(conflict)} onclick={() => resolveConflict(conflict, 'bind_to_entity', bindTarget(conflict))}>Bind</button>
                </div>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
</div>
