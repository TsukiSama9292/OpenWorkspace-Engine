<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { formatRemaining, remainingMs, severity } from './countdown';
  import { TIMEOUT_ACTION_LABELS } from './countdown';
  import type { TimeoutAction } from '$lib/types';

  interface ResyncResult {
    deadline: string | null;
    action: TimeoutAction | null;
  }

  interface Props {
    deadline?: string | null;
    action?: TimeoutAction | null;
    onResync?: (() => Promise<ResyncResult | null>) | null;
  }

  let { deadline = null, action = null, onResync = null }: Props = $props();

  const RESYNC_MS = 30_000;

  let now = $state(Date.now());
  let tickTimer: ReturnType<typeof setInterval> | undefined;
  let resyncTimer: ReturnType<typeof setInterval> | undefined;

  async function resync() {
    if (!onResync) return;
    const next = await onResync();
    if (next) {
      deadline = next.deadline;
      action = next.action;
    }
  }

  function startTimers() {
    stopTimers();
    tickTimer = setInterval(() => {
      now = Date.now();
    }, 1000);
    resyncTimer = setInterval(resync, RESYNC_MS);
  }

  function stopTimers() {
    if (tickTimer) clearInterval(tickTimer);
    if (resyncTimer) clearInterval(resyncTimer);
    tickTimer = undefined;
    resyncTimer = undefined;
  }

  function onVisibilityChange() {
    if (document.visibilityState === 'visible') {
      now = Date.now();
      resync();
    }
  }

  onMount(() => {
    startTimers();
    document.addEventListener('visibilitychange', onVisibilityChange);
  });

  onDestroy(() => {
    stopTimers();
    document.removeEventListener('visibilitychange', onVisibilityChange);
  });

  const remaining = $derived(remainingMs(deadline, now));
  const level = $derived(remaining === null ? null : severity(remaining));
  const expired = $derived(remaining !== null && remaining <= 0);

  let zeroResynced = $state(false);

  $effect(() => {
    if (remaining === null) return;
    if (remaining <= 0) {
      if (!zeroResynced) {
        zeroResynced = true;
        resync();
      }
    } else {
      zeroResynced = false;
    }
  });

  const levelClasses: Record<'normal' | 'warning' | 'critical', string> = {
    normal: 'bg-black/60 text-surface-100 border border-white/10',
    warning: 'bg-amber-500/70 text-black backdrop-blur-sm',
    critical: 'bg-red-600/70 text-white backdrop-blur-sm'
  };
</script>

{#if remaining !== null}
  <div
    class="fixed top-4 right-4 z-[9999] pointer-events-none select-none rounded-lg px-3 py-1.5 shadow-lg {level === 'normal'
      ? levelClasses.normal
      : level === 'warning'
        ? levelClasses.warning
        : levelClasses.critical}"
  >
    <div class="text-sm font-semibold tabular-nums leading-tight">
      {#if expired}
        已到期
      {:else}
        {formatRemaining(remaining)}
      {/if}
    </div>
    {#if action && !expired}
      <div class="text-[10px] opacity-80 leading-tight text-center">
        到期將{TIMEOUT_ACTION_LABELS[action]}
      </div>
    {/if}
  </div>
{/if}
