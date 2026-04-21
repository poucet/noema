<script lang="ts">
  import type { ConversationInfo } from '@simply/client';

  type Props = {
    conversations: ConversationInfo[];
    currentId: string | null;
    onNew: () => void;
    onSelect: (id: string) => void;
    onRename: (id: string, name: string) => void;
    onDelete: (id: string) => void;
  };

  const { conversations, currentId, onNew, onSelect, onRename, onDelete }: Props = $props();

  let editingId = $state<string | null>(null);
  let editName = $state('');

  function startRename(conv: ConversationInfo) {
    editingId = conv.id;
    editName = conv.name ?? '';
  }

  function submitRename() {
    if (editingId) {
      onRename(editingId, editName);
      editingId = null;
    }
  }

  function formatDate(ms: number): string {
    const date = new Date(ms);
    const diffDays = Math.floor((Date.now() - date.getTime()) / (1000 * 60 * 60 * 24));
    if (diffDays === 0) return 'Today';
    if (diffDays === 1) return 'Yesterday';
    if (diffDays < 7) return `${diffDays} days ago`;
    return date.toLocaleDateString();
  }
</script>

<div class="border-b border-gray-700 p-4">
  <button
    class="w-full rounded-lg bg-teal-600 px-4 py-2 font-medium text-white transition-colors hover:bg-teal-700"
    onclick={onNew}
  >
    + New Chat
  </button>
</div>

<div class="flex-1 overflow-y-auto">
  {#if conversations.length === 0}
    <p class="p-4 text-center text-sm text-muted">No conversations yet</p>
  {:else}
    <ul class="py-2">
      {#each conversations as conv (conv.id)}
        {@const isCurrent = conv.id === currentId}
        {@const displayName = conv.name ?? `Chat (${conv.messageCount} messages)`}
        <li class="px-2">
          {#if editingId === conv.id}
            <div class="p-2">
              <!-- svelte-ignore a11y_autofocus -->
              <input
                type="text"
                bind:value={editName}
                onblur={submitRename}
                onkeydown={(e) => {
                  if (e.key === 'Enter') submitRename();
                  if (e.key === 'Escape') editingId = null;
                }}
                class="w-full rounded border border-gray-600 bg-elevated px-2 py-1 text-sm text-foreground"
                autofocus
              />
            </div>
          {:else}
            <div
              class="group flex items-center rounded-lg transition-colors {isCurrent
                ? 'bg-teal-900/50 text-teal-100'
                : 'text-gray-300 hover:bg-elevated'}"
            >
              <button
                class="flex min-w-0 flex-1 flex-col items-start p-3 text-left"
                onclick={() => onSelect(conv.id)}
              >
                <span class="w-full truncate text-sm font-medium">{displayName}</span>
                <span class="text-xs text-muted">{formatDate(conv.createdAt)}</span>
              </button>
              <div class="flex shrink-0 gap-1 pr-2 opacity-0 group-hover:opacity-100">
                <button
                  class="p-1 text-muted hover:text-foreground"
                  title="Rename"
                  onclick={() => startRename(conv)}
                >
                  <svg class="h-4 w-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path
                      stroke-linecap="round"
                      stroke-linejoin="round"
                      stroke-width="2"
                      d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z"
                    />
                  </svg>
                </button>
                <button
                  class="p-1 text-muted hover:text-red-500"
                  title="Delete"
                  onclick={() => onDelete(conv.id)}
                >
                  <svg class="h-4 w-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path
                      stroke-linecap="round"
                      stroke-linejoin="round"
                      stroke-width="2"
                      d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"
                    />
                  </svg>
                </button>
              </div>
            </div>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
</div>
