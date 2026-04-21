<!--
  Detail half of the entity browser. Takes an `EntitiesBrowser` and renders
  either the capability-dispatched detail view (when an entity is selected)
  or an empty-state message.
-->
<script lang="ts">
  import EntityDetail from './EntityDetail.svelte';
  import type { EntitiesBrowser } from './useEntitiesBrowser.svelte.ts';

  type Props = {
    browser: EntitiesBrowser;
    /** Empty-state message shown when nothing is selected. */
    emptyMessage?: string;
  };

  let { browser, emptyMessage = 'Select an entity or create a new one' }: Props = $props();
</script>

{#if browser.selected}
  <EntityDetail
    entity={browser.selected}
    editOnMount={browser.editOnMount}
    viewEntity={browser.viewEntity}
    viewContent={browser.viewContent}
    contained={browser.contained}
    creatingChild={browser.creatingChild}
    onopenChild={(c) => browser.openChild(c)}
    onstartCreateChild={() => browser.startCreateChild()}
    oncancelCreateChild={() => browser.cancelCreateChild()}
    onsubmitCreateChild={(t) => browser.createChild(t)}
    ondeleteChild={(id) => browser.deleteChild(id)}
    onsaveContent={(c) => browser.saveContent(c)}
  />
{:else}
  <div class="flex h-full items-center justify-center text-muted text-sm">
    {emptyMessage}
  </div>
{/if}
