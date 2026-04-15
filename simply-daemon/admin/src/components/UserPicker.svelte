<script lang="ts">
  import { onMount } from 'svelte';
  import { api, setCurrentUser, getCurrentUser, type UserInfo } from '../lib/api';

  let users = $state<UserInfo[]>([]);
  let selected = $state<string | null>(null);

  onMount(async () => {
    try {
      users = await api.getUsers();
      // Auto-select first user if none selected
      if (users.length > 0 && !getCurrentUser()) {
        selected = users[0].id;
        setCurrentUser(selected);
      } else {
        selected = getCurrentUser();
      }
    } catch (e) {
      console.error('Failed to load users:', e);
    }
  });

  function onChange(e: Event) {
    const value = (e.target as HTMLSelectElement).value;
    selected = value || null;
    setCurrentUser(selected);
    // Reload page to refresh data with new context
    window.location.reload();
  }
</script>

{#if users.length > 0}
  <select
    value={selected ?? ''}
    onchange={onChange}
    class="text-xs px-2 py-1 bg-bg border border-border rounded text-fg"
  >
    {#each users as user}
      <option value={user.id}>{user.email ?? user.id}</option>
    {/each}
  </select>
{/if}
