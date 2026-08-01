<script lang="ts">
  import { page } from '$app/stores';
  import { onMount, onDestroy } from 'svelte';
  import VncSession from '$lib/components/vnc/VncSession.svelte';
  import CountdownOverlay from '$lib/countdown/CountdownOverlay.svelte';
  import { startKeepalive } from '$lib/keepalive/keepalive';
  import { api } from '$lib/api/client';
  import type { Instance, TimeoutAction } from '$lib/types';

  const token = $page.params.token ?? '';
  let password = $state('password');
  let status = $state<'loading' | 'starting' | 'ready'>('loading');
  let autoSleepsAt = $state<string | null>(null);
  let timeoutAction = $state<TimeoutAction | null>(null);
  let keepTimeDeadline = $state<string | null>(null);
  let keepTimeAction = $state<TimeoutAction | null>(null);
  let keepTimeSeconds = $state<number | null>(null);
  let lastHeartbeatAt = $state<number | null>(null);
  let hadDeadline = $state(false);
  let pollTimer: ReturnType<typeof setInterval> | undefined;
  let stopKeepalive: (() => void) | undefined;

  async function findInstance(): Promise<Instance | undefined> {
    const res = await api.get<{ instances: Instance[] }>('/instances');
    return res.data?.instances?.find(i => i.access_token === token);
  }

  function startKeepaliveIfRunning(inst?: Instance) {
    if (inst?.status === 'running' && !stopKeepalive) {
      stopKeepalive = startKeepalive(inst.id, {
        onHeartbeat: at => {
          lastHeartbeatAt = at;
        }
      });
    }
  }

  function applyInstance(inst?: Instance) {
    if (!inst) return;
    if (inst.access_password) password = inst.access_password;
    autoSleepsAt = inst.auto_sleeps_at ?? null;
    timeoutAction = inst.timeout_action ?? null;
    keepTimeDeadline = inst.keep_time_deadline ?? null;
    keepTimeAction = inst.keep_time_action ?? null;
    keepTimeSeconds = inst.keep_time_seconds ?? null;
    if (inst.auto_sleeps_at || inst.keep_time_deadline) hadDeadline = true;
  }

  async function resyncDeadline() {
    const inst = await findInstance();
    if (!inst || (hadDeadline && inst.status !== 'running')) {
      window.location.href = '/';
      return null;
    }
    applyInstance(inst);
    return {
      auto_sleeps_at: autoSleepsAt,
      timeout_action: timeoutAction,
      keep_time_deadline: keepTimeDeadline,
      keep_time_action: keepTimeAction
    };
  }

  onMount(async () => {
    const inst = await findInstance();
    if (!inst) {
      status = 'ready';
      return;
    }
    applyInstance(inst);
    if (inst.status === 'running') {
      startKeepaliveIfRunning(inst);
      status = 'ready';
      return;
    }
    if (inst.status === 'starting') {
      status = 'starting';
      pollTimer = setInterval(async () => {
        const updated = await findInstance();
        if (!updated) return;
        applyInstance(updated);
        if (updated.status === 'running') {
          clearInterval(pollTimer);
          pollTimer = undefined;
          startKeepaliveIfRunning(updated);
          status = 'ready';
        } else if (updated.status === 'error') {
          clearInterval(pollTimer);
          pollTimer = undefined;
          status = 'ready';
        }
      }, 2000);
    } else {
      status = 'ready';
    }
  });

  onDestroy(() => {
    if (pollTimer) clearInterval(pollTimer);
    if (stopKeepalive) stopKeepalive();
  });
</script>

{#if status === 'starting'}
  <div class="flex flex-col items-center justify-center min-h-screen">
    <div class="w-12 h-12 border-4 border-primary-500 border-t-transparent rounded-full animate-spin mb-6"></div>
    <h2 class="text-xl font-semibold text-surface-100 mb-2">Starting Instance</h2>
    <p class="text-surface-400 text-sm">The instance is booting up. This may take a moment.</p>
  </div>
{:else}
  <CountdownOverlay
    auto_sleeps_at={autoSleepsAt}
    timeout_action={timeoutAction}
    keep_time_deadline={keepTimeDeadline}
    keep_time_action={keepTimeAction}
    keep_time_seconds={keepTimeSeconds}
    last_heartbeat_at={lastHeartbeatAt}
    onResync={resyncDeadline}
  />
  <VncSession {token} {password} />
{/if}
