<script lang="ts">
  import { onMount } from 'svelte';
  import { listOrphanedVolumes, cleanupOrphanedVolume } from '$lib/api/rbac-actions';
  import { mayManageUsers } from '$lib/permissions';
  import type { EffectiveContext, PersistentVolume } from '$lib/types';

  let { ctx = null }: { ctx?: EffectiveContext | null } = $props();

  let volumes = $state<PersistentVolume[]>([]);
  let loading = $state(false);
  let loadError = $state('');
  let cleanupTarget = $state<PersistentVolume | null>(null);
  let confirmText = $state('');
  let cleanupError = $state('');
  let cleaning = $state(false);
  let search = $state('');

  const canManage = $derived(mayManageUsers(ctx));

  const hasFilters = $derived(search.trim() !== '');

  const filteredVolumes = $derived(
    volumes.filter((v) => {
      const q = search.trim().toLowerCase();
      if (!q) return true;
      const owner = (v.owner_username ?? 'deleted user').toLowerCase();
      return v.host_path.toLowerCase().includes(q) || owner.includes(q);
    })
  );

  async function load() {
    loading = true;
    loadError = '';
    const res = await listOrphanedVolumes();
    if (res.error) {
      loadError = res.error;
    } else if (res.volumes) {
      volumes = res.volumes;
    }
    loading = false;
  }

  onMount(() => {
    if (canManage) load();
  });

  function formatSince(createdAt: string): string {
    return new Date(createdAt).toLocaleDateString();
  }

  function openCleanup(volume: PersistentVolume) {
    cleanupTarget = volume;
    confirmText = '';
    cleanupError = '';
  }

  function closeCleanup() {
    cleanupTarget = null;
    confirmText = '';
    cleanupError = '';
  }

  const confirmed = $derived(cleanupTarget !== null && confirmText === cleanupTarget.host_path);

  async function onCleanup() {
    const target = cleanupTarget;
    if (!target || !confirmed) return;
    cleaning = true;
    cleanupError = '';
    const res = await cleanupOrphanedVolume(target.id);
    cleaning = false;
    if (res.error) {
      cleanupError = res.error;
      return;
    }
    await load();
    closeCleanup();
  }
</script>

