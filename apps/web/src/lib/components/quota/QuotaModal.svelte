<script lang="ts">
  import Modal from '$lib/components/ui/Modal.svelte';
  import Button from '$lib/components/ui/Button.svelte';
  import type { QuotaPayload } from '$lib/types';
  import { quotaMessage } from '$lib/quota';

  interface Props {
    error: string;
    quota: QuotaPayload | null;
    onclose?: () => void;
  }

  let { error, quota, onclose }: Props = $props();
</script>

{#if quota}
  <Modal open title="資源配額已達上限" width="28rem" {onclose}>
    <div class="flex flex-col gap-3" data-testid="quota-notice">
      <p class="text-sm leading-relaxed text-surface-200">{quotaMessage(quota)}</p>
      <p class="text-xs text-surface-500 break-words">{error}</p>
      <div class="flex justify-end">
        <Button variant="primary" onclick={onclose}>了解</Button>
      </div>
    </div>
  </Modal>
{/if}
