<script lang="ts">
  import { page } from '$app/stores';
  import VncSession from '$lib/components/vnc/VncSession.svelte';
</script>

<VncSession token={$page.params.token ?? ''} />
