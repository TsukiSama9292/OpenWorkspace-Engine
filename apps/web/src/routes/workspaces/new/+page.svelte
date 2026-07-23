<script>
  import { goto } from '$app/navigation';
  import { onMount } from 'svelte';
  import { api } from '$lib/api';

  let name = $state('');
  let image = $state('kasmweb/desktop:1.19.0-rolling-daily');
  let cores = $state(2);
  let memory = $state(4);
  let gpuCount = $state(0);
  let persistentStorage = $state(true);
  let loading = $state(false);
  let error = $state('');

  let registry = $state(null);
  let selectedType = $state(null);

  onMount(async () => {
    const res = await api.get('/registry');
    if (res.data) {
      registry = res.data;
    }
  });

  function selectType(ws) {
    selectedType = ws;
    image = ws.image || image;
    cores = ws.cores || cores;
    memory = ws.memory ? Math.round(ws.memory / (1024 * 1024 * 1024)) : memory;
    gpuCount = ws.gpu_count || gpuCount;
  }

  async function createWorkspace() {
    if (!name.trim()) { error = 'Name is required'; return; }
    loading = true;
    error = '';
    const res = await api.post('/workspaces', {
      name: name.trim(),
      image,
      cores,
      memory: memory * 1024 * 1024 * 1024,
      gpu_count: gpuCount,
      persistent_storage: persistentStorage,
    });
    loading = false;
    if (res.error) {
      error = res.error;
    } else if (res.data?.workspace?.status === 'error') {
      error = 'Workspace created but container failed to start';
    } else if (res.data) {
      goto(`/workspaces/${res.data.workspace.id}/`);
    } else {
      error = 'Failed to create workspace';
    }
  }
</script>

<div class="new-workspace">
  <h1>New Workspace</h1>

  {#if registry?.workspaces?.length}
    <div class="type-section">
      <h2>Choose Type</h2>
      <div class="type-grid">
        {#each registry.workspaces as ws}
          <button
            class="type-card"
            class:selected={selectedType?.friendly_name === ws.friendly_name}
            onclick={() => selectType(ws)}
          >
            {#if ws.icon_url}
              <img src={ws.icon_url} alt={ws.friendly_name} class="type-icon" />
            {:else}
              <div class="type-icon-placeholder"></div>
            {/if}
            <span class="type-name">{ws.friendly_name}</span>
            {#if ws.description}
              <span class="type-desc">{ws.description}</span>
            {/if}
          </button>
        {/each}
      </div>
    </div>
  {/if}

  <form onsubmit={createWorkspace}>
    <label>
      Name
      <input type="text" bind:value={name} placeholder="my-workspace" required />
    </label>

    <label>
      Image
      <input type="text" bind:value={image} placeholder="kasmweb/desktop:1.19.0-rolling-daily" />
    </label>

    <div class="row">
      <label>
        CPU Cores
        <input type="number" bind:value={cores} min="1" max="16" />
      </label>
      <label>
        RAM (GB)
        <input type="number" bind:value={memory} min="1" max="64" />
      </label>
      <label>
        GPU
        <input type="number" bind:value={gpuCount} min="0" max="8" />
      </label>
    </div>

    <label class="checkbox-label">
      <input type="checkbox" bind:checked={persistentStorage} />
      Persistent Storage
    </label>

    {#if error}
      <p class="error">{error}</p>
    {/if}

    <div class="actions">
      <a href="/">Cancel</a>
      <button type="submit" disabled={loading}>
        {loading ? 'Creating...' : 'Create'}
      </button>
    </div>
  </form>
</div>

<style>
  .new-workspace {
    max-width: 640px;
    margin: 0 auto;
  }
  h1 {
    color: var(--text-primary, #fff);
    margin-bottom: 1.5rem;
  }
  h2 {
    color: var(--text-primary, #fff);
    font-size: 1rem;
    margin: 1.5rem 0 0.75rem;
  }
  .type-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
    gap: 0.75rem;
  }
  .type-card {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.5rem;
    padding: 1rem;
    border: 1px solid var(--border, #333);
    border-radius: 8px;
    background: var(--bg-secondary, #1a1a2e);
    color: var(--text-primary, #fff);
    cursor: pointer;
    text-align: center;
  }
  .type-card.selected {
    border-color: var(--accent, #6366f1);
    background: rgba(99, 102, 241, 0.1);
  }
  .type-icon {
    width: 48px;
    height: 48px;
    object-fit: contain;
  }
  .type-icon-placeholder {
    width: 48px;
    height: 48px;
    background: var(--border, #333);
    border-radius: 8px;
  }
  .type-name {
    font-size: 0.85rem;
    font-weight: 500;
  }
  .type-desc {
    font-size: 0.7rem;
    color: var(--text-secondary, #888);
    display: -webkit-box;
    -webkit-line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  form {
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    color: var(--text-secondary, #aaa);
  }
  .row {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 1rem;
  }
  .checkbox-label {
    flex-direction: row;
    align-items: center;
    gap: 0.5rem;
  }
  .checkbox-label input {
    width: auto;
  }
  input {
    padding: 0.75rem;
    border: 1px solid var(--border, #333);
    border-radius: 4px;
    background: var(--bg-primary, #0f0f1a);
    color: var(--text-primary, #fff);
    font-size: 1rem;
  }
  .actions {
    display: flex;
    gap: 1rem;
    justify-content: flex-end;
    margin-top: 1rem;
  }
  a {
    color: var(--text-secondary, #aaa);
    text-decoration: none;
    padding: 0.5rem 1rem;
  }
  button[type="submit"] {
    padding: 0.5rem 1rem;
    border: none;
    border-radius: 4px;
    background: var(--accent, #6366f1);
    color: white;
    cursor: pointer;
  }
  button:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }
  .error {
    color: #ef4444;
    margin: 0;
  }
</style>
