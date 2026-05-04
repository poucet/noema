<script lang="ts">
  import { MessageBubble as SharedMessageBubble } from '@simply/entity-ui';
  import type { DisplayMessage } from '../../lib/stores/chat.svelte';

  interface Props {
    message: DisplayMessage;
    streaming?: boolean;
  }
  let { message, streaming = false }: Props = $props();
</script>

<SharedMessageBubble
  {message}
  {streaming}
  baseBubbleClass="max-w-[80%] space-y-2 rounded-lg px-3 py-2 text-sm"
  userBubbleClass="bg-accent/20 text-fg"
  assistantBubbleClass="bg-white/5 text-fg"
  systemBubbleClass="bg-white/5 text-muted"
  streamingBubbleClass="border border-accent/30"
  entityChipClass="border-accent/40 bg-accent/10 text-accent"
/>
