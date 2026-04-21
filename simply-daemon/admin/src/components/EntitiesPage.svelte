<script lang="ts">
  import { EntityBrowser } from '@simply/entity-ui';
  import { entityApi, getTransport } from '@simply/client';
</script>

<EntityBrowser client={entityApi(getTransport())} showOwner height="calc(100vh - 3rem)" />
