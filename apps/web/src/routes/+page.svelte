<script>
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import { formatMemory } from './utils.js';
  import './dashboard.css';

  let activeTab = $state('configs');
  let configs = $state([]);
  let instances = $state([]);
  let loading = $state(true);

  onMount(async () => {
    const [configRes, instanceRes] = await Promise.all([
      api.get('/configs'),
      api.get('/instances'),
    ]);
    if (configRes.data) configs = configRes.data.configs;
    if (instanceRes.data) instances = instanceRes.data.instances;
    loading = false;
  });
</script>

<div class="dashboard">
  <div class="header">
    <h1>Dashboard</h1>
    <a href="/configs/new/" class="btn">New Config</a>
  </div>

  <div class="tabs">
    <button class="tab" class:active={activeTab === 'configs'} onclick={() => activeTab = 'configs'}>
      Configs ({configs.length})
    </button>
    <button class="tab" class:active={activeTab === 'instances'} onclick={() => activeTab = 'instances'}>
      Instances ({instances.length})
    </button>
  </div>

  {#if loading}
    <p class="loading">Loading...</p>
  {:else if activeTab === 'configs'}
    {#if configs.length === 0}
      <p class="empty">No configs yet. Create one to get started.</p>
    {:else}
      <div class="grid">
        {#each configs as config}
          <a href="/configs/{config.id}/" class="card">
            <h3>{config.name}</h3>
            <p class="card-meta">{config.image}</p>
            <p class="card-resources">{config.cores} cores · {formatMemory(config.memory)}</p>
            <div class="card-footer">
              <span class="instance-count">{config.instance_count} instance{config.instance_count !== 1 ? 's' : ''}</span>
            </div>
          </a>
        {/each}
      </div>
    {/if}
  {:else}
    {#if instances.length === 0}
      <p class="empty">No instances running.</p>
    {:else}
      <div class="grid">
        {#each instances as inst}
          <a href="/instances/{inst.id}/" class="card">
            <h3>{inst.name}</h3>
            <p class="card-meta">{inst.config_name || 'Unknown config'}</p>
            <div class="card-footer">
              <span class="status" class:running={inst.status === 'running'} class:paused={inst.status === 'paused'} class:stopped={inst.status === 'stopped'}>
                {inst.status}
              </span>
              {#if inst.owner_username}
                <span class="owner">{inst.owner_username}</span>
              {/if}
            </div>
          </a>
        {/each}
      </div>
    {/if}
  {/if}
</div>
