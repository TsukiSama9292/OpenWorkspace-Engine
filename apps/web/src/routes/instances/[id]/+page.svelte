<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { page } from '$app/stores';
  import { api } from '$lib/api/client';
  import { performAction, deleteInstance } from '$lib/api/instance-actions';
  import { wrapperUrl } from '$lib/countdown/countdown';
  import KeepTimeLine from '$lib/components/instances/KeepTimeLine.svelte';
  import type { Instance } from '$lib/types';

  let instance = $state<Instance | null>(null);
  let loading = $state(true);
  let error = $state('');

  let pollTimer: ReturnType<typeof setInterval> | null = null;

  function instanceUrl(inst: Instance): string {
    return wrapperUrl(inst.remote_type, inst.access_token ?? '');
  }

  async function loadInstance() {
    const id = $page.params.id;
    if (!id) { error = 'No instance ID'; loading = false; return; }
    const res = await api.get<{ instance: Instance }>('/instances/' + id);
    if (res.error) { error = res.error; loading = false; return; }
    if (!res.data?.instance) { error = 'Instance not found'; loading = false; return; }
    instance = res.data.instance;

    if (instance.status === 'running') {
      window.location.href = instanceUrl(instance);
      return;
    }
    if (instance.status === 'starting') {
      startPolling();
    }
    loading = false;
  }

  function startPolling() {
    pollTimer = setInterval(async () => {
      const id = $page.params.id;
      if (!id) return;
      const res = await api.get<{ instance: Instance }>('/instances/' + id);
      if (res.data?.instance) {
        instance = res.data.instance;
        if (instance.status === 'running') {
          if (pollTimer) clearInterval(pollTimer);
          window.location.href = instanceUrl(instance);
        }
      }
    }, 2000);
  }

  function stopPolling() {
    if (pollTimer) {
      clearInterval(pollTimer);
      pollTimer = null;
    }
  }

  onMount(loadInstance);
  onDestroy(stopPolling);

  async function onAction(action: 'start' | 'stop' | 'pause' | 'unpause') {
    if (!instance) return;
    stopPolling();
    const result = await performAction(instance.id, action);
    if (result.status) {
      instance = { ...instance, status: result.status };
      if (result.status === 'starting') {
        startPolling();
      }
    }
  }

  async function onDelete() {
    if (!instance) return;
    await deleteInstance(instance.id);
  }

  const statusColors: Record<string, string> = {
    running: '#22c55e',
    paused: '#eab308',
    stopped: '#6b7280',
    error: '#ef4444',
    starting: '#3b82f6',
  };
</script>

