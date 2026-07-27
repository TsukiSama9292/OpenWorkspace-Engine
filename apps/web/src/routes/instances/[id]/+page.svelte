<script lang="ts">
  import { page } from '$app/stores';
  import { onMount } from 'svelte';
  import { loadInstanceDetail } from './instance-data';
  import { performAction, deleteInstance } from '$lib/api/instance-actions';
  import type { Instance } from '$lib/types';

  let instance = $state<Instance | null>(null);
  let loading = $state(true);
  let actionLoading = $state('');
  let error = $state('');

  let instanceId = $derived($page.params.id as string);

  onMount(async () => {
    instance = await loadInstanceDetail(instanceId);
    loading = false;
  });

  async function onAction(action: string) {
    actionLoading = action;
    error = '';
    const result = await performAction(instanceId, action);
    actionLoading = '';
    if (result.error) { error = result.error; }
    else if (result.status && instance) { instance.status = result.status; }
  }

  async function onDelete() {
    const result = await deleteInstance(instanceId);
    if (result.error) error = result.error;
  }
</script>

<div class="max-w-3xl mx-auto">
  {#if loading}
    <p class="text-surface-500">Loading...</p>
  {:else if !instance}
    <p class="text-surface-500">Instance not found.</p>
  {:else}
    <div class="mb-6">
      <a href="/" class="text-sm text-surface-500 no-underline hover:text-surface-700">&larr; Dashboard</a>
      <h1 class="text-2xl font-bold text-surface-800 mt-1">{instance.name}</h1>
      <p class="text-surface-500 text-sm mt-1">
        {instance.config_name || 'Unknown config'}
        {#if instance.owner_username}
          <span class="mx-1">&middot;</span> Owner: {instance.owner_username}
        {/if}
      </p>
    </div>

    <div class="mb-4">
      <span class="px-3 py-1 rounded-full text-sm font-medium {instance.status === 'running' ? 'bg-success-500/20 text-success-700' : instance.status === 'paused' ? 'bg-warning-500/20 text-warning-700' : instance.status === 'error' ? 'bg-error-500/20 text-error-700' : 'bg-surface-300 text-surface-600'}">
        {instance.status}
      </span>
    </div>

    <div class="flex gap-2 mb-6">
      {#if instance.status === 'stopped' || instance.status === 'error'}
        <button class="px-4 py-2 bg-success-500 text-white border-none rounded cursor-pointer text-sm disabled:opacity-60 hover:bg-success-600 transition-colors" onclick={() => onAction('start')} disabled={actionLoading === 'start'}>
          {actionLoading === 'start' ? 'Starting...' : 'Start'}
        </button>
      {:else if instance.status === 'running'}
        <button class="px-4 py-2 bg-error-500 text-white border-none rounded cursor-pointer text-sm disabled:opacity-60 hover:bg-error-600 transition-colors" onclick={() => onAction('stop')} disabled={actionLoading === 'stop'}>
          {actionLoading === 'stop' ? 'Stopping...' : 'Stop'}
        </button>
        <button class="px-4 py-2 bg-warning-500 text-white border-none rounded cursor-pointer text-sm disabled:opacity-60 hover:bg-warning-600 transition-colors" onclick={() => onAction('pause')} disabled={actionLoading === 'pause'}>
          {actionLoading === 'pause' ? 'Pausing...' : 'Pause'}
        </button>
      {:else if instance.status === 'paused'}
        <button class="px-4 py-2 bg-success-500 text-white border-none rounded cursor-pointer text-sm disabled:opacity-60 hover:bg-success-600 transition-colors" onclick={() => onAction('unpause')} disabled={actionLoading === 'unpause'}>
          {actionLoading === 'unpause' ? 'Resuming...' : 'Resume'}
        </button>
        <button class="px-4 py-2 bg-error-500 text-white border-none rounded cursor-pointer text-sm disabled:opacity-60 hover:bg-error-600 transition-colors" onclick={() => onAction('stop')} disabled={actionLoading === 'stop'}>
          {actionLoading === 'stop' ? 'Stopping...' : 'Stop'}
        </button>
      {/if}
      <button class="px-4 py-2 bg-error-500 text-white border-none rounded cursor-pointer text-sm hover:bg-error-600 transition-colors" onclick={onDelete}>Delete</button>
    </div>

    {#if instance.status === 'running' && instance.vnc_token}
      <div class="p-4 bg-surface-50 border border-surface-300 rounded-lg mb-6">
        <h2 class="text-base font-semibold text-surface-700 mb-2">VNC Access</h2>
        <a href="/vnc/{instance.vnc_token}/" target="_blank" class="text-primary-500 no-underline hover:text-primary-600 font-medium">
          Open VNC Session &rarr;
        </a>
        <p class="text-surface-500 text-xs mt-2">Token: <code class="font-mono bg-surface-200 px-1 rounded">{instance.vnc_token.slice(0, 16)}...</code></p>
      </div>
    {/if}

    <div class="grid grid-cols-2 md:grid-cols-3 gap-4 mb-6">
      <div class="flex flex-col gap-1">
        <span class="text-xs text-surface-500">Instance ID</span>
        <span class="text-sm text-surface-800 font-mono">{instance.id}</span>
      </div>
      <div class="flex flex-col gap-1">
        <span class="text-xs text-surface-500">Config ID</span>
        <span class="text-sm text-surface-800 font-mono">{instance.config_id}</span>
      </div>
      <div class="flex flex-col gap-1">
        <span class="text-xs text-surface-500">Instance #</span>
        <span class="text-sm text-surface-800">{instance.instance_number}</span>
      </div>
      {#if instance.container_id}
        <div class="flex flex-col gap-1">
          <span class="text-xs text-surface-500">Container</span>
          <span class="text-sm text-surface-800 font-mono">{instance.container_id.slice(0, 12)}</span>
        </div>
      {/if}
      <div class="flex flex-col gap-1">
        <span class="text-xs text-surface-500">Mount Persistent</span>
        <span class="text-sm text-surface-800">{instance.mount_persistent ? 'Yes' : 'No'}</span>
      </div>
      {#if instance.resolved_volume_host_path}
        <div class="flex flex-col gap-1">
          <span class="text-xs text-surface-500">Host Path</span>
          <span class="text-sm text-surface-800 font-mono">{instance.resolved_volume_host_path}</span>
        </div>
      {/if}
      <div class="flex flex-col gap-1">
        <span class="text-xs text-surface-500">Created</span>
        <span class="text-sm text-surface-800">{new Date(instance.created_at).toLocaleString()}</span>
      </div>
      <div class="flex flex-col gap-1">
        <span class="text-xs text-surface-500">Updated</span>
        <span class="text-sm text-surface-800">{new Date(instance.updated_at).toLocaleString()}</span>
      </div>
    </div>

    {#if error}
      <p class="text-error-500 text-sm m-0">{error}</p>
    {/if}
  {/if}
</div>
