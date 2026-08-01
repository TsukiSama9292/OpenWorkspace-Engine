<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { formatRemaining, remainingMs, selectDeadline, severity } from './countdown';
  import { TIMEOUT_ACTION_LABELS } from './countdown';
  import { tabHasFocus } from '$lib/keepalive/keepalive';
  import type { TimeoutAction } from '$lib/types';

  interface ResyncResult {
    auto_sleeps_at: string | null;
    timeout_action: TimeoutAction | null;
    keep_time_deadline: string | null;
    keep_time_action: TimeoutAction | null;
  }

  interface Props {
    auto_sleeps_at?: string | null;
    timeout_action?: TimeoutAction | null;
    keep_time_deadline?: string | null;
    keep_time_action?: TimeoutAction | null;
    keep_time_seconds?: number | null;
    last_heartbeat_at?: number | null;
    onResync?: (() => Promise<ResyncResult | null>) | null;
  }

  let {
    auto_sleeps_at = null,
    timeout_action = null,
    keep_time_deadline = null,
    keep_time_action = null,
    keep_time_seconds = null,
    last_heartbeat_at = null,
    onResync = null
  }: Props = $props();

  const RESYNC_MS = 30_000;

  let now = $state(Date.now());
  let tickTimer: ReturnType<typeof setInterval> | undefined;
  let resyncTimer: ReturnType<typeof setInterval> | undefined;
  let visible = $state(
    typeof document === 'undefined' ? false : document.visibilityState === 'visible'
  );
  let focused = $state(typeof document === 'undefined' ? false : tabHasFocus());

  async function resync() {
    if (!onResync) return;
    const next = await onResync();
    if (next) {
      auto_sleeps_at = next.auto_sleeps_at;
      timeout_action = next.timeout_action;
      keep_time_deadline = next.keep_time_deadline;
      keep_time_action = next.keep_time_action;
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
    const visibleNow = document.visibilityState === 'visible';
    if (visibleNow) {
      now = Date.now();
      resync();
    }
    visible = visibleNow;
    focused = tabHasFocus();
  }

  function recomputeFocus() {
    const wasFocused = focused;
    focused = tabHasFocus();
    if (wasFocused !== focused) {
      now = Date.now();
      resync();
    }
  }

  onMount(() => {
    startTimers();
    document.addEventListener('visibilitychange', onVisibilityChange);
    window.addEventListener('focus', recomputeFocus);
    window.addEventListener('blur', recomputeFocus);
    document.addEventListener('focusin', recomputeFocus);
    document.addEventListener('focusout', recomputeFocus);
  });

  onDestroy(() => {
    stopTimers();
    document.removeEventListener('visibilitychange', onVisibilityChange);
    window.removeEventListener('focus', recomputeFocus);
    window.removeEventListener('blur', recomputeFocus);
    document.removeEventListener('focusin', recomputeFocus);
    document.removeEventListener('focusout', recomputeFocus);
  });

  const keepDeadline = $derived(
    (() => {
      const serverMs = keep_time_deadline ? Date.parse(keep_time_deadline) : Number.NaN;
      const localMs =
        keep_time_seconds && last_heartbeat_at
          ? last_heartbeat_at + keep_time_seconds * 1000
          : Number.NaN;
      if (Number.isNaN(serverMs) && Number.isNaN(localMs)) return null;
      if (Number.isNaN(serverMs)) return new Date(localMs).toISOString();
      if (Number.isNaN(localMs)) return keep_time_deadline;
      return new Date(Math.max(serverMs, localMs)).toISOString();
    })()
  );

  const selected = $derived(
    selectDeadline(auto_sleeps_at, timeout_action, keepDeadline, keep_time_action)
  );
  const remaining = $derived(selected === null ? null : remainingMs(selected.deadline, now));
  const action = $derived(selected?.action ?? null);
  const level = $derived(remaining === null ? null : severity(remaining));
  const expired = $derived(remaining !== null && remaining <= 0);
  const show = $derived(
    selected !== null && remaining !== null && visible && !focused
  );

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

{#if show}
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
      {:else if remaining !== null}
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
