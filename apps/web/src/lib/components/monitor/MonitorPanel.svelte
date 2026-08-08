<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { fetchMonitorSnapshot } from '$lib/api/monitor';
  import { mayViewMonitoring } from '$lib/permissions';
  import { formatBytes, formatPercent, formatUptime } from '$lib/utils/format';
  import Sparkline from './Sparkline.svelte';
  import TimeSeriesChart from './TimeSeriesChart.svelte';
  import InstanceDetailModal from './InstanceDetailModal.svelte';
  import type { EffectiveContext, MonitorInstance, MonitorSnapshot, SeriesPoint } from '$lib/types';

  let {
    ctx = null
  }: {
    ctx?: EffectiveContext | null;
  } = $props();

  type SortKey = 'name' | 'owner' | 'template' | 'runtime' | 'status' | 'uptime' | 'cpu' | 'mem';

  let snapshot = $state<MonitorSnapshot | null>(null);
  let loading = $state(true);
  let error = $state('');
  let sortKey = $state<SortKey>('cpu');
  let sortDir = $state<'asc' | 'desc'>('desc');
  let selected = $state<MonitorInstance | null>(null);

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
            fine: snapshot.host.cpu_fine,
            coarse: snapshot.host.cpu_coarse,
            min: 0,
            max: 100,
            color: '#6366f1',
            format: (v: number) => formatPercent(v)
          },
          {
            key: 'ram' as const,
            label: 'Memory',
            value: formatBytes(snapshot.host.mem_used_bytes),
            detail: `of ${formatBytes(snapshot.host.mem_total_bytes)}`,
            fine: snapshot.host.mem_fine,
            coarse: snapshot.host.mem_coarse,
            min: 0,
            max: snapshot.host.mem_total_bytes,
            color: '#34d399',
            format: (v: number) => formatBytes(v)
          },
          {
            key: 'disk' as const,
            label: 'Disk',
            value: formatBytes(snapshot.host.disk_used_bytes),
            detail: `of ${formatBytes(snapshot.host.disk_total_bytes)}`,
            fine: snapshot.host.disk_fine,
            coarse: snapshot.host.disk_coarse,
            min: 0,
            max: snapshot.host.disk_total_bytes,
            color: '#fbbf24',
            format: (v: number) => formatBytes(v)
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

  function memPercent(used: number, limit: number): string {
    if (limit <= 0) return '—';
    return `${Math.round((used / limit) * 100)}%`;
  }

  function cpuPctOfLimit(cpu: number, limit: number): string {
    if (limit <= 0) return '—';
    return `${Math.round((cpu / limit) * 100)}%`;
  }

  function sparkSeries(points: SeriesPoint[]): { values: number[]; timestamps: number[] } {
    return { values: points.map((p) => p.v), timestamps: points.map((p) => p.t) };
  }

  async function load() {
    if (!canView) return;
    loading = true;
    error = '';
    const res = await fetchMonitorSnapshot();
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

  function closeModal() {
    selected = null;
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
          <div class="host-card" data-testid="host-card">
            <div class="host-card-head">
              <span class="host-card-label">{card.label}</span>
              <span class="host-card-value">
                {card.value}
                {#if card.detail}<span class="host-card-detail">{card.detail}</span>{/if}
              </span>
            </div>
            <TimeSeriesChart
              fine={card.fine}
              coarse={card.coarse}
              color={card.color}
              domainMin={card.min}
              domainMax={card.max}
              format={card.format}
              height={180}
            />
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
              <th class="detail-col">Detail</th>
            </tr>
          </thead>
          <tbody>
            {#if sortedInstances.length === 0}
              <tr>
                <td colspan={COLUMNS.length + 1} class="empty-cell">No active instances.</td>
              </tr>
            {:else}
              {#each sortedInstances as inst (inst.id)}
                {@const cpu = sparkSeries(inst.cpu_fine)}
                {@const mem = sparkSeries(inst.mem_fine)}
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
                      {#if inst.cpu_limit_percent > 0}
                        <Sparkline
                          values={cpu.values}
                          timestamps={cpu.timestamps}
                          format={formatPercent}
                          color="#6366f1"
                          min={0}
                          max={inst.cpu_limit_percent}
                        />
                        <span>
                          {formatPercent(inst.cpu_percent)}
                          <span class="limit-note">/ {formatPercent(inst.cpu_limit_percent)}</span>
                          <span class="pct-note">({cpuPctOfLimit(inst.cpu_percent, inst.cpu_limit_percent)})</span>
                        </span>
                      {:else}
                        <Sparkline
                          values={cpu.values}
                          timestamps={cpu.timestamps}
                          format={formatPercent}
                          color="#6366f1"
                          min={0}
                          max={snapshot.host.cpu_cores * 100}
                        />
                        <span>
                          {formatPercent(inst.cpu_percent)}
                          <span class="limit-note">(unlimited)</span>
                        </span>
                      {/if}
                    </div>
                  </td>
                  <td class="td-numeric">
                    <div class="spark-cell">
                      {#if inst.mem_limit_bytes > 0}
                        <Sparkline
                          values={mem.values}
                          timestamps={mem.timestamps}
                          format={formatBytes}
                          color="#34d399"
                          min={0}
                          max={inst.mem_limit_bytes}
                        />
                        <span>
                          {formatBytes(inst.mem_used_bytes)}
                          <span class="limit-note">/ {formatBytes(inst.mem_limit_bytes)}</span>
                          <span class="pct-note">({memPercent(inst.mem_used_bytes, inst.mem_limit_bytes)})</span>
                        </span>
                      {:else}
                        <Sparkline
                          values={mem.values}
                          timestamps={mem.timestamps}
                          format={formatBytes}
                          color="#34d399"
                          min={0}
                          max={snapshot.host.mem_total_bytes}
                        />
                        <span>
                          {formatBytes(inst.mem_used_bytes)}
                          <span class="limit-note">(unlimited)</span>
                        </span>
                      {/if}
                    </div>
                  </td>
                  <td class="td-numeric detail-col">
                    <button
                      type="button"
                      class="row-detail"
                      data-testid="row-detail"
                      onclick={() => (selected = inst)}
                    >
                      Detail
                    </button>
                  </td>
                </tr>
              {/each}
            {/if}
          </tbody>
        </table>
      </div>
    {/if}
  </section>

  {#if selected}
    <InstanceDetailModal
      instance={selected}
      hostCores={snapshot?.host.cpu_cores ?? 0}
      hostMemTotal={snapshot?.host.mem_total_bytes ?? 0}
      onClose={closeModal}
    />
  {/if}
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

  .host-cards {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(280px, 1fr));
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
    font-size: 0.7rem;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: #71717a;
  }

  .host-card-value {
    font-size: 1.25rem;
    font-weight: 600;
    color: #f4f4f5;
  }

  .host-card-detail {
    font-size: 0.8rem;
    font-weight: 400;
    color: #71717a;
  }

  .sub-title {
    font-size: 0.95rem;
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
    font-size: 0.72rem;
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
    font-size: 0.72rem;
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
    font-size: 0.85rem;
    color: #a1a1aa;
    text-transform: capitalize;
  }

  .td-numeric {
    text-align: left;
    font-variant-numeric: tabular-nums;
  }

  .detail-col {
    width: 1%;
  }

  .row-detail {
    background: rgba(99, 102, 241, 0.15);
    border: 1px solid rgba(99, 102, 241, 0.35);
    border-radius: 8px;
    color: #a5b4fc;
    font-family: inherit;
    font-size: 0.75rem;
    font-weight: 600;
    padding: 0.25rem 0.6rem;
    cursor: pointer;
    transition: background 0.15s;
  }

  .row-detail:hover {
    background: rgba(99, 102, 241, 0.3);
  }

  .spark-cell {
    display: inline-flex;
    align-items: center;
    gap: 0.5rem;
    justify-content: flex-start;
    font-size: 0.85rem;
  }

  .spark-cell :global(.sparkline) {
    width: 64px;
  }

  .limit-note {
    color: #71717a;
    font-weight: 400;
  }

  .pct-note {
    color: #a1a1aa;
  }

  .empty-cell {
    padding: 1.5rem;
    text-align: center;
    color: #71717a;
  }
</style>
