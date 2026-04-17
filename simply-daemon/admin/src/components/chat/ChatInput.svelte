<script lang="ts">
  import { chatStore } from '../../lib/stores/chat.svelte';
  import VoiceControls from './VoiceControls.svelte';

  let text = $state('');
  let textarea: HTMLTextAreaElement;

  async function send() {
    const msg = text.trim();
    if (!msg) return;
    text = '';
    if (textarea) textarea.style.height = 'auto';
    await chatStore.sendMessage(msg);
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      send();
    }
  }

  function autoResize() {
    if (!textarea) return;
    textarea.style.height = 'auto';
    textarea.style.height = Math.min(textarea.scrollHeight, 200) + 'px';
  }

  async function handleTranscription(transcribed: string) {
    // Auto-send voice transcriptions directly
    await chatStore.sendMessage(transcribed);
  }
</script>

<div class="border-t border-border px-4 py-3">
  <div class="flex gap-2 items-end">
    <VoiceControls onTranscription={handleTranscription} />
    <textarea
      bind:this={textarea}
      bind:value={text}
      oninput={autoResize}
      onkeydown={handleKeydown}
      placeholder="Type a message… (Enter to send, Shift+Enter for newline)"
      rows="1"
      disabled={chatStore.isLoading}
      class="flex-1 bg-white/5 border border-border rounded-lg px-3 py-2 text-sm text-fg
             placeholder:text-muted/50 resize-none outline-none focus:border-accent/50
             disabled:opacity-50"
    ></textarea>
    <button
      onclick={send}
      disabled={chatStore.isLoading || !text.trim()}
      class="px-4 py-2 bg-accent text-bg text-sm font-medium rounded-lg
             hover:bg-accent/80 disabled:opacity-30 disabled:cursor-not-allowed shrink-0"
    >Send</button>
  </div>
</div>
