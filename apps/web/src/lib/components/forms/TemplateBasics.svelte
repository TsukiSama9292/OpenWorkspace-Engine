<script lang="ts">
  import type { RemoteType, TemplateVisibility } from '$lib/types';

  interface Props {
    name: string;
    description: string;
    image: string;
    remoteType: RemoteType;
    visibility: TemplateVisibility;
  }

  let { name = $bindable(), description = $bindable(), image = $bindable(), remoteType = $bindable(), visibility = $bindable() }: Props = $props();

  const inputClass = 'px-3 py-2 bg-black/40 border border-white/10 rounded text-white placeholder:text-zinc-500 focus:outline-none focus:border-indigo-400 focus:ring-2 focus:ring-indigo-500/25 w-full';
  const labelClass = 'flex flex-col gap-1';
  const spanClass = 'text-sm text-zinc-400';
  const hintClass = 'text-xs text-zinc-500';
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
  <span class={spanClass}>Visibility *</span>
  <select bind:value={visibility} class={inputClass}>
    <option value="private">Private — only the whitelisted groups can use</option>
    <option value="public">Public — every group can use</option>
    <option value="hidden">Hidden — nobody can use (kept for reference)</option>
  </select>
  <span class={hintClass}>Visibility only affects who can start instances; the template stays visible in the catalogue either way.</span>
</label>

<label class={labelClass}>
  <span class={spanClass}>Image *</span>
  <input type="text" bind:value={image} placeholder="tsukisama9292/ow-kasmvnc-ubuntu-dini:jammy" class={inputClass} />
</label>
