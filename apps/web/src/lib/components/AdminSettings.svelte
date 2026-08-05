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

<div class="page">
  <header class="page-header">
    <h2 class="page-title">Server Settings</h2>
    <p class="page-desc">Host-wide resource settings that apply to every tier.</p>
  </header>

  <div class="settings-card">
    <header class="card-header">
      <div class="card-icon" aria-hidden="true">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <rect x="2" y="3" width="20" height="14" rx="2" /><line x1="8" y1="21" x2="16" y2="21" /><line x1="12" y1="17" x2="12" y2="21" />
        </svg>
      </div>
      <div class="card-heading">
        <h3 class="card-title">Host Resource Policy</h3>
        <p class="card-desc">Controls how many instances the whole host may run.</p>
      </div>
    </header>

    {#if loading}
      <div class="field-loading">Loading settings&hellip;</div>
    {:else if settings}
      <div class="field">
        <div class="field-label-row">
          <label class="field-label" for="admin-instance-limit">Global Instance Limit</label>
          <span class="field-current">
            Current: {settings.host_instance_limit === 0 ? 'unlimited' : settings.host_instance_limit}
          </span>
        </div>
        <p class="field-desc">
          The maximum number of instances allowed to run at the same time across the entire host,
          all tiers and users combined.
        </p>
        <div class="field-control">
          <input
            id="admin-instance-limit"
            class="field-input"
            type="number"
            min="0"
            step="1"
            value={settings.host_instance_limit}
            oninput={(e) => update('host_instance_limit', e.currentTarget.value)}
            disabled={saving}
          />
          <span class="field-suffix">instances</span>
        </div>
        <p class="field-hint">Set to 0 for no limit.</p>
      </div>

      {#if error}
        <div class="error-banner" role="alert">{error}</div>
      {/if}

      <footer class="card-footer">
        <span class="save-status" aria-live="polite">
          {#if saved}
            <span class="save-saved">Saved</span>
          {/if}
        </span>
        <button class="save-btn" onclick={save} disabled={saving || loading}>
          {saving ? 'Saving&hellip;' : 'Save Changes'}
        </button>
      </footer>
    {:else}
      <div class="error-banner" role="alert">{error || 'Failed to load settings'}</div>
    {/if}
  </div>
</div>

<style>
  .page {
    display: flex;
    flex-direction: column;
    gap: 1.5rem;
    max-width: 720px;
  }

  .page-header {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  .page-title {
    font-size: 1.15rem;
    font-weight: 700;
    color: #f4f4f5;
    margin: 0;
  }

  .page-desc {
    font-size: 0.85rem;
    color: #71717a;
    margin: 0;
  }

  .settings-card {
    background: #141417;
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 14px;
    padding: 1.5rem;
    display: flex;
    flex-direction: column;
    gap: 1.25rem;
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.35);
  }

  .card-header {
    display: flex;
    align-items: flex-start;
    gap: 0.85rem;
  }

  .card-icon {
    width: 42px;
    height: 42px;
    border-radius: 10px;
    background: rgba(99, 102, 241, 0.14);
    border: 1px solid rgba(99, 102, 241, 0.3);
    color: #a5b4fc;
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
  }

  .card-icon svg {
    width: 22px;
    height: 22px;
  }

  .card-heading {
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
  }

  .card-title {
    font-size: 1rem;
    font-weight: 600;
    color: #f4f4f5;
    margin: 0;
  }

  .card-desc {
    font-size: 0.82rem;
    color: #71717a;
    margin: 0;
  }

  .field-loading {
    font-size: 0.85rem;
    color: #71717a;
    padding: 0.5rem 0;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 0.45rem;
    padding: 1.1rem;
    background: rgba(255, 255, 255, 0.02);
    border: 1px solid rgba(255, 255, 255, 0.06);
    border-radius: 10px;
  }

  .field-label-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
  }

  .field-label {
    font-size: 0.9rem;
    font-weight: 600;
    color: #e4e4e7;
  }

  .field-current {
    font-size: 0.75rem;
    font-weight: 500;
    color: #a78bfa;
    background: rgba(139, 92, 246, 0.1);
    border: 1px solid rgba(139, 92, 246, 0.25);
    padding: 2px 8px;
    border-radius: 999px;
    white-space: nowrap;
  }

  .field-desc {
    font-size: 0.8rem;
    line-height: 1.5;
    color: #a1a1aa;
    margin: 0;
  }

  .field-control {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    margin-top: 0.4rem;
  }

  .field-input {
    width: 160px;
    font-family: 'JetBrains Mono', monospace;
    font-size: 0.95rem;
    font-weight: 500;
    color: #f4f4f5;
    background: #0d0d10;
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 8px;
    padding: 0.55rem 0.75rem;
    outline: none;
    transition: border-color 0.2s, box-shadow 0.2s;
    -moz-appearance: textfield;
    appearance: textfield;
  }

  .field-input::-webkit-outer-spin-button,
  .field-input::-webkit-inner-spin-button {
    -webkit-appearance: none;
    margin: 0;
  }

  .field-input:focus {
    border-color: #6366f1;
    box-shadow: 0 0 0 3px rgba(99, 102, 241, 0.25);
  }

  .field-input:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .field-suffix {
    font-size: 0.8rem;
    color: #71717a;
  }

  .field-hint {
    font-size: 0.75rem;
    color: #71717a;
    margin: 0;
  }

  .error-banner {
    background: rgba(239, 68, 68, 0.1);
    border: 1px solid rgba(239, 68, 68, 0.3);
    color: #fca5a5;
    font-size: 0.82rem;
    padding: 0.65rem 0.9rem;
    border-radius: 8px;
  }

  .card-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    border-top: 1px solid rgba(255, 255, 255, 0.06);
    padding-top: 1.1rem;
  }

  .save-status {
    min-height: 1.2rem;
  }

  .save-saved {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 0.8rem;
    font-weight: 600;
    color: #4ade80;
  }

  .save-saved::before {
    content: '';
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: #4ade80;
  }

  .save-btn {
    background: #6366f1;
    color: #fff;
    border: none;
    border-top: 1px solid rgba(255, 255, 255, 0.35);
    font-family: inherit;
    font-size: 0.85rem;
    font-weight: 600;
    padding: 0.6rem 1.4rem;
    border-radius: 8px;
    cursor: pointer;
    box-shadow: 0 4px 16px rgba(99, 102, 241, 0.3);
    transition: all 0.2s;
  }

  .save-btn:hover:not(:disabled) {
    background: #4f46e5;
    transform: translateY(-1px);
  }

  .save-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
</style>
