<script lang="ts">
  import { EntityBrowser } from '@simply/entity-ui';
  import { entityApi, getTransport } from '@simply/client';
  import VaultControls from './VaultControls.svelte';
</script>

<div class="flex h-[calc(100vh-3rem)] min-h-0 flex-col">
  <VaultControls />
  <div class="min-h-0 flex-1">
    <EntityBrowser client={entityApi(getTransport())} showOwner height="100%" />
  </div>
</div>
