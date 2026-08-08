<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { getTemplateIcon } from '$lib/utils/template-icons';
  import TemplatePanel from '$lib/components/templates/TemplatePanel.svelte';
  import { parseDashboardHash, serializeDashboardHash, isTemplatesEditor, confirmDiscardChanges, type DashboardView, type DashboardTab } from '$lib/templates/dashboard-view';
  import { loadDashboard } from './dashboard-data';
  import { performAction, deleteInstance } from '$lib/api/instance-actions';
  import { launchInstance, deleteTemplate } from '$lib/api/template-actions';
  import {
    auth,
    isAdmin,
    canCreateTemplate,
    canManageUsers,
    canManageGroupInstances,
    canViewMonitoring,
    canViewAuditLogs,
    effectiveMaxInstances
  } from '$lib/stores/auth';
  import { mayControlInstance, mayLaunchTemplate } from '$lib/permissions';
  import { api } from '$lib/api/client';
  import { wrapperUrl, formatRemaining, remainingMs } from '$lib/countdown/countdown';
  import AdminSettings from '$lib/components/AdminSettings.svelte';
  import RejectionNotice from '$lib/components/RejectionNotice.svelte';
  import GroupPanel from '$lib/components/groups/GroupPanel.svelte';
  import UserManagementPanel from '$lib/components/users/UserManagementPanel.svelte';
  import OrphanedVolumesPanel from '$lib/components/volumes/OrphanedVolumesPanel.svelte';
  import MonitorPanel from '$lib/components/monitor/MonitorPanel.svelte';
  import LogsPanel from '$lib/components/logs/LogsPanel.svelte';
  import ContainerLogPanel from '$lib/components/instances/ContainerLogPanel.svelte';
  import type { Template, Instance, PreflightRejection } from '$lib/types';

  let sidebarOpen = $state(false);
  let view = $state<DashboardView>({ tab: 'instances' });
  let activeTab = $derived(view.tab);
  let showSettings = $state(false);
  let panelDirty = $state(false);
  let configs = $state<Template[]>([]);
  let instances = $state<Instance[]>([]);
  let loading = $state(true);
  let rejectionNotice = $state<{ error: string; rejection: PreflightRejection } | null>(null);
  let logsInstance = $state<Instance | null>(null);

  let launchModal = $state<{ open: boolean; config: Template | null }>({ open: false, config: null });
  let launchTarget = $state<'current' | 'tab'>('current');
  let launchPersistence = $state<'use_persistent' | 'no_persistent' | 'reset_persistent'>('use_persistent');
  let prevLaunchPersistence = $state<'use_persistent' | 'no_persistent' | 'reset_persistent'>('use_persistent');
  let showPersistenceSelect = $derived(!!launchModal.config?.persistent_storage_path);

  let filterUser = $state('');
  let filterStatus = $state('');

  let pwCurrent = $state('');
  let pwNew = $state('');
  let pwError = $state('');
  let pwSuccess = $state(false);
  let pwSaving = $state(false);

  async function onChangePassword() {
    pwError = '';
    pwSuccess = false;
    if (!pwCurrent || !pwNew) {
      pwError = 'Both fields are required';
      return;
    }
    pwSaving = true;
    const res = await api.post('/auth/change-password', {
      current_password: pwCurrent,
      new_password: pwNew
    });
    pwSaving = false;
    if (res.error) {
      pwError = res.error;
      return;
    }
    pwCurrent = '';
    pwNew = '';
    pwSuccess = true;
  }

  let canManage = $derived($canCreateTemplate || $canManageUsers || $canManageGroupInstances);
  let effectiveLimitLabel = $derived($isAdmin || $effectiveMaxInstances === 0 ? 'Unlimited' : String($effectiveMaxInstances));
  let allowedTemplateLabel = $derived(String(configs.filter((c) => mayLaunchTemplate($auth, c)).length));
  // The session-launch surface only offers usable templates: hidden templates
  // are excluded here (they remain visible in the templates-management panel,
  // where managers can restore them).
  let quickLaunchTemplates = $derived(configs.filter((c) => c.visibility !== 'hidden'));

  function navigateToHash(hash: string) {
    view = parseDashboardHash(hash);
    if (window.location.hash !== hash) {
      window.location.hash = hash;
    }
  }

  function confirmLeaveEditor(hash: string): boolean {
    if (!isTemplatesEditor(view) || !panelDirty) return true;
    if (serializeDashboardHash(parseDashboardHash(hash)) === serializeDashboardHash(view)) return true;
    return confirmDiscardChanges();
  }

  function navigateTab(tab: DashboardTab) {
    const next: DashboardView = tab === 'templates' ? { tab: 'templates', editor: 'list' } : { tab };
    const hash = serializeDashboardHash(next);
    if (!confirmLeaveEditor(hash)) return;
    navigateToHash(hash);
  }

  function onHashChange() {
    const next = parseDashboardHash(window.location.hash);
    if (!confirmLeaveEditor(window.location.hash)) {
      history.replaceState(null, '', serializeDashboardHash(view));
      return;
    }
    view = next;
  }

  function onBeforeUnload(e: BeforeUnloadEvent) {
    if (panelDirty && isTemplatesEditor(view)) {
      e.preventDefault();
      e.returnValue = '';
    }
  }

  onMount(() => {
    view = parseDashboardHash(window.location.hash);
    window.addEventListener('hashchange', onHashChange);
    window.addEventListener('beforeunload', onBeforeUnload);

    loadDashboard().then((data) => {
      configs = data.configs;
      instances = data.instances;
      loading = false;
    });

    const poll = setInterval(async () => {
      const res = await api.get<{ instances: Instance[] }>('/instances');
      if (res.data?.instances) instances = res.data.instances;
    }, 5000);

    return () => {
      clearInterval(poll);
      window.removeEventListener('hashchange', onHashChange);
      window.removeEventListener('beforeunload', onBeforeUnload);
    };
  });

  function openLaunch(config: Template) {
    if (!mayLaunchTemplate($auth, config)) return;
    launchModal = { open: true, config };
    launchTarget = 'current';
    launchPersistence = 'use_persistent';
    prevLaunchPersistence = 'use_persistent';
  }

  function onLaunchPersistenceChange(event: Event) {
    const next = (event.currentTarget as HTMLSelectElement).value as
      | 'use_persistent'
      | 'no_persistent'
      | 'reset_persistent';
    if (next === 'reset_persistent') {
      const proceed = window.confirm(
        'Reset persistent storage will erase the existing data and start a fresh environment. Continue?'
      );
      if (!proceed) {
        launchPersistence = prevLaunchPersistence;
        (event.currentTarget as HTMLSelectElement).value = prevLaunchPersistence;
        return;
      }
    }
    prevLaunchPersistence = next;
  }

  async function confirmLaunch() {
    if (!launchModal.config) return;
    const result = await launchInstance(launchModal.config.id, launchPersistence);
    if (result.error) {
      if (result.rejection) {
        launchModal = { open: false, config: null };
        rejectionNotice = { error: result.error, rejection: result.rejection };
      } else {
        alert(result.error);
      }
      return;
    }
    const inst = result.instance;
    if (inst) {
      instances = [...instances, inst];
      const url = '/instances/' + inst.id;
      if (launchTarget === 'tab') {
        window.open(url, '_blank');
      } else {
        goto(url);
      }
    }
    launchModal = { open: false, config: null };
  }

  function instanceUrl(inst: Instance): string {
    return wrapperUrl(inst.remote_type, inst.access_token ?? '');
  }

  function sleepLabel(inst: Instance): string | null {
    if (inst.status !== 'running') return null;
    const remaining = remainingMs(inst.auto_sleeps_at, Date.now());
    if (remaining === null || remaining <= 0) return null;
    return `Left ${formatRemaining(remaining)}`;
  }

  async function onDeleteConfig(config: Template) {
    const result = await deleteTemplate(config.id);
    if (result.cancelled) return;
    if (result.error) {
      alert(result.error);
      return;
    }
    configs = configs.filter(c => c.id !== config.id);
  }

  async function onAction(inst: Instance, action: 'start' | 'stop' | 'pause' | 'unpause') {
    const result = await performAction(inst.id, action);
    if (result.rejection) {
      rejectionNotice = { error: result.error ?? '', rejection: result.rejection };
      return;
    }
    if (result.status) {
      instances = instances.map(i => i.id === inst.id ? { ...i, status: result.status! } : i);
    }
  }

  async function onRemove(inst: Instance) {
    if (!confirm(`Delete "${inst.name}"? The container will be removed.`)) return;
    const result = await deleteInstance(inst.id);
    if (!result.error) {
      instances = instances.filter(i => i.id !== inst.id);
    }
  }

  const myInstances = $derived(instances.filter(i => mayControlInstance($auth, i)));

  const uniqueUsers = $derived([...new Set(myInstances.map(i => i.owner_username).filter(Boolean))].sort());
  const filteredInstances = $derived(
    myInstances.filter(i => {
      if (filterUser && i.owner_username !== filterUser) return false;
      if (filterStatus && i.status !== filterStatus) return false;
      return true;
    })
  );

  const statusColors: Record<string, string> = {
    running: 'dot-active',
    paused: 'dot-paused',
    stopped: 'dot-stopped',
    error: 'dot-error',
    starting: 'dot-starting',
  };
