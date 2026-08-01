<script lang="ts">
  import { keepTimePolicyLine } from '$lib/countdown/countdown';
  import type { TimeoutAction } from '$lib/types';

  let {
    keepTimeSeconds = undefined,
    keepTimeAction = undefined
  }: {
    keepTimeSeconds?: number | null;
    keepTimeAction?: TimeoutAction | null;
  } = $props();

  const line = $derived(keepTimePolicyLine(keepTimeSeconds, keepTimeAction));
</script>

{#if line}
  <div class="info-item">
    <span class="info-label">Keep Time</span>
    <span class="info-value">{line}</span>
  </div>
{/if}

<style>
  .info-item { display: flex; flex-direction: column; gap: 2px; }
  .info-label { font-size: 0.7rem; color: #71717a; text-transform: uppercase; letter-spacing: 0.05em; }
  .info-value { font-size: 0.85rem; color: #d4d4d8; }
</style>
