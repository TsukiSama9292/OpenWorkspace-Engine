<script>
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import { isAdmin } from '$lib/stores/auth';

  let users = $state([]);
  let loading = $state(true);

  onMount(async () => {
    if (!$isAdmin) {
      window.location.href = '/';
      return;
    }
    const res = await api.get('/users');
    if (res.data) {
      users = res.data.users;
    }
    loading = false;
  });
</script>

<div class="admin-users">
  <h1>Users</h1>

  {#if loading}
    <p>Loading...</p>
  {:else}
    <table>
      <thead>
        <tr>
          <th>Username</th>
          <th>Role</th>
          <th>Created</th>
        </tr>
      </thead>
      <tbody>
        {#each users as user}
          <tr>
            <td>{user.username}</td>
            <td>{user.role}</td>
            <td>{new Date(user.created_at).toLocaleDateString()}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
</div>

<style>
  .admin-users {
    max-width: 960px;
    margin: 0 auto;
  }
  h1 {
    color: var(--text-primary, #fff);
    margin-bottom: 1.5rem;
  }
  table {
    width: 100%;
    border-collapse: collapse;
  }
  th, td {
    padding: 0.75rem;
    text-align: left;
    border-bottom: 1px solid var(--border, #333);
  }
  th {
    color: var(--text-secondary, #888);
    font-weight: normal;
  }
  td {
    color: var(--text-primary, #fff);
  }
</style>
