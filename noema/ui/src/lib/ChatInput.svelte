<script lang="ts">
  type Props = {
    onSend: (text: string) => void;
    disabled?: boolean;
    isLoading?: boolean;
  };
  const { onSend, disabled = false, isLoading = false }: Props = $props();

  let text = $state('');
  let textareaEl: HTMLTextAreaElement | undefined = $state();

  function autoresize() {
    if (!textareaEl) return;
    textareaEl.style.height = 'auto';
    // Cap at ~10 lines so long drafts don't eat the chat area.
    textareaEl.style.height = Math.min(textareaEl.scrollHeight, 240) + 'px';
  }

  function send() {
    const trimmed = text.trim();
    if (!trimmed || disabled) return;
    onSend(trimmed);
    text = '';
    // Next tick — after Svelte applies the cleared value.
    queueMicrotask(autoresize);
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      send();
    }
  }
</script>

<div class="border-t border-gray-700 bg-background p-4">
  <div class="flex gap-2">
    <textarea
      bind:this={textareaEl}
      bind:value={text}
      oninput={autoresize}
      onkeydown={onKeydown}
      rows="1"
      placeholder={disabled ? 'Select a conversation to start chatting' : 'Message…'}
      {disabled}
      class="flex-1 resize-none rounded-lg border border-gray-700 bg-surface px-3 py-2 text-foreground placeholder-muted focus:border-teal-500 focus:outline-none"
    ></textarea>
    <button
      onclick={send}
      disabled={disabled || !text.trim() || isLoading}
      class="self-end rounded-lg bg-teal-600 px-4 py-2 font-medium text-white transition-colors hover:bg-teal-700 disabled:cursor-not-allowed disabled:opacity-50"
    >
      {isLoading ? '…' : 'Send'}
    </button>
  </div>
</div>
