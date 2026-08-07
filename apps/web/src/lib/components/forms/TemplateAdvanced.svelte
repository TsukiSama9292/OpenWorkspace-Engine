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
    dockerInInstance: boolean;
    bandwidthUpMbps: number;
    bandwidthDownMbps: number;
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
    dockerInInstance = $bindable(),
    bandwidthUpMbps = $bindable(),
    bandwidthDownMbps = $bindable(),
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
    <label class="col-span-2 flex flex-col gap-1">
      <span class={spanClass}>Runtime</span>
      <select data-testid="runtime-select" bind:value={containerRuntime} class={inputClass}>
        <option value="runc">runc (OCI default)</option>
        <option value="runsc">runsc (gVisor)</option>
      </select>
      <span class="text-xs text-surface-400">runc (OCI default): fast, GPU-compatible (default). runsc (gVisor): sandboxed, slower.</span>
    </label>
  </div>

  <div class="flex flex-col gap-2 p-3 border border-surface-800 rounded-lg">
    <label class="flex items-center gap-3 cursor-pointer">
      <input
        type="checkbox"
        data-testid="dini-toggle"
        bind:checked={dockerInInstance}
        class="w-4 h-4 accent-indigo-500"
      />
      <span class="text-sm text-surface-300">Run Docker inside the instance</span>
    </label>
    {#if dockerInInstance}
      {#if containerRuntime === 'runsc'}
        <p class="text-sm text-emerald-400 m-0" data-testid="dini-safe">Sandboxed via gVisor</p>
      {:else}
        <p class="text-sm text-red-400 m-0" data-testid="dini-warning">Runs with full host privileges. Switch to the runsc (gVisor) runtime to sandbox it.</p>
      {/if}
    {/if}
  </div>

  <h2 class="text-base font-semibold text-surface-200">Network Bandwidth (Mbps)</h2>
  <p class="text-sm text-zinc-500 -mt-2">0 = unlimited. Applied per container via kernel traffic shaping (tc/HTB) on the host.</p>
  <div class="grid grid-cols-2 gap-3">
    <label class={labelClass}>
      <span class={spanClass}>Upload Limit (Mbps)</span>
      <input type="number" min="0" step="1" bind:value={bandwidthUpMbps} placeholder="0" class={inputClass} />
    </label>
    <label class={labelClass}>
      <span class={spanClass}>Download Limit (Mbps)</span>
      <input type="number" min="0" step="1" bind:value={bandwidthDownMbps} placeholder="0" class={inputClass} />
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
