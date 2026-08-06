<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api/client';
  import { listGroups } from '$lib/api/rbac-actions';
  import { auth } from '$lib/stores/auth';
  import { mayManageUsers, mayManageUser, userTier, assignableGroups } from '$lib/permissions';
  import {
    createInitialUserPolicyForm,
    userPolicyFormFromRow,
    submitUserPolicy,
    type UserPolicyFormState
  } from '$lib/users/user-policy-form';
  import type { EffectiveContext, Group } from '$lib/types';

  let {
    ctx = null
  }: {
    ctx?: EffectiveContext | null;
  } = $props();

  type UserRow = {
    id: string;
    username: string;
    created_at: string;
    group_ids?: string[];
    direct_max_instances?: number | null;
    tier?: number;
    is_admin?: boolean;
  };

  let users = $state<UserRow[]>([]);
  let groups = $state<Group[]>([]);
  let loading = $state(false);
  let loadError = $state('');
  let showCreate = $state(false);
  let createForm = $state<{ username: string; password: string; group_ids: string[] }>({
    username: '',
    password: '',
    group_ids: []
  });
  let createError = $state('');
  let showPolicy = $state(false);
  let policyTarget = $state<UserRow | null>(null);
  let policyForm = $state<UserPolicyFormState>(createInitialUserPolicyForm());
  let policyError = $state('');
  let search = $state('');
  let groupFilter = $state('all');
  let ceilingFilter = $state('all');

  const canManage = $derived(mayManageUsers(ctx));

  const assignable = $derived(assignableGroups(ctx, groups));
  const userGroup = $derived(assignable.find((g) => g.kind === 'user'));

  const hasFilters = $derived(
    search.trim() !== '' || groupFilter !== 'all' || ceilingFilter !== 'all'
  );

  const filteredUsers = $derived(
    users.filter((u) => {
      const q = search.trim().toLowerCase();
      const matchQ =
        !q || u.username.toLowerCase().includes(q) || u.id.slice(0, 8).toLowerCase().includes(q);
      const matchG = groupFilter === 'all' || (u.group_ids ?? []).includes(groupFilter);
      const matchC =
        ceilingFilter === 'all' ||
        (ceilingFilter === 'inherit'
          ? u.direct_max_instances == null
          : u.direct_max_instances != null);
      return matchQ && matchG && matchC;
    })
  );

  async function load() {
    loading = true;
    loadError = '';
    const res = await api.get<{ users: UserRow[] }>('/users');
    if (res.data?.users) {
      users = res.data.users;
    } else if (res.error) {
      loadError = res.error;
    }
    if (canManage) {
      const groupsRes = await listGroups();
      if (groupsRes.groups) groups = groupsRes.groups;
    }
    loading = false;
  }

  onMount(load);

  function groupName(id: string): string {
    return groups.find((g) => g.id === id)?.name ?? id;
  }

  function clearFilters() {
    search = '';
    groupFilter = 'all';
    ceilingFilter = 'all';
  }

  function toggleGroup(id: string) {
    policyForm.group_ids = policyForm.group_ids.includes(id)
      ? policyForm.group_ids.filter((g) => g !== id)
      : [...policyForm.group_ids, id];
  }

  function toggleCreateGroup(id: string) {
    createForm.group_ids = createForm.group_ids.includes(id)
      ? createForm.group_ids.filter((g) => g !== id)
      : [...createForm.group_ids, id];
  }

  function openCreate() {
    createForm = {
      username: '',
      password: '',
      group_ids: userGroup ? [userGroup.id] : []
    };
    createError = '';
    showCreate = true;
  }

  async function onCreate() {
    createError = '';
    if (!createForm.username || !createForm.password) {
      createError = 'Username and password are required';
      return;
    }
    const assigned: string[] = [];
    if (userGroup) assigned.push(userGroup.id);
    for (const id of createForm.group_ids) if (!assigned.includes(id)) assigned.push(id);
    const res = await api.post<{ user: UserRow }>('/users', {
      username: createForm.username,
      password: createForm.password,
      group_ids: [...assigned]
    });
    if (res.error) {
      createError = res.error;
      return;
    }
    if (res.data?.user) {
      users = [...users, res.data.user];
    }
    showCreate = false;
    await auth.check();
  }

  function openPolicy(user: UserRow) {
    policyTarget = user;
    policyForm = userPolicyFormFromRow(user);
    policyError = '';
    showPolicy = true;
  }

  async function onSavePolicy() {
    const target = policyTarget;
    if (!target) return;
    policyError = '';
    policyForm.loading = true;
    const result = await submitUserPolicy(target.id, policyForm, {
      omitGroupIds: !!target.is_admin
    });
    policyForm.loading = false;
    if (result.error) {
      policyError = result.error;
      return;
    }
    showPolicy = false;
    policyTarget = null;
    await load();
    await auth.check();
  }

  async function onDelete(user: UserRow) {
    if (!confirm(`Delete user "${user.username}"?`)) return;
    const res = await api.delete(`/users/${user.id}`);
    if (!res.error) {
      users = users.filter((u) => u.id !== user.id);
      await auth.check();
    }
  }
