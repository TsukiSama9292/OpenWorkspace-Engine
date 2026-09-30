<script lang="ts">
  import { onMount } from 'svelte';
  import { listOrphanedVolumes, cleanupOrphanedVolume } from '$lib/api/rbac-actions';
  import { mayManageUsers } from '$lib/permissions';
  import Modal from '$lib/components/ui/Modal.svelte';
  import Button from '$lib/components/ui/Button.svelte';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import { statusBadgeClass } from '$lib/status-badge';
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
      <EmptyState message="Loading volumes..." />
    {:else if loadError}
      <EmptyState message={loadError} />
    {:else if volumes.length === 0}
      <EmptyState message="No orphaned volumes. Removed or failed instances leave nothing behind right now." />
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
        <EmptyState message="No volumes match your filters. Try different keywords, or clear the search to browse everything." />
      {:else}
        <div class="instances-table-wrap">
          <table class="instances-table">
            <thead>
              <tr>
                <th>Host Path</th>
                <th>Owner</th>
                <th>Status</th>
                <th>Orphaned Since</th>
                <th>Actions</th>
              </tr>
            </thead>
            <tbody>
              {#each filteredVolumes as volume (volume.id)}
                <tr>
                  <td class="td-path">
                    <span class="td-name-text">{volume.host_path}</span>
                  </td>
                  <td class="td-owner">{volume.owner_username ?? 'deleted user'}</td>
                  <td>
                    <span class="status-badge {statusBadgeClass(volume.status)}">
                      <span class="status-dot-inline"></span>
                      {volume.status}
                    </span>
                  </td>
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
    <Modal open title="Thorough Cleanup" width="28rem" onclose={closeCleanup}>
      <div class="flex flex-col gap-3" data-testid="volume-cleanup">
        <p class="text-sm leading-relaxed text-surface-200">This permanently deletes the volume directory. Type the full host path to confirm.</p>
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
          <p class="text-error-500 text-sm m-0">{cleanupError}</p>
        {/if}
        <div class="flex justify-end gap-2">
          <Button variant="secondary" onclick={closeCleanup}>Cancel</Button>
          <Button variant="error" onclick={onCleanup} disabled={!confirmed || cleaning}>
            {cleaning ? 'Cleaning...' : 'Permanently Delete'}
          </Button>
        </div>
      </div>
    </Modal>
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
</style>
