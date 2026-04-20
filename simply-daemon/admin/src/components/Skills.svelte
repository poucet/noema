<script lang="ts">
  import { onMount } from 'svelte';
  import { getTransport } from '../lib/transport';
  import { skillsApi } from '../lib/generated/api';
  import type { SkillInfo } from '../lib/generated/types';

  const skills = skillsApi(getTransport());

  let items = $state<SkillInfo[]>([]);
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

  function kindLabel(kind: string): string {
    if (kind === 'embedded') return 'Embedded';
    if (kind === 'ws') return 'WebSocket';
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
          </div>
        </div>
      {/each}
    </div>
  {/if}
</section>
