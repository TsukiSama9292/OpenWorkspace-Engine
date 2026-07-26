<script lang="ts">
  import { onMount } from 'svelte';
  import { loadUsers } from './users-data';
  import type { AdminUser } from './users-data';

  let users = $state<AdminUser[]>([]);
  let loading = $state(true);

  onMount(async () => {
    users = await loadUsers();
    loading = false;
  });
</script>

<div class="max-w-4xl mx-auto">
  <h1 class="text-2xl font-bold text-surface-800 mb-6">Users</h1>

  {#if loading}
    <p class="text-surface-500">Loading...</p>
  {:else}
    <table class="w-full border-collapse">
      <thead>
        <tr>
          <th class="px-3 py-2 text-left text-surface-500 font-normal border-b border-surface-300">Username</th>
          <th class="px-3 py-2 text-left text-surface-500 font-normal border-b border-surface-300">Role</th>
          <th class="px-3 py-2 text-left text-surface-500 font-normal border-b border-surface-300">Created</th>
        </tr>
      </thead>
      <tbody>
        {#each users as user}
          <tr>
            <td class="px-3 py-2 text-surface-800 border-b border-surface-300">{user.username}</td>
            <td class="px-3 py-2 text-surface-800 border-b border-surface-300">{user.role}</td>
            <td class="px-3 py-2 text-surface-800 border-b border-surface-300">{new Date(user.created_at).toLocaleDateString()}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
</div>
