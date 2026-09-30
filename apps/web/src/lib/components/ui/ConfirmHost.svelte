<script lang="ts">
  import ConfirmDialog from './ConfirmDialog.svelte';
  import type { PendingConfirm } from './confirm';

  let { request = $bindable<PendingConfirm | null>(null) }: { request?: PendingConfirm | null } =
    $props();

  function run() {
    const current = request;
    request = null;
    current?.onConfirm();
  }
</script>

<ConfirmDialog
  open={request !== null}
  title={request?.title ?? ''}
  body={request?.body ?? ''}
  confirmLabel={request?.confirmLabel ?? 'Confirm'}
  danger={request?.danger ?? false}
  onconfirm={run}
  oncancel={() => (request = null)}
/>
