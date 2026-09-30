<script lang="ts">
  import UnlimitedInput from './UnlimitedInput.svelte';
  import TriStateInput from './TriStateInput.svelte';
  import { triStateFromValue, valueFromTriState } from '$lib/tri-state';
  import type { TriState } from '$lib/tri-state';
  import type { TimeoutAction } from '$lib/templates/template-form';

  interface Props {
    cores: number;
    ramGb: number;
    gpuCount: number;
    dockerRegistry: string;
    persistentStoragePath: string;
    maxRunSeconds: number;
    timeoutAction: TimeoutAction;
    keepTimeSeconds: number;
    keepTimeAction: TimeoutAction;
  }

  let {
    cores = $bindable(),
    ramGb = $bindable(),
    gpuCount = $bindable(),
    dockerRegistry = $bindable(),
    persistentStoragePath = $bindable(),
    maxRunSeconds = $bindable(),
    timeoutAction = $bindable(),
    keepTimeSeconds = $bindable(),
    keepTimeAction = $bindable(),
  }: Props = $props();

  const STORAGE_HINT = '/data/persistent';
  const inputClass = 'px-3 py-2 bg-black/40 border border-white/10 rounded text-white placeholder:text-zinc-500 focus:outline-none focus:border-indigo-400 focus:ring-2 focus:ring-indigo-500/25 w-full';
  const labelClass = 'flex flex-col gap-1';
  const spanClass = 'text-sm text-zinc-400';

  let gpuTri = $state<TriState>(triStateFromValue(gpuCount, 0));

  $effect(() => {
    gpuCount = valueFromTriState(gpuTri);
  });
</script>

<div class="grid grid-cols-3 gap-3">
  <UnlimitedInput label="CPU Cores *" bind:value={cores} unit="cores" placeholder="e.g. 8" />
  <UnlimitedInput label="RAM (GB) *" bind:value={ramGb} unit="GB" placeholder="e.g. 16" />
  <TriStateInput label="GPU" bind:value={gpuTri} unit="GPUs" placeholder="e.g. 2" />
</div>

{#if gpuTri.mode === 'unlimited'}
  <span class="text-xs text-amber-400 -mt-1">Unlimited GPU allocates all host GPUs to the instance.</span>
{/if}

<label class={labelClass}>
  <span class={spanClass}>Docker Registry</span>
  <input type="text" bind:value={dockerRegistry} placeholder="https://index.docker.io/v1/" class={inputClass} />
</label>

<label class={labelClass}>
  <span class={spanClass}>Persistent Root Directory</span>
  <input type="text" bind:value={persistentStoragePath} placeholder={STORAGE_HINT} class={inputClass} />
  <span class="text-xs text-surface-400">Host root directory; per-instance subfolders are appended by the API</span>
</label>

<div class="grid grid-cols-2 gap-3">
  <UnlimitedInput label="Usage Limit (seconds)" bind:value={maxRunSeconds} unit="seconds" min={60} placeholder="e.g. 3600 (1 hour)" />
  {#if maxRunSeconds > 0}
    <label class={labelClass}>
      <span class={spanClass}>Timeout Action</span>
      <select class={inputClass} bind:value={timeoutAction}>
        <option value="remove">remove</option>
        <option value="stop">stop</option>
        <option value="pause">pause</option>
      </select>
    </label>
  {/if}
</div>

<div class="grid grid-cols-2 gap-3">
  <UnlimitedInput label="Idle Keep Time (seconds)" bind:value={keepTimeSeconds} unit="seconds" min={60} placeholder="e.g. 3600 (1 hour)" />
  {#if keepTimeSeconds > 0}
    <label class={labelClass}>
      <span class={spanClass}>Keep Time Action</span>
      <select class={inputClass} bind:value={keepTimeAction}>
        <option value="remove">remove</option>
        <option value="stop">stop</option>
        <option value="pause">pause</option>
      </select>
    </label>
  {/if}
</div>
