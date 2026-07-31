<script lang="ts">
  import { page } from '$app/stores';
  import { onMount, onDestroy } from 'svelte';
  import CountdownOverlay from '$lib/countdown/CountdownOverlay.svelte';
  import { iframeSrc } from '$lib/countdown/countdown';
  import { api } from '$lib/api/client';
  import type { Instance, TimeoutAction } from '$lib/types';

  const token = $page.params.token ?? '';
  let instance = $state<Instance | null>(null);
  let loading = $state(true);
  let missing = $state(false);
  let hadDeadline = $state(false);
  let pollTimer: ReturnType<typeof setInterval> | undefined;

  async function findInstance(): Promise<Instance | undefined> {
    const res = await api.get<{ instances: Instance[] }>('/instances');
    return res.data?.instances?.find(i => i.access_token === token);
  }

  async function refreshInstance(): Promise<Instance | null> {
    const inst = await findInstance();
    if (!inst) {
      missing = true;
      instance = null;
      return null;
    }
    instance = inst;
    missing = false;
    if (inst.auto_sleeps_at) hadDeadline = true;
    return inst;
  }

  async function resyncDeadline() {
    const inst = await refreshInstance();
    if (!inst) {
      if (hadDeadline) window.location.href = '/';
      return null;
    }
    if (hadDeadline && inst.status !== 'running') {
      window.location.href = '/';
      return null;
    }
    return {
      deadline: inst.auto_sleeps_at ?? null,
      action: (inst.timeout_action as TimeoutAction | null) ?? null
    };
  }

  onMount(async () => {
    const inst = await refreshInstance();
    loading = false;
    if (!inst) return;
    if (inst.status === 'running' && inst.remote_type === 'kasmvnc') {
      window.location.href = `/kasmvnc/${token}/`;
      return;
    }
    if (inst.status === 'starting') {
      pollTimer = setInterval(async () => {
        const updated = await refreshInstance();
        if (!updated) return;
        if (updated.status === 'running' || updated.status === 'error') {
          clearInterval(pollTimer);
          pollTimer = undefined;
        }
      }, 2000);
    }
  });

  onDestroy(() => {
    if (pollTimer) clearInterval(pollTimer);
  });
</script>

{#if loading}
  <div class="flex flex-col items-center justify-center min-h-screen">
    <div class="w-12 h-12 border-4 border-primary-500 border-t-transparent rounded-full animate-spin mb-6"></div>
    <h2 class="text-xl font-semibold text-surface-100 mb-2">Loading Instance</h2>
  </div>
{:else if missing}
  <div class="flex flex-col items-center justify-center min-h-screen gap-4">
    <h2 class="text-xl font-semibold text-surface-100">Instance Not Found</h2>
    <a href="/" class="text-sm text-primary-400 hover:text-primary-300">Back to Dashboard</a>
  </div>
{:else if instance?.status === 'running'}
  <CountdownOverlay
    deadline={instance.auto_sleeps_at ?? null}
    action={instance.timeout_action ?? null}
    onResync={resyncDeadline}
  />
  {#if instance.remote_type === 'kasmvnc'}
    <div class="flex flex-col items-center justify-center min-h-screen">
      <h2 class="text-xl font-semibold text-surface-100">Redirecting...</h2>
    </div>
  {:else}
    <iframe
      class="fixed inset-0 h-full w-full border-0"
      src={iframeSrc(instance.remote_type, token, instance.access_password ?? '')}
      title={instance.name}
    ></iframe>
  {/if}
{:else if instance?.status === 'starting'}
  <div class="flex flex-col items-center justify-center min-h-screen">
    <div class="w-12 h-12 border-4 border-primary-500 border-t-transparent rounded-full animate-spin mb-6"></div>
    <h2 class="text-xl font-semibold text-surface-100 mb-2">Starting Instance</h2>
    <p class="text-sm text-surface-400">The instance is booting up. This may take a moment.</p>
  </div>
{:else}
  <div class="flex flex-col items-center justify-center min-h-screen gap-2">
    <h2 class="text-xl font-semibold text-surface-100">
      {instance?.status === 'paused' ? '已暫停' : '已停止'}
    </h2>
    <p class="text-sm text-surface-400">This instance is not running.</p>
    <a href="/" class="mt-2 text-sm text-primary-400 hover:text-primary-300">Back to Dashboard</a>
  </div>
{/if}
