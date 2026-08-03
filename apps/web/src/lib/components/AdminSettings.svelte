<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api/client';
  import type { SystemSettingsValue } from '$lib/system-settings';

  let settings = $state<SystemSettingsValue | null>(null);
  let loading = $state(true);
  let error = $state('');
  let saved = $state(false);
  let saving = $state(false);

  async function load() {
    loading = true;
    error = '';
    const res = await api.get<{ settings: SystemSettingsValue }>('/admin/settings');
    if (res.data?.settings) {
      settings = res.data.settings;
    } else if (res.error) {
      error = res.error;
    } else {
      error = 'Failed to load system settings';
    }
    loading = false;
  }

  async function save() {
    if (!settings) return;
    saving = true;
    error = '';
    saved = false;
    const res = await api.put<{ settings: SystemSettingsValue }>('/admin/settings', settings);
    if (res.data?.settings) {
      settings = res.data.settings;
      saved = true;
    } else if (res.error) {
      error = res.error;
    } else {
      error = 'Failed to save system settings';
    }
    saving = false;
  }

  function numeric(value: unknown): number {
    const n = Number(value);
    return Number.isFinite(n) ? n : 0;
  }

  function update(key: keyof SystemSettingsValue, value: unknown) {
    if (settings) {
      settings = { ...settings, [key]: numeric(value) };
    }
  }

  onMount(load);
</script>

{#if loading}
  <div class="admin-settings-loading">Loading settings...</div>
{:else if settings}
  <div class="settings-section">
    <span class="settings-label">Resource Policy</span>
    <p class="settings-desc">Host capacity and global instance limits.</p>
    <div class="settings-row">
      <label class="modal-label" for="admin-max-cpu">Max CPU Cores</label>
      <input id="admin-max-cpu" class="modal-input" type="number" min="0" step="1" value={settings.max_cpu_cores}
        oninput={(e) => update('max_cpu_cores', e.currentTarget.value)} />
    </div>
    <div class="settings-row">
      <label class="modal-label" for="admin-max-ram">Max RAM Bytes</label>
      <input id="admin-max-ram" class="modal-input" type="number" min="0" step="1" value={settings.max_ram_bytes}
        oninput={(e) => update('max_ram_bytes', e.currentTarget.value)} />
    </div>
    <div class="settings-row">
      <label class="modal-label" for="admin-instance-limit">Instance Limit (0 = unlimited)</label>
      <input id="admin-instance-limit" class="modal-input" type="number" min="0" step="1" value={settings.host_instance_limit}
        oninput={(e) => update('host_instance_limit', e.currentTarget.value)} />
    </div>
    <div class="settings-row">
      <label class="modal-label" for="admin-shared-cpu">Shared Max CPU (0 = off)</label>
      <input id="admin-shared-cpu" class="modal-input" type="number" min="0" step="1" value={settings.shared_max_cpu}
        oninput={(e) => update('shared_max_cpu', e.currentTarget.value)} />
    </div>
    <div class="settings-row">
      <label class="modal-label" for="admin-shared-ram">Shared Max RAM (0 = off)</label>
      <input id="admin-shared-ram" class="modal-input" type="number" min="0" step="1" value={settings.shared_max_ram}
        oninput={(e) => update('shared_max_ram', e.currentTarget.value)} />
    </div>
    {#if error}
      <div class="error-badge">{error}</div>
    {/if}
    <div class="settings-row">
      <button class="modal-confirm" onclick={save} disabled={saving}>
        {saving ? 'Saving...' : 'Save'}
      </button>
      {#if saved}
        <span class="settings-value admin-settings-saved">Saved</span>
      {/if}
    </div>
  </div>
{:else if error}
  <div class="settings-section">
    <span class="settings-label">Resource Policy</span>
    <div class="error-badge">{error}</div>
  </div>
{/if}

<style>
  .settings-row {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-bottom: 0.75rem;
  }

  .settings-row:last-child {
    flex-direction: row;
    align-items: center;
    justify-content: flex-start;
    gap: 10px;
  }

  .admin-settings-loading {
    font-size: 0.8rem;
    color: #71717a;
    padding: 0.5rem 0;
  }

  .admin-settings-saved {
    font-size: 0.75rem;
    color: #4ade80;
  }

  :global(.modal-input:disabled) {
    opacity: 0.5;
    cursor: not-allowed;
  }
</style>
