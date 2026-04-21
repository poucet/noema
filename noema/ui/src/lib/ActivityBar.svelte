<script lang="ts" module>
  export type ActivityId = 'conversations' | 'documents';
</script>

<script lang="ts">
  type Props = {
    active: ActivityId;
    onChange: (id: ActivityId) => void;
    onOpenSettings: () => void;
  };
  const { active, onChange, onOpenSettings }: Props = $props();

  const activities: { id: ActivityId; label: string; path: string }[] = [
    {
      id: 'conversations',
      label: 'Conversations',
      path: 'M8 12h.01M12 12h.01M16 12h.01M21 12c0 4.418-4.03 8-9 8a9.863 9.863 0 01-4.255-.949L3 20l1.395-3.72C3.512 15.042 3 13.574 3 12c0-4.418 4.03-8 9-8s9 3.582 9 8z',
    },
    {
      id: 'documents',
      label: 'Documents',
      path: 'M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z',
    },
  ];
</script>

<div class="flex w-12 flex-col items-center border-r border-gray-700 bg-background py-2">
  <div class="flex flex-col items-center">
    {#each activities as activity}
      {@const isActive = activity.id === active}
      <button
        class="relative flex h-12 w-12 items-center justify-center transition-colors {isActive
          ? 'text-foreground'
          : 'text-muted hover:text-gray-300'}"
        title={activity.label}
        onclick={() => onChange(activity.id)}
      >
        {#if isActive}
          <div class="absolute bottom-2 left-0 top-2 w-0.5 bg-teal-500"></div>
        {/if}
        <svg class="h-6 w-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="2"
            d={activity.path}
          />
        </svg>
      </button>
    {/each}
  </div>

  <div class="flex-1"></div>

  <button
    class="flex h-12 w-12 items-center justify-center text-muted transition-colors hover:text-gray-300"
    title="Settings"
    onclick={onOpenSettings}
  >
    <svg class="h-6 w-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
      <path
        stroke-linecap="round"
        stroke-linejoin="round"
        stroke-width="2"
        d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z"
      />
      <path
        stroke-linecap="round"
        stroke-linejoin="round"
        stroke-width="2"
        d="M15 12a3 3 0 11-6 0 3 3 0 016 0z"
      />
    </svg>
  </button>
</div>
