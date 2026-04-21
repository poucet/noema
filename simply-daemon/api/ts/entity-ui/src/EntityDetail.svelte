<!--
  Capability-dispatched entity detail pane.

  Dispatches purely on the `incomingCounts` / `hasContent` capabilities of
  the summary (no kind-specific logic beyond that). "Incoming under
  structure::contained_in" means this entity has children contained inside
  it (e.g. a tabbed doc's tabs, a directory's files).

  Consumers inject the data-loading callbacks so admin/noema can scope
  differently (e.g. admin hits the cross-user list, noema scopes to the
  current user).
-->
<script lang="ts">
  import type { EntitySummary, RelatedEntity, EntityContent } from '@simply/client';
  import MarkdownView from './MarkdownView.svelte';
  import DocumentEditor from './DocumentEditor.svelte';

  type Props = {
    entity: EntitySummary;
    /** True to mount straight into edit mode (e.g. entity was just created). */
    editOnMount?: boolean;
    /** Content of the currently-viewed entity (for flat docs: entity itself;
     *  for tabbed docs: whichever contained tab is selected). */
    viewEntity: EntitySummary | null;
    viewContent: EntityContent | null;
    /** Entities contained *inside* this one (incoming `structure::contained_in`). */
    contained: RelatedEntity[];
    creatingChild?: boolean;
    /** Callbacks injected by the consumer. */
    onopenChild: (child: EntitySummary) => void;
    onstartCreateChild?: () => void;
    oncancelCreateChild?: () => void;
    onsubmitCreateChild?: (title: string) => void;
    ondeleteChild?: (childId: string) => void;
    onsaveContent: (content: string) => void;
  };

  let {
    entity,
    editOnMount = false,
    viewEntity,
    viewContent,
    contained,
    creatingChild = false,
    onopenChild,
    onstartCreateChild,
    oncancelCreateChild,
    onsubmitCreateChild,
    ondeleteChild,
    onsaveContent,
  }: Props = $props();

  let editing = $state(editOnMount);
  let newChildTitle = $state('');

  // Reset editing state whenever the viewed entity changes, seeded by the
  // caller's editOnMount preference for fresh creations.
  $effect(() => {
    if (viewEntity) {
      editing = editOnMount;
    }
  });

  function autofocus(node: HTMLInputElement) {
    queueMicrotask(() => node.focus());
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      editing = false;
    }
  }

  const hasContained = $derived(
    (entity.incomingCounts['structure::contained_in'] ?? 0) > 0 || contained.length > 0,
  );
  const viewContentMarkdown = $derived(viewContent?.contentMarkdown ?? '');

  /** True if `title` already starts with an emoji (pictographic codepoint),
   *  e.g. imported Google Doc tabs that carry their own icon. Used to skip
   *  the default 📄 prefix so we don't double up. */
  function startsWithEmoji(title: string | null | undefined): boolean {
    return !!title && /^\p{Extended_Pictographic}/u.test(title);
  }

  function displayKind(kind: string): string {
    return kind.startsWith('document::') ? kind.slice('document::'.length) : kind;
  }
</script>

<div class="flex h-full min-h-0">
  {#if hasContained}
    <!-- Tab tree sidebar -->
    <div class="w-56 shrink-0 border-r border-border flex flex-col bg-surface">
      <div class="p-2 border-b border-border flex items-center justify-between">
        <span class="text-xs font-medium text-muted truncate">{entity.title ?? '(untitled)'}</span>
        {#if onstartCreateChild}
          {#if creatingChild}
            <form
              class="flex-1 ml-2"
              onsubmit={(e) => {
                e.preventDefault();
                const t = newChildTitle.trim();
                if (t) { onsubmitCreateChild?.(t); newChildTitle = ''; }
              }}
            >
              <input
                type="text"
                bind:value={newChildTitle}
                placeholder="Tab name"
                use:autofocus
                onkeydown={(e) => { if (e.key === 'Escape') oncancelCreateChild?.(); }}
                class="w-full px-1.5 py-0.5 text-xs bg-bg border border-border rounded text-fg placeholder:text-muted focus:outline-none focus:border-accent"
              />
            </form>
          {:else}
            <button
              class="text-xs text-muted hover:text-accent px-1"
              onclick={() => onstartCreateChild?.()}
            >+</button>
          {/if}
        {/if}
      </div>

      <div class="flex-1 overflow-y-auto">
        {#each contained as child (child.summary.id)}
          {@const isActive = viewEntity?.id === child.summary.id}
          <button
            class="w-full text-left py-1.5 pr-2 text-xs hover:bg-elevated transition-colors group flex items-center gap-1 pl-2
                   {isActive ? 'bg-elevated text-fg' : 'text-muted'}"
            onclick={() => onopenChild(child.summary)}
          >
            {#if !startsWithEmoji(child.summary.title)}
              <span class="shrink-0">📄</span>
            {/if}
            <span class="truncate flex-1">{child.summary.title ?? '(untitled)'}</span>
            {#if ondeleteChild && contained.length > 1}
              <span
                role="button"
                tabindex="-1"
                class="text-muted hover:text-danger opacity-0 group-hover:opacity-100 shrink-0 p-0.5 -m-0.5 rounded hover:bg-danger/10"
                onclick={(e) => { e.stopPropagation(); ondeleteChild?.(child.summary.id); }}
                onkeydown={(e) => { if (e.key === 'Enter') { e.stopPropagation(); ondeleteChild?.(child.summary.id); } }}
                aria-label="Delete"
              >
                <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="3 6 5 6 21 6"/><path d="M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6"/><path d="M10 11v6"/><path d="M14 11v6"/><path d="M9 6V4a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1v2"/></svg>
              </span>
            {/if}
          </button>
        {/each}
      </div>
    </div>
  {/if}

  <!-- Content pane -->
  <div class="flex-1 flex flex-col min-w-0 relative" onkeydown={onKeydown} role="region">
    {#if viewEntity}
      <div class="flex items-center justify-between gap-2 px-3 py-1.5 border-b border-border text-xs">
        <span class="text-muted">
          {displayKind(viewEntity.kind)}
          {#if viewEntity.origin}
            · <span class="text-muted/60">{viewEntity.origin}</span>
          {/if}
        </span>
        <button
          class="p-1 rounded transition-colors hover:bg-elevated text-muted hover:text-accent"
          title={editing ? 'Save & view (Esc)' : 'Edit'}
          onclick={() => editing = !editing}
          aria-label={editing ? 'Save and view' : 'Edit'}
        >
          {#if editing}
            <!-- Check icon -->
            <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"/></svg>
          {:else}
            <!-- Pencil icon -->
            <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 20h9"/><path d="M16.5 3.5a2.121 2.121 0 0 1 3 3L7 19l-4 1 1-4 12.5-12.5z"/></svg>
          {/if}
        </button>
      </div>
      {#if editing}
        {#key viewEntity.id}
          <DocumentEditor
            content={viewContentMarkdown}
            onsave={onsaveContent}
            oncommit={() => editing = false}
          />
        {/key}
      {:else}
        <div class="flex-1 overflow-auto p-6">
          <MarkdownView content={viewContentMarkdown} />
        </div>
      {/if}
    {:else if hasContained}
      <div class="flex items-center justify-center h-full text-muted text-sm">
        No tabs yet — click + to create one
      </div>
    {:else}
      <div class="flex items-center justify-center h-full text-muted text-sm">
        Select or create an entity
      </div>
    {/if}
  </div>
</div>
