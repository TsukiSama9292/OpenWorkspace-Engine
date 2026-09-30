<script lang="ts">
  import Modal from '$lib/components/ui/Modal.svelte';
  import Button from '$lib/components/ui/Button.svelte';

  interface Props {
    open: boolean;
    title: string;
    body: string;
    confirmLabel?: string;
    danger?: boolean;
    onconfirm?: () => void;
    oncancel?: () => void;
  }

  let {
    open,
    title,
    body,
    confirmLabel = 'Confirm',
    danger = false,
    onconfirm,
    oncancel
  }: Props = $props();
</script>

{#if open}
  <Modal open {title} onclose={oncancel}>
    <div class="flex flex-col gap-3" data-testid="confirm-dialog">
      <p class="text-sm leading-relaxed text-surface-200">{body}</p>
      <div class="flex justify-end gap-2">
        <Button variant="secondary" onclick={oncancel}>Cancel</Button>
        <Button variant={danger ? 'error' : 'primary'} onclick={onconfirm}>{confirmLabel}</Button>
      </div>
    </div>
  </Modal>
{/if}
