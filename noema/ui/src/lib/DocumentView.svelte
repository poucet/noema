<script lang="ts">
  import { EntityPane } from '@simply/entity-ui';
  import { documentsBrowser } from './stores/documents.svelte';
</script>

<EntityPane browser={documentsBrowser()} emptyMessage="Select a document or create a new one." />
