<script>
  import { page } from '$app/stores';
  import { onMount } from 'svelte';
  import { api } from '$lib/api';

  let workspace = $state(null);
  let loading = $state(true);

  const id = $derived($page.params.id);

  onMount(async () => {
    const res = await api.get(`/workspaces/${id}`);
    if (res.data) {
      workspace = res.data.workspace;
    }
    loading = false;
  });

  async function refresh() {
    const res = await api.get(`/workspaces/${id}`);
    if (res.data) workspace = res.data.workspace;
  }

  function openVnc() {
    if (workspace?.vnc_token) {
      window.open(`/vnc/${workspace.vnc_token}/`, '_blank');
    }
  }

  async function startWorkspace() {
    const res = await api.post(`/workspaces/${id}/start`);
    if (res.error) { alert(res.error); return; }
    await refresh();
  }

  async function stopWorkspace() {
    const res = await api.post(`/workspaces/${id}/stop`);
    if (res.error) { alert(res.error); return; }
    await refresh();
  }

  async function pauseWorkspace() {
    const res = await api.post(`/workspaces/${id}/pause`);
    if (res.error) { alert(res.error); return; }
    await refresh();
  }

  async function unpauseWorkspace() {
    const res = await api.post(`/workspaces/${id}/unpause`);
    if (res.error) { alert(res.error); return; }
    await refresh();
  }

  async function deleteWorkspace() {
    if (confirm('Delete this workspace?')) {
      await api.delete(`/workspaces/${id}`);
      window.location.href = '/';
    }
  }

  function formatMemory(bytes) {
    if (!bytes) return '—';
    const gb = bytes / (1024 * 1024 * 1024);
    return gb >= 1 ? `${gb} GB` : `${bytes / (1024 * 1024)} MB`;
  }
</script>

<div class="workspace-detail">
  {#if loading}
    <p>Loading...</p>
  {:else if !workspace}
    <p>Workspace not found</p>
  {:else}
    <div class="header">
      <div>
        <h1>{workspace.name}</h1>
        <span class="status" class:running={workspace.status === 'running'} class:paused={workspace.status === 'paused'}>
          {workspace.status}
        </span>
      </div>
      <div class="actions">
        {#if workspace.status === 'running'}
          <button class="vnc-btn" onclick={openVnc}>Connect</button>
          <button onclick={pauseWorkspace}>Pause</button>
          <button onclick={stopWorkspace}>Stop</button>
        {:else if workspace.status === 'paused'}
          <button class="vnc-btn" onclick={openVnc}>Connect</button>
          <button onclick={unpauseWorkspace}>Resume</button>
          <button onclick={stopWorkspace}>Stop</button>
        {:else}
          <button onclick={startWorkspace}>Start</button>
        {/if}
        <button onclick={deleteWorkspace} class="danger">Delete</button>
      </div>
    </div>

    <div class="info-grid">
      <div class="info-item">
        <span class="label">Image</span>
        <span class="value">{workspace.image || '—'}</span>
      </div>
      <div class="info-item">
        <span class="label">CPU</span>
        <span class="value">{workspace.cores || '—'} cores</span>
      </div>
      <div class="info-item">
        <span class="label">RAM</span>
        <span class="value">{formatMemory(workspace.memory)}</span>
      </div>
      <div class="info-item">
        <span class="label">GPU</span>
        <span class="value">{workspace.gpu_count || 0}</span>
      </div>
      <div class="info-item">
        <span class="label">Persistent Storage</span>
        <span class="value">{workspace.persistent_storage ? 'Enabled' : 'Disabled'}</span>
      </div>
      {#if workspace.owner_username}
        <div class="info-item">
          <span class="label">Owner</span>
          <span class="value">{workspace.owner_username}</span>
        </div>
      {/if}
      {#if workspace.volume_host_path}
        <div class="info-item">
          <span class="label">Storage Path</span>
          <span class="value mono">{workspace.volume_host_path}</span>
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .workspace-detail {
    max-width: 960px;
    margin: 0 auto;
  }
  .header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    margin-bottom: 1.5rem;
  }
  h1 {
    margin: 0 0 0.5rem;
    color: var(--text-primary, #fff);
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
  .actions {
    display: flex;
    gap: 0.5rem;
  }
  button {
    padding: 0.5rem 1rem;
    border: 1px solid var(--border, #333);
    border-radius: 4px;
    background: var(--bg-secondary, #1a1a2e);
    color: var(--text-primary, #fff);
    cursor: pointer;
  }
  button.vnc-btn {
    background: var(--accent, #6366f1);
    border-color: var(--accent, #6366f1);
    color: white;
  }
  button.danger {
    border-color: #ef4444;
    color: #ef4444;
  }
  .info-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
    gap: 1rem;
    margin-top: 1.5rem;
  }
  .info-item {
    background: var(--bg-secondary, #1a1a2e);
    border: 1px solid var(--border, #333);
    border-radius: 8px;
    padding: 1rem;
  }
  .label {
    display: block;
    font-size: 0.8rem;
    color: var(--text-secondary, #888);
    margin-bottom: 0.25rem;
  }
  .value {
    display: block;
    color: var(--text-primary, #fff);
    font-size: 0.95rem;
  }
  .value.mono {
    font-family: monospace;
    font-size: 0.8rem;
    word-break: break-all;
  }
</style>
