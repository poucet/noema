<!--
  Full entity-browser shell: sidebar + detail pane, sharing a single browser
  instance created from the passed client. For use when both halves live
  next to each other (admin). If your layout needs the list and detail in
  separate parts of the page (Noema), create the browser yourself via
  `createEntitiesBrowser` and mount `<EntitySidebar>` / `<EntityPane>`
  independently.
-->
<script lang="ts">
  import type { Transport } from '@simply/client';
  import EntitySidebar from './EntitySidebar.svelte';
  import EntityPane from './EntityPane.svelte';
  import {
    createEntitiesBrowser,
    type EntityClient,
  } from './useEntitiesBrowser.svelte.ts';

  type Props = {
    /** Generated `entityApi(transport)` client, or a `Transport` to wrap. */
    client: EntityClient | Transport;
    showOwner?: boolean;
    /** CSS height for the root container. */
    height?: string;
    emptyMessage?: string;
  };

  let { client, showOwner = false, height = '100%', emptyMessage }: Props = $props();

  const browser = createEntitiesBrowser(client);
</script>

<div class="flex min-h-0" style:height>
  <EntitySidebar {browser} {showOwner} />
  <div class="flex-1 min-w-0">
    <EntityPane {browser} {emptyMessage} />
  </div>
</div>
