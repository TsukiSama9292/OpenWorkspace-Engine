<script>
  import { onMount } from 'svelte';
  import { api } from '$lib/api';

  let instances = $state([]);
  let loading = $state(true);

  onMount(async () => {
    const res = await api.get('/instances');
    if (res.data) {
      instances = res.data.instances;
    }
    loading = false;
  });

  function openVnc(token, e) {
    e.preventDefault();
    e.stopPropagation();
    window.open(`/vnc/${token}/`, '_blank');
  }
</script>

<div class="dashboard">
  <div class="header">
    <h1>Instances</h1>
    <a href="/instances/new/" class="btn">New Instance</a>
  </div>

  {#if loading}
    <p>Loading...</p>
  {:else if instances.length === 0}
    <p class="empty">No instances yet. Create one to get started.</p>
  {:else}
    <div class="grid">
      {#each instances as instance}
        <a href="/instances/{instance.id}/" class="card">
          <h3>{instance.name}</h3>
          <div class="card-footer">
            <span class="status" class:running={instance.status === 'running'}>
              {instance.status}
            </span>
            {#if instance.status === 'running' && instance.vnc_token}
              <button class="vnc-btn" onclick={(e) => openVnc(instance.vnc_token, e)} title="Open VNC">
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <rect x="2" y="3" width="20" height="14" rx="2"/>
                  <path d="M8 21h8M12 17v4"/>
                </svg>
                VNC
              </button>
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
  .vnc-btn {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 4px 10px;
    background: var(--accent, #4ecca3);
    color: #0a0a1a;
    border: none;
    border-radius: 4px;
    font-size: 0.8rem;
    font-weight: 500;
    cursor: pointer;
    text-decoration: none;
  }
  .vnc-btn:hover {
    opacity: 0.9;
  }
</style>
