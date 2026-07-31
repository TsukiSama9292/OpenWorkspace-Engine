<script lang="ts">
  import { page } from '$app/stores';
  import { onMount, onDestroy } from 'svelte';
  import VncSession from '$lib/components/vnc/VncSession.svelte';
  import { api } from '$lib/api/client';
  import type { Instance } from '$lib/types';

  const token = $page.params.token ?? '';
  let password = $state('password');
  let status = $state<'loading' | 'starting' | 'ready'>('loading');
  let pollTimer: ReturnType<typeof setInterval> | undefined;

  onMount(async () => {
    const res = await api.get<{ instances: Instance[] }>('/instances');
    const inst = res.data?.instances?.find(i => i.access_token === token);
    if (!inst) {
      status = 'ready';
      return;
    }
    if (inst.access_password) password = inst.access_password;
    if (inst.status === 'running') {
      status = 'ready';
      return;
    }
    if (inst.status === 'starting') {
      status = 'starting';
      pollTimer = setInterval(async () => {
        const r = await api.get<{ instances: Instance[] }>('/instances');
        const updated = r.data?.instances?.find(i => i.access_token === token);
        if (!updated) return;
        if (updated.access_password) password = updated.access_password;
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
  <VncSession {token} {password} />
{/if}
