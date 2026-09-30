<script lang="ts">
  import { tick } from 'svelte';
  import type { Snippet } from 'svelte';
  import { wrapTabFocus } from '$lib/utils/focus';

  interface Props {
    open: boolean;
    title?: string;
    width?: string;
    children: Snippet;
    onclose?: () => void;
  }

  let { open = $bindable(false), title = '', width = '24rem', children, onclose }: Props = $props();

  let dialogEl = $state<HTMLDivElement | null>(null);

  // Keep focus inside the dialog while it lives and hand focus back to the
  // element that held it before, whichever way the dialog unmounts.
  $effect(() => {
    if (!open || !dialogEl) return;
    const previouslyFocused = document.activeElement;
    const dialog = dialogEl;
    tick().then(() => dialog.focus());
    return () => {
      if (previouslyFocused instanceof HTMLElement) previouslyFocused.focus();
    };
  });

  function handleOverlayClick(e: MouseEvent) {
    if (e.target !== e.currentTarget) return;
    open = false;
    onclose?.();
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      open = false;
      onclose?.();
      return;
    }
    if (dialogEl) wrapTabFocus(dialogEl, e);
  }
</script>

{#if open}
  <div
    bind:this={dialogEl}
    class="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm"
    onclick={handleOverlayClick}
    onkeydown={handleKeydown}
    role="dialog"
    aria-modal="true"
    aria-label={title || 'Dialog'}
    tabindex="-1"
  >
    <div
      class="bg-surface-100-900 border border-surface-300-700 rounded-lg shadow-xl w-full max-w-[90vw] max-h-[88vh] overflow-y-auto"
      style="max-width: {width}"
      role="document"
    >
      {#if title}
        <div class="flex items-center justify-between px-4 py-3 border-b border-surface-300-700">
          <h3 class="text-sm font-semibold">{title}</h3>
          <button
            class="rounded px-2 py-0.5 text-lg leading-none text-surface-500 hover:text-surface-200 hover:bg-surface-200-800"
            aria-label="Close dialog"
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
