<script lang="ts">
  import { onMount } from 'svelte';
  import { SvelteSet } from 'svelte/reactivity';
  import { fetchAudit } from '$lib/api/audit';
  import { mayViewAuditLogs } from '$lib/permissions';
  import type { AuditEntry, AuditOutcome, EffectiveContext } from '$lib/types';

  let {
    ctx = null
  }: {
    ctx?: EffectiveContext | null;
  } = $props();

  const canView = $derived(mayViewAuditLogs(ctx));

  interface ActionOption {
    value: string;
    label: string;
  }

  interface ActionGroup {
    label: string;
    options: ActionOption[];
  }

  const ACTION_GROUPS: ActionGroup[] = [
    {
      label: 'Auth',
      options: [
        { value: 'auth.login', label: 'Sign in' },
        { value: 'auth.logout', label: 'Sign out' },
        { value: 'auth.login_failure', label: 'Failed sign-in' },
        { value: 'auth.forbidden', label: 'Access denied' }
      ]
    },
    {
      label: 'Instances',
      options: [
        { value: 'instance.create', label: 'Instance created' },
        { value: 'instance.start', label: 'Instance started' },
        { value: 'instance.stop', label: 'Instance stopped' },
        { value: 'instance.pause', label: 'Instance paused' },
        { value: 'instance.unpause', label: 'Instance resumed' },
        { value: 'instance.delete', label: 'Instance deleted' },
        { value: 'instance.auto_sleep', label: 'Instance auto-slept' }
      ]
    },
    {
      label: 'Templates',
      options: [
        { value: 'template.create', label: 'Template created' },
        { value: 'template.update', label: 'Template updated' },
        { value: 'template.delete', label: 'Template deleted' }
      ]
    },
    {
      label: 'Groups',
      options: [
        { value: 'group.create', label: 'Group created' },
        { value: 'group.update', label: 'Group updated' },
        { value: 'group.delete', label: 'Group deleted' },
        { value: 'group.membership_change', label: 'Membership changed' }
      ]
    },
    {
      label: 'Users',
      options: [
        { value: 'user.create', label: 'User created' },
        { value: 'user.update', label: 'User updated' },
        { value: 'user.delete', label: 'User deleted' },
        { value: 'user.password_change', label: 'Password changed' }
      ]
    },
    {
      label: 'Server',
      options: [
        { value: 'settings.update', label: 'Settings updated' },
        { value: 'registry.update', label: 'Registry updated' }
      ]
    }
  ];

  let action = $state('');
  let actor = $state('');
  let target = $state('');
  let outcome = $state('');
  let afterDate = $state('');
  let beforeDate = $state('');

  let entries = $state<AuditEntry[]>([]);
  let nextCursor = $state<string | null>(null);
  let loading = $state(true);
  let loadingMore = $state(false);
  let error = $state('');
  let expanded = new SvelteSet<string>();

  const hasFilters = $derived(
    action !== '' || actor.trim() !== '' || target.trim() !== '' ||
    outcome !== '' || afterDate !== '' || beforeDate !== ''
  );

  function dayStartIso(date: string): string {
    return `${date}T00:00:00Z`;
  }

  function dayEndIso(date: string): string {
    return `${date}T23:59:59Z`;
  }

  function currentFilters(): Record<string, string> {
    const filters: Record<string, string> = {};
    if (action) filters.action = action;
    if (actor.trim()) filters.actor = actor.trim();
    if (target.trim()) filters.target = target.trim();
    if (outcome) filters.outcome = outcome;
    if (afterDate) filters.after = dayStartIso(afterDate);
    if (beforeDate) filters.before = dayEndIso(beforeDate);
    return filters;
  }

  async function loadFirstPage() {
    if (!canView) return;
    loading = true;
    error = '';
    entries = [];
    nextCursor = null;
    const res = await fetchAudit(currentFilters());
    if (res.error) {
      error = res.error;
    } else if (res.page) {
      entries = res.page.entries;
      nextCursor = res.page.next_cursor;
    }
    loading = false;
  }

  async function loadMore() {
    if (!canView || !nextCursor || loadingMore) return;
    loadingMore = true;
    const res = await fetchAudit({ ...currentFilters(), cursor: nextCursor });
    if (res.error) {
      error = res.error;
    } else if (res.page) {
      entries = [...entries, ...res.page.entries];
      nextCursor = res.page.next_cursor;
    }
    loadingMore = false;
  }

  function clearFilters() {
    action = '';
    actor = '';
    target = '';
    outcome = '';
    afterDate = '';
    beforeDate = '';
    loadFirstPage();
  }

  function formatTime(iso: string): string {
    const d = new Date(iso);
    if (Number.isNaN(d.getTime())) return iso;
    return d.toLocaleString();
  }

  function actionLabel(value: string): string {
    for (const group of ACTION_GROUPS) {
      const found = group.options.find((o) => o.value === value);
      if (found) return found.label;
    }
    return value;
  }

  function isDiffEntry(entry: AuditEntry): boolean {
    if (!entry.detail || typeof entry.detail !== 'object') return false;
    const values = Object.values(entry.detail);
    return values.some(
      (v) =>
        v !== null &&
        typeof v === 'object' &&
        'before' in (v as Record<string, unknown>) &&
        'after' in (v as Record<string, unknown>)
    );
  }

  function diffFields(entry: AuditEntry): [string, unknown, unknown][] {
    if (!entry.detail || typeof entry.detail !== 'object') return [];
    return Object.entries(entry.detail).map(([field, value]) => {
      const v = value as Record<string, unknown>;
      return [field, v.before, v.after];
    });
  }

  function toggleExpand(id: string) {
    if (expanded.has(id)) expanded.delete(id);
    else expanded.add(id);
  }

  function renderValue(value: unknown): string {
    if (value === null || value === undefined) return 'null';
    if (typeof value === 'string') return value;
    try {
      return JSON.stringify(value);
    } catch {
      return String(value);
    }
  }

  const OUTCOME_CLASS: Record<AuditOutcome, string> = {
    success: 'dot-active',
    failure: 'dot-error'
  };

  onMount(() => {
    if (canView) loadFirstPage();
  });
