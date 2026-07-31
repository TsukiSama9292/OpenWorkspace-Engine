<script lang="ts">
  import { page } from '$app/stores';
  import { onMount, onDestroy } from 'svelte';
  import VncSession from '$lib/components/vnc/VncSession.svelte';
  import CountdownOverlay from '$lib/countdown/CountdownOverlay.svelte';
  import { api } from '$lib/api/client';
  import type { Instance, TimeoutAction } from '$lib/types';

  const token = $page.params.token ?? '';
  let password = $state('password');
  let status = $state<'loading' | 'starting' | 'ready'>('loading');
  let autoSleepsAt = $state<string | null>(null);
  let timeoutAction = $state<TimeoutAction | null>(null);
  let hadDeadline = $state(false);
  let pollTimer: ReturnType<typeof setInterval> | undefined;

  async function findInstance(): Promise<Instance | undefined> {
    const res = await api.get<{ instances: Instance[] }>('/instances');
    return res.data?.instances?.find(i => i.access_token === token);
  }

  function applyInstance(inst?: Instance) {
    if (!inst) return;
    if (inst.access_password) password = inst.access_password;
    autoSleepsAt = inst.auto_sleeps_at ?? null;
    timeoutAction = inst.timeout_action ?? null;
    if (inst.auto_sleeps_at) hadDeadline = true;
  }

  async function resyncDeadline() {
    const inst = await findInstance();
    if (!inst || (hadDeadline && inst.status !== 'running')) {
      window.location.href = '/';
      return null;
    }
    applyInstance(inst);
    return { deadline: autoSleepsAt, action: timeoutAction };
  }

  onMount(async () => {
    const inst = await findInstance();
    if (!inst) {
      status = 'ready';
      return;
    }
    applyInstance(inst);
    if (inst.status === 'running') {
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
  });
</script>

{#if status === 'starting'}
  <div class="flex flex-col items-center justify-center min-h-screen">
    <div class="w-12 h-12 border-4 border-primary-500 border-t-transparent rounded-full animate-spin mb-6"></div>
    <h2 class="text-xl font-semibold text-surface-100 mb-2">Starting Instance</h2>
    <p class="text-surface-400 text-sm">The instance is booting up. This may take a moment.</p>
  </div>
{:else}
  <CountdownOverlay deadline={autoSleepsAt} action={timeoutAction} onResync={resyncDeadline} />
  <VncSession {token} {password} />
{/if}