</script>

<section class="ws-section panel-card">
  <div class="panel-head">
    <div>
      <h2 class="panel-head-title">User Management</h2>
      <p class="panel-head-desc">Accounts, group memberships, and personal overrides.</p>
    </div>
    {#if canManage}
      <button class="btn-create" onclick={openCreate}>+ New User</button>
    {/if}
  </div>

  {#if loading}
    <p class="empty-text">Loading users...</p>
  {:else if loadError}
    <p class="empty-text">{loadError}</p>
  {:else if users.length === 0}
    <p class="empty-text">No users found.</p>
  {:else}
    <div class="panel-toolbar">
      <div class="panel-search-wrap">
        <svg class="panel-search-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <circle cx="11" cy="11" r="7"></circle>
          <line x1="21" y1="21" x2="16.65" y2="16.65"></line>
        </svg>
        <input class="panel-search" type="text" placeholder="Search users..." bind:value={search} />
      </div>
      <select class="panel-select" aria-label="Filter by group" bind:value={groupFilter}>
        <option value="all">All groups</option>
        {#each groups as group (group.id)}
          <option value={group.id}>{group.name}</option>
        {/each}
      </select>
      <select class="panel-select" aria-label="Filter by ceiling" bind:value={ceilingFilter}>
        <option value="all">Any ceiling</option>
        <option value="inherit">Inherit</option>
        <option value="limited">Limited</option>
      </select>
      <span class="panel-count">{filteredUsers.length} of {users.length}</span>
      {#if hasFilters}
        <button class="panel-clear" onclick={clearFilters}>Clear</button>
      {/if}
    </div>
    {#if filteredUsers.length === 0}
      <p class="empty-text">No users match your filters.</p>
    {:else}
      <div class="instances-table-wrap">
        <table class="instances-table">
          <thead>
            <tr>
              <th>Username</th>
              <th>Memberships</th>
              <th>Personal Ceiling</th>
              <th>Created</th>
              {#if canManage}
                <th>Actions</th>
              {/if}
            </tr>
          </thead>
          <tbody>
            {#each filteredUsers as user (user.id)}
              <tr>
                <td class="td-name">
                  <span class="td-name-text">{user.username}</span>
                  <span class="td-id">
                    {#if user.is_admin}
                      <span class="tier-badge tier-admin">Admin</span>
                    {:else if userTier(user) === 1}
                      <span class="tier-badge tier-manager">Manager</span>
                    {:else}
                      <span class="tier-badge tier-user">User</span>
                    {/if}
                  </span>
                </td>
                <td class="td-memberships">
                  {#if (user.group_ids ?? []).length === 0}
                    <span class="no-memberships">None</span>
                  {:else}
                    {#each user.group_ids ?? [] as gid (gid)}
                      <span class="member-badge">{groupName(gid)}</span>
                    {/each}
                  {/if}
                </td>
                <td>
                  {#if user.direct_max_instances == null}
                    <span class="ceiling-inherit">Inherit</span>
                  {:else}
                    <span class="ceiling-set">{user.direct_max_instances}{user.direct_max_instances === 0 ? ' (unlimited)' : ''}</span>
                  {/if}
                </td>
                <td class="td-date">{new Date(user.created_at).toLocaleDateString()}</td>
                {#if canManage && mayManageUser(ctx, { user_id: user.id, tier: user.tier })}
                  <td class="td-actions">
                    <div class="action-buttons">
                      <button class="launch-btn edit" onclick={() => openPolicy(user)}>Edit</button>
                      {#if !user.is_admin}
                        <button class="launch-btn remove" onclick={() => onDelete(user)}>Delete</button>
                      {/if}
                    </div>
                  </td>
                {/if}
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {/if}
  {/if}
</section>

{#if canManage && showCreate}
  <div class="modal-overlay" onclick={() => showCreate = false} role="presentation"></div>
  <div class="modal-card">
    <h3 class="modal-title">Create User</h3>
    <form onsubmit={(e) => { e.preventDefault(); onCreate(); }}>
      <div class="modal-field">
        <label for="user-username" class="modal-label">Username</label>
        <input id="user-username" class="modal-input" type="text" bind:value={createForm.username} required />
      </div>
      <div class="modal-field">
        <label for="user-password" class="modal-label">Password</label>
        <input id="user-password" class="modal-input" type="password" bind:value={createForm.password} required />
      </div>
      <div class="modal-field" data-testid="create-user-groups">
        <span class="modal-label">Groups</span>
        {#if assignable.length === 0}
          <p class="empty-text">No assignable groups.</p>
        {:else}
          {#each assignable as group (group.id)}
            <label class="policy-toggle-row">
              <input
                type="checkbox"
                data-testid="create-user-group-{group.id}"
                checked={group.kind === 'user' || createForm.group_ids.includes(group.id)}
                disabled={group.kind === 'user'}
                onchange={() => toggleCreateGroup(group.id)}
              />
              <span class="policy-toggle-label">{group.name}</span>
            </label>
          {/each}
        {/if}
      </div>
      {#if createError}
        <div class="error-badge">{createError}</div>
      {/if}
      <div class="modal-actions">
        <button type="button" class="modal-cancel" onclick={() => showCreate = false}>Cancel</button>
        <button type="submit" class="modal-confirm">Create</button>
      </div>
    </form>
  </div>
{/if}

{#if canManage && showPolicy && policyTarget}
  <div class="modal-overlay" onclick={() => { showPolicy = false; policyTarget = null; }} role="presentation"></div>
  <div class="modal-card">
    <h3 class="modal-title">Edit Policy — {policyTarget.username}</h3>
    <form onsubmit={(e) => { e.preventDefault(); onSavePolicy(); }}>
      <div class="modal-field" data-testid="user-policy-groups">
        <span class="modal-label">Group Memberships</span>
        {#if policyTarget.is_admin}
          <p class="empty-text">Admin membership is protected — memberships cannot be changed here.</p>
        {:else if assignable.length === 0}
          <p class="empty-text">No assignable groups.</p>
        {:else}
          {#each assignable as group (group.id)}
            <label class="policy-toggle-row">
              <input
                type="checkbox"
                data-testid="user-policy-group-{group.id}"
                checked={policyForm.group_ids.includes(group.id)}
                onchange={() => toggleGroup(group.id)}
              />
              <span class="policy-toggle-label">{group.name}</span>
            </label>
          {/each}
        {/if}
      </div>
      <div class="modal-field">
        <label for="user-policy-ceiling" class="modal-label">Personal Max Instances (blank = inherit)</label>
        <input
          id="user-policy-ceiling"
          class="modal-input"
          type="text"
          inputmode="numeric"
          placeholder="inherit"
          bind:value={policyForm.direct_max_instances}
        />
      </div>
      {#if policyError}
        <div class="error-badge">{policyError}</div>
      {/if}
      <div class="modal-actions">
        <button type="button" class="modal-cancel" onclick={() => { showPolicy = false; policyTarget = null; }}>Cancel</button>
        <button type="submit" class="modal-confirm" disabled={policyForm.loading}>Save Policy</button>
      </div>
    </form>
  </div>
{/if}

<style>
  .td-memberships { min-width: 0; }

  .tier-badge {
    display: inline-flex;
    align-items: center;
    margin-right: 6px;
    font-size: 0.6rem;
    font-weight: 700;
    padding: 0.1rem 0.45rem;
    border-radius: 999px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .tier-admin {
    color: #fca5a5;
    background: rgba(239, 68, 68, 0.12);
    border: 1px solid rgba(248, 113, 113, 0.3);
  }

  .tier-manager {
    color: #a5b4fc;
    background: rgba(99, 102, 241, 0.12);
    border: 1px solid rgba(129, 140, 248, 0.25);
  }

  .tier-user {
    color: #71717a;
    background: rgba(255, 255, 255, 0.04);
    border: 1px solid rgba(255, 255, 255, 0.08);
  }

  .member-badge {
    display: inline-flex;
    align-items: center;
    margin: 2px 4px 2px 0;
    font-size: 0.65rem;
    font-weight: 600;
    padding: 0.15rem 0.5rem;
    border-radius: 999px;
    color: #a5b4fc;
    background: rgba(99, 102, 241, 0.12);
    border: 1px solid rgba(129, 140, 248, 0.25);
  }

  .no-memberships {
    font-size: 0.75rem;
    color: #71717a;
    font-style: italic;
  }

  .ceiling-inherit {
    font-size: 0.75rem;
    color: #71717a;
    font-style: italic;
  }

  .ceiling-set {
    font-size: 0.8rem;
    font-weight: 600;
    color: #f4f4f5;
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

  .modal-field {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-bottom: 1rem;
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

  .policy-toggle-row {
    display: flex;
    align-items: center;
    gap: 8px;
    cursor: pointer;
  }

  .policy-toggle-label {
    font-size: 0.8rem;
    color: #d4d4d8;
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

  .modal-confirm:hover {
    background: #4f46e5;
  }

  .modal-confirm:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }
</style>
