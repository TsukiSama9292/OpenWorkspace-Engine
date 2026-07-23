<script>
  import { page } from '$app/stores';
  import { onMount } from 'svelte';
  import { api } from '$lib/api';

  let instance = $state(null);
  let loading = $state(true);

  const id = $derived($page.params.id);

  onMount(async () => {
    const res = await api.get(`/instances/${id}`);
    if (res.data) {
      instance = res.data.instance;
    }
    loading = false;
  });

  function openVnc() {
    if (instance?.vnc_token) {
      window.open(`/vnc/${instance.vnc_token}/`, '_blank');
    }
  }

  async function startInstance() {
    const startRes = await api.post(`/instances/${id}/start`);
    if (startRes.error) { alert(startRes.error); return; }
    const res = await api.get(`/instances/${id}`);
    if (res.data) instance = res.data.instance;
  }

  async function stopInstance() {
    const stopRes = await api.post(`/instances/${id}/stop`);
    if (stopRes.error) { alert(stopRes.error); return; }
    const res = await api.get(`/instances/${id}`);
    if (res.data) instance = res.data.instance;
  }

  async function deleteInstance() {
    if (confirm('Delete this instance?')) {
      await api.delete(`/instances/${id}`);
      window.location.href = '/';
    }
  }
</script>

<div class="instance-detail">
  {#if loading}
    <p>Loading...</p>
  {:else if !instance}
    <p>Instance not found</p>
  {:else}
    <div class="header">
      <div>
        <h1>{instance.name}</h1>
        <span class="status" class:running={instance.status === 'running'}>
          {instance.status}
        </span>
      </div>
      <div class="actions">
        {#if instance.status === 'running'}
          <button class="vnc-btn" onclick={openVnc}>Open VNC</button>
          <button onclick={stopInstance}>Stop</button>
        {:else}
          <button onclick={startInstance}>Start</button>
        {/if}
        <button onclick={deleteInstance} class="danger">Delete</button>
      </div>
    </div>
  {/if}
</div>

<style>
  .instance-detail {
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
</style>
