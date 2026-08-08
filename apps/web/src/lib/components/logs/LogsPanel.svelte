<script lang="ts">
  import { onMount } from 'svelte';
  import { SvelteSet } from 'svelte/reactivity';
  import { fetchAudit } from '$lib/api/audit';
  import { mayViewAuditLogs } from '$lib/permissions';
  import { formatAuditTime, fullAuditTime } from '$lib/logs/log-helpers';
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
  let narrow = $state(false);

  $effect(() => {
    if (typeof window.matchMedia !== 'function') return;
    const mq = window.matchMedia('(max-width: 899px)');
    narrow = mq.matches;
    const onChange = (event: MediaQueryListEvent) => {
      narrow = event.matches;
    };
    mq.addEventListener('change', onChange);
    return () => mq.removeEventListener('change', onChange);
  });

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

  function actionLabel(value: string): string {
    for (const group of ACTION_GROUPS) {
      const found = group.options.find((o) => o.value === value);
      if (found) return found.label;
    }
    return value;
  }

  function diffFields(entry: AuditEntry): [string, unknown, unknown][] {
    if (!entry.detail || typeof entry.detail !== 'object') return [];
    const fields: [string, unknown, unknown][] = [];
    for (const [field, value] of Object.entries(entry.detail)) {
      if (value !== null && typeof value === 'object' && 'before' in value && 'after' in value) {
        const v = value as Record<string, unknown>;
        fields.push([field, v.before, v.after]);
      }
    }
    return fields;
  }

  function isDiffEntry(entry: AuditEntry): boolean {
    return diffFields(entry).length > 0;
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
      <div class="filter-grid">
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
          <input id="log-filter-actor" class="modal-input" type="text" placeholder="Username…" bind:value={actor} />
        </div>
        <div class="filter-group">
          <label class="filter-label" for="log-filter-target">Target</label>
          <input id="log-filter-target" class="modal-input" type="text" placeholder="Instance / template…" bind:value={target} />
        </div>
        <div class="filter-group">
          <label class="filter-label" for="log-filter-outcome">Outcome</label>
          <select id="log-filter-outcome" class="filter-select" bind:value={outcome}>
            <option value="">Any</option>
            <option value="success">Success</option>
            <option value="failure">Failure</option>
          </select>
        </div>
        <div class="filter-pair">
          <div class="filter-group">
            <label class="filter-label" for="log-filter-after">After</label>
            <input id="log-filter-after" class="modal-input" type="date" bind:value={afterDate} />
          </div>
          <div class="filter-group">
            <label class="filter-label" for="log-filter-before">Before</label>
            <input id="log-filter-before" class="modal-input" type="date" bind:value={beforeDate} />
          </div>
        </div>
      </div>
      <div class="filter-actions-row">
        <span class="filter-count">{entries.length} entr{entries.length === 1 ? 'y' : 'ies'}</span>
        <button class="modal-confirm filter-apply" onclick={loadFirstPage}>Apply</button>
        {#if hasFilters}
          <button class="filter-clear" onclick={clearFilters}>Clear</button>
        {/if}
      </div>
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
        <table class="instances-table audit-table" class:ip-hidden={narrow}>
          <thead>
            <tr>
              <th>Time</th>
              <th>Actor</th>
              <th>Event</th>
              <th>Target</th>
              <th>Outcome</th>
              <th class="ip-th">IP</th>
            </tr>
          </thead>
          <tbody>
            {#each entries as entry (entry.id)}
              {@const diff = isDiffEntry(entry)}
              <tr class="audit-row">
                <td class="td-date" title={fullAuditTime(entry.created_at)}>{formatAuditTime(entry.created_at)}</td>
                <td class="td-owner">{entry.actor_name || 'system'}</td>
                <td class="td-action">
                  <span class="action-chip">{actionLabel(entry.action)}</span>
                  {#if diff}
                    <button
                      type="button"
                      class="diff-toggle"
                      class:open={expanded.has(entry.id)}
                      aria-expanded={expanded.has(entry.id)}
                      aria-controls={`audit-diff-${entry.id}`}
                      onclick={() => toggleExpand(entry.id)}
                    >
                      <span class="diff-chevron" aria-hidden="true">▸</span>
                      <span class="sr-only">{expanded.has(entry.id) ? 'Hide changes' : 'Show changes'}</span>
                    </button>
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
                  <td colspan="6" id={`audit-diff-${entry.id}`}>
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
  .modal-input {
    background: rgba(0, 0, 0, 0.4);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 6px;
    padding: 0.45rem 0.65rem;
    color: #f4f4f5;
    font-size: 0.8rem;
    font-family: inherit;
    outline: none;
    width: 100%;
    min-width: 0;
    box-sizing: border-box;
  }

  .modal-input:focus {
    border-color: #818cf8;
  }

  .modal-input::placeholder {
    color: #52525b;
  }

  .modal-confirm {
    background: #6366f1;
    border: 1px solid #6366f1;
    color: #fff;
    padding: 0.45rem 0.9rem;
    border-radius: 6px;
    font-size: 0.75rem;
    font-weight: 600;
    cursor: pointer;
    font-family: inherit;
    transition: all 0.2s;
  }

  .modal-confirm:hover {
    background: #4f46e5;
  }

  .filter-apply {
    padding: 0.45rem 0.9rem;
    font-size: 0.75rem;
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
  .status-badge.dot-error { color: #f87171; border-color: rgba(239, 68, 68, 0.2); }
  .status-badge.dot-error .status-dot-inline { background: #ef4444; }

  .audit-table td {
    cursor: default;
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
    width: 22px;
    height: 22px;
    margin-left: 8px;
    vertical-align: middle;
    background: rgba(99, 102, 241, 0.12);
    border: 1px solid rgba(129, 140, 248, 0.3);
    border-radius: 6px;
    color: #a5b4fc;
    cursor: pointer;
    line-height: 1;
    transition: border-color 0.15s, background 0.15s;
  }

  .diff-toggle:hover {
    background: rgba(99, 102, 241, 0.22);
  }

  .diff-toggle:focus-visible {
    outline: 2px solid #818cf8;
    outline-offset: 1px;
  }

  .diff-chevron {
    display: inline-block;
    transition: transform 0.15s ease;
  }

  .diff-toggle.open .diff-chevron {
    transform: rotate(90deg);
  }

  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
    white-space: nowrap;
    border: 0;
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

  .audit-table.ip-hidden th.ip-th,
  .audit-table.ip-hidden td.td-ip {
    display: none;
  }

  :global(.instances-table.audit-table th:first-child),
  :global(.instances-table.audit-table td:first-child) {
    min-width: 0;
  }

  .audit-diff-row td {
    background: rgba(0, 0, 0, 0.25);
    border-bottom: 1px solid rgba(255, 255, 255, 0.06);
    animation: audit-diff-in 0.18s ease-out;
  }

  @keyframes audit-diff-in {
    from {
      opacity: 0;
      transform: translateY(-2px);
    }
    to {
      opacity: 1;
      transform: none;
    }
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

  @media (prefers-reduced-motion: reduce) {
    .diff-chevron {
      transition: none;
    }

    .audit-diff-row td {
      animation: none;
    }
  }
</style>
