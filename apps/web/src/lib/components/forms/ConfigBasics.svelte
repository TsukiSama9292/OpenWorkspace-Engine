<script lang="ts">
  interface Props {
    name: string;
    description: string;
    image: string;
  }

  let { name = $bindable(), description = $bindable(), image = $bindable() }: Props = $props();

  const inputClass = 'px-3 py-2 border border-surface-300 rounded bg-surface-50 text-surface-800 focus:outline-none focus:border-primary-500 w-full';
  const labelClass = 'flex flex-col gap-1';
  const spanClass = 'text-sm text-surface-600';
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
  <span class={spanClass}>Image *</span>
  <input type="text" bind:value={image} placeholder="kasmweb/desktop:1.19.0-rolling-daily" class={inputClass} />
</label>
