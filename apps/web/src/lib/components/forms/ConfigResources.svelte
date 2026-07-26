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

  const STORAGE_HINT = '/data/persistent/{workspace_name}/{user_id}';
  const inputClass = 'px-3 py-2 border border-surface-300 rounded bg-surface-50 text-surface-800 focus:outline-none focus:border-primary-500 w-full';
  const labelClass = 'flex flex-col gap-1';
  const spanClass = 'text-sm text-surface-600';
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
  <span class="text-xs text-surface-400">Template variables: {'{'}workspace_name{'}'}, {'{'}user_id{'}'}</span>
</label>
