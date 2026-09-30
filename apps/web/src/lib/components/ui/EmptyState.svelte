<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    mark?: string;
    message: string;
    children?: Snippet;
  }

  let { mark = '/icons/generic.svg', message, children }: Props = $props();
</script>

<div class="empty-state">
  <img class="empty-mark" src={mark} alt="" aria-hidden="true" />
  <div>
    <p class="empty-text">{message}</p>
    {@render children?.()}
  </div>
</div>
