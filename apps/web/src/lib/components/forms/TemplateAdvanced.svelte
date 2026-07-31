<script lang="ts">
  import EnvVarRows from './EnvVarRows.svelte';
  import VolumeRows from './VolumeRows.svelte';
  import type { EnvVar, VolumeMapping } from '$lib/utils/format';

  interface Props {
    hostname: string;
    dns: string;
    shmSize: string;
    networkMode: string;
    containerRuntime: string;
    envVars: EnvVar[];
    execCommand: string;
    volumeMappings: VolumeMapping[];
  }

  let {
    hostname = $bindable(),
    dns = $bindable(),
    shmSize = $bindable(),
    networkMode = $bindable(),
    containerRuntime = $bindable(),
    envVars = $bindable(),
    execCommand = $bindable(),
    volumeMappings = $bindable()
  }: Props = $props();

  const inputClass = 'px-3 py-2 bg-black/40 border border-white/10 rounded text-white placeholder:text-zinc-500 focus:outline-none focus:border-indigo-400 focus:ring-2 focus:ring-indigo-500/25 w-full';
  const labelClass = 'flex flex-col gap-1';
  const spanClass = 'text-sm text-zinc-400';
</script>

<div class="p-4 border border-surface-800 rounded-lg bg-transparent flex flex-col gap-4">
  <h2 class="text-base font-semibold text-surface-200">Run Config</h2>
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
    <label class={labelClass}>
      <span class={spanClass}>Runtime</span>
      <select bind:value={containerRuntime} class={inputClass}>
        <option value="">Default</option>
        <option value="runsc">runsc (gVisor)</option>
      </select>
    </label>
  </div>

  <EnvVarRows bind:envVars />

  <h2 class="text-base font-semibold text-surface-200">Exec Config</h2>
  <label class={labelClass}>
    <span class={spanClass}>Post-start Command</span>
    <input type="text" bind:value={execCommand} placeholder="bash -c 'echo hello'" class={inputClass} />
  </label>

  <VolumeRows bind:volumeMappings />
</div>
