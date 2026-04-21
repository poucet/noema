<!--
  List half of the entity browser. Takes an `EntitiesBrowser` and forwards
  its state to `EntityList`. Use this when the list and detail panes need to
  live in non-adjacent parts of the page (e.g. Noema's sidebar vs. main area).

  Auto-loads the browser on mount (idempotent — wrapping in a second pane
  doesn't re-fetch).
-->
<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import EntityList from './EntityList.svelte';
  import type { EntitiesBrowser } from './useEntitiesBrowser.svelte.ts';

  type Props = {
    browser: EntitiesBrowser;
    /** Admin surfaces the owner column; single-user apps hide it. */
    showOwner?: boolean;
  };

  let { browser, showOwner = false }: Props = $props();

  let handleUnload: (() => void) | null = null;

  onMount(() => {
    browser.load();
    handleUnload = () => browser.flushCurrent();
    window.addEventListener('beforeunload', handleUnload);
  });

  onDestroy(() => {
    if (typeof window !== 'undefined' && handleUnload) {
      window.removeEventListener('beforeunload', handleUnload);
    }
    browser.flushCurrent();
  });
</script>

<EntityList
  entities={browser.entities}
  selectedId={browser.selected?.id ?? null}
  loading={browser.loading}
  activeFilter={browser.typeFilter}
  searchQuery={browser.searchQuery}
  creating={browser.creating}
  newEntityTitle={browser.newEntityTitle}
  {showOwner}
  onsearch={(q) => browser.setSearchQuery(q)}
  onfilter={(id) => browser.setTypeFilter(id)}
  onselect={(id) => browser.selectEntity(id)}
  ondelete={(id) => browser.deleteEntity(id)}
  onstartCreate={() => browser.startCreate()}
  oncancelCreate={() => browser.cancelCreate()}
  onsubmitCreate={(t) => browser.createEntity(t)}
  ontitleInput={(v) => browser.setNewEntityTitle(v)}
/>
