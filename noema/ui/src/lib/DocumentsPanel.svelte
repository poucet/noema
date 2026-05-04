<script lang="ts">
  import { EntitySidebar } from '@simply/entity-ui';
  import { documentsBrowser } from './stores/documents.svelte';
  import VaultControls from './VaultControls.svelte';

  const browser = documentsBrowser();
</script>

<div class="flex h-full min-h-0 flex-col">
  <VaultControls {browser} />
  <div class="min-h-0 flex-1">
    <EntitySidebar {browser} />
  </div>
</div>
