<script lang="ts">
  import { onMount } from 'svelte';
  import { getTransport } from '../lib/transport';
  import { skillsApi } from '../lib/generated/api';
  import type { SkillInfo, McpTool } from '../lib/generated/types';

  const skills = skillsApi(getTransport());

  let items = $state<SkillInfo[]>([]);
  let expandedId = $state<string | null>(null);
  let toolsById = $state<Record<string, McpTool[]>>({});
  let error = $state<string | null>(null);
  let loading = $state(false);

  onMount(() => refresh());

  async function refresh() {
    loading = true;
    try {
      items = await skills.listSkills();
    } catch (e) {
      console.error('[skills]', e);
      error = `${e}`;
    }
    loading = false;
  }

  async function toggle(skill: SkillInfo) {
    if (expandedId === skill.id) {
      expandedId = null;
      return;
    }
    expandedId = skill.id;
    if (!toolsById[skill.id]) {
      try {
        toolsById[skill.id] = await skills.listSkillTools(skill.id);
      } catch (e) {
        console.error('[skills] load tools failed:', e);
        error = `Failed to load tools: ${e}`;
      }
    }
  }

  function kindLabel(kind: string): string {
    if (kind === 'in-process') return 'In-process';
    if (kind === 'remote') return 'Remote';
    return kind;
  }
</script>

<section class="space-y-4">
  <div class="flex items-center justify-between">
    <div>
      <h2 class="text-lg font-medium">Skills</h2>
      <p class="text-xs text-muted mt-1">
        In-process and client-registered tool providers. Skills are registered by their hosting client at startup — not editable here.
      </p>
    </div>
    <button
      class="text-xs px-3 py-1 rounded bg-white/5 text-muted hover:text-fg"
      onclick={refresh}
    >Refresh</button>
  </div>

  {#if error}
    <div class="px-3 py-2 bg-red-900/30 text-red-300 text-xs rounded">
      {error}
      <button class="ml-2 underline" onclick={() => error = null}>dismiss</button>
    </div>
  {/if}

  {#if loading && items.length === 0}
    <div class="text-sm text-muted">Loading…</div>
  {:else if items.length === 0}
    <div class="text-sm text-muted">No skills registered.</div>
  {:else}
    <div class="space-y-2">
      {#each items as skill}
        <div class="border border-border rounded-lg p-3">
          <div class="flex items-center gap-3">
            <div class="flex-1 min-w-0">
              <div class="text-sm font-medium truncate">{skill.displayName}</div>
              <div class="text-xs text-muted/70 truncate">{skill.id}</div>
              <div class="text-xs mt-1 flex items-center gap-2 flex-wrap">
                <span class="text-muted">{kindLabel(skill.kind)}</span>
                <span class="text-muted">·</span>
                <span class={skill.isConnected ? 'text-green-400' : 'text-red-400'}>
                  {skill.isConnected ? 'connected' : 'disconnected'}
                </span>
                <span class="text-muted">·</span>
                <span class="text-muted">{skill.toolCount} tool(s)</span>
                {#if skill.oauthProviderIds.length > 0}
                  <span class="text-muted">· needs</span>
                  {#each skill.oauthProviderIds as pid}
                    <span class="px-2 py-0.5 rounded bg-accent/10 text-accent text-[11px]">{pid}</span>
                  {/each}
                {/if}
              </div>
            </div>
            <div class="shrink-0">
              <button
                class="text-xs px-2 py-1 rounded bg-white/5 text-muted hover:text-fg"
                onclick={() => toggle(skill)}
              >{expandedId === skill.id ? 'Hide' : 'Tools'}</button>
            </div>
          </div>

          {#if expandedId === skill.id}
            <div class="mt-3 border-t border-border/50 pt-2">
              {#if toolsById[skill.id]}
                {#if toolsById[skill.id].length === 0}
                  <div class="text-xs text-muted">No tools.</div>
                {:else}
                  <div class="grid gap-1">
                    {#each toolsById[skill.id] as tool}
                      <div class="text-xs p-2 bg-black/20 rounded">
                        <div class="font-mono text-accent">{tool.name}</div>
                        {#if tool.description}
                          <div class="text-muted mt-0.5">{tool.description}</div>
                        {/if}
                      </div>
                    {/each}
                  </div>
                {/if}
              {:else}
                <div class="text-xs text-muted">Loading tools…</div>
              {/if}
            </div>
          {/if}
        </div>
      {/each}
    </div>
  {/if}
</section>
