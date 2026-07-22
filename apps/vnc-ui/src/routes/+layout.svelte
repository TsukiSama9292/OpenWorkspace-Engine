<script>
  import { onMount } from 'svelte';
  import { theme } from '$lib/stores/theme.js';

  let { children } = $props();

  onMount(() => {
    theme.init();
  });
</script>

{@render children()}
