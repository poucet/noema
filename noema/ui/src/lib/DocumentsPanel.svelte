<script lang="ts">
  import { EntitySidebar } from '@simply/entity-ui';
  import { documentsBrowser } from './stores/documents.svelte';
</script>

<EntitySidebar browser={documentsBrowser()} />