<div class="page">
  {#if loading}
    <div class="state-box">
      <div class="spinner"></div>
      <p class="state-text">Loading instance...</p>
    </div>
  {:else if error}
    <div class="state-box">
      <p class="error-text">{error}</p>
      <a href="/" class="btn-link">Back to Dashboard</a>
    </div>
  {:else if instance}
    <div class="card">
      <div class="card-header">
        <div>
          <h1 class="card-title">{instance.name}</h1>
          <span class="card-id">{instance.id.slice(0, 8)}</span>
        </div>
        <span class="status-badge" style="color: {statusColors[instance.status]}">
          <span class="dot" style="background: {statusColors[instance.status]}"></span>
          {instance.status}
        </span>
      </div>

      <div class="card-body">
        <div class="info-grid">
          <div class="info-item">
            <span class="info-label">Template</span>
            <span class="info-value">{instance.template_name || '---'}</span>
          </div>
          <div class="info-item">
            <span class="info-label">Remote Type</span>
            <span class="info-value">{instance.remote_type}</span>
          </div>
          <div class="info-item">
            <span class="info-label">Owner</span>
            <span class="info-value">{instance.owner_username || '---'}</span>
          </div>
          <div class="info-item">
            <span class="info-label">Container</span>
            <span class="info-value">{instance.container_id ? instance.container_id.slice(0, 12) : '---'}</span>
          </div>
          <KeepTimeLine
            keepTimeSeconds={instance.keep_time_seconds}
            keepTimeAction={instance.keep_time_action}
          />
        </div>

        {#if instance.status === 'starting'}
          <div class="starting-section">
            <div class="spinner"></div>
            <p class="starting-text">Starting up...</p>
            <p class="starting-sub">The container is being initialized. This usually takes a few seconds.</p>
            <div class="action-buttons">
              <button class="btn btn-stop" onclick={() => onAction('stop')}>Stop</button>
            </div>
          </div>
        {:else if instance.status === 'error'}
          <div class="state-section">
            <p class="error-text">Startup failed or container is in an error state.</p>
            <div class="action-buttons">
              <button class="btn btn-start" onclick={() => onAction('start')}>Retry</button>
              <button class="btn btn-stop" onclick={() => onAction('stop')}>Stop</button>
            </div>
          </div>
        {:else if instance.status === 'running'}
          <div class="state-section">
            <div class="action-buttons">
              <a href={instanceUrl(instance)} target="_blank" class="btn btn-open">Open</a>
              <button class="btn btn-pause" onclick={() => onAction('pause')}>Pause</button>
              <button class="btn btn-stop" onclick={() => onAction('stop')}>Stop</button>
            </div>
          </div>
        {:else if instance.status === 'paused'}
          <div class="state-section">
            <div class="action-buttons">
              <button class="btn btn-start" onclick={() => onAction('unpause')}>Resume</button>
              <button class="btn btn-stop" onclick={() => onAction('stop')}>Stop</button>
            </div>
          </div>
        {:else if instance.status === 'stopped'}
          <div class="state-section">
            <div class="action-buttons">
              <button class="btn btn-start" onclick={() => onAction('start')}>Start</button>
            </div>
          </div>
        {/if}
      </div>

      <div class="card-footer">
        <button class="btn btn-danger" onclick={onDelete}>Delete Instance</button>
      </div>
    </div>
  {/if}
</div>

<style>
  :global(html), :global(body) {
    margin: 0;
    padding: 0;
    background-color: #09090b;
    color: #f4f4f5;
  }

  :global(main.has-nav) {
    background: radial-gradient(circle at 0% 0%, #18181b 0%, #09090b 100%);
    min-height: calc(100vh - 56px);
  }

  .page {
    max-width: 640px;
    margin: 0 auto;
    padding: 2rem 1rem;
  }

  .state-box { display: flex; flex-direction: column; align-items: center; gap: 1rem; padding: 4rem 1rem; text-align: center; }
  .state-text { color: #a1a1aa; font-size: 0.9rem; }
  .error-text { color: #ef4444; font-size: 0.9rem; }
  .btn-link { color: #3b82f6; text-decoration: none; font-size: 0.85rem; }
  .btn-link:hover { text-decoration: underline; }

  .card { background: rgba(20, 20, 26, 0.6); border: 1px solid rgba(255,255,255,0.06); border-top: 1px solid rgba(255,255,255,0.12); border-radius: 12px; overflow: hidden; }

  .card-header { display: flex; justify-content: space-between; align-items: flex-start; padding: 1.25rem; border-bottom: 1px solid rgba(255,255,255,0.06); }
  .card-title { font-size: 1.1rem; font-weight: 600; color: #f4f4f5; margin: 0; }
  .card-id { font-size: 0.75rem; color: #71717a; }

  .status-badge { display: flex; align-items: center; gap: 6px; font-size: 0.8rem; font-weight: 600; text-transform: uppercase; }
  .dot { width: 8px; height: 8px; border-radius: 50%; display: inline-block; }

  .card-body { padding: 1.25rem; }
  .card-footer { padding: 1rem 1.25rem; border-top: 1px solid rgba(255,255,255,0.06); display: flex; justify-content: flex-end; }

  .info-grid { display: grid; grid-template-columns: repeat(2, 1fr); gap: 1rem; margin-bottom: 1.5rem; }
  .info-item { display: flex; flex-direction: column; gap: 2px; }
  .info-label { font-size: 0.7rem; color: #71717a; text-transform: uppercase; letter-spacing: 0.05em; }
  .info-value { font-size: 0.85rem; color: #d4d4d8; }

  .starting-section { display: flex; flex-direction: column; align-items: center; gap: 0.75rem; padding: 2rem 0; text-align: center; }
  .starting-text { font-size: 1rem; font-weight: 600; color: #3b82f6; margin: 0; }
  .starting-sub { font-size: 0.8rem; color: #71717a; margin: 0; }

  .state-section { padding: 1rem 0; }

  .spinner { width: 32px; height: 32px; border: 3px solid rgba(59,130,246,0.2); border-top-color: #3b82f6; border-radius: 50%; animation: spin 0.8s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }

  .action-buttons { display: flex; flex-wrap: wrap; gap: 8px; justify-content: center; }

  .btn { font-size: 0.8rem; font-weight: 600; padding: 0.5rem 1.2rem; border-radius: 6px; border: 1px solid rgba(255,255,255,0.08); background: rgba(255,255,255,0.05); color: #d4d4d8; cursor: pointer; transition: all 0.2s; text-decoration: none; display: inline-flex; align-items: center; gap: 4px; font-family: inherit; }
  .btn:hover { background: rgba(255,255,255,0.12); color: #fff; }

  .btn-open { border-color: #3b82f6; color: #60a5fa; }
  .btn-open:hover { border-color: #3b82f6; color: #60a5fa; }
  .btn-start { border-color: #22c55e; color: #4ade80; }
  .btn-start:hover { border-color: #22c55e; color: #4ade80; }
  .btn-pause { border-color: #eab308; color: #facc15; }
  .btn-pause:hover { border-color: #eab308; color: #facc15; }
  .btn-stop { border-color: #f97316; color: #fb923c; }
  .btn-stop:hover { border-color: #f97316; color: #fb923c; }
  .btn-danger { border-color: #ef4444; color: #f87171; }
  .btn-danger:hover { border-color: #ef4444; color: #f87171; }
</style>
