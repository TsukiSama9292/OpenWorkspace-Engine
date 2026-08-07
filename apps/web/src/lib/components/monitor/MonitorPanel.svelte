<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { fetchMonitorSnapshot } from '$lib/api/monitor';
  import { mayViewMonitoring } from '$lib/permissions';
  import { formatBytes, formatPercent, formatUptime } from '$lib/utils/format';
  import Sparkline from './Sparkline.svelte';
  import type { EffectiveContext, MonitorInstance, MonitorRange, MonitorSnapshot } from '$lib/types';

  let {
    ctx = null
  }: {
    ctx?: EffectiveContext | null;
  } = $props();

  type SortKey = 'name' | 'owner' | 'template' | 'runtime' | 'status' | 'uptime' | 'cpu' | 'mem';

  let snapshot = $state<MonitorSnapshot | null>(null);
  let loading = $state(true);
  let error = $state('');
  let range = $state<MonitorRange>('1h');
  let sortKey = $state<SortKey>('cpu');
  let sortDir = $state<'asc' | 'desc'>('desc');

  let timer: ReturnType<typeof setInterval> | null = null;

  const canView = $derived(mayViewMonitoring(ctx));

  const sortedInstances = $derived(
    snapshot
      ? [...snapshot.instances].sort((a, b) => {
          const cmp = compare(a, b, sortKey);
          return sortDir === 'asc' ? cmp : -cmp;
        })
      : []
  );

  const hostCards = $derived(
    snapshot
      ? [
          {
            key: 'cpu' as const,
            label: 'CPU',
            value: formatPercent(snapshot.host.cpu_percent),
            detail: '',
            series: snapshot.host.cpu_series,
            color: '#6366f1'
          },
          {
            key: 'ram' as const,
            label: 'Memory',
            value: formatBytes(snapshot.host.mem_used_bytes),
            detail: `of ${formatBytes(snapshot.host.mem_total_bytes)}`,
            series: snapshot.host.mem_series,
            color: '#34d399'
          },
          {
            key: 'disk' as const,
            label: 'Disk',
            value: formatBytes(snapshot.host.disk_used_bytes),
            detail: `of ${formatBytes(snapshot.host.disk_total_bytes)}`,
            series: snapshot.host.disk_series,
            color: '#fbbf24'
          }
        ]
      : []
  );

  const COLUMNS: { key: SortKey; label: string; numeric?: boolean }[] = [
    { key: 'name', label: 'Instance' },
    { key: 'owner', label: 'Owner' },
    { key: 'template', label: 'Template' },
    { key: 'runtime', label: 'Runtime' },
    { key: 'status', label: 'Status' },
    { key: 'uptime', label: 'Uptime', numeric: true },
    { key: 'cpu', label: 'CPU', numeric: true },
    { key: 'mem', label: 'Memory', numeric: true }
  ];

  function compare(a: MonitorInstance, b: MonitorInstance, key: SortKey): number {
    const av = valueOf(a, key);
    const bv = valueOf(b, key);
    if (av < bv) return -1;
    if (av > bv) return 1;
    return 0;
  }

  function valueOf(inst: MonitorInstance, key: SortKey): string | number {
    switch (key) {
      case 'name':
        return inst.name;
      case 'owner':
        return inst.owner;
      case 'template':
        return inst.template;
      case 'runtime':
        return inst.runtime;
      case 'status':
        return inst.status;
      case 'uptime':
        return inst.uptime_secs ?? -1;
      case 'cpu':
        return inst.cpu_percent;
      case 'mem':
        return inst.mem_used_bytes;
    }
  }

  function isPaused(inst: MonitorInstance): boolean {
    return inst.status === 'paused';
  }

  async function load() {
    if (!canView) return;
    loading = true;
    error = '';
    const res = await fetchMonitorSnapshot(range);
    if (res.snapshot) {
      snapshot = res.snapshot;
    } else if (res.error) {
      error = res.error;
    }
    loading = false;
  }

  function onSort(key: SortKey) {
    if (sortKey === key) {
      sortDir = sortDir === 'asc' ? 'desc' : 'asc';
    } else {
      sortKey = key;
      sortDir = 'desc';
    }
  }

  onMount(() => {
    if (canView) {
      load();
      timer = setInterval(load, 5000);
    }
  });

  onDestroy(() => {
    if (timer) clearInterval(timer);
  });
</script>

