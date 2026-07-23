<script>
  import { onMount } from 'svelte';
  import { api } from '$lib/api';

  let workspaces = $state([]);
  let loading = $state(true);

  onMount(async () => {
    const res = await api.get('/workspaces');
    if (res.data) {
      workspaces = res.data.workspaces;
    }
    loading = false;
  });
</script>

<div class="dashboard">
  <div class="header">
    <h1>Workspaces</h1>
    <a href="/workspaces/new/" class="btn">New Workspace</a>
  </div>

  {#if loading}
    <p>Loading...</p>
  {:else if workspaces.length === 0}
    <p class="empty">No workspaces yet. Create one to get started.</p>
  {:else}
    <div class="grid">
      {#each workspaces as ws}
        <a href="/workspaces/{ws.id}/" class="card">
          <h3>{ws.name}</h3>
          <div class="card-footer">
            <span class="status" class:running={ws.status === 'running'} class:paused={ws.status === 'paused'}>
              {ws.status}
            </span>
            {#if ws.owner_username}
              <span class="owner">{ws.owner_username}</span>
            {/if}
          </div>
        </a>
      {/each}
    </div>
  {/if}
</div>

<style>
  .dashboard {
    max-width: 960px;
    margin: 0 auto;
  }
  .header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 1.5rem;
  }
  h1 {
    color: var(--text-primary, #fff);
  }
  .btn {
    padding: 0.5rem 1rem;
    background: var(--accent, #6366f1);
    color: white;
    text-decoration: none;
    border-radius: 4px;
  }
  .empty {
    color: var(--text-secondary, #888);
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
    gap: 1rem;
  }
  .card {
    background: var(--bg-secondary, #1a1a2e);
    padding: 1.25rem;
    border-radius: 8px;
    text-decoration: none;
    border: 1px solid var(--border, #333);
  }
  .card h3 {
    margin: 0 0 0.75rem;
    color: var(--text-primary, #fff);
  }
  .card-footer {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  .status {
    font-size: 0.85rem;
    color: var(--text-secondary, #888);
  }
  .status.running {
    color: #22c55e;
  }
  .status.paused {
    color: #f59e0b;
  }
  .owner {
    font-size: 0.8rem;
    color: var(--text-secondary, #888);
  }
</style>
