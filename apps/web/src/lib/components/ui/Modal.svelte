<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    open: boolean;
    title?: string;
    width?: string;
    children: Snippet;
    onclose?: () => void;
  }

  let { open = $bindable(false), title = '', width = '24rem', children, onclose }: Props = $props();

  function handleOverlayClick() {
    open = false;
    onclose?.();
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      open = false;
      onclose?.();
    }
  }

  function stopPropagation(e: Event) {
    e.stopPropagation();
  }
</script>

{#if open}
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    class="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm"
    onclick={handleOverlayClick}
    onkeydown={handleKeydown}
    role="dialog"
    tabindex="-1"
  >
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <div
      class="bg-surface-100-900 border border-surface-300-700 rounded-lg shadow-xl w-full max-w-[90vw] overflow-hidden"
      style="max-width: {width}"
      onclick={stopPropagation}
      onkeydown={stopPropagation}
      role="document"
    >
      {#if title}
        <div class="flex items-center justify-between px-4 py-3 border-b border-surface-300-700">
          <h3 class="text-sm font-semibold">{title}</h3>
          <button
            class="btn-icon text-surface-500 hover:text-surface-200 hover:bg-surface-200-800 rounded"
            onclick={handleOverlayClick}
          >
            &times;
          </button>
        </div>
      {/if}
      <div class="p-4">
        {@render children()}
      </div>
    </div>
  </div>
{/if}
