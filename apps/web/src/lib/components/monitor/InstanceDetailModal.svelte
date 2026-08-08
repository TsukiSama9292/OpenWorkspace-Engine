<script lang="ts">
  import { formatBytes, formatPercent, formatUptime } from '$lib/utils/format';
  import TimeSeriesChart from './TimeSeriesChart.svelte';
  import type { MonitorInstance } from '$lib/types';

  let {
    instance = null,
    hostCores = 0,
    hostMemTotal = 0,
    onClose = () => {}
  }: {
    instance?: MonitorInstance | null;
    hostCores?: number;
    hostMemTotal?: number;
    onClose?: () => void;
  } = $props();

  const cpuDomainMax = $derived(
    instance && instance.cpu_limit_percent > 0 ? instance.cpu_limit_percent : hostCores * 100
  );
  const memDomainMax = $derived(
    instance && instance.mem_limit_bytes > 0 ? instance.mem_limit_bytes : hostMemTotal
  );
</script>

<svelte:window
  onkeydown={(e) => {
    if (e.key === 'Escape') onClose();
  }}
/>

{#if instance}
  <div class="modal-overlay" data-testid="modal-overlay" onclick={onClose} role="presentation"></div>
  <div class="modal-card" data-testid="instance-modal" role="dialog" aria-modal="true">
    <div class="modal-head">
      <div>
        <h3 class="modal-title">{instance.name}</h3>
        <p class="modal-desc">
          {instance.owner} · {instance.template} · {instance.status} · {formatUptime(
            instance.uptime_secs
          )}
        </p>
      </div>
      <button type="button" class="modal-close" data-testid="modal-close" onclick={onClose} aria-label="Close">
        &times;
      </button>
    </div>

    <div class="detail-block">
      <div class="detail-head">
        <span class="detail-label">CPU</span>
        <span class="detail-value">
          {formatPercent(instance.cpu_percent)}
          {#if instance.cpu_limit_percent > 0}
            <span class="limit-note">/ {formatPercent(instance.cpu_limit_percent)}</span>
          {:else}
            <span class="limit-note">(unlimited)</span>
          {/if}
        </span>
      </div>
      <TimeSeriesChart
        fine={instance.cpu_fine}
        coarse={instance.cpu_coarse}
        color="#6366f1"
        domainMin={0}
        domainMax={cpuDomainMax}
        format={(v) => formatPercent(v)}
        height={170}
      />
    </div>

    <div class="detail-block">
      <div class="detail-head">
        <span class="detail-label">Memory</span>
        <span class="detail-value">
          {formatBytes(instance.mem_used_bytes)}
          {#if instance.mem_limit_bytes > 0}
            <span class="limit-note">/ {formatBytes(instance.mem_limit_bytes)}</span>
          {:else}
            <span class="limit-note">(unlimited)</span>
          {/if}
        </span>
      </div>
      <TimeSeriesChart
        fine={instance.mem_fine}
        coarse={instance.mem_coarse}
        color="#34d399"
        domainMin={0}
        domainMax={memDomainMax}
        format={(v) => formatBytes(v)}
        height={170}
      />
    </div>
  </div>
{/if}

<style>
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
    width: 680px;
    max-width: 92vw;
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
    gap: 1.25rem;
  }

  .modal-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 1rem;
  }

  .modal-title {
    font-size: 1.1rem;
    font-weight: 600;
    margin: 0;
  }

  .modal-desc {
    font-size: 0.8rem;
    color: #71717a;
    margin: 0;
    text-transform: capitalize;
  }

  .modal-close {
    background: none;
    border: none;
    color: #a1a1aa;
    font-size: 1.4rem;
    line-height: 1;
    cursor: pointer;
    padding: 0 0.25rem;
  }

  .modal-close:hover {
    color: #f4f4f5;
  }

  .detail-block {
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
  }

  .detail-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 1rem;
  }

  .detail-label {
    font-size: 0.7rem;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: #71717a;
  }

  .detail-value {
    font-size: 0.95rem;
    font-weight: 600;
    color: #f4f4f5;
    font-variant-numeric: tabular-nums;
  }

  .limit-note {
    color: #71717a;
    font-weight: 400;
    font-size: 0.8rem;
  }
</style>
