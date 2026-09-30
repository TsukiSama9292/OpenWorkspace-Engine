<script lang="ts">
  import Modal from '$lib/components/ui/Modal.svelte';
  import Button from '$lib/components/ui/Button.svelte';

  interface Props {
    open: boolean;
    onSend?: (text: string) => void;
  }

  let { open = $bindable(false), onSend }: Props = $props();
  let clipboardText = $state('');
  let syncStatus = $state('');
  let syncTimeout = $state<ReturnType<typeof setTimeout> | null>(null);

  async function handleSend() {
    if (onSend && clipboardText.trim()) {
      onSend(clipboardText);
      clipboardText = '';
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
      handleSend();
    }
  }

  async function handlePaste() {
    try {
      const text = await navigator.clipboard.readText();
      clipboardText = text;
      showSyncStatus('Pasted from clipboard');
    } catch {
      showSyncStatus('Clipboard read blocked - use Ctrl+V');
    }
  }

  async function handleCopy() {
    try {
      await navigator.clipboard.writeText(clipboardText);
      showSyncStatus('Copied to clipboard');
    } catch {
      showSyncStatus('Clipboard write blocked');
    }
  }

  function showSyncStatus(message: string) {
    syncStatus = message;
    if (syncTimeout) clearTimeout(syncTimeout);
    syncTimeout = setTimeout(() => {
      syncStatus = '';
    }, 2000);
  }
</script>

<Modal bind:open title="Clipboard" width="420px">
  <div class="flex flex-col gap-3">
    <div class="text-[11px] text-surface-500">
      <kbd class="px-1 py-0.5 rounded bg-primary-500/15 text-[10px] font-mono">Ctrl+V</kbd> to paste locally &middot;
      <kbd class="px-1 py-0.5 rounded bg-primary-500/15 text-[10px] font-mono">Ctrl+Enter</kbd> to send to remote
    </div>
    <textarea
      bind:value={clipboardText}
      onkeydown={handleKeydown}
      placeholder="Paste or type text to send to remote..."
      rows="6"
      class="w-full rounded p-2.5 text-[13px] font-mono resize-y min-h-[120px] box-border bg-black/40 border border-surface-700 text-surface-100 focus:outline-none focus:border-primary-400 placeholder:text-surface-500"
    ></textarea>
    {#if syncStatus}
      <div class="text-[11px] text-primary-500">{syncStatus}</div>
    {/if}
    <div class="flex gap-2 justify-end">
      <Button variant="secondary" onclick={() => void handlePaste()}>Read from clipboard</Button>
      <Button variant="secondary" onclick={() => void handleCopy()}>Copy to clipboard</Button>
      <Button variant="primary" onclick={() => void handleSend()}>Send to remote</Button>
    </div>
  </div>
</Modal>
