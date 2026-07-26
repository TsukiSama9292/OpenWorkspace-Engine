<script lang="ts">
  import { page } from '$app/stores';
  import { onMount } from 'svelte';
  import { formatMemory } from '$lib/utils/format';
  import { loadConfigDetail } from './config-data';
  import { launchInstance, deleteConfig } from './config-actions';
  import type { Config, Instance } from '$lib/types';

  let config = $state<Config | null>(null);
  let instances = $state<Instance[]>([]);
  let loading = $state(true);
  let launching = $state(false);
  let error = $state('');

  let configId = $derived($page.params.id as string);

  onMount(async () => {
    const data = await loadConfigDetail(configId);
    config = data.config;
    instances = data.instances;
    loading = false;
  });

  async function onLaunch() {
    launching = true;
    error = '';
    const result = await launchInstance(configId);
    launching = false;
    if (result.error) error = result.error;
  }

  async function onDelete() {
    const result = await deleteConfig(configId);
    if (result.error) error = result.error;
  }
</script>

<div class="max-w-3xl mx-auto">
  {#if loading}
    <p class="text-surface-500">Loading...</p>
  {:else if !config}
    <p class="text-surface-500">Config not found.</p>
  {:else}
    <div class="flex justify-between items-start mb-6">
      <div>
        <a href="/" class="text-sm text-surface-500 no-underline hover:text-surface-700">&larr; Dashboard</a>
        <h1 class="text-2xl font-bold text-surface-800 mt-1">{config.name}</h1>
        {#if config.description}
          <p class="text-surface-500 text-sm mt-1">{config.description}</p>
        {/if}
      </div>
      <div class="flex gap-2">
        <button class="px-4 py-2 bg-primary-500 text-white border-none rounded cursor-pointer text-sm disabled:opacity-60 hover:bg-primary-600 transition-colors" onclick={onLaunch} disabled={launching}>
          {launching ? 'Launching...' : 'Launch Instance'}
        </button>
        <button class="px-4 py-2 bg-error-500 text-white border-none rounded cursor-pointer text-sm hover:bg-error-600 transition-colors" onclick={onDelete}>Delete</button>
      </div>
    </div>

    <div class="grid grid-cols-2 md:grid-cols-3 gap-4 mb-6">
      <div class="flex flex-col gap-1">
        <span class="text-xs text-surface-500">Image</span>
        <span class="text-sm text-surface-800 font-mono">{config.image}</span>
      </div>
      <div class="flex flex-col gap-1">
        <span class="text-xs text-surface-500">CPU</span>
        <span class="text-sm text-surface-800">{config.cpu_cores} cores</span>
      </div>
      <div class="flex flex-col gap-1">
        <span class="text-xs text-surface-500">RAM</span>
        <span class="text-sm text-surface-800">{formatMemory(config.ram_bytes)}</span>
      </div>
      <div class="flex flex-col gap-1">
        <span class="text-xs text-surface-500">GPU</span>
        <span class="text-sm text-surface-800">{config.gpu_count || 'None'}</span>
      </div>
      {#if config.docker_registry}
        <div class="flex flex-col gap-1">
          <span class="text-xs text-surface-500">Registry</span>
          <span class="text-sm text-surface-800 font-mono">{config.docker_registry}</span>
        </div>
      {/if}
      {#if config.persistent_storage_path}
        <div class="flex flex-col gap-1">
          <span class="text-xs text-surface-500">Persistent Storage</span>
          <span class="text-sm text-surface-800 font-mono">{config.persistent_storage_path}</span>
        </div>
      {/if}
    </div>

    {#if Object.keys(config.run_config || {}).length > 0}
      <div class="mb-6">
        <h2 class="text-base font-semibold text-surface-700 mb-2">Run Config</h2>
        <pre class="p-3 bg-surface-50 border border-surface-300 rounded text-sm font-mono text-surface-700 overflow-x-auto">{JSON.stringify(config.run_config, null, 2)}</pre>
      </div>
    {/if}

    {#if Object.keys(config.exec_config || {}).length > 0}
      <div class="mb-6">
        <h2 class="text-base font-semibold text-surface-700 mb-2">Exec Config</h2>
        <pre class="p-3 bg-surface-50 border border-surface-300 rounded text-sm font-mono text-surface-700 overflow-x-auto">{JSON.stringify(config.exec_config, null, 2)}</pre>
      </div>
    {/if}

    {#if Object.keys(config.volume_mappings || {}).length > 0}
      <div class="mb-6">
        <h2 class="text-base font-semibold text-surface-700 mb-2">Volume Mappings</h2>
        <pre class="p-3 bg-surface-50 border border-surface-300 rounded text-sm font-mono text-surface-700 overflow-x-auto">{JSON.stringify(config.volume_mappings, null, 2)}</pre>
      </div>
    {/if}

    <div class="mb-6">
      <h2 class="text-base font-semibold text-surface-700 mb-2">Instances ({instances.length})</h2>
      {#if instances.length === 0}
        <p class="text-surface-500 text-sm">No instances launched from this config.</p>
      {:else}
        <div class="flex flex-col gap-2">
          {#each instances as inst}
            <a href="/instances/{inst.id}/" class="flex justify-between items-center p-3 bg-surface-50 border border-surface-300 rounded no-underline hover:border-primary-500 transition-colors">
              <span class="text-surface-800 font-medium">{inst.name}</span>
              <span class="px-2 py-0.5 rounded text-xs font-medium {inst.status === 'running' ? 'bg-success-500/20 text-success-700' : inst.status === 'paused' ? 'bg-warning-500/20 text-warning-700' : inst.status === 'error' ? 'bg-error-500/20 text-error-700' : 'bg-surface-300 text-surface-600'}">
                {inst.status}
              </span>
            </a>
          {/each}
        </div>
      {/if}
    </div>

    {#if error}
      <p class="text-error-500 text-sm m-0">{error}</p>
    {/if}
  {/if}
</div>
