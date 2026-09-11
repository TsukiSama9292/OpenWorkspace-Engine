<script lang="ts">
  import { onMount } from 'svelte';
  import { listGroups, deleteGroup, updateMemberQuota } from '$lib/api/rbac-actions';
  import { auth } from '$lib/stores/auth';
  import { mayEditMemberQuota } from '$lib/permissions';
  import {
    GROUP_FLAGS,
    createInitialGroupForm,
    groupFormFromGroup,
    submitGroup,
    submitGroupUpdate,
    isSystemGroup,
    describeGroupPool,
    type GroupFlag,
    type GroupFormState
  } from '$lib/groups/group-form';
  import TriStateInput from '$lib/components/forms/TriStateInput.svelte';
  import {
    memoryMbFromTriState,
    memoryMbToTriState,
    triStateFromValue,
    valueFromTriState,
    type TriState
  } from '$lib/tri-state';
  import type { EffectiveContext, Group, GroupMember, Template } from '$lib/types';

  let {
    ctx = null,
    templates = []
  }: {
    ctx?: EffectiveContext | null;
    templates?: Template[];
  } = $props();

  let groups = $state<Group[]>([]);
  let loading = $state(false);
  let loadError = $state('');
  let showModal = $state(false);
  let editing = $state<Group | null>(null);
  let form = $state<GroupFormState>(createInitialGroupForm());
  let search = $state('');
  let flagFilter = $state<'all' | GroupFlag>('all');
  let membersOpen = $state<string[]>([]);
  let showQuotaModal = $state(false);
  let quotaGroup = $state<Group | null>(null);
  let quotaMember = $state<GroupMember | null>(null);
  let quotaForm = $state<{ cpu: TriState; memory: TriState; gpu: TriState }>({
    cpu: { mode: 'unlimited', value: 1 },
    memory: { mode: 'unlimited', value: 1 },
    gpu: { mode: 'unlimited', value: 1 }
  });
  let quotaError = $state('');
  let quotaSaving = $state(false);

  const canManageUsers = $derived(ctx?.can_manage_users === true || ctx?.is_admin === true);
  const isAdmin = $derived(ctx?.is_admin === true);

  const editingSystemKind = $derived(editing?.kind ?? null);
  const flagsLocked = $derived(editingSystemKind !== null && editingSystemKind !== 'manager');
  const nameLocked = $derived(editingSystemKind !== null);

  const KIND_LABELS: Record<string, string> = {
    admin: 'Admin',
    manager: 'Manager',
    user: 'User'
  };

  const hasFilters = $derived(search.trim() !== '' || flagFilter !== 'all');

  const filteredGroups = $derived(
    groups.filter((g) => {
      const q = search.trim().toLowerCase();
      const matchQ =
        !q || g.name.toLowerCase().includes(q) || (g.description ?? '').toLowerCase().includes(q);
      const matchFlag = flagFilter === 'all' || g[flagFilter] === true;
      return matchQ && matchFlag;
    })
  );

  const FLAG_LABELS: Record<GroupFlag, string> = {
    can_create_template: 'Create templates',
    can_manage_users: 'Manage users',
    can_manage_group_instances: 'Manage group instances',
    can_manage_docker: 'Manage Docker',
    can_manage_registry: 'Manage registry',
    can_view_monitoring: 'View monitoring',
    can_view_audit_logs: 'View audit logs'
  };

  function describeMaxInstances(v: number | null | undefined): string {
    if (v == null || v < 0) return 'Unlimited';
    if (v === 0) return 'Disabled (0)';
    return String(v);
  }

  function describeQuota(v: number): string {
    if (v < 0) return 'unlimited';
    if (v === 0) return 'blocked';
    return String(v);
  }

  function describeMemQuota(mb: number): string {
    if (mb < 0) return 'unlimited';
    if (mb === 0) return 'blocked';
    return `${Math.round(mb / 1024)} GB`;
  }

  async function load() {
    if (!canManageUsers) return;
    loading = true;
    loadError = '';
    const res = await listGroups();
    if (res.error) {
      loadError = res.error;
    } else if (res.groups) {
      groups = res.groups;
    }
    loading = false;
  }

  onMount(() => {
    if (canManageUsers) load();
  });

  function openCreate() {
    if (!isAdmin) return;
    editing = null;
    form = createInitialGroupForm();
    showModal = true;
  }

  function openEdit(group: Group) {
    if (!isAdmin) return;
    editing = group;
    form = groupFormFromGroup(group);
    showModal = true;
  }

  function closeModal() {
    showModal = false;
    editing = null;
  }

  function toggleTemplate(id: string) {
    form.template_ids = form.template_ids.includes(id)
      ? form.template_ids.filter((t) => t !== id)
      : [...form.template_ids, id];
  }

  async function onSave() {
    form.loading = true;
    form.error = '';
    const result = editing
      ? await submitGroupUpdate(editing.id, form)
      : await submitGroup(form);
    form.loading = false;
    if (result.error) {
      form.error = result.error;
      return;
    }
    closeModal();
    await load();
    // Group flags/whitelist feed the effective context; re-fetch it so the
    // launch gating on the Instances page reflects the change immediately.
    await auth.check();
  }

  async function onDelete(group: Group) {
    if (!confirm(`Delete group "${group.name}"? Its memberships are removed.`)) return;
    const res = await deleteGroup(group.id);
    if (!res.error) {
      groups = groups.filter((g) => g.id !== group.id);
      await auth.check();
    }
  }

  function toggleMembers(groupId: string) {
    membersOpen = membersOpen.includes(groupId)
      ? membersOpen.filter((id) => id !== groupId)
      : [...membersOpen, groupId];
  }

  function openQuotaEditor(group: Group, member: GroupMember) {
    if (!mayEditMemberQuota(ctx, group, member)) return;
    quotaGroup = group;
    quotaMember = member;
    quotaForm = {
      cpu: triStateFromValue(member.cpu_quota),
      memory: memoryMbToTriState(member.memory_quota),
      gpu: triStateFromValue(member.gpu_quota)
    };
    quotaError = '';
    quotaSaving = false;
    showQuotaModal = true;
  }

  function closeQuotaModal() {
    showQuotaModal = false;
    quotaGroup = null;
    quotaMember = null;
  }

  async function onSaveQuota() {
    if (!quotaGroup || !quotaMember) return;
    quotaSaving = true;
    quotaError = '';
    const res = await updateMemberQuota(quotaGroup.id, quotaMember.user_id, {
      cpu_quota: valueFromTriState(quotaForm.cpu),
      memory_quota: memoryMbFromTriState(quotaForm.memory),
      gpu_quota: valueFromTriState(quotaForm.gpu)
    });
    quotaSaving = false;
    if (res.error) {
      quotaError = res.error;
      return;
    }
    closeQuotaModal();
    await load();
  }

  async function onResetAllQuotas(group: Group) {
    if (!isAdmin) return;
    const members = group.members ?? [];
    if (members.length === 0) return;
    if (!confirm(`Reset all ${members.length} member quotas in "${group.name}" to 0 (blocked)?`)) return;
    for (const member of members) {
      const res = await updateMemberQuota(group.id, member.user_id, {
        cpu_quota: 0,
        memory_quota: 0,
        gpu_quota: 0
      });
      if (res.error) {
        loadError = res.error;
        return;
      }
    }
    await load();
  }
