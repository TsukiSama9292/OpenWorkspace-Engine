<script lang="ts">
  import Modal from '$lib/components/ui/Modal.svelte';
  import type { VncSettings } from '$lib/types';

  interface Props {
    open: boolean;
    settings: VncSettings;
  }

  let { open = $bindable(false), settings = $bindable() }: Props = $props();
</script>

<Modal bind:open title="Settings" width="360px">
  <div class="flex flex-col gap-4">
    <div class="flex flex-col gap-1.5">
      <!-- svelte-ignore a11y_label_has_associated_control -->
      <label class="text-[13px] text-surface-500">Quality</label>
      <div class="flex items-center gap-2.5">
        <input type="range" min="1" max="10" bind:value={settings.quality} class="flex-1 h-1 rounded appearance-none bg-surface-300 accent-primary-500" />
        <span class="text-xs text-primary-500 font-mono min-w-[20px] text-right">{settings.quality || 5}</span>
      </div>
    </div>

    <div class="flex flex-col gap-1.5">
      <!-- svelte-ignore a11y_label_has_associated_control -->
      <label class="text-[13px] text-surface-500">Compression</label>
      <div class="flex items-center gap-2.5">
        <input type="range" min="1" max="9" bind:value={settings.compression} class="flex-1 h-1 rounded appearance-none bg-surface-300 accent-primary-500" />
        <span class="text-xs text-primary-500 font-mono min-w-[20px] text-right">{settings.compression || 5}</span>
      </div>
    </div>

    <div class="flex flex-col gap-1.5">
      <label class="text-[13px] text-surface-500 flex items-center gap-2 cursor-pointer">
        <input type="checkbox" bind:checked={settings.viewOnly} class="accent-primary-500 w-3.5 h-3.5" />
        View only (no input)
      </label>
    </div>

    <div class="flex flex-col gap-1.5">
      <label class="text-[13px] text-surface-500 flex items-center gap-2 cursor-pointer">
        <input type="checkbox" bind:checked={settings.clipboardSync} class="accent-primary-500 w-3.5 h-3.5" />
        Sync clipboard
      </label>
    </div>
  </div>
</Modal>
