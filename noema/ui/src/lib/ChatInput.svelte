<script lang="ts">
  import { getTransport, type EntitySummary, type InputContent } from '@simply/client';
  import { createDocumentMentionController, DocumentMentionList } from '@simply/entity-ui';
  import VoiceControls from './VoiceControls.svelte';

  type Props = {
    onSend: (content: InputContent[]) => void;
    disabled?: boolean;
    isLoading?: boolean;
  };
  const { onSend, disabled = false, isLoading = false }: Props = $props();

  let text = $state('');
  let textareaEl: HTMLTextAreaElement | undefined = $state();
  const mentions = createDocumentMentionController(getTransport());

  function autoresize() {
    if (!textareaEl) return;
    textareaEl.style.height = 'auto';
    // Cap at ~10 lines so long drafts don't eat the chat area.
    textareaEl.style.height = Math.min(textareaEl.scrollHeight, 240) + 'px';
  }

  function updateMentionState() {
    if (!textareaEl || disabled) {
      mentions.close();
      return;
    }

    mentions.update(text, textareaEl.selectionStart ?? text.length, disabled);
  }

  function insertMention(doc: EntitySummary) {
    const inserted = mentions.insertMention(doc, text);
    text = inserted.value;

    queueMicrotask(() => {
      if (!textareaEl) return;
      textareaEl.focus();
      textareaEl.selectionStart = inserted.cursor;
      textareaEl.selectionEnd = inserted.cursor;
      autoresize();
    });
  }

  function send() {
    const trimmed = text.trim();
    if (!trimmed || disabled || isLoading) return;
    onSend(mentions.parseContent(trimmed));
    text = '';
    mentions.close();
    // Next tick — after Svelte applies the cleared value.
    queueMicrotask(autoresize);
  }

  function onKeydown(e: KeyboardEvent) {
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

  function onInput() {
    autoresize();
    updateMentionState();
  }

  // Autosend transcribed speech so voice feels end-to-end. If the user had
  // a draft in the textarea, prepend it so we don't lose their typing.
  function onTranscription(transcript: string) {
    const draft = text.trim();
    const combined = draft ? `${draft} ${transcript}` : transcript;
    text = '';
    mentions.close();
    queueMicrotask(autoresize);
    if (combined.trim() && !disabled && !isLoading) onSend(mentions.parseContent(combined.trim()));
  }
</script>

<div class="border-t border-gray-700 bg-background p-4">
  <div class="flex items-end gap-2">
    <div class="relative flex-1">
      {#if mentions.open}
        <DocumentMentionList
          {mentions}
          onSelect={insertMention}
          panelClass="absolute bottom-full left-0 right-0 z-20 mb-2 overflow-hidden rounded-lg border border-gray-700 bg-surface shadow-xl"
          itemClass="text-muted hover:bg-elevated hover:text-foreground"
          selectedItemClass="bg-elevated text-foreground"
          iconClass="text-teal-400"
        />
      {/if}

      <textarea
        bind:this={textareaEl}
        bind:value={text}
        oninput={onInput}
        onkeydown={onKeydown}
        onkeyup={updateMentionState}
        onclick={updateMentionState}
        onselect={updateMentionState}
        rows="1"
        placeholder={disabled ? 'Select a conversation to start chatting' : 'Message…'}
        {disabled}
        class="w-full resize-none rounded-lg border border-gray-700 bg-surface px-3 py-2 text-foreground placeholder-muted focus:border-teal-500 focus:outline-none"
      ></textarea>
    </div>
    <VoiceControls {onTranscription} />
    <button
      onclick={send}
      disabled={disabled || !text.trim() || isLoading}
      class="rounded-lg bg-teal-600 px-4 py-2 font-medium text-white transition-colors hover:bg-teal-700 disabled:cursor-not-allowed disabled:opacity-50"
    >
      {isLoading ? '…' : 'Send'}
    </button>
  </div>
</div>