</script>

{#if !canView}
  <section class="ws-section">
    <h2 class="section-title">Logs</h2>
    <p class="empty-text">You do not have permission to view the audit trail.</p>
  </section>
{:else}
  <section class="ws-section panel-card">
    <div class="panel-head">
      <div>
        <h2 class="panel-head-title">Audit Logs</h2>
        <p class="panel-head-desc">A read-only trail of who did what, when. Diff rows expand to show redacted before/after values.</p>
      </div>
    </div>

    <div class="filter-bar">
      <div class="filter-group">
        <label class="filter-label" for="log-filter-action">Event type</label>
        <select id="log-filter-action" class="filter-select" bind:value={action}>
          <option value="">All events</option>
          {#each ACTION_GROUPS as group (group.label)}
            <optgroup label={group.label}>
              {#each group.options as option (option.value)}
                <option value={option.value}>{option.label}</option>
              {/each}
            </optgroup>
          {/each}
        </select>
      </div>
      <div class="filter-group">
        <label class="filter-label" for="log-filter-actor">Actor</label>
        <input id="log-filter-actor" class="modal-input filter-text" type="text" placeholder="Username…" bind:value={actor} />
      </div>
      <div class="filter-group">
        <label class="filter-label" for="log-filter-target">Target</label>
        <input id="log-filter-target" class="modal-input filter-text" type="text" placeholder="Instance / template…" bind:value={target} />
      </div>
      <div class="filter-group">
        <label class="filter-label" for="log-filter-outcome">Outcome</label>
        <select id="log-filter-outcome" class="filter-select" bind:value={outcome}>
          <option value="">Any</option>
          <option value="success">Success</option>
          <option value="failure">Failure</option>
        </select>
      </div>
      <div class="filter-group">
        <label class="filter-label" for="log-filter-after">After</label>
        <input id="log-filter-after" class="modal-input filter-text" type="date" bind:value={afterDate} />
      </div>
      <div class="filter-group">
        <label class="filter-label" for="log-filter-before">Before</label>
        <input id="log-filter-before" class="modal-input filter-text" type="date" bind:value={beforeDate} />
      </div>
      <div class="filter-group filter-actions">
        <button class="modal-confirm filter-apply" onclick={loadFirstPage}>Apply</button>
        {#if hasFilters}
          <button class="filter-clear" onclick={clearFilters}>Clear</button>
        {/if}
      </div>
      <span class="filter-count">{entries.length} entr{entries.length === 1 ? 'y' : 'ies'}</span>
    </div>

    {#if error}
      <div class="error-badge">{error}</div>
    {/if}

    {#if loading}
      <p class="empty-text">Loading audit trail…</p>
    {:else if entries.length === 0}
      <p class="empty-text">No audit entries match the current filters.</p>
    {:else}
      <div class="instances-table-wrap">
        <table class="instances-table audit-table">
          <thead>
            <tr>
              <th>Time</th>
              <th>Actor</th>
              <th>Event</th>
              <th>Target</th>
              <th>Outcome</th>
              <th>IP</th>
            </tr>
          </thead>
          <tbody>
            {#each entries as entry (entry.id)}
              {@const diff = isDiffEntry(entry)}
              <tr class="audit-row" class:expandable={diff} onclick={() => diff && toggleExpand(entry.id)}>
                <td class="td-date">{formatTime(entry.created_at)}</td>
                <td class="td-owner">{entry.actor_name || 'system'}</td>
                <td class="td-action">
                  <span class="action-chip">{actionLabel(entry.action)}</span>
                  {#if diff}
                    <span class="diff-toggle">{expanded.has(entry.id) ? '−' : '+'}</span>
                  {/if}
                </td>
                <td class="td-target">
                  {#if entry.target_name}
                    <span class="target-name">{entry.target_name}</span>
                  {:else}
                    <span class="td-date">—</span>
                  {/if}
                </td>
                <td>
                  <span class="status-badge {OUTCOME_CLASS[entry.outcome] || ''}">
                    <span class="status-dot-inline"></span>
                    {entry.outcome}
                  </span>
                </td>
                <td class="td-date td-ip">{entry.client_ip || '—'}</td>
              </tr>
              {#if diff && expanded.has(entry.id)}
                <tr class="audit-diff-row">
                  <td colspan="6">
                    <div class="audit-diff">
                      {#each diffFields(entry) as [field, before, after] (field)}
                        <div class="audit-diff-field">
                          <span class="audit-diff-name">{field}</span>
                          <span class="audit-diff-before">{renderValue(before)}</span>
                          <span class="audit-diff-arrow">→</span>
                          <span class="audit-diff-after">{renderValue(after)}</span>
                        </div>
                      {/each}
                    </div>
                  </td>
                </tr>
              {/if}
            {/each}
          </tbody>
        </table>
      </div>

      {#if nextCursor}
        <div class="load-more-row">
          <button class="launch-btn resume" onclick={loadMore} disabled={loadingMore}>
            {loadingMore ? 'Loading…' : 'Load more'}
          </button>
        </div>
      {/if}
    {/if}
  </section>
{/if}

<style>
  .filter-text {
    width: 160px;
    padding: 0.45rem 0.65rem;
  }

  .filter-actions {
    flex-direction: row;
    align-items: center;
    gap: 6px;
  }

  .filter-apply {
    padding: 0.45rem 0.9rem;
    font-size: 0.75rem;
  }

  .audit-table td {
    cursor: default;
  }

  .audit-row.expandable {
    cursor: pointer;
  }

  .audit-row.expandable:hover .action-chip {
    border-color: rgba(99, 102, 241, 0.4);
  }

  .action-chip {
    display: inline-flex;
    align-items: center;
    font-size: 0.72rem;
    font-weight: 600;
    color: #c7d2fe;
    background: rgba(99, 102, 241, 0.12);
    border: 1px solid rgba(129, 140, 248, 0.25);
    border-radius: 6px;
    padding: 0.15rem 0.5rem;
    white-space: nowrap;
  }

  .diff-toggle {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 16px;
    height: 16px;
    margin-left: 6px;
    font-size: 0.75rem;
    color: #a5b4fc;
    border: 1px solid rgba(129, 140, 248, 0.3);
    border-radius: 4px;
    vertical-align: middle;
  }

  .target-name {
    color: #a1a1aa;
    font-family: monospace;
    font-size: 0.75rem;
  }

  .td-action {
    white-space: nowrap;
  }

  .td-ip {
    font-family: monospace;
  }

  .audit-diff-row td {
    background: rgba(0, 0, 0, 0.25);
    border-bottom: 1px solid rgba(255, 255, 255, 0.06);
  }

  .audit-diff {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .audit-diff-field {
    display: flex;
    align-items: baseline;
    gap: 10px;
    font-family: monospace;
    font-size: 0.75rem;
  }

  .audit-diff-name {
    font-weight: 700;
    color: #f4f4f5;
    min-width: 140px;
  }

  .audit-diff-before {
    color: #f87171;
    text-decoration: line-through;
    text-decoration-color: rgba(248, 113, 113, 0.4);
    word-break: break-all;
  }

  .audit-diff-arrow {
    color: #52525b;
  }

  .audit-diff-after {
    color: #4ade80;
    word-break: break-all;
  }

  .load-more-row {
    display: flex;
    justify-content: center;
    margin-top: 1rem;
  }
</style>
