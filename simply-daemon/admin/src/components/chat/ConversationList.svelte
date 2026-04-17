<script lang="ts">
  import { chatStore } from '../../lib/stores/chat.svelte';

  let editingId = $state<string | null>(null);
  let editName = $state('');

  function startRename(id: string, currentName: string | null) {
    editingId = id;
    editName = currentName || '';
  }

  async function commitRename() {
    if (editingId && editName.trim()) {
      await chatStore.renameConversation(editingId, editName.trim());
    }
    editingId = null;
  }
</script>

<div class="flex-1 overflow-y-auto">
  {#each chatStore.conversations as conv}
    <div
      role="button"
      tabindex="0"
      class="w-full text-left px-3 py-2 text-sm border-b border-border/50 hover:bg-white/5 flex items-center gap-2 group cursor-pointer
             {chatStore.currentConversationId === conv.id ? 'bg-accent/10 text-accent' : 'text-muted'}"
      onclick={() => chatStore.selectConversation(conv.id)}
      onkeydown={(e) => { if (e.key === 'Enter') chatStore.selectConversation(conv.id); }}
    >
      {#if editingId === conv.id}
        <input
          class="flex-1 bg-transparent border border-border rounded px-1 py-0.5 text-xs text-fg outline-none"
          bind:value={editName}
          onkeydown={(e) => { if (e.key === 'Enter') commitRename(); if (e.key === 'Escape') editingId = null; }}
          onclick={(e) => e.stopPropagation()}
          autofocus
        />
      {:else}
        <span class="flex-1 truncate">{conv.name || 'Untitled'}</span>
        <span class="text-xs text-muted/50">{conv.messageCount}</span>
      {/if}

      <span class="hidden group-hover:flex gap-1 text-muted/50">
        <button
          class="hover:text-fg text-xs"
          title="Rename"
          onclick={(e) => { e.stopPropagation(); startRename(conv.id, conv.name); }}
        >✎</button>
        <button
          class="hover:text-red-400 text-xs"
          title="Delete"
          onclick={(e) => { e.stopPropagation(); chatStore.deleteConversation(conv.id); }}
        >✕</button>
      </span>
    </div>
  {:else}
    <div class="px-3 py-4 text-xs text-muted/50 text-center">No conversations yet</div>
  {/each}
</div>