</script>

<div class="dashboard">
  <aside
    class="sidebar"
    class:expanded={sidebarOpen}
    onmouseenter={() => sidebarOpen = true}
    onmouseleave={() => { sidebarOpen = false; }}
  >
    <div class="sidebar-top">
      <div class="brand-icon">
        <span class="pulse"></span>
      </div>
      {#if sidebarOpen}
        <span class="brand-name">OpenWorkspace</span>
      {/if}
    </div>

    <nav class="nav-list">
      <div class="nav-section">
        {#if sidebarOpen}
          <span class="nav-section-label">Workspaces</span>
        {/if}

        <button
          class="nav-item"
          class:active={activeTab === 'instances'}
          onclick={() => navigateTab('instances')}
        >
          <svg class="nav-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <rect x="2" y="3" width="20" height="14" rx="2" /><line x1="8" y1="21" x2="16" y2="21" /><line x1="12" y1="17" x2="12" y2="21" />
          </svg>
          {#if sidebarOpen}<span class="nav-text">Instances</span>{/if}
        </button>

        {#if canManage}
          <button
            class="nav-item"
            class:active={activeTab === 'templates'}
            onclick={() => navigateTab('templates')}
          >
            <svg class="nav-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path d="M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z" />
            </svg>
            {#if sidebarOpen}<span class="nav-text">Templates</span>{/if}
          </button>

          <button
            class="nav-item"
            class:active={activeTab === 'sessions'}
            onclick={() => navigateTab('sessions')}
          >
            <svg class="nav-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <rect x="2" y="2" width="20" height="8" rx="2" /><rect x="2" y="14" width="20" height="8" rx="2" /><line x1="6" y1="6" x2="6.01" y2="6" /><line x1="6" y1="18" x2="6.01" y2="18" />
            </svg>
            {#if sidebarOpen}<span class="nav-text">Sessions</span>{/if}
          </button>
        {/if}

        {#if $canManageUsers}
          <button
            class="nav-item"
            class:active={activeTab === 'volumes'}
            onclick={() => navigateTab('volumes')}
          >
            <svg class="nav-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path d="M22 12H2" /><path d="M5.45 5.11 2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z" /><line x1="6" y1="16" x2="6.01" y2="16" /><line x1="10" y1="16" x2="10.01" y2="16" />
            </svg>
            {#if sidebarOpen}<span class="nav-text">Volumes</span>{/if}
          </button>
        {/if}
      </div>

      <div class="nav-section">
        {#if sidebarOpen}
          <span class="nav-section-label">RBAC</span>
        {/if}

        {#if $isAdmin}
          <button
            class="nav-item"
            class:active={activeTab === 'groups'}
            onclick={() => navigateTab('groups')}
          >
            <svg class="nav-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path d="M17 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2" /><circle cx="9" cy="7" r="4" /><path d="M23 21v-2a4 4 0 0 0-3-3.87" /><path d="M16 3.13a4 4 0 0 1 0 7.75" />
            </svg>
            {#if sidebarOpen}<span class="nav-text">Groups</span>{/if}
          </button>
        {/if}

        {#if $canManageUsers}
          <button
            class="nav-item"
            class:active={activeTab === 'users'}
            onclick={() => navigateTab('users')}
          >
            <svg class="nav-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2" /><circle cx="12" cy="7" r="4" />
            </svg>
            {#if sidebarOpen}<span class="nav-text">Users</span>{/if}
          </button>
        {/if}
      </div>

      <div class="nav-section">
        {#if sidebarOpen}
          <span class="nav-section-label">Server</span>
        {/if}

        {#if $canViewMonitoring}
          <button
            class="nav-item"
            class:active={activeTab === 'monitor'}
            onclick={() => navigateTab('monitor')}
          >
            <svg class="nav-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path d="M3 3v18h18" /><path d="M7 14l4-4 3 3 5-6" />
            </svg>
            {#if sidebarOpen}<span class="nav-text">Monitor</span>{/if}
          </button>
        {/if}

        {#if $isAdmin}
          <button
            class="nav-item"
            class:active={activeTab === 'settings'}
            onclick={() => navigateTab('settings')}
          >
            <svg class="nav-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path d="M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6z" /><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" />
            </svg>
            {#if sidebarOpen}<span class="nav-text">Settings</span>{/if}
          </button>
        {/if}

        {#if $canViewAuditLogs}
          <button
            class="nav-item"
            class:active={activeTab === 'logs'}
            onclick={() => navigateTab('logs')}
          >
            <svg class="nav-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" /><path d="M14 2v6h6" /><path d="M8 13h8" /><path d="M8 17h8" /><path d="M8 9h2" />
            </svg>
            {#if sidebarOpen}<span class="nav-text">Logs</span>{/if}
          </button>
        {/if}
      </div>
    </nav>

    <div class="sidebar-bottom">
      <button class="avatar-btn" onclick={() => showSettings = !showSettings}>
        <div class="avatar">OW</div>
      </button>
      {#if sidebarOpen}
        <button class="user-btn" onclick={() => showSettings = !showSettings}>
          <span class="user-name">Developer</span>
          <span class="user-role">Settings</span>
        </button>
      {/if}
    </div>
  </aside>

  {#if showSettings}
    <div class="settings-overlay" onclick={() => showSettings = false} role="presentation"></div>
    <div class="settings-panel">
      <div class="settings-header">
        <h2 class="settings-title">Account</h2>
        <button class="settings-close" onclick={() => showSettings = false}>&times;</button>
      </div>
      <div class="settings-section">
        <span class="settings-label">Change Password</span>
        <p class="settings-desc">Update your account password.</p>
        <div class="settings-row col">
          <input
            id="pw-current"
            class="modal-input"
            type="password"
            placeholder="Current password"
            bind:value={pwCurrent}
          />
        </div>
        <div class="settings-row col">
          <input
            id="pw-new"
            class="modal-input"
            type="password"
            placeholder="New password"
            bind:value={pwNew}
          />
        </div>
        {#if pwError}
          <div class="error-badge">{pwError}</div>
        {/if}
        {#if pwSuccess}
          <span class="pw-saved">Password updated</span>
        {/if}
        <div class="settings-row">
          <button class="modal-confirm" onclick={onChangePassword} disabled={pwSaving}>
            {pwSaving ? 'Saving...' : 'Update Password'}
          </button>
        </div>
      </div>
      <div class="settings-section">
        <span class="settings-label">Session</span>
        <div class="settings-row">
          <button class="settings-action" onclick={() => auth.logout()}>Sign Out</button>
        </div>
      </div>
    </div>
  {/if}

  {#if launchModal.open && launchModal.config}
    <div class="modal-overlay" onclick={() => launchModal = { open: false, config: null }} role="presentation"></div>
    <div class="modal-card">
      <h3 class="modal-title">Launch {launchModal.config.name}</h3>
      <p class="modal-desc">Choose how to open this instance.</p>
      <div class="modal-field">
        <label for="launch-select" class="modal-label">Open in</label>
        <select id="launch-select" class="modal-select" bind:value={launchTarget}>
          <option value="current">Current Page</option>
          <option value="tab">New Tab</option>
        </select>
      </div>
      {#if showPersistenceSelect}
        <div class="modal-field">
          <label for="launch-persistence" class="modal-label">Data Persistence</label>
          <select
            id="launch-persistence"
            class="modal-select"
            bind:value={launchPersistence}
            onchange={onLaunchPersistenceChange}
          >
            <option value="use_persistent">Use persistent storage</option>
            <option value="no_persistent">No persistent storage</option>
            <option value="reset_persistent">Reset persistent storage</option>
          </select>
        </div>
      {/if}
      <div class="modal-actions">
        <button class="modal-cancel" onclick={() => launchModal = { open: false, config: null }}>Cancel</button>
        <button class="modal-confirm" onclick={confirmLaunch}>Launch</button>
      </div>
    </div>
  {/if}

  {#if logsInstance}
    <ContainerLogPanel instance={logsInstance} onclose={() => logsInstance = null} />
  {/if}

  <RejectionNotice
    error={rejectionNotice?.error ?? ''}
    rejection={rejectionNotice?.rejection ?? null}
    onclose={() => rejectionNotice = null}
  />

  <main class="main-content">
    {#if loading}
      <p class="loading-text">Loading instances...</p>

    {:else if activeTab === 'instances'}
      <section class="ws-section">
        <h2 class="section-title">Instances</h2>
        <p class="section-desc">Effective ceiling: {effectiveLimitLabel} instances · Allowed templates: {allowedTemplateLabel}</p>
        {#if myInstances.length === 0}
          <p class="empty-text">No instances yet. Launch a template to get started.</p>
        {:else}
          <div class="instance-grid">
            {#each myInstances as inst (inst.id)}
              <div class="ws-card" class:dimmed={inst.status !== 'running'}>
                <div class="ws-card-header">
                  <div>
                    <div class="ws-title-row">
                      <span class="status-dot {statusColors[inst.status] || 'dot-stopped'}"></span>
                      <h3 class="ws-name">{inst.name}</h3>
                      {#if inst.mount_persistent}
                        <span class="persist-badge">persist</span>
                      {/if}
                    </div>
                    <span class="ws-template">{inst.template_name || 'Unknown template'}</span>
                    {#if sleepLabel(inst)}
                      <span class="ws-sleep">{sleepLabel(inst)}</span>
                    {/if}
                  </div>
                  <span class="ws-id">{inst.id.slice(0, 8)}</span>
                </div>
                <div class="ws-actions">
                  {#if mayControlInstance($auth, inst)}
                    <div class="action-buttons">
                      {#if inst.status === 'running'}
                        {#if inst.access_token}
                           <a href={instanceUrl(inst)} target="_blank" class="launch-btn vnc">Open</a>
                        {/if}
                        <button class="launch-btn pause" onclick={() => onAction(inst, 'pause')}>Pause</button>
                        <button class="launch-btn stop" onclick={() => onAction(inst, 'stop')}>Stop</button>
                      {:else if inst.status === 'paused'}
                        <button class="launch-btn resume" onclick={() => onAction(inst, 'unpause')}>Resume</button>
                        <button class="launch-btn stop" onclick={() => onAction(inst, 'stop')}>Stop</button>
                      {:else}
                        <button class="launch-btn resume" onclick={() => onAction(inst, 'start')}>Start</button>
                      {/if}
                      <button class="launch-btn logs" onclick={() => logsInstance = inst}>Logs</button>
                      <button class="launch-btn remove" onclick={() => onRemove(inst)}>Remove</button>
                    </div>
                  {/if}
                </div>
              </div>
            {/each}
          </div>
        {/if}
      </section>

      <section class="ws-section">
        <h2 class="section-title">Quick Launch</h2>
        <p class="section-desc">Pick a template to spin up a new instance.</p>
        <div class="template-grid">
          {#each quickLaunchTemplates as config (config.id)}
            {@const launchable = mayLaunchTemplate($auth, config)}
            <button class="template-card" class:locked={!launchable} onclick={() => openLaunch(config)}>
              <span class="template-icon">{getTemplateIcon(config.name)}</span>
              <span class="template-name">{config.name}</span>
              <span class="template-access">{launchable ? 'Allowed' : 'Not allowed'}</span>
            </button>
          {/each}
        </div>
      </section>

    {:else if activeTab === 'sessions' && canManage}
      <section class="ws-section">
        <h2 class="section-title">All Instances</h2>

        <div class="filter-bar">
          <div class="filter-grid">
            <div class="filter-group">
              <label class="filter-label" for="filter-user">User</label>
              <select id="filter-user" class="filter-select" bind:value={filterUser}>
                <option value="">All Users</option>
                {#each uniqueUsers as user (user)}
                  <option value={user}>{user}</option>
                {/each}
              </select>
            </div>
            <div class="filter-group">
              <label class="filter-label" for="filter-status">Status</label>
              <select id="filter-status" class="filter-select" bind:value={filterStatus}>
                <option value="">All Statuses</option>
                <option value="running">Running</option>
                <option value="starting">Starting</option>
                <option value="paused">Paused</option>
                <option value="stopped">Stopped</option>
                <option value="error">Error</option>
              </select>
            </div>
          </div>
          <div class="filter-actions-row">
            <span class="filter-count">{filteredInstances.length} instance{filteredInstances.length !== 1 ? 's' : ''}</span>
            {#if filterUser || filterStatus}
              <button class="filter-clear" onclick={() => { filterUser = ''; filterStatus = ''; }}>Clear filters</button>
            {/if}
          </div>
        </div>

        {#if filteredInstances.length === 0}
          <p class="empty-text">No instances match the current filters.</p>
        {:else}
          <div class="instances-table-wrap">
            <table class="instances-table">
              <thead>
                <tr>
                  <th>Name</th>
                  <th>Owner</th>
                  <th>Template</th>
                  <th>Status</th>
                  <th>Auto-Sleep</th>
                  <th>Created</th>
                  <th>Actions</th>
                </tr>
              </thead>
              <tbody>
                {#each filteredInstances as inst (inst.id)}
                  <tr>
                    <td class="td-name">
                      <span class="td-name-text">{inst.name}</span>
                      <span class="td-id">{inst.id.slice(0, 8)}</span>
                    </td>
                    <td class="td-owner">{inst.owner_username || '---'}</td>
                    <td>{inst.template_name || '---'}</td>
                    <td>
                      <span class="status-badge {statusColors[inst.status] || ''}">
                        <span class="status-dot-inline"></span>
                        {inst.status}
                      </span>
                    </td>
                    <td class="td-sleep">
                      {#if sleepLabel(inst)}
                        {sleepLabel(inst)}
                      {/if}
                    </td>
                    <td class="td-date">{new Date(inst.created_at).toLocaleDateString()}</td>
                    <td class="td-actions">
                      {#if mayControlInstance($auth, inst)}
                        <div class="action-buttons">
                          {#if inst.status === 'running'}
                            <button class="launch-btn pause sm" onclick={() => onAction(inst, 'pause')}>Pause</button>
                            <button class="launch-btn stop sm" onclick={() => onAction(inst, 'stop')}>Stop</button>
                          {:else if inst.status === 'paused'}
                            <button class="launch-btn resume sm" onclick={() => onAction(inst, 'unpause')}>Resume</button>
                            <button class="launch-btn stop sm" onclick={() => onAction(inst, 'stop')}>Stop</button>
                          {:else}
                            <button class="launch-btn resume sm" onclick={() => onAction(inst, 'start')}>Start</button>
                          {/if}
                          <button class="launch-btn logs sm" onclick={() => logsInstance = inst}>Logs</button>
                          <button class="launch-btn remove sm" onclick={() => onRemove(inst)}>Remove</button>
                        </div>
                      {:else}
                        <span class="td-date">No access</span>
                      {/if}
                    </td>
                  </tr>
                {/each}
              </tbody>
            </table>
          </div>
        {/if}
      </section>

    {:else if activeTab === 'users' && $canManageUsers}
      <UserManagementPanel ctx={$auth} />

    {:else if activeTab === 'groups' && $isAdmin}
      <GroupPanel ctx={$auth} templates={configs} />

    {:else if activeTab === 'volumes' && $canManageUsers}
      <OrphanedVolumesPanel ctx={$auth} />

    {:else if activeTab === 'templates'}
      <TemplatePanel
        {view}
        bind:configs
        bind:dirty={panelDirty}
        onnavigate={navigateToHash}
        ondelete={onDeleteConfig}
        ctx={$auth}
      />

    {:else if activeTab === 'settings' && $isAdmin}
      <AdminSettings />

    {:else if activeTab === 'monitor' && $canViewMonitoring}
      <MonitorPanel ctx={$auth} />

    {:else if activeTab === 'logs' && $canViewAuditLogs}
      <LogsPanel ctx={$auth} />
    {/if}
  </main>
</div>

<style>
  :global(body) {
    margin: 0;
    background-color: #09090b;
    color: #f4f4f5;
    font-family: 'Plus Jakarta Sans', -apple-system, sans-serif;
  }

  .dashboard {
    display: flex;
    width: 100vw;
    height: 100vh;
    overflow: hidden;
    background: radial-gradient(circle at 0% 0%, #18181b 0%, #09090b 100%);
    position: relative;
  }

  /* Sidebar */
  .sidebar {
    width: 64px;
    height: 100%;
    background: rgba(18, 18, 22, 0.85);
    border-right: 1px solid rgba(255, 255, 255, 0.08);
    backdrop-filter: blur(20px);
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    padding: 1.2rem 0.8rem;
    transition: width 0.25s cubic-bezier(0.16, 1, 0.3, 1);
    z-index: 50;
    box-sizing: border-box;
  }

  .sidebar.expanded { width: 240px; }

  .sidebar-top {
    display: flex;
    align-items: center;
    gap: 12px;
    padding-bottom: 1.5rem;
    border-bottom: 1px solid rgba(255, 255, 255, 0.05);
  }

  .brand-icon {
    width: 32px;
    height: 32px;
    border-radius: 8px;
    background: rgba(99, 102, 241, 0.2);
    border: 1px solid rgba(99, 102, 241, 0.4);
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
  }

  .pulse {
    width: 8px;
    height: 8px;
    background: #6366f1;
    border-radius: 50%;
    box-shadow: 0 0 10px #6366f1;
  }

  .brand-name {
    font-size: 0.95rem;
    font-weight: 700;
    letter-spacing: -0.01em;
    white-space: nowrap;
  }

  .nav-list {
    display: flex;
    flex-direction: column;
    gap: 1.1rem;
    margin-top: 1.5rem;
  }

  .nav-section {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .nav-section-label {
    font-size: 0.68rem;
    font-weight: 600;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    color: #52525b;
    padding: 0 0.75rem 0.25rem;
    white-space: nowrap;
  }

  .nav-item {
    display: flex;
    align-items: center;
    gap: 12px;
    background: transparent;
    border: none;
    color: #71717a;
    padding: 0.75rem;
    border-radius: 8px;
    cursor: pointer;
    transition: all 0.2s;
    width: 100%;
    text-align: left;
  }

  .nav-item:hover,
  .nav-item.active {
    background: rgba(255, 255, 255, 0.05);
    color: #f4f4f5;
  }

  .nav-item.active { border-left: 2px solid #6366f1; }

  .nav-icon { width: 20px; height: 20px; flex-shrink: 0; }

  .nav-text {
    font-size: 0.85rem;
    font-weight: 500;
    white-space: nowrap;
  }

  .sidebar-bottom {
    display: flex;
    align-items: center;
    gap: 10px;
    padding-top: 1rem;
    border-top: 1px solid rgba(255, 255, 255, 0.05);
  }

  .avatar-btn {
    background: none;
    border: none;
    padding: 0;
    cursor: pointer;
    flex-shrink: 0;
  }

  .avatar {
    width: 32px;
    height: 32px;
    border-radius: 50%;
    background: #27272a;
    color: #a1a1aa;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 0.75rem;
    font-weight: 700;
    transition: background 0.2s;
  }

  .avatar-btn:hover .avatar { background: #3f3f46; }

  .user-btn {
    display: flex;
    flex-direction: column;
    background: none;
    border: none;
    cursor: pointer;
    text-align: left;
    padding: 0;
    white-space: nowrap;
  }

  .user-name { font-size: 0.8rem; font-weight: 600; color: #f4f4f5; }
  .user-role { font-size: 0.65rem; color: #71717a; }

  /* Settings Panel */
  .settings-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.5);
    z-index: 100;
  }

  .settings-panel {
    position: fixed;
    bottom: 0;
    left: 0;
    width: 320px;
    height: 100vh;
    background: rgba(18, 18, 22, 0.95);
    backdrop-filter: blur(24px);
    border-right: 1px solid rgba(255, 255, 255, 0.08);
    z-index: 101;
    padding: 1.5rem;
    box-sizing: border-box;
    display: flex;
    flex-direction: column;
    gap: 1.5rem;
    overflow-y: auto;
  }

  .settings-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .settings-title { font-size: 1.1rem; font-weight: 600; margin: 0; }

  .settings-close {
    background: none;
    border: none;
    color: #71717a;
    font-size: 1.5rem;
    cursor: pointer;
    padding: 0;
    line-height: 1;
    transition: color 0.2s;
  }

  .settings-close:hover { color: #f4f4f5; }

  .settings-section {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 1rem;
    background: rgba(0, 0, 0, 0.3);
    border: 1px solid rgba(255, 255, 255, 0.06);
    border-radius: 10px;
  }

  .settings-label {
    font-size: 0.7rem;
    font-weight: 600;
    color: #71717a;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .settings-desc { font-size: 0.8rem; color: #a1a1aa; margin: 0; }

  .settings-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .settings-row.col {
    flex-direction: column;
    align-items: stretch;
    gap: 6px;
    margin-bottom: 0.75rem;
  }

  .settings-value { font-size: 0.85rem; color: #d4d4d8; }

  .pw-saved {
    font-size: 0.75rem;
    color: #4ade80;
    margin-bottom: 0.75rem;
  }

  .settings-action {
    background: rgba(239, 68, 68, 0.1);
    border: 1px solid rgba(239, 68, 68, 0.2);
    color: #f87171;
    font-size: 0.75rem;
    font-weight: 600;
    padding: 0.35rem 0.75rem;
    border-radius: 6px;
    cursor: pointer;
    font-family: inherit;
    transition: all 0.2s;
  }

  .settings-action:hover { background: rgba(239, 68, 68, 0.2); }

  /* Launch Modal */
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
    width: 360px;
    background: rgba(20, 20, 26, 0.95);
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

  .modal-title { font-size: 1.1rem; font-weight: 600; margin: 0; }
  .modal-desc { font-size: 0.8rem; color: #71717a; margin: 0; }

  .modal-field { display: flex; flex-direction: column; gap: 6px; margin-bottom: 1rem; }
  .modal-label {
    font-size: 0.7rem;
    font-weight: 600;
    color: #71717a;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .modal-input, .modal-select {
    background: rgba(0, 0, 0, 0.4);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 8px;
    padding: 0.6rem 0.75rem;
    color: #f4f4f5;
    font-size: 0.85rem;
    font-family: inherit;
    outline: none;
  }

  .modal-input:focus, .modal-select:focus { border-color: #818cf8; }

  .modal-input:disabled { opacity: 0.5; cursor: not-allowed; }

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

  .modal-cancel:hover { background: rgba(255, 255, 255, 0.1); color: #fff; }

  .modal-confirm {
    background: #6366f1;
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

  .modal-confirm:hover { background: #4f46e5; }

  /* Main Content */
  .main-content {
    flex: 1;
    padding: 2.5rem 3rem;
    overflow-y: auto;
  }

  .ws-section { margin-bottom: 2.5rem; }

  .section-title {
    font-size: 0.7rem;
    font-weight: 700;
    color: #52525b;
    text-transform: uppercase;
    letter-spacing: 0.1em;
    margin: 0 0 1rem 0;
  }

  .section-desc {
    font-size: 0.8rem;
    color: #71717a;
    margin: -0.5rem 0 1rem 0;
  }

  :global(.btn-create) {
    background: #6366f1;
    color: #fff;
    border: 1px solid rgba(255, 255, 255, 0.2);
    border-top: 1px solid rgba(255, 255, 255, 0.35);
    padding: 0.65rem 1.25rem;
    border-radius: 8px;
    font-size: 0.85rem;
    font-weight: 600;
    cursor: pointer;
    box-shadow: 0 4px 16px rgba(99, 102, 241, 0.3);
    text-decoration: none;
    transition: all 0.2s;
  }

  :global(.btn-create:hover) { background: #4f46e5; transform: translateY(-1px); }

  .loading-text,
  :global(.empty-text) { color: #71717a; font-size: 0.9rem; }

  /* Workspace Grid */
  :global(.instance-grid) {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
    gap: 1rem;
  }

  :global(.ws-card) {
    background: rgba(20, 20, 26, 0.7);
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-top: 1px solid rgba(255, 255, 255, 0.18);
    border-radius: 12px;
    padding: 1rem;
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    backdrop-filter: blur(12px);
    transition: all 0.2s;
  }

  :global(.ws-card:hover) {
    border-color: rgba(99, 102, 241, 0.4);
    box-shadow: 0 10px 30px -10px rgba(0, 0, 0, 0.5);
  }

  :global(.ws-card.dimmed) { opacity: 0.65; }

  :global(.ws-card-header) {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
  }

  :global(.ws-title-row) {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .persist-badge {
    display: inline-flex;
    align-items: center;
    font-size: 0.68rem;
    font-weight: 600;
    padding: 0.1rem 0.45rem;
    border-radius: 999px;
    color: #a5b4fc;
    background: rgba(99, 102, 241, 0.12);
    border: 1px solid rgba(129, 140, 248, 0.25);
  }

  .status-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: #52525b;
    flex-shrink: 0;
  }

  .dot-active {
    background: #22c55e;
    box-shadow: 0 0 8px #22c55e;
  }

  .dot-paused {
    background: #eab308;
    box-shadow: 0 0 8px #eab308;
  }

  .dot-stopped {
    background: #52525b;
  }

  .dot-error {
    background: #ef4444;
    box-shadow: 0 0 8px #ef4444;
  }

  .dot-starting {
    background: #3b82f6;
    box-shadow: 0 0 8px #3b82f6;
    animation: pulse 1.5s ease-in-out infinite;
  }

  @keyframes pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.4; }
  }

  :global(.ws-name) { font-size: 0.95rem; font-weight: 600; margin: 0; }
  :global(.ws-template) { font-size: 0.72rem; color: #71717a; display: block; margin-top: 2px; }

  .ws-sleep {
    display: block;
    font-size: 0.72rem;
    font-weight: 600;
    color: #a1a1aa;
    margin-top: 4px;
    font-variant-numeric: tabular-nums;
  }

  .td-sleep {
    font-size: 0.75rem;
    color: #a1a1aa;
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
  }

  :global(.ws-id) {
    font-family: monospace;
    font-size: 0.65rem;
    color: #52525b;
    background: rgba(255, 255, 255, 0.03);
    padding: 2px 6px;
    border-radius: 4px;
  }

  :global(.ws-metrics) {
    display: flex;
    gap: 1.5rem;
    margin: 1rem 0;
    padding: 0.6rem;
    background: rgba(0, 0, 0, 0.3);
    border-radius: 8px;
    border: 1px solid rgba(255, 255, 255, 0.04);
  }

  :global(.metric-item) { display: flex; flex-direction: column; gap: 2px; }
  :global(.metric-label) { font-size: 0.6rem; color: #71717a; text-transform: uppercase; }
  :global(.metric-value) { font-size: 0.8rem; font-weight: 600; color: #a1a1aa; }

  :global(.ws-actions) { display: flex; flex-direction: column; gap: 6px; }

  :global(.action-buttons) { display: flex; flex-wrap: wrap; gap: 6px; }

  :global(a.launch-btn),
  :global(.launch-btn) {
    font-size: 0.72rem;
    font-weight: 600;
    padding: 0.4rem 0.7rem;
    border-radius: 6px;
    text-decoration: none;
    color: #d4d4d8;
    background: rgba(255, 255, 255, 0.05);
    border: 1px solid rgba(255, 255, 255, 0.08);
    cursor: pointer;
    transition: all 0.2s;
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-family: inherit;
  }

  :global(a.launch-btn:hover),
  :global(.launch-btn:hover) { background: rgba(255, 255, 255, 0.12); color: #fff; }
  :global(.launch-btn.vnc:hover) { border-color: #3b82f6; color: #60a5fa; }
  :global(.launch-btn.pause:hover) { border-color: #eab308; color: #facc15; }
  :global(.launch-btn.resume:hover) { border-color: #22c55e; color: #4ade80; }
  :global(.launch-btn.stop:hover) { border-color: #f97316; color: #fb923c; }
  :global(.launch-btn.logs:hover) { border-color: #818cf8; color: #a5b4fc; }
  :global(.launch-btn.remove:hover) { border-color: #ef4444; color: #f87171; }
  :global(.launch-btn.edit:hover) { border-color: #22c55e; color: #4ade80; }
  :global(.launch-btn.sm) { font-size: 0.65rem; padding: 0.3rem 0.55rem; }

  /* Template Quick Launch */
  .template-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(120px, 1fr));
    gap: 0.75rem;
  }

  .template-card {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    padding: 1.25rem 0.75rem;
    background: rgba(20, 20, 26, 0.6);
    border: 1px solid rgba(255, 255, 255, 0.06);
    border-top: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 12px;
    cursor: pointer;
    transition: all 0.2s;
    font-family: inherit;
    text-align: center;
  }

  .template-card:hover {
    border-color: rgba(99, 102, 241, 0.4);
    background: rgba(30, 30, 38, 0.8);
    transform: translateY(-2px);
  }

  .template-icon { font-size: 1.5rem; }

  .template-name {
    max-width: 100%;
  }

  .template-card.locked {
    opacity: 0.55;
    cursor: not-allowed;
  }

  .template-card.locked:hover {
    border-color: rgba(255, 255, 255, 0.06);
    background: rgba(20, 20, 26, 0.6);
    transform: none;
  }

  .template-access {
    font-size: 0.62rem;
    font-weight: 600;
    color: #4ade80;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .template-card.locked .template-access {
    color: #71717a;
  }

  /* Filter Bar (shared: the Sessions view here and the audit filter bar in
     the child LogsPanel both use this chrome) */
  :global(.filter-bar) {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
    margin-bottom: 1.5rem;
    padding: 0.75rem 1rem;
    background: rgba(0, 0, 0, 0.25);
    border: 1px solid rgba(255, 255, 255, 0.06);
    border-radius: 10px;
  }

  :global(.filter-grid) {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(160px, 1fr));
    gap: 0.75rem 1rem;
    align-items: end;
  }

  :global(.filter-pair) {
    grid-column: span 2;
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0 1rem;
  }

  :global(.filter-group) {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
  }

  :global(.filter-label) {
    font-size: 0.6rem;
    font-weight: 600;
    color: #71717a;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  :global(.filter-select) {
    background: rgba(0, 0, 0, 0.4);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 6px;
    padding: 0.45rem 0.65rem;
    color: #f4f4f5;
    font-size: 0.8rem;
    font-family: inherit;
    outline: none;
    cursor: pointer;
    min-width: 140px;
  }

  :global(.filter-grid) select {
    width: 100%;
    min-width: 0;
    box-sizing: border-box;
  }

  :global(.filter-select:focus) { border-color: #818cf8; }

  :global(.filter-clear) {
    background: rgba(255, 255, 255, 0.05);
    border: 1px solid rgba(255, 255, 255, 0.08);
    color: #a1a1aa;
    padding: 0.45rem 0.75rem;
    border-radius: 6px;
    font-size: 0.75rem;
    cursor: pointer;
    font-family: inherit;
    transition: all 0.2s;
  }

  :global(.filter-clear:hover) { background: rgba(255, 255, 255, 0.1); color: #fff; }

  :global(.filter-actions-row) {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 8px;
  }

  :global(.filter-count) {
    font-size: 0.75rem;
    color: #52525b;
    margin-right: auto;
    white-space: nowrap;
  }

  .error-badge {
    background: rgba(239, 68, 68, 0.1);
    border: 1px solid rgba(239, 68, 68, 0.2);
    color: #f87171;
    font-size: 0.8rem;
    padding: 0.5rem;
    border-radius: 6px;
    text-align: center;
    margin-top: 0.5rem;
  }

  /* Shared table + panel chrome (used by the Instances view and the
     Groups/Users/Volumes admin panels, which are child components) */
  :global(.panel-card) {
    background: rgba(0, 0, 0, 0.25);
    border: 1px solid rgba(255, 255, 255, 0.06);
    border-radius: 14px;
    padding: 1.25rem;
  }

  :global(.panel-head) {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 1rem;
    margin-bottom: 1.25rem;
  }

  :global(.panel-head-title) {
    font-size: 1.15rem;
    font-weight: 700;
    color: #f4f4f5;
    margin: 0;
  }

  :global(.panel-head-desc) {
    font-size: 0.8rem;
    color: #71717a;
    margin: 0.25rem 0 0;
  }

  :global(.panel-toolbar) {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.6rem;
    margin-bottom: 1rem;
  }

  :global(.panel-search-wrap) {
    position: relative;
    flex: 1 1 240px;
    min-width: 200px;
  }

  :global(.panel-search) {
    width: 100%;
    box-sizing: border-box;
    background: rgba(0, 0, 0, 0.3);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 8px;
    padding: 0.55rem 0.75rem 0.55rem 2.1rem;
    color: #f4f4f5;
    font-size: 0.82rem;
    font-family: inherit;
    outline: none;
    transition: border-color 0.2s, box-shadow 0.2s;
  }

  :global(.panel-search::placeholder) { color: #52525b; }

  :global(.panel-search:focus) {
    border-color: #818cf8;
    box-shadow: 0 0 0 3px rgba(99, 102, 241, 0.2);
  }

  :global(.panel-search-icon) {
    position: absolute;
    left: 10px;
    top: 50%;
    transform: translateY(-50%);
    width: 14px;
    height: 14px;
    color: #52525b;
    pointer-events: none;
  }

  :global(.panel-select) {
    background: rgba(0, 0, 0, 0.3);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 8px;
    padding: 0.55rem 0.75rem;
    color: #d4d4d8;
    font-size: 0.8rem;
    font-family: inherit;
    outline: none;
    cursor: pointer;
    transition: border-color 0.2s;
  }

  :global(.panel-select:focus) { border-color: #818cf8; }

  :global(.panel-count) {
    margin-left: auto;
    font-size: 0.75rem;
    color: #71717a;
    white-space: nowrap;
  }

  :global(.panel-clear) {
    background: transparent;
    border: 1px solid rgba(255, 255, 255, 0.1);
    color: #a1a1aa;
    font-size: 0.72rem;
    padding: 0.45rem 0.75rem;
    border-radius: 8px;
    cursor: pointer;
    font-family: inherit;
    transition: all 0.2s;
  }

  :global(.panel-clear:hover) {
    color: #f4f4f5;
    border-color: rgba(255, 255, 255, 0.25);
  }

  :global(.instances-table-wrap) {
    overflow-x: auto;
    border: 1px solid rgba(255, 255, 255, 0.06);
    border-radius: 10px;
  }

  :global(.instances-table) {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.8rem;
  }

  :global(.instances-table thead) {
    background: rgba(0, 0, 0, 0.3);
  }

  :global(.instances-table th) {
    text-align: left;
    padding: 0.65rem 0.75rem;
    font-size: 0.65rem;
    font-weight: 700;
    color: #71717a;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    border-bottom: 1px solid rgba(255, 255, 255, 0.06);
    white-space: nowrap;
  }

  :global(.instances-table td) {
    padding: 0.6rem 0.75rem;
    border-bottom: 1px solid rgba(255, 255, 255, 0.04);
    color: #d4d4d8;
    vertical-align: middle;
  }

  :global(.instances-table tbody tr:hover) {
    background: rgba(255, 255, 255, 0.02);
  }

  :global(.instances-table tbody tr:last-child td) {
    border-bottom: none;
  }

  :global(.td-name) { min-width: 0; }

  :global(.td-name-text) { display: block; font-weight: 600; color: #f4f4f5; }

  :global(.td-id) {
    display: block;
    font-family: monospace;
    font-size: 0.65rem;
    color: #52525b;
    margin-top: 2px;
  }

  :global(.td-owner) { color: #a1a1aa; }

  :global(.td-date) {
    font-size: 0.75rem;
    color: #71717a;
    white-space: nowrap;
  }

  :global(.td-actions) { white-space: nowrap; }

  :global(.instances-table th:first-child),
  :global(.instances-table td:first-child) {
    min-width: 190px;
  }

  .status-badge {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 0.72rem;
    font-weight: 500;
    padding: 0.2rem 0.55rem;
    border-radius: 999px;
    background: rgba(255, 255, 255, 0.05);
    border: 1px solid rgba(255, 255, 255, 0.06);
    text-transform: capitalize;
  }

  .status-dot-inline {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: #52525b;
    flex-shrink: 0;
  }

  .status-badge.dot-active { color: #4ade80; border-color: rgba(34, 197, 94, 0.2); }
  .status-badge.dot-active .status-dot-inline { background: #22c55e; }

  .status-badge.dot-paused { color: #facc15; border-color: rgba(234, 179, 8, 0.2); }
  .status-badge.dot-paused .status-dot-inline { background: #eab308; }

  .status-badge.dot-stopped { color: #71717a; }
  .status-badge.dot-stopped .status-dot-inline { background: #52525b; }

  .status-badge.dot-error { color: #f87171; border-color: rgba(239, 68, 68, 0.2); }
  .status-badge.dot-error .status-dot-inline { background: #ef4444; }

  .status-badge.dot-starting { color: #60a5fa; border-color: rgba(59, 130, 246, 0.2); }
  .status-badge.dot-starting .status-dot-inline { background: #3b82f6; animation: pulse 1.5s ease-in-out infinite; }
</style>
