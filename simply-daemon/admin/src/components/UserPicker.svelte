<script lang="ts">
  import { onMount } from 'svelte';
  import { api, setCurrentUser, getCurrentUser, type UserInfo } from '../lib/api';

  let users = $state<UserInfo[]>([]);
  let selected = $state<string | null>(null);

  onMount(async () => {
    try {
      users = await api.getUsers();
      // Validate the stored selection against the current users list —
      // after a DB nuke + reimport the localStorage id no longer exists,
      // and every `create_entity` would fail with a FK constraint error.
      const stored = getCurrentUser();
      const storedValid = stored != null && users.some(u => u.id === stored);
      if (storedValid) {
        selected = stored;
      } else if (users.length > 0) {
        selected = users[0].id;
        setCurrentUser(selected);
      } else {
        setCurrentUser(null);
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
