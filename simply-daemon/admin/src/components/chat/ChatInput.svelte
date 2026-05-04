<script lang="ts">
  import { getTransport, type EntitySummary } from '@simply/client';
  import { createDocumentMentionController, DocumentMentionList } from '@simply/entity-ui';
  import { chatStore } from '../../lib/stores/chat.svelte';
  import VoiceControls from './VoiceControls.svelte';

  let text = $state('');
  let textarea: HTMLTextAreaElement;
  const mentions = createDocumentMentionController(getTransport());

  async function send() {
    const msg = text.trim();
    if (!msg || chatStore.isLoading) return;
    const content = mentions.parseContent(msg);
    text = '';
    mentions.close();
    if (textarea) textarea.style.height = 'auto';
    await chatStore.sendMessage(content);
  }

  function handleKeydown(e: KeyboardEvent) {
    if (mentions.open) {
      if (e.key === 'ArrowDown' && mentions.matches.length > 0) {
        e.preventDefault();
        mentions.moveSelection(1);
        return;
      }
      if (e.key === 'ArrowUp' && mentions.matches.length > 0) {
        e.preventDefault();
        mentions.moveSelection(-1);
        return;
      }
      if ((e.key === 'Enter' || e.key === 'Tab') && mentions.selectedMatch) {
        e.preventDefault();
        insertMention(mentions.selectedMatch);
        return;
      }
      if (e.key === 'Escape') {
        e.preventDefault();
        mentions.close();
        return;
      }
    }

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

  function updateMentionState() {
    if (!textarea || chatStore.isLoading) {
      mentions.close();
      return;
    }
    mentions.update(text, textarea.selectionStart ?? text.length, chatStore.isLoading);
  }

  function handleInput() {
    autoResize();
    updateMentionState();
  }

  function insertMention(doc: EntitySummary) {
    const inserted = mentions.insertMention(doc, text);
    text = inserted.value;

    queueMicrotask(() => {
      if (!textarea) return;
      textarea.focus();
      textarea.selectionStart = inserted.cursor;
      textarea.selectionEnd = inserted.cursor;
      autoResize();
    });
  }

  async function handleTranscription(transcribed: string) {
    const draft = text.trim();
    const combined = draft ? `${draft} ${transcribed}` : transcribed;
    text = '';
    mentions.close();
    queueMicrotask(autoResize);
    if (combined.trim() && !chatStore.isLoading) {
      await chatStore.sendMessage(mentions.parseContent(combined.trim()));
    }
  }
</script>

<div class="border-t border-border px-4 py-3">
  <div class="flex gap-2 items-end">
    <VoiceControls onTranscription={handleTranscription} />
    <div class="relative flex-1">
      {#if mentions.open}
        <DocumentMentionList
          {mentions}
          onSelect={insertMention}
          panelClass="absolute bottom-full left-0 right-0 z-20 mb-2 overflow-hidden rounded-lg border border-border bg-surface shadow-xl"
          itemClass="text-muted hover:bg-elevated hover:text-fg"
          selectedItemClass="bg-elevated text-fg"
          iconClass="text-accent"
        />
      {/if}

      <textarea
        bind:this={textarea}
        bind:value={text}
        oninput={handleInput}
        onkeydown={handleKeydown}
        onkeyup={updateMentionState}
        onclick={updateMentionState}
        onselect={updateMentionState}
        placeholder="Type a message… (Enter to send, Shift+Enter for newline)"
        rows="1"
        disabled={chatStore.isLoading}
        class="w-full bg-white/5 border border-border rounded-lg px-3 py-2 text-sm text-fg
               placeholder:text-muted/50 resize-none outline-none focus:border-accent/50
               disabled:opacity-50"
      ></textarea>
    </div>
    <button
      onclick={send}
      disabled={chatStore.isLoading || !text.trim()}
      class="px-4 py-2 bg-accent text-bg text-sm font-medium rounded-lg
             hover:bg-accent/80 disabled:opacity-30 disabled:cursor-not-allowed shrink-0"
    >Send</button>
  </div>
</div>
