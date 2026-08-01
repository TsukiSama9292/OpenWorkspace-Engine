<script lang="ts">
  import type { TimeoutAction } from '$lib/templates/template-form';

  interface Props {
    cores: number;
    ramGb: number;
    gpuCount: number;
    dockerRegistry: string;
    persistentStoragePath: string;
    maxRunSeconds: number | null;
    timeoutAction: TimeoutAction;
    keepTimeSeconds: number | null;
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
    keepTimeAction = $bindable()
  }: Props = $props();

  const STORAGE_HINT = '/data/persistent';
  const inputClass = 'px-3 py-2 bg-black/40 border border-white/10 rounded text-white placeholder:text-zinc-500 focus:outline-none focus:border-indigo-400 focus:ring-2 focus:ring-indigo-500/25 w-full';
  const labelClass = 'flex flex-col gap-1';
  const spanClass = 'text-sm text-zinc-400';

  const DEFAULT_SECONDS = 3600;

  let usageEnabled = $state(maxRunSeconds !== null);
  let keepTimeEnabled = $state(keepTimeSeconds !== null);
  let maxRunSecondsInput = $state(String(maxRunSeconds ?? DEFAULT_SECONDS));
  let keepTimeSecondsInput = $state(String(keepTimeSeconds ?? DEFAULT_SECONDS));

  function parseSeconds(raw: string | null): number | null {
    if (raw === null || raw.trim() === '') return null;
    const n = Number.parseInt(raw, 10);
    return Number.isFinite(n) && n > 0 ? n : null;
  }

  $effect(() => {
    if (maxRunSeconds !== null) {
      usageEnabled = true;
      maxRunSecondsInput = String(maxRunSeconds);
    } else {
      usageEnabled = false;
    }
  });

  $effect(() => {
    if (keepTimeSeconds !== null) {
      keepTimeEnabled = true;
      keepTimeSecondsInput = String(keepTimeSeconds);
    } else {
      keepTimeEnabled = false;
    }
  });

  function onUsageLimitEnabledChange(event: Event) {
    const enabled = (event.currentTarget as HTMLInputElement).checked;
    maxRunSeconds = enabled ? parseSeconds(maxRunSecondsInput) ?? DEFAULT_SECONDS : null;
  }

  function onMaxRunSecondsInput() {
    if (!usageEnabled) return;
    const n = parseSeconds(maxRunSecondsInput);
    if (n !== null) maxRunSeconds = n;
  }

  function onKeepTimeEnabledChange(event: Event) {
    const enabled = (event.currentTarget as HTMLInputElement).checked;
    keepTimeSeconds = enabled ? parseSeconds(keepTimeSecondsInput) ?? DEFAULT_SECONDS : null;
  }

  function onKeepTimeSecondsInput() {
    if (!keepTimeEnabled) return;
    const n = parseSeconds(keepTimeSecondsInput);
    if (n !== null) keepTimeSeconds = n;
  }
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
  <span class={spanClass}>Persistent Root Directory</span>
  <input type="text" bind:value={persistentStoragePath} placeholder={STORAGE_HINT} class={inputClass} />
  <span class="text-xs text-surface-400">Host root directory; per-instance subfolders are appended by the API</span>
</label>

<div class="grid grid-cols-2 gap-3">
  <label class={labelClass}>
    <span class={spanClass}>Usage Limit (seconds)</span>
    <div class="flex items-center gap-2">
      <input
        type="checkbox"
        checked={usageEnabled}
        onchange={onUsageLimitEnabledChange}
        class="accent-indigo-500 shrink-0"
      />
      <span class="text-sm text-zinc-400">Enabled</span>
      {#if usageEnabled}
        <input
          type="number"
          value={maxRunSecondsInput}
          min="60"
          step="60"
          oninput={(e) => {
            maxRunSecondsInput = (e.currentTarget as HTMLInputElement).value;
            onMaxRunSecondsInput();
          }}
          class={inputClass}
          placeholder="e.g. 3600 (1 hour)"
        />
      {/if}
    </div>
  </label>
  {#if usageEnabled}
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
  <label class={labelClass}>
    <span class={spanClass}>Idle Keep Time (seconds)</span>
    <div class="flex items-center gap-2">
      <input
        type="checkbox"
        checked={keepTimeEnabled}
        onchange={onKeepTimeEnabledChange}
        class="accent-indigo-500 shrink-0"
      />
      <span class="text-sm text-zinc-400">Enabled</span>
      {#if keepTimeEnabled}
        <input
          type="number"
          value={keepTimeSecondsInput}
          min="60"
          step="60"
          oninput={(e) => {
            keepTimeSecondsInput = (e.currentTarget as HTMLInputElement).value;
            onKeepTimeSecondsInput();
          }}
          class={inputClass}
          placeholder="e.g. 3600 (1 hour)"
        />
      {/if}
    </div>
  </label>
  {#if keepTimeEnabled}
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
