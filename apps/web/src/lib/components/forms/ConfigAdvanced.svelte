<script lang="ts">
  import EnvVarRows from './EnvVarRows.svelte';
  import VolumeRows from './VolumeRows.svelte';
  import type { EnvVar, VolumeMapping } from '$lib/utils/format';

  interface Props {
    hostname: string;
    dns: string;
    shmSize: string;
    networkMode: string;
    envVars: EnvVar[];
    execCommand: string;
    volumeMappings: VolumeMapping[];
  }

  let {
    hostname = $bindable(),
    dns = $bindable(),
    shmSize = $bindable(),
    networkMode = $bindable(),
    envVars = $bindable(),
    execCommand = $bindable(),
    volumeMappings = $bindable()
  }: Props = $props();

  const inputClass = 'px-3 py-2 border border-surface-300 rounded bg-white text-surface-800 focus:outline-none focus:border-primary-500 w-full';
  const labelClass = 'flex flex-col gap-1';
  const spanClass = 'text-sm text-surface-600';
</script>

<div class="p-4 border border-surface-300 rounded-lg bg-surface-50 flex flex-col gap-4">
  <h2 class="text-base font-semibold text-surface-700">Run Config</h2>
  <label class={labelClass}>
    <span class={spanClass}>Hostname</span>
    <input type="text" bind:value={hostname} placeholder="kasm-ubuntu" class={inputClass} />
  </label>
  <label class={labelClass}>
    <span class={spanClass}>DNS (comma-separated)</span>
    <input type="text" bind:value={dns} placeholder="1.1.1.1, 8.8.8.8" class={inputClass} />
  </label>
  <div class="grid grid-cols-2 gap-3">
    <label class={labelClass}>
      <span class={spanClass}>SHM Size (bytes)</span>
      <input type="number" bind:value={shmSize} placeholder="67108864" class={inputClass} />
    </label>
    <label class={labelClass}>
      <span class={spanClass}>Network Mode</span>
      <input type="text" bind:value={networkMode} placeholder="bridge" class={inputClass} />
    </label>
  </div>

  <EnvVarRows bind:envVars />

  <h2 class="text-base font-semibold text-surface-700">Exec Config</h2>
  <label class={labelClass}>
    <span class={spanClass}>Post-start Command</span>
    <input type="text" bind:value={execCommand} placeholder="bash -c 'echo hello'" class={inputClass} />
  </label>

  <VolumeRows bind:volumeMappings />
</div>
