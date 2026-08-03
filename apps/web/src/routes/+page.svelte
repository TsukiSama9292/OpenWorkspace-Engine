<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { getTemplateIcon } from '$lib/utils/template-icons';
  import TemplatePanel from '$lib/components/templates/TemplatePanel.svelte';
  import { parseDashboardHash, serializeDashboardHash, isTemplatesEditor, confirmDiscardChanges, type DashboardView, type DashboardTab } from '$lib/templates/dashboard-view';
  import { loadDashboard } from './dashboard-data';
  import { performAction, deleteInstance } from '$lib/api/instance-actions';
  import { launchInstance, deleteTemplate } from '$lib/api/template-actions';
  import { auth, isManager } from '$lib/stores/auth';
  import { api } from '$lib/api/client';
  import { wrapperUrl, formatRemaining, remainingMs } from '$lib/countdown/countdown';
  import { formatMemory } from '$lib/utils/format';
  import { emptyQuotaForm, quotaFormFromUser, buildQuotaOverrides, type UserQuotaForm, type UserRow } from '$lib/users/user-quota';
  import AdminSettings from '$lib/components/AdminSettings.svelte';
  import QuotaModal from '$lib/components/quota/QuotaModal.svelte';
  import type { Template, Instance, Role, QuotaPayload } from '$lib/types';

  let sidebarOpen = $state(false);
  let view = $state<DashboardView>({ tab: 'instances' });
  let activeTab = $derived(view.tab);
  let showSettings = $state(false);
  let panelDirty = $state(false);
  let configs = $state<Template[]>([]);
  let instances = $state<Instance[]>([]);
  let loading = $state(true);
  let quotaNotice = $state<{ error: string; quota: QuotaPayload } | null>(null);

  let launchModal = $state<{ open: boolean; config: Template | null }>({ open: false, config: null });
  let launchTarget = $state<'current' | 'tab'>('current');
  let launchPersistence = $state<'use_persistent' | 'no_persistent' | 'reset_persistent'>('use_persistent');
  let prevLaunchPersistence = $state<'use_persistent' | 'no_persistent' | 'reset_persistent'>('use_persistent');
  let showPersistenceSelect = $derived(!!launchModal.config?.persistent_storage_path);

  let filterUser = $state('');
  let filterStatus = $state('');

  let isAdmin = $derived($auth?.role === 'admin');
  let canManage = $derived($isManager);

  type UserFormFields = { username: string; password: string; role: string } & UserQuotaForm;

  let users = $state<UserRow[]>([]);
  let usersLoading = $state(false);
  let showUserModal = $state(false);
  let editingUser = $state<UserRow | null>(null);
  let userForm = $state<UserFormFields>({ username: '', password: '', role: 'user', ...emptyQuotaForm() });
  let userFormError = $state('');

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

  async function loadUsers() {
    if (users.length > 0 || usersLoading) return;
    usersLoading = true;
    const { api } = await import('$lib/api/client');
    const res = await api.get<{ users: UserRow[] }>('/users');
    users = res.data?.users ?? [];
    usersLoading = false;
  }

  function copySshCommand(id: string) {
    const cmd = `ssh -J gateway.openworkspace.engine:2222 instance@${id}`;
    navigator.clipboard.writeText(cmd);
  }

  function openLaunch(config: Template) {
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
      if (result.quota) {
        launchModal = { open: false, config: null };
        quotaNotice = { error: result.error, quota: result.quota };
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
    if (result.quota) {
      quotaNotice = { error: result.error ?? '', quota: result.quota };
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

  const myInstances = $derived(instances);
  const runningInstances = $derived(myInstances.filter(i => i.status === 'running'));
  const pausedInstances = $derived(myInstances.filter(i => i.status === 'paused'));
  const stoppedInstances = $derived(myInstances.filter(i => i.status === 'stopped'));
  const errorInstances = $derived(myInstances.filter(i => i.status === 'error'));

  const uniqueUsers = $derived([...new Set(instances.map(i => i.owner_username).filter(Boolean))].sort());
  const filteredInstances = $derived(
    instances.filter(i => {
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

  function canControlInstance(inst: Instance): boolean {
    if (inst.owner_id === $auth?.id) return true;
    if ($auth?.role === 'admin') return true;
    if ($auth?.role === 'manager' && inst.owner_role === 'user') return true;
    return false;
  }

  function canEditUser(user: { role: string }): boolean {
    if (user.role === 'admin') return false;
    if ($auth?.role === 'manager' && user.role === 'manager') return false;
    return true;
  }

  function canDeleteUser(user: { role: string }): boolean {
    if (user.role === 'admin') return false;
    if ($auth?.role === 'manager' && user.role === 'manager') return false;
    return true;
  }

  function openEditUser(user: UserRow) {
    editingUser = user;
    userForm = { username: user.username, password: '', role: user.role, ...quotaFormFromUser(user) };
    showUserModal = true;
    userFormError = '';
  }

  async function onSubmitUser() {
    userFormError = '';
    const { api } = await import('$lib/api/client');

    if (editingUser) {
      const editingId = editingUser.id;
      const body: Record<string, string | number | null> = {};
      if (userForm.username) body.username = userForm.username;
      if (userForm.password) body.password = userForm.password;
      if (userForm.role) body.role = userForm.role;
      if (isAdmin) {
        const overrides = buildQuotaOverrides(userForm);
        body.instance_limit = overrides.instance_limit;
        body.max_cpu_cores = overrides.max_cpu_cores;
        body.max_ram_bytes = overrides.max_ram_bytes;
      }
      const res = await api.put<{ user: UserRow }>(`/users/${editingId}`, body);
      if (res.error) {
        userFormError = res.error;
        return;
      }
      const updated = res.data?.user;
      if (updated) {
        users = users.map(u => u.id === editingId ? updated : u);
      }
    } else {
      if (!userForm.username || !userForm.password) {
        userFormError = 'Username and password are required';
        return;
      }
      const res = await api.post<{ user: UserRow }>('/users', {
        username: userForm.username,
        password: userForm.password,
        role: userForm.role,
      });
      if (res.error) {
        userFormError = res.error;
        return;
      }
      if (res.data?.user) {
        users = [...users, res.data.user];
      }
    }
    showUserModal = false;
    editingUser = null;
    userForm = { username: '', password: '', role: 'user', ...emptyQuotaForm() };
  }

  async function onDeleteUser(user: { id: string; username: string }) {
    if (!confirm(`Delete user "${user.username}"?`)) return;
    const { api } = await import('$lib/api/client');
    const res = await api.delete(`/users/${user.id}`);
    if (!res.error) {
      users = users.filter(u => u.id !== user.id);
    }
  }
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
            <path d="M17 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2" /><circle cx="9" cy="7" r="4" /><path d="M23 21v-2a4 4 0 0 0-3-3.87" /><path d="M16 3.13a4 4 0 0 1 0 7.75" />
          </svg>
          {#if sidebarOpen}<span class="nav-text">Sessions</span>{/if}
        </button>

        <button
          class="nav-item"
          class:active={activeTab === 'users'}
          onclick={() => { navigateTab('users'); loadUsers(); }}
        >
          <svg class="nav-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2" /><circle cx="12" cy="7" r="4" />
          </svg>
          {#if sidebarOpen}<span class="nav-text">Users</span>{/if}
        </button>
      {/if}
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
        <h2 class="settings-title">Settings</h2>
        <button class="settings-close" onclick={() => showSettings = false}>&times;</button>
      </div>
      <div class="settings-section">
        <span class="settings-label">SSH ProxyCommand</span>
        <p class="settings-desc">Use this command to connect directly from your terminal.</p>
        <code class="settings-code">ssh -J gateway.openworkspace.engine:2222 instance@&lt;instance-id&gt;</code>
      </div>
      <div class="settings-section">
        <span class="settings-label">Account</span>
        <div class="settings-row">
          <span class="settings-value">Developer</span>
          <button class="settings-action" onclick={() => auth.logout()}>Sign Out</button>
        </div>
      </div>
      <div class="settings-section">
        <span class="settings-label">License</span>
        <span class="settings-value">Apache 2.0 &mdash; Open Source</span>
      </div>
      {#if isAdmin}
        <AdminSettings />
      {/if}
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

  <QuotaModal
    error={quotaNotice?.error ?? ''}
    quota={quotaNotice?.quota ?? null}
    onclose={() => quotaNotice = null}
  />

  {#if showUserModal}
    <div class="modal-overlay" onclick={() => { showUserModal = false; editingUser = null; }} role="presentation"></div>
    <div class="modal-card">
      <h3 class="modal-title">{editingUser ? 'Edit User' : 'Create User'}</h3>
      <form onsubmit={(e) => { e.preventDefault(); onSubmitUser(); }}>
        <div class="modal-field">
          <label for="user-username" class="modal-label">Username</label>
          <input id="user-username" class="modal-input" type="text" bind:value={userForm.username} disabled={!!editingUser} required />
        </div>
        <div class="modal-field">
          <label for="user-password" class="modal-label">{editingUser ? 'New Password (leave blank to keep)' : 'Password'}</label>
          <input id="user-password" class="modal-input" type="password" bind:value={userForm.password} required={!editingUser} />
        </div>
        <div class="modal-field">
          <label for="user-role" class="modal-label">Role</label>
          <select id="user-role" class="modal-select" bind:value={userForm.role} disabled={editingUser?.role === 'admin'}>
            <option value="user">User</option>
            {#if isAdmin}
              <option value="manager">Manager</option>
              <option value="admin">Admin</option>
            {/if}
          </select>
        </div>
        {#if editingUser && isAdmin}
          <div class="modal-field">
            <label for="user-instance-limit" class="modal-label">Instance Limit (empty = role default)</label>
            <input id="user-instance-limit" class="modal-input" type="number" min="0" step="1" bind:value={userForm.instance_limit} />
          </div>
          <div class="modal-field">
            <label for="user-max-cpu" class="modal-label">Max CPU Cores (empty = role default)</label>
            <input id="user-max-cpu" class="modal-input" type="number" min="0" step="1" bind:value={userForm.max_cpu_cores} />
          </div>
          <div class="modal-field">
            <label for="user-max-ram" class="modal-label">Max RAM Bytes (empty = role default)</label>
            <input id="user-max-ram" class="modal-input" type="number" min="0" step="1" bind:value={userForm.max_ram_bytes} />
          </div>
        {/if}
        {#if userFormError}
          <div class="error-badge">{userFormError}</div>
        {/if}
        <div class="modal-actions">
          <button type="button" class="modal-cancel" onclick={() => { showUserModal = false; editingUser = null; }}>Cancel</button>
          <button type="submit" class="modal-confirm">{editingUser ? 'Save' : 'Create'}</button>
        </div>
      </form>
    </div>
  {/if}

  <main class="main-content">
    {#if loading}
      <p class="loading-text">Loading instances...</p>

    {:else if activeTab === 'instances'}
      <section class="ws-section">
        <h2 class="section-title">Instances</h2>
        {#if myInstances.length === 0}
          <p class="empty-text">No instances yet. Launch a template to get started.</p>
        {:else}
          <div class="instance-grid">
            {#each myInstances as inst}
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
                  {#if canControlInstance(inst)}
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
          {#each configs as config}
            <button class="template-card" onclick={() => openLaunch(config)}>
              <span class="template-icon">{getTemplateIcon(config.name)}</span>
              <span class="template-name">{config.name}</span>
            </button>
          {/each}
        </div>
      </section>

    {:else if activeTab === 'sessions' && canManage}
      <section class="ws-section">
        <h2 class="section-title">All Instances</h2>

        <div class="filter-bar">
          <div class="filter-group">
            <label class="filter-label" for="filter-user">User</label>
            <select id="filter-user" class="filter-select" bind:value={filterUser}>
              <option value="">All Users</option>
              {#each uniqueUsers as user}
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
          {#if filterUser || filterStatus}
            <button class="filter-clear" onclick={() => { filterUser = ''; filterStatus = ''; }}>Clear filters</button>
          {/if}
          <span class="filter-count">{filteredInstances.length} instance{filteredInstances.length !== 1 ? 's' : ''}</span>
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
                {#each filteredInstances as inst}
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
                      {#if canControlInstance(inst)}
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

    {:else if activeTab === 'users' && canManage}
      <section class="ws-section">
        <div class="section-header">
          <h2 class="section-title">User Management</h2>
          <button class="btn-create" onclick={() => showUserModal = true}>+ New User</button>
        </div>

        {#if usersLoading}
          <p class="empty-text">Loading users...</p>
        {:else if users.length === 0}
          <p class="empty-text">No users found.</p>
        {:else}
          <div class="instances-table-wrap">
            <table class="instances-table">
              <thead>
                <tr>
                  <th>Username</th>
                  <th>Role</th>
                  <th>Quota</th>
                  <th>Created</th>
                  <th>Actions</th>
                </tr>
              </thead>
              <tbody>
                {#each users as user}
                  <tr>
                    <td class="td-name">
                      <span class="td-name-text">{user.username}</span>
                      <span class="td-id">{user.id.slice(0, 8)}</span>
                    </td>
                    <td>
                      <span class="status-badge {user.role === 'admin' ? 'dot-active' : ''}">{user.role}</span>
                    </td>
                    <td class="td-quota">
                      {#if user.quota_exempt}
                        <span class="status-badge dot-active">Exempt</span>
                      {:else}
                        <span class="td-quota-text">
                          {user.effective_instance_limit} inst · {user.effective_max_cpu_cores} cores · {formatMemory(user.effective_max_ram_bytes)}
                        </span>
                      {/if}
                    </td>
                    <td class="td-date">{new Date(user.created_at).toLocaleDateString()}</td>
                    <td class="td-actions">
                      <div class="action-buttons">
                        {#if canEditUser(user)}
                          <button class="launch-btn pause sm" onclick={() => openEditUser(user)}>Edit</button>
                        {/if}
                        {#if canDeleteUser(user)}
                          <button class="launch-btn remove sm" onclick={() => onDeleteUser(user)}>Delete</button>
                        {/if}
                      </div>
                    </td>
                  </tr>
                {/each}
              </tbody>
            </table>
          </div>
        {/if}
      </section>

    {:else if activeTab === 'templates'}
      <TemplatePanel
        {view}
        bind:configs
        bind:dirty={panelDirty}
        onnavigate={navigateToHash}
        ondelete={onDeleteConfig}
      />
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
    gap: 8px;
    margin-top: 1.5rem;
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

  .settings-code {
    font-family: 'JetBrains Mono', monospace;
    font-size: 0.75rem;
    color: #a1a1aa;
    padding: 0.5rem;
    background: rgba(0, 0, 0, 0.4);
    border-radius: 4px;
    display: block;
  }

  .settings-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .settings-value { font-size: 0.85rem; color: #d4d4d8; }

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

  .section-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 1rem;
  }

  .section-header .section-title { margin: 0; }

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

  /* Filter Bar */
  .filter-bar {
    display: flex;
    align-items: flex-end;
    gap: 1rem;
    margin-bottom: 1.5rem;
    padding: 0.75rem 1rem;
    background: rgba(0, 0, 0, 0.25);
    border: 1px solid rgba(255, 255, 255, 0.06);
    border-radius: 10px;
    flex-wrap: wrap;
  }

  .filter-group {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .filter-label {
    font-size: 0.6rem;
    font-weight: 600;
    color: #71717a;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .filter-select {
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

  .filter-select:focus { border-color: #818cf8; }

  .filter-clear {
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

  .filter-clear:hover { background: rgba(255, 255, 255, 0.1); color: #fff; }

  .filter-count {
    font-size: 0.75rem;
    color: #52525b;
    margin-left: auto;
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

  /* Instances Table */
  .instances-table-wrap {
    overflow-x: auto;
    border: 1px solid rgba(255, 255, 255, 0.06);
    border-radius: 10px;
  }

  .instances-table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.8rem;
  }

  .instances-table thead {
    background: rgba(0, 0, 0, 0.3);
  }

  .instances-table th {
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

  .instances-table td {
    padding: 0.6rem 0.75rem;
    border-bottom: 1px solid rgba(255, 255, 255, 0.04);
    color: #d4d4d8;
    vertical-align: middle;
  }

  .instances-table tbody tr:hover {
    background: rgba(255, 255, 255, 0.02);
  }

  .instances-table tbody tr:last-child td {
    border-bottom: none;
  }

  .td-name {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .td-name-text { font-weight: 600; color: #f4f4f5; }

  .td-id {
    font-family: monospace;
    font-size: 0.65rem;
    color: #52525b;
  }

  .td-owner { color: #a1a1aa; }

  .td-date {
    font-size: 0.75rem;
    color: #71717a;
    white-space: nowrap;
  }

  .td-actions { white-space: nowrap; }

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
