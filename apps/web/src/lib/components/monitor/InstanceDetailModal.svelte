<script lang="ts">
  import { formatBytes, formatPercent, formatUptime } from '$lib/utils/format';
  import Modal from '$lib/components/ui/Modal.svelte';
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

{#if instance}
  <Modal open title={instance.name} width="680px" onclose={onClose}>
    <div data-testid="instance-modal" class="flex flex-col gap-5">
      <p class="modal-desc">
        {instance.owner} · {instance.template} · {instance.status} · {formatUptime(
          instance.uptime_secs
        )}
      </p>

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
  </Modal>
{/if}

<style>
  .modal-desc {
    font-size: 0.8rem;
    color: #71717a;
    margin: 0;
    text-transform: capitalize;
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