{#if canManage}
  <section class="ws-section panel-card">
    <div class="panel-head">
      <div>
        <h2 class="panel-head-title">Orphaned Volumes</h2>
        <p class="panel-head-desc">Persistent volumes left behind by removed or failed instances.</p>
      </div>
    </div>

    {#if loading}
      <p class="empty-text">Loading volumes...</p>
    {:else if loadError}
      <p class="empty-text">{loadError}</p>
    {:else if volumes.length === 0}
      <p class="empty-text">No orphaned volumes.</p>
    {:else}
      <div class="panel-toolbar">
        <div class="panel-search-wrap">
          <svg class="panel-search-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <circle cx="11" cy="11" r="7"></circle>
            <line x1="21" y1="21" x2="16.65" y2="16.65"></line>
          </svg>
          <input class="panel-search" type="text" placeholder="Search host path or owner..." bind:value={search} />
        </div>
        <span class="panel-count">{filteredVolumes.length} of {volumes.length}</span>
        {#if hasFilters}
          <button class="panel-clear" onclick={() => (search = '')}>Clear</button>
        {/if}
      </div>
      {#if filteredVolumes.length === 0}
        <p class="empty-text">No volumes match your filters.</p>
      {:else}
        <div class="instances-table-wrap">
          <table class="instances-table">
            <thead>
              <tr>
                <th>Host Path</th>
                <th>Owner</th>
                <th>Orphaned Since</th>
                <th>Actions</th>
              </tr>
            </thead>
            <tbody>
              {#each filteredVolumes as volume}
                <tr>
                  <td class="td-path">
                    <span class="td-name-text">{volume.host_path}</span>
                  </td>
                  <td class="td-owner">{volume.owner_username ?? 'deleted user'}</td>
                  <td class="td-date">{formatSince(volume.created_at)}</td>
                  <td class="td-actions">
                    <button class="launch-btn remove" onclick={() => openCleanup(volume)}>Clean Up</button>
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
    {/if}
  </section>

  {#if cleanupTarget}
    <div class="modal-overlay" onclick={closeCleanup} role="presentation"></div>
    <div class="modal-card">
      <h3 class="modal-title">Thorough Cleanup</h3>
      <p class="modal-desc">This permanently deletes the volume directory. Type the full host path to confirm.</p>
      <code class="cleanup-path">{cleanupTarget.host_path}</code>
      <div class="modal-field">
        <label for="cleanup-confirm" class="modal-label">Host Path</label>
        <input
          id="cleanup-confirm"
          class="modal-input"
          type="text"
          autocomplete="off"
          bind:value={confirmText}
          placeholder={cleanupTarget.host_path}
        />
      </div>
      {#if cleanupError}
        <div class="error-badge">{cleanupError}</div>
      {/if}
      <div class="modal-actions">
        <button type="button" class="modal-cancel" onclick={closeCleanup}>Cancel</button>
        <button
          type="button"
          class="cleanup-confirm"
          disabled={!confirmed || cleaning}
          onclick={onCleanup}
        >
          {cleaning ? 'Cleaning...' : 'Permanently Delete'}
        </button>
      </div>
    </div>
  {/if}
{/if}

<style>
  .td-path {
    font-family: 'JetBrains Mono', monospace;
    font-size: 0.72rem;
    color: #d4d4d8;
  }

  .cleanup-path {
    display: block;
    font-family: 'JetBrains Mono', monospace;
    font-size: 0.75rem;
    color: #a1a1aa;
    padding: 0.5rem;
    background: rgba(0, 0, 0, 0.4);
    border: 1px solid rgba(255, 255, 255, 0.06);
    border-radius: 6px;
    word-break: break-all;
  }

  .modal-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.6);
    z-index: 200;
  }

  .modal-card {
    position: fixed;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    width: 440px;
    max-height: 88vh;
    overflow-y: auto;
    background: rgba(20, 20, 26, 0.98);
    backdrop-filter: blur(24px);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-top: 1px solid rgba(255, 255, 255, 0.2);
    border-radius: 16px;
    padding: 1.5rem;
    z-index: 201;
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }

  .modal-title {
    font-size: 1.1rem;
    font-weight: 600;
    margin: 0;
  }

  .modal-desc {
    font-size: 0.8rem;
    color: #71717a;
    margin: 0;
  }

  .modal-field {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .modal-label {
    font-size: 0.7rem;
    font-weight: 600;
    color: #71717a;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .modal-input {
    background: rgba(0, 0, 0, 0.4);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 8px;
    padding: 0.6rem 0.75rem;
    color: #f4f4f5;
    font-size: 0.85rem;
    font-family: inherit;
    outline: none;
  }

  .modal-input:focus {
    border-color: #818cf8;
  }

  .modal-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 0.5rem;
  }

  .modal-cancel {
    background: rgba(255, 255, 255, 0.05);
    border: 1px solid rgba(255, 255, 255, 0.08);
    color: #a1a1aa;
    padding: 0.5rem 1rem;
    border-radius: 8px;
    font-size: 0.8rem;
    cursor: pointer;
    font-family: inherit;
    transition: all 0.2s;
  }

  .modal-cancel:hover {
    background: rgba(255, 255, 255, 0.1);
    color: #fff;
  }

  .cleanup-confirm {
    background: #dc2626;
    border: none;
    color: #fff;
    padding: 0.5rem 1.25rem;
    border-radius: 8px;
    font-size: 0.8rem;
    font-weight: 600;
    cursor: pointer;
    font-family: inherit;
    transition: background 0.2s;
  }

  .cleanup-confirm:hover {
    background: #b91c1c;
  }

  .cleanup-confirm:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
</style>
