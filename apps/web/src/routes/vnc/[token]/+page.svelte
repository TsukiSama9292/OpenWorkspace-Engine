<script lang="ts">
  import { page } from '$app/stores';
  import { onMount } from 'svelte';
  import VncSession from '$lib/components/vnc/VncSession.svelte';
  import { api } from '$lib/api/client';
  import type { Instance } from '$lib/types';

  const token = $page.params.token ?? '';
  let password = $state('password');

  onMount(async () => {
    const res = await api.get<Instance[]>('/instances');
    if (res.data) {
      const inst = res.data.find(i => i.vnc_token === token);
      if (inst?.vnc_password) password = inst.vnc_password;
    }
  });
</script>

<VncSession {token} {password} />
