<script lang="ts">
  import { MessageBubble as SharedMessageBubble } from '@simply/entity-ui';
  import type { DisplayMessage } from './stores/chat.svelte';

  type Props = { message: DisplayMessage; streaming?: boolean };
  const { message, streaming = false }: Props = $props();
</script>

<SharedMessageBubble
  {message}
  {streaming}
  baseBubbleClass="max-w-[85%] space-y-2 rounded-lg px-4 py-3"
  userBubbleClass="bg-teal-800 text-foreground"
  assistantBubbleClass="bg-surface text-foreground"
  systemBubbleClass="bg-elevated text-muted"
  streamingBubbleClass="border border-teal-500/40"
/>
