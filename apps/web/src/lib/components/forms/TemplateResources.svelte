<script lang="ts">
  interface Props {
    cores: number;
    ramGb: number;
    gpuCount: number;
    dockerRegistry: string;
    persistentStoragePath: string;
  }

  let {
    cores = $bindable(),
    ramGb = $bindable(),
    gpuCount = $bindable(),
    dockerRegistry = $bindable(),
    persistentStoragePath = $bindable()
  }: Props = $props();

  const STORAGE_HINT = '/data/persistent/{template_name}/{user_id}';
  const inputClass = 'px-3 py-2 bg-black/40 border border-white/10 rounded text-white placeholder:text-zinc-500 focus:outline-none focus:border-indigo-400 focus:ring-2 focus:ring-indigo-500/25 w-full';
  const labelClass = 'flex flex-col gap-1';
  const spanClass = 'text-sm text-zinc-400';
</script>

<div class="grid grid-cols-3 gap-3">
  <label class={labelClass}>
    <span class={spanClass}>CPU Cores *</span>
    <input type="number" bind:value={cores} min="1" max="64" class={inputClass} />
  </label>
  <label class={labelClass}>
    <span class={spanClass}>RAM (GB) *</span>
    <input type="number" bind:value={ramGb} min="1" max="256" class={inputClass} />
  </label>
  <label class={labelClass}>
    <span class={spanClass}>GPU</span>
    <input type="number" bind:value={gpuCount} min="0" max="8" class={inputClass} />
  </label>
</div>

<label class={labelClass}>
  <span class={spanClass}>Docker Registry</span>
  <input type="text" bind:value={dockerRegistry} placeholder="https://index.docker.io/v1/" class={inputClass} />
</label>

<label class={labelClass}>
  <span class={spanClass}>Persistent Storage Path</span>
  <input type="text" bind:value={persistentStoragePath} placeholder={STORAGE_HINT} class={inputClass} />
  <span class="text-xs text-surface-400">Template variables: {'{'}template_name{'}'}, {'{'}user_id{'}'}</span>
</label>
