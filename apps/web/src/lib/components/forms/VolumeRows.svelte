<script lang="ts">
  import type { VolumeMapping } from '$lib/utils/format';
  import { createEmptyVolume } from '$lib/utils/format';

  interface Props {
    volumeMappings: VolumeMapping[];
  }

  let { volumeMappings = $bindable() }: Props = $props();

  function add() { volumeMappings = [...volumeMappings, createEmptyVolume()]; }
  function remove(i: number) { volumeMappings = volumeMappings.filter((_, idx) => idx !== i); }
</script>

<h2 class="text-base font-semibold text-surface-200">Volume Mappings</h2>
{#each volumeMappings as _, i}
  <div class="flex gap-2 items-center">
    <input type="text" bind:value={volumeMappings[i].host} placeholder="/host/path" class="flex-1 px-3 py-2 bg-black/40 border border-white/10 rounded text-white placeholder:text-zinc-500 focus:outline-none focus:border-indigo-400 focus:ring-2 focus:ring-indigo-500/25" />
    <input type="text" bind:value={volumeMappings[i].container} placeholder="/container/path" class="flex-1 px-3 py-2 bg-black/40 border border-white/10 rounded text-white placeholder:text-zinc-500 focus:outline-none focus:border-indigo-400 focus:ring-2 focus:ring-indigo-500/25" />
    <button type="button" class="px-2 py-1 text-surface-400 hover:text-error-500 bg-transparent border-none cursor-pointer text-lg" onclick={() => remove(i)}>×</button>
  </div>
{/each}
<button type="button" class="text-sm text-primary-500 hover:text-primary-600 bg-transparent border-none cursor-pointer text-left p-0" onclick={add}>+ Add Volume</button>