</script>

{#if canManageUsers}
  <section class="ws-section panel-card">
    <div class="panel-head">
      <div>
        <h2 class="panel-head-title">Group Management</h2>
        <p class="panel-head-desc">Permission groups control what their members can do.</p>
      </div>
      {#if isAdmin}
        <button class="btn-create" onclick={openCreate}>+ New Group</button>
      {/if}
    </div>

    {#if loading}
      <p class="empty-text">Loading groups...</p>
    {:else if loadError}
      <p class="empty-text">{loadError}</p>
    {:else if groups.length === 0}
      <p class="empty-text">No groups yet.</p>
    {:else}
      <div class="panel-toolbar">
        <div class="panel-search-wrap">
          <svg class="panel-search-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <circle cx="11" cy="11" r="7"></circle>
            <line x1="21" y1="21" x2="16.65" y2="16.65"></line>
          </svg>
          <input class="panel-search" type="text" placeholder="Search groups..." bind:value={search} />
        </div>
        <select class="panel-select" aria-label="Filter by permission" bind:value={flagFilter}>
          <option value="all">All permissions</option>
          {#each GROUP_FLAGS as flag (flag)}
            <option value={flag}>{FLAG_LABELS[flag]}</option>
          {/each}
        </select>
        <span class="panel-count">{filteredGroups.length} of {groups.length}</span>
        {#if hasFilters}
          <button
            class="panel-clear"
            onclick={() => {
              search = '';
              flagFilter = 'all';
            }}
          >
            Clear
          </button>
        {/if}
      </div>
      {#if filteredGroups.length === 0}
        <p class="empty-text">No groups match your filters.</p>
      {:else}
        <div class="instances-table-wrap">
          <table class="instances-table">
            <thead>
              <tr>
                <th>Name</th>
                <th>Permissions</th>
                <th>Max Instances</th>
                <th>Resource Pool</th>
                <th>Members</th>
                <th>Templates</th>
                <th>Actions</th>
              </tr>
            </thead>
            <tbody>
              {#each filteredGroups as group (group.id)}
                <tr>
                  <td class="td-name">
                    <span class="td-name-text">{group.name}</span>
                    {#if group.kind}
                      <span class="system-badge">System · {KIND_LABELS[group.kind] ?? group.kind}</span>
                    {/if}
                    {#if group.description}
                      <span class="td-id">{group.description}</span>
                    {/if}
                  </td>
                  <td class="td-groups">
                    {#each GROUP_FLAGS as flag (flag)}
                      <span class="group-flag-badge" class:on={group[flag]}>{FLAG_LABELS[flag]}</span>
                    {/each}
                  </td>
                  <td>{describeMaxInstances(group.max_instances)}</td>
                  <td class="td-id">{describeGroupPool(group)}</td>
                  <td class="td-groups">
                    {#if (group.members?.length ?? 0) === 0}
                      <span class="td-id">None</span>
                    {:else}
                      <button class="link-btn" onclick={() => toggleMembers(group.id)}>
                        {(group.members?.length ?? 0)} member{(group.members?.length ?? 0) !== 1 ? 's' : ''}
                      </button>
                    {/if}
                  </td>
                  <td class="td-groups">
                    {#if group.template_ids.length === 0}
                      <span class="td-id">None</span>
                    {:else}
                      <span class="td-owner">{group.template_ids.length} template{group.template_ids.length !== 1 ? 's' : ''}</span>
                    {/if}
                  </td>
                  <td class="td-actions">
                    <div class="action-buttons">
                      {#if isAdmin}
                        <button class="launch-btn edit" onclick={() => openEdit(group)}>Edit</button>
                        {#if !isSystemGroup(group)}
                          <button class="launch-btn remove" onclick={() => onDelete(group)}>Delete</button>
                        {/if}
                      {/if}
                    </div>
                  </td>
                </tr>
                {#if membersOpen.includes(group.id)}
                  <tr class="member-expand-row">
                    <td colspan="7">
                      <div class="member-panel">
                        {#if (group.members?.length ?? 0) === 0}
                          <p class="empty-text">No members in this group.</p>
                        {:else}
                          {#each group.members ?? [] as member (member.user_id)}
                            <div class="member-row">
                              <span class="member-name">{member.username}</span>
                              <span class="member-quota">
                                CPU {describeQuota(member.cpu_quota)} · Mem {describeMemQuota(member.memory_quota)} · GPU {describeQuota(member.gpu_quota)}
                              </span>
                              {#if mayEditMemberQuota(ctx, group, member)}
                                <button class="launch-btn edit" onclick={() => openQuotaEditor(group, member)}>Edit quotas</button>
                              {/if}
                            </div>
                          {/each}
                          {#if isAdmin}
                            <div class="member-reset-row">
                              <button class="launch-btn remove" onclick={() => onResetAllQuotas(group)}>Reset all quotas to 0</button>
                            </div>
                          {/if}
                        {/if}
                      </div>
                    </td>
                  </tr>
                {/if}
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
    {/if}
  </section>

  {#if showModal}
    <div class="modal-overlay" onclick={closeModal} role="presentation"></div>
    <div class="modal-card">
      <h3 class="modal-title">{editing ? 'Edit Group' : 'New Group'}</h3>
      <form onsubmit={(e) => { e.preventDefault(); onSave(); }}>
        <div class="modal-field">
          <label for="group-name" class="modal-label">Name</label>
          <input id="group-name" class="modal-input" type="text" bind:value={form.name} required disabled={nameLocked} />
          {#if nameLocked}
            <p class="modal-hint">System group names are fixed.</p>
          {/if}
        </div>
        <div class="modal-field">
          <label for="group-description" class="modal-label">Description</label>
          <input id="group-description" class="modal-input" type="text" bind:value={form.description} />
        </div>
        <div class="modal-field">
          <span class="modal-label">Permissions</span>
          {#if flagsLocked}
            <p class="modal-hint">These permissions are fixed for the system group.</p>
          {/if}
          {#each GROUP_FLAGS as flag (flag)}
            <label class="group-toggle-row">
              <input type="checkbox" data-testid="group-flag-{flag}" bind:checked={form[flag]} disabled={flagsLocked} />
              <span class="group-toggle-label">{FLAG_LABELS[flag]}</span>
            </label>
          {/each}
        </div>
        <div class="modal-field">
          <TriStateInput label="Max Instances" bind:value={form.max_instances} unit="instances" placeholder="e.g. 5" />
          <p class="modal-hint">Unlimited (-1) means no instance ceiling; Disabled (0) blocks new launches; Custom sets an exact cap.</p>
        </div>
        <div class="modal-field">
          <span class="modal-label">Resource Pool</span>
          <label class="group-toggle-row">
            <span class="group-toggle-label">Billing model</span>
            <select
              class="modal-input"
              aria-label="Billing model"
              bind:value={form.billing_model}
            >
              <option value="shared">shared — every member bills the same pool</option>
              <option value="dedicated">dedicated — each member sees their own quota</option>
            </select>
          </label>
          <div class="pool-grid">
            <TriStateInput label="Pool CPU (cores)" bind:value={form.poolCpu} unit="cores" placeholder="e.g. 8" />
            <TriStateInput label="Pool Memory (GB)" bind:value={form.poolMemory} unit="GB" placeholder="e.g. 16" />
            <TriStateInput label="Pool GPU" bind:value={form.poolGpu} unit="GPUs" placeholder="e.g. 2" />
          </div>
          <p class="modal-hint">Unlimited (-1) means no pool cap; Disabled (0) means members cannot bill this resource; Custom sets an exact cap.</p>
        </div>
        <div class="modal-field">
          <span class="modal-label">Template Whitelist</span>
          {#if templates.length === 0}
            <p class="empty-text">No templates available.</p>
          {:else}
            {#each templates as tpl (tpl.id)}
              <label class="group-toggle-row">
                <input
                  type="checkbox"
                  data-testid="group-template-{tpl.id}"
                  checked={form.template_ids.includes(tpl.id)}
                  onchange={() => toggleTemplate(tpl.id)}
                />
                <span class="group-toggle-label">{tpl.name}</span>
              </label>
            {/each}
          {/if}
        </div>
        {#if form.error}
          <div class="error-badge">{form.error}</div>
        {/if}
        <div class="modal-actions">
          <button type="button" class="modal-cancel" onclick={closeModal}>Cancel</button>
          <button type="submit" class="modal-confirm" disabled={form.loading}>
            {editing ? 'Save Changes' : 'Create Group'}
          </button>
        </div>
      </form>
    </div>
  {/if}

  {#if showQuotaModal && quotaGroup && quotaMember}
    <div class="modal-overlay" onclick={closeQuotaModal} role="presentation"></div>
    <div class="modal-card">
      <h3 class="modal-title">Edit quotas — {quotaMember.username}</h3>
      <p class="modal-hint">Group "{quotaGroup.name}". -1 = unlimited (bounded by the pool), 0 = blocked, custom = exact cap.</p>
      <form onsubmit={(e) => { e.preventDefault(); onSaveQuota(); }}>
        <div class="modal-field">
          <div class="pool-grid">
            <TriStateInput label="CPU Quota (cores)" bind:value={quotaForm.cpu} unit="cores" placeholder="e.g. 4" />
            <TriStateInput label="Memory Quota (GB)" bind:value={quotaForm.memory} unit="GB" placeholder="e.g. 8" />
            <TriStateInput label="GPU Quota" bind:value={quotaForm.gpu} unit="GPUs" placeholder="e.g. 1" />
          </div>
        </div>
        {#if quotaError}
          <div class="error-badge">{quotaError}</div>
        {/if}
        <div class="modal-actions">
          <button type="button" class="modal-cancel" onclick={closeQuotaModal}>Cancel</button>
          <button type="submit" class="modal-confirm" disabled={quotaSaving}>Save Quotas</button>
        </div>
      </form>
    </div>
  {/if}
{/if}

<style>
  .td-groups { min-width: 0; }

  .system-badge {
    display: inline-flex;
    align-items: center;
    margin: 2px 6px 2px 0;
    font-size: 0.6rem;
    font-weight: 700;
    padding: 0.1rem 0.45rem;
    border-radius: 999px;
    color: #fde68a;
    background: rgba(245, 158, 11, 0.12);
    border: 1px solid rgba(252, 211, 77, 0.3);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .modal-hint {
    font-size: 0.72rem;
    color: #71717a;
    font-style: italic;
    margin: 0;
  }

  .group-flag-badge {
    display: inline-flex;
    align-items: center;
    margin: 2px 4px 2px 0;
    font-size: 0.62rem;
    font-weight: 600;
    padding: 0.15rem 0.45rem;
    border-radius: 999px;
    color: #71717a;
    background: rgba(255, 255, 255, 0.04);
    border: 1px solid rgba(255, 255, 255, 0.08);
  }

  .group-flag-badge.on {
    color: #a5b4fc;
    background: rgba(99, 102, 241, 0.12);
    border: 1px solid rgba(129, 140, 248, 0.25);
  }

  .link-btn {
    background: none;
    border: none;
    padding: 0;
    color: #818cf8;
    font-size: 0.8rem;
    cursor: pointer;
    font-family: inherit;
  }

  .link-btn:hover {
    text-decoration: underline;
  }

  .member-expand-row td {
    background: rgba(255, 255, 255, 0.02);
    padding: 0.75rem 1rem;
  }

  .member-panel {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .member-row {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 0.4rem 0.6rem;
    border-radius: 8px;
    background: rgba(0, 0, 0, 0.25);
  }

  .member-name {
    font-size: 0.82rem;
    font-weight: 600;
    color: #e4e4e7;
    min-width: 140px;
  }

  .member-quota {
    font-size: 0.76rem;
    color: #a1a1aa;
    flex: 1;
    min-width: 0;
  }

  .member-reset-row {
    display: flex;
    justify-content: flex-end;
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

  .pool-grid {
    display: flex;
    gap: 10px;
    align-items: flex-end;
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

  .group-toggle-row {
    display: flex;
    align-items: center;
    gap: 8px;
    cursor: pointer;
  }

  .group-toggle-label {
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
