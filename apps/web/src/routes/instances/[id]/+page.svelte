<script>
  import { page } from '$app/stores';
  import { goto } from '$app/navigation';
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import './instance-detail.css';

  let instance = $state(null);
  let loading = $state(true);
  let actionLoading = $state('');
  let error = $state('');

  const instanceId = $derived($page.params.id);

  onMount(async () => {
    const res = await api.get(`/instances/${instanceId}`);
    if (res.data) instance = res.data.instance;
    loading = false;
  });

  async function performAction(action) {
    actionLoading = action;
    error = '';
    const res = await api.post(`/instances/${instanceId}/${action}`);
    actionLoading = '';
    if (res.error) { error = res.error; }
    else {
      const statusMap = { start: 'running', stop: 'stopped', pause: 'paused', unpause: 'running' };
      if (instance) instance.status = statusMap[action] || instance.status;
    }
  }

  async function deleteInstance() {
    if (!confirm('Delete this instance? The container will be removed.')) return;
    const res = await api.delete(`/instances/${instanceId}`);
    if (res.error) { error = res.error; }
    else { goto('/'); }
  }
</script>

<div class="detail-page">
  {#if loading}
    <p class="loading">Loading...</p>
  {:else if !instance}
    <p class="error-text">Instance not found.</p>
  {:else}
    <div class="header">
      <div>
        <a href="/" class="back">← Dashboard</a>
        <h1>{instance.name}</h1>
        <p class="meta">
          {instance.config_name || 'Unknown config'}
          {#if instance.owner_username}
            <span class="sep">·</span> Owner: {instance.owner_username}
          {/if}
        </p>
      </div>
    </div>

    <div class="status-bar">
      <span class="status" class:running={instance.status === 'running'} class:paused={instance.status === 'paused'} class:stopped={instance.status === 'stopped'} class:error={instance.status === 'error'}>
        {instance.status}
      </span>
    </div>

    <div class="controls">
      {#if instance.status === 'stopped' || instance.status === 'error'}
        <button class="btn-start" onclick={() => performAction('start')} disabled={actionLoading === 'start'}>
          {actionLoading === 'start' ? 'Starting...' : 'Start'}
        </button>
      {:else if instance.status === 'running'}
        <button class="btn-stop" onclick={() => performAction('stop')} disabled={actionLoading === 'stop'}>
          {actionLoading === 'stop' ? 'Stopping...' : 'Stop'}
        </button>
        <button class="btn-pause" onclick={() => performAction('pause')} disabled={actionLoading === 'pause'}>
          {actionLoading === 'pause' ? 'Pausing...' : 'Pause'}
        </button>
      {:else if instance.status === 'paused'}
        <button class="btn-start" onclick={() => performAction('unpause')} disabled={actionLoading === 'unpause'}>
          {actionLoading === 'unpause' ? 'Resuming...' : 'Resume'}
        </button>
        <button class="btn-stop" onclick={() => performAction('stop')} disabled={actionLoading === 'stop'}>
          {actionLoading === 'stop' ? 'Stopping...' : 'Stop'}
        </button>
      {/if}
      <button class="btn-danger" onclick={deleteInstance}>Delete</button>
    </div>

    {#if instance.status === 'running' && instance.vnc_token}
      <div class="vnc-section">
        <h2>VNC Access</h2>
        <a href="/vnc/{instance.vnc_token}/" target="_blank" class="vnc-link">
          Open VNC Session →
        </a>
        <p class="vnc-token">Token: <code>{instance.vnc_token.slice(0, 16)}...</code></p>
      </div>
    {/if}

    <div class="info-grid">
      <div class="info-item">
        <span class="label">Instance ID</span>
        <span class="value mono">{instance.id}</span>
      </div>
      <div class="info-item">
        <span class="label">Config ID</span>
        <span class="value mono">{instance.config_id}</span>
      </div>
      <div class="info-item">
        <span class="label">Instance #</span>
        <span class="value">{instance.instance_number}</span>
      </div>
      {#if instance.container_id}
        <div class="info-item">
          <span class="label">Container</span>
          <span class="value mono">{instance.container_id.slice(0, 12)}</span>
        </div>
      {/if}
      <div class="info-item">
        <span class="label">Mount Persistent</span>
        <span class="value">{instance.mount_persistent ? 'Yes' : 'No'}</span>
      </div>
      {#if instance.resolved_volume_host_path}
        <div class="info-item">
          <span class="label">Host Path</span>
          <span class="value mono">{instance.resolved_volume_host_path}</span>
        </div>
      {/if}
      <div class="info-item">
        <span class="label">Created</span>
        <span class="value">{new Date(instance.created_at).toLocaleString()}</span>
      </div>
      <div class="info-item">
        <span class="label">Updated</span>
        <span class="value">{new Date(instance.updated_at).toLocaleString()}</span>
      </div>
    </div>

    {#if error}
      <p class="error-msg">{error}</p>
    {/if}
  {/if}
</div>