{#if !canView}
  <section class="ws-section">
    <p class="empty-text">You do not have permission to view monitoring.</p>
  </section>
{:else}
  <section class="ws-section monitor-panel">
    <div class="monitor-toolbar">
      <div>
        <h2 class="panel-head-title">System Monitor</h2>
        <p class="panel-head-desc">Live host resources and active instances.</p>
      </div>
      <div class="range-toggle" role="group" aria-label="Time range">
        <button
          class="range-btn"
          class:active={range === '1h'}
          onclick={() => {
            range = '1h';
            load();
          }}
        >
          1h
        </button>
        <button
          class="range-btn"
          class:active={range === '24h'}
          onclick={() => {
            range = '24h';
            load();
          }}
        >
          24h
        </button>
      </div>
    </div>

    {#if error}
      <p class="empty-text">{error}</p>
    {/if}

    {#if !snapshot}
      {#if loading}
        <p class="empty-text">Loading monitor data...</p>
      {/if}
    {:else}
      <div class="host-cards">
        {#each hostCards as card (card.key)}
          <div class="host-card">
            <div class="host-card-head">
              <span class="host-card-label">{card.label}</span>
              <span class="host-card-value">
                {card.value}
                {#if card.detail}<span class="host-card-detail">{card.detail}</span>{/if}
              </span>
            </div>
            <Sparkline values={card.series} color={card.color} width={140} height={36} />
          </div>
        {/each}
      </div>

      <h3 class="sub-title">Active Instances</h3>
      <div class="instances-table-wrap">
        <table class="instances-table">
          <thead>
            <tr>
              {#each COLUMNS as col (col.key)}
                <th
                  class:sortable={true}
                  aria-sort={sortKey === col.key ? (sortDir === 'asc' ? 'ascending' : 'descending') : 'none'}
                >
                  <button class="sort-btn" onclick={() => onSort(col.key)}>
                    {col.label}
                    {#if sortKey === col.key}
                      <span class="sort-arrow">{sortDir === 'asc' ? '↑' : '↓'}</span>
                    {/if}
                  </button>
                </th>
              {/each}
            </tr>
          </thead>
          <tbody>
            {#if sortedInstances.length === 0}
              <tr>
                <td colspan={COLUMNS.length} class="empty-cell">No active instances.</td>
              </tr>
            {:else}
              {#each sortedInstances as inst (inst.id)}
                <tr class="monitor-row" class:paused={isPaused(inst)}>
                  <td class="td-name">
                    <span class="td-name-text">{inst.name}</span>
                  </td>
                  <td>{inst.owner}</td>
                  <td>{inst.template}</td>
                  <td>
                    <span class="runtime-badge">{inst.runtime || 'runc'}</span>
                  </td>
                  <td>
                    {#if isPaused(inst)}
                      <span class="status-badge paused">[paused]</span>
                    {:else}
                      <span class="status-text">{inst.status}</span>
                    {/if}
                  </td>
                  <td class="td-numeric">{formatUptime(inst.uptime_secs)}</td>
                  <td class="td-numeric">
                    <div class="spark-cell">
                      <Sparkline values={inst.cpu_series} color="#6366f1" />
                      <span>{formatPercent(inst.cpu_percent)}</span>
                    </div>
                  </td>
                  <td class="td-numeric">
                    <div class="spark-cell">
                      <Sparkline values={inst.mem_series} color="#34d399" />
                      <span>
                        {formatBytes(inst.mem_used_bytes)}
                        <span class="mem-limit">/ {formatBytes(inst.mem_limit_bytes)}</span>
                      </span>
                    </div>
                  </td>
                </tr>
              {/each}
            {/if}
          </tbody>
        </table>
      </div>
    {/if}
  </section>
{/if}

<style>
  .monitor-panel {
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }

  .monitor-toolbar {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 1rem;
  }

  .range-toggle {
    display: inline-flex;
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 8px;
    overflow: hidden;
  }

  .range-btn {
    background: rgba(255, 255, 255, 0.04);
    border: none;
    color: #a1a1aa;
    font-family: inherit;
    font-size: 0.75rem;
    font-weight: 600;
    padding: 0.4rem 0.9rem;
    cursor: pointer;
    transition: background 0.15s, color 0.15s;
  }

  .range-btn + .range-btn {
    border-left: 1px solid rgba(255, 255, 255, 0.1);
  }

  .range-btn.active {
    background: #6366f1;
    color: #fff;
  }

  .host-cards {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
    gap: 1rem;
  }

  .host-card {
    background: rgba(255, 255, 255, 0.03);
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 12px;
    padding: 0.9rem 1rem;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .host-card-head {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .host-card-label {
    font-size: 0.65rem;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: #71717a;
  }

  .host-card-value {
    font-size: 1.15rem;
    font-weight: 600;
    color: #f4f4f5;
  }

  .host-card-detail {
    font-size: 0.7rem;
    font-weight: 400;
    color: #71717a;
  }

  .sub-title {
    font-size: 0.85rem;
    font-weight: 600;
    color: #d4d4d8;
    margin: 0.5rem 0 0;
  }

  .sortable {
    padding: 0;
  }

  .sort-btn {
    background: none;
    border: none;
    color: inherit;
    font-family: inherit;
    font-size: inherit;
    font-weight: inherit;
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    gap: 3px;
    padding: 0;
  }

  .sort-arrow {
    color: #818cf8;
  }

  .monitor-row.paused {
    opacity: 0.45;
  }

  .runtime-badge {
    display: inline-flex;
    align-items: center;
    font-size: 0.62rem;
    font-weight: 600;
    padding: 0.15rem 0.45rem;
    border-radius: 999px;
    color: #86efac;
    background: rgba(34, 197, 94, 0.1);
    border: 1px solid rgba(74, 222, 128, 0.2);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .status-badge.paused {
    display: inline-flex;
    align-items: center;
    font-size: 0.62rem;
    font-weight: 700;
    padding: 0.15rem 0.45rem;
    border-radius: 999px;
    color: #fca5a5;
    background: rgba(239, 68, 68, 0.1);
    border: 1px solid rgba(248, 113, 113, 0.25);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .status-text {
    font-size: 0.75rem;
    color: #a1a1aa;
    text-transform: capitalize;
  }

  .td-numeric {
    text-align: right;
    font-variant-numeric: tabular-nums;
  }

  .spark-cell {
    display: inline-flex;
    align-items: center;
    gap: 0.5rem;
    justify-content: flex-end;
    font-size: 0.75rem;
  }

  .spark-cell :global(.sparkline) {
    width: 64px;
  }

  .mem-limit {
    color: #71717a;
  }

  .empty-cell {
    padding: 1.5rem;
    text-align: center;
    color: #71717a;
  }
</style>
