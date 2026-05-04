<script lang="ts">
  import { onMount } from 'svelte';
  import { entityApi, getTransport, type EntitySummary } from '@simply/client';

  type Props = {
    entityId: string;
    chipClass?: string;
  };

  let { entityId, chipClass = '' }: Props = $props();
  let entity = $state<EntitySummary | null>(null);

  onMount(async () => {
    try {
      entity = await entityApi(getTransport()).getEntity(entityId);
    } catch (e) {
      console.warn('[entity-ref-chip] failed to load entity:', e);
    }
  });

  const label = $derived(entity?.title ?? entityId.slice(0, 8));
  const tooltip = $derived(entity ? `${entity.kind} · ${entityId}` : entityId);
</script>

<span
  class="inline-flex max-w-full items-center gap-1 rounded border border-teal-500/40 bg-teal-900/30 px-2 py-0.5 text-xs text-teal-200 {chipClass}"
  title={tooltip}
>
  <svg class="h-3.5 w-3.5 shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true">
    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12h6m-6 4h6m2 5H7a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5.586a1 1 0 0 1 .707.293l5.414 5.414a1 1 0 0 1 .293.707V19a2 2 0 0 1-2 2z" />
  </svg>
  <span class="truncate">@{label}</span>
</span>
