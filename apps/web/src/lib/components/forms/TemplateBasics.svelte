<script lang="ts">
  import type { RemoteType } from '$lib/types';

  interface Props {
    name: string;
    description: string;
    image: string;
    remoteType: RemoteType;
  }

  let { name = $bindable(), description = $bindable(), image = $bindable(), remoteType = $bindable() }: Props = $props();

  const inputClass = 'px-3 py-2 bg-black/40 border border-white/10 rounded text-white placeholder:text-zinc-500 focus:outline-none focus:border-indigo-400 focus:ring-2 focus:ring-indigo-500/25 w-full';
  const labelClass = 'flex flex-col gap-1';
  const spanClass = 'text-sm text-zinc-400';
</script>

<label class={labelClass}>
  <span class={spanClass}>Name *</span>
  <input type="text" bind:value={name} placeholder="e.g. AI Lab" required class={inputClass} />
</label>

<label class={labelClass}>
  <span class={spanClass}>Description</span>
  <input type="text" bind:value={description} placeholder="Optional description" class={inputClass} />
</label>

<label class={labelClass}>
  <span class={spanClass}>Remote Type *</span>
  <select bind:value={remoteType} class={inputClass}>
    <option value="kasmvnc">KasmVNC (Desktop)</option>
    <option value="ttyd">ttyd (Terminal)</option>
    <option value="jupyter">Jupyter Lab (Python)</option>
  </select>
</label>

<label class={labelClass}>
  <span class={spanClass}>Image *</span>
  <input type="text" bind:value={image} placeholder="tsukisama9292/ow-kasmvnc-ubuntu:jammy" class={inputClass} />
</label>
