<script lang="ts">
  import Modal from '$lib/components/ui/Modal.svelte';
  import Button from '$lib/components/ui/Button.svelte';
  import type { PreflightRejection } from '$lib/types';
  import { preflightMessage } from '$lib/preflight';

  interface Props {
    error: string;
    rejection: PreflightRejection | null;
    onclose?: () => void;
  }

  let { error, rejection, onclose }: Props = $props();
</script>

{#if rejection}
  <Modal open title="Launch rejected" width="28rem" {onclose}>
    <div class="flex flex-col gap-3" data-testid="rejection-notice">
      <p class="text-sm leading-relaxed text-surface-200">{preflightMessage(rejection, error)}</p>
      <div class="flex justify-end">
        <Button variant="primary" onclick={onclose}>OK</Button>
      </div>
    </div>
  </Modal>
{/if}
