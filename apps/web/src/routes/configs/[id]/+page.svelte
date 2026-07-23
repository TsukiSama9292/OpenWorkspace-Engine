<script>
  import { page } from '$app/stores';
  import { goto } from '$app/navigation';
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import { formatMemory } from './utils.js';
  import './config-detail.css';

  let config = $state(null);
  let instances = $state([]);
  let loading = $state(true);
  let launching = $state(false);
  let error = $state('');

  const configId = $derived($page.params.id);

  onMount(async () => {
    const [configRes, instancesRes] = await Promise.all([
      api.get(`/configs/${configId}`),
      api.get('/instances'),
    ]);
    if (configRes.data) config = configRes.data.config;
    if (instancesRes.data) {
      instances = instancesRes.data.instances.filter(i => i.config_id === configId);
    }
    loading = false;
  });

  async function launchInstance() {
    launching = true;
    error = '';
    const res = await api.post('/instances', { config_id: configId });
    launching = false;
    if (res.error) { error = res.error; }
    else if (res.data?.instance) { goto(`/instances/${res.data.instance.id}/`); }
    else { error = 'Failed to launch instance'; }
  }

  async function deleteConfig() {
    if (!confirm('Delete this config? Instances must be stopped first.')) return;
    const res = await api.delete(`/configs/${configId}`);
    if (res.error) { error = res.error; }
    else { goto('/'); }
  }
</script>

<div class="detail-page">
  {#if loading}
    <p class="loading">Loading...</p>
  {:else if !config}
    <p class="error-text">Config not found.</p>
  {:else}
    <div class="header">
      <div>
        <a href="/" class="back">← Dashboard</a>
        <h1>{config.name}</h1>
        {#if config.description}
          <p class="description">{config.description}</p>
        {/if}
      </div>
      <div class="actions">
        <button class="btn-primary" onclick={launchInstance} disabled={launching}>
          {launching ? 'Launching...' : 'Launch Instance'}
        </button>
        <button class="btn-danger" onclick={deleteConfig}>Delete</button>
      </div>
    </div>

    <div class="info-grid">
      <div class="info-item">
        <span class="label">Image</span>
        <span class="value mono">{config.image}</span>
      </div>
      <div class="info-item">
        <span class="label">CPU</span>
        <span class="value">{config.cores} cores</span>
      </div>
      <div class="info-item">
        <span class="label">RAM</span>
        <span class="value">{formatMemory(config.memory)}</span>
      </div>
      <div class="info-item">
        <span class="label">GPU</span>
        <span class="value">{config.gpu_count || 'None'}</span>
      </div>
      {#if config.docker_registry}
        <div class="info-item">
          <span class="label">Registry</span>
          <span class="value mono">{config.docker_registry}</span>
        </div>
      {/if}
      {#if config.persistent_storage_path}
        <div class="info-item">
          <span class="label">Persistent Storage</span>
          <span class="value mono">{config.persistent_storage_path}</span>
        </div>
      {/if}
    </div>

    {#if Object.keys(config.run_config || {}).length > 0}
      <div class="section">
        <h2>Run Config</h2>
        <pre class="code-block">{JSON.stringify(config.run_config, null, 2)}</pre>
      </div>
    {/if}

    {#if Object.keys(config.exec_config || {}).length > 0}
      <div class="section">
        <h2>Exec Config</h2>
        <pre class="code-block">{JSON.stringify(config.exec_config, null, 2)}</pre>
      </div>
    {/if}

    {#if Object.keys(config.volume_mappings || {}).length > 0}
      <div class="section">
        <h2>Volume Mappings</h2>
        <pre class="code-block">{JSON.stringify(config.volume_mappings, null, 2)}</pre>
      </div>
    {/if}

    <div class="section">
      <h2>Instances ({instances.length})</h2>
      {#if instances.length === 0}
        <p class="empty">No instances launched from this config.</p>
      {:else}
        <div class="instances-list">
          {#each instances as inst}
            <a href="/instances/{inst.id}/" class="instance-row">
              <span class="inst-name">{inst.name}</span>
              <span class="status" class:running={inst.status === 'running'} class:paused={inst.status === 'paused'} class:stopped={inst.status === 'stopped'} class:error={inst.status === 'error'}>
                {inst.status}
              </span>
            </a>
          {/each}
        </div>
      {/if}
    </div>

    {#if error}
      <p class="error-msg">{error}</p>
    {/if}
  {/if}
</div>
