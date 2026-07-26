<script lang="ts">
  import type { EnvVar } from '$lib/utils/format';
  import { createEmptyEnvVar } from '$lib/utils/format';

  interface Props {
    envVars: EnvVar[];
  }

  let { envVars = $bindable() }: Props = $props();

  function add() { envVars = [...envVars, createEmptyEnvVar()]; }
  function remove(i: number) { envVars = envVars.filter((_, idx) => idx !== i); }
</script>

<h3 class="text-sm font-semibold text-surface-700">Environment Variables</h3>
{#each envVars as _, i}
  <div class="flex gap-2 items-center">
    <input type="text" bind:value={envVars[i].key} placeholder="KEY" class="flex-1 px-3 py-2 border border-surface-300 rounded bg-white text-surface-800 focus:outline-none focus:border-primary-500" />
    <input type="text" bind:value={envVars[i].value} placeholder="value" class="flex-1 px-3 py-2 border border-surface-300 rounded bg-white text-surface-800 focus:outline-none focus:border-primary-500" />
    <button type="button" class="px-2 py-1 text-surface-400 hover:text-error-500 bg-transparent border-none cursor-pointer text-lg" onclick={() => remove(i)}>×</button>
  </div>
{/each}
<button type="button" class="text-sm text-primary-500 hover:text-primary-600 bg-transparent border-none cursor-pointer text-left p-0" onclick={add}>+ Add Variable</button>
