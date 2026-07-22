<script>
  import { theme } from '$lib/stores/theme.js';

  let { open = $bindable(false), onSend } = $props();
  let clipboardText = $state('');
  let textarea = $state(null);
  let syncStatus = $state('');
  let syncTimeout = $state(null);

  async function handleSend() {
    if (onSend && clipboardText.trim()) {
      onSend(clipboardText);
      clipboardText = '';
    }
  }

  function handleKeydown(e) {
    if (e.key === 'Escape') {
      open = false;
    }
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
      handleSend();
    }
  }

  async function handlePaste() {
    try {
      const text = await navigator.clipboard.readText();
      clipboardText = text;
      showSyncStatus('Pasted from clipboard');
    } catch (e) {
      showSyncStatus('Clipboard read blocked - use Ctrl+V');
    }
  }

  async function handleCopy() {
    try {
      await navigator.clipboard.writeText(clipboardText);
      showSyncStatus('Copied to clipboard');
    } catch (e) {
      showSyncStatus('Clipboard write blocked');
    }
  }

  function showSyncStatus(message) {
    syncStatus = message;
    if (syncTimeout) clearTimeout(syncTimeout);
    syncTimeout = setTimeout(() => {
      syncStatus = '';
    }, 2000);
  }

  function handleOverlayKeydown(e) {
    if (e.key === 'Escape') {
      open = false;
    }
  }
</script>

{#if open}
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div class="clipboard-overlay" onclick={() => open = false} onkeydown={handleOverlayKeydown} role="dialog" tabindex="-1">
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <div class="clipboard-panel" onclick={(e) => e.stopPropagation()} onkeydown={handleKeydown} role="document">
      <div class="panel-header">
        <h3>Clipboard</h3>
        <button class="close-btn" onclick={() => open = false}>×</button>
      </div>
      <div class="panel-body">
        <div class="clipboard-hint">
          <kbd>Ctrl+V</kbd> to paste locally &middot; <kbd>Ctrl+Enter</kbd> to send to remote
        </div>
        <textarea
          bind:this={textarea}
          bind:value={clipboardText}
          placeholder="Paste or type text to send to remote..."
          rows="6"
        ></textarea>
        {#if syncStatus}
          <div class="sync-status">{syncStatus}</div>
        {/if}
        <div class="panel-actions">
          <button class="btn secondary" onclick={handlePaste}>Read from clipboard</button>
          <button class="btn secondary" onclick={handleCopy}>Copy to clipboard</button>
          <button class="btn primary" onclick={handleSend}>Send to remote</button>
        </div>
      </div>
    </div>
  </div>
{/if}

<style>
  .clipboard-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.5);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 200;
    backdrop-filter: blur(2px);
  }

  .clipboard-panel {
    border-radius: 8px;
    width: 420px;
    max-width: 90vw;
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.4);
    transition: background 0.3s ease;
  }

  :global([data-theme="dark"]) .clipboard-panel {
    background: #1e2a3a;
    border: 1px solid #0f3460;
  }

  :global([data-theme="light"]) .clipboard-panel {
    background: #ffffff;
    border: 1px solid #e0e0e0;
  }

  .panel-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 16px;
    transition: border-color 0.3s ease;
  }

  :global([data-theme="dark"]) .panel-header {
    border-bottom: 1px solid #0f3460;
  }

  :global([data-theme="light"]) .panel-header {
    border-bottom: 1px solid #e0e0e0;
  }

  .panel-header h3 {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
    font-family: system-ui, -apple-system, sans-serif;
  }

  :global([data-theme="dark"]) .panel-header h3 {
    color: #e0e0e0;
  }

  :global([data-theme="light"]) .panel-header h3 {
    color: #1a1a2e;
  }

  .close-btn {
    background: none;
    border: none;
    color: #a0a0b0;
    font-size: 20px;
    cursor: pointer;
    padding: 0;
    line-height: 1;
    width: 24px;
    height: 24px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 4px;
    transition: all 0.2s ease;
  }

  .close-btn:hover {
    background: rgba(255, 255, 255, 0.1);
    color: #e0e0e0;
  }

  .panel-body {
    padding: 16px;
  }

  .clipboard-hint {
    font-size: 11px;
    color: #888;
    margin-bottom: 8px;
    font-family: system-ui, -apple-system, sans-serif;
  }

  :global([data-theme="light"]) .clipboard-hint {
    color: #666;
  }

  .clipboard-hint kbd {
    background: rgba(78, 204, 163, 0.15);
    padding: 1px 4px;
    border-radius: 3px;
    font-family: 'SF Mono', 'Fira Code', monospace;
    font-size: 10px;
  }

  :global([data-theme="light"]) .clipboard-hint kbd {
    background: rgba(0, 0, 0, 0.08);
  }

  textarea {
    width: 100%;
    border-radius: 4px;
    padding: 10px;
    font-size: 13px;
    font-family: 'SF Mono', 'Fira Code', monospace;
    resize: vertical;
    min-height: 120px;
    box-sizing: border-box;
    transition: background 0.3s ease, border-color 0.3s ease, color 0.3s ease;
  }

  :global([data-theme="dark"]) textarea {
    background: #0a0a1a;
    border: 1px solid #0f3460;
    color: #e0e0e0;
  }

  :global([data-theme="light"]) textarea {
    background: #f8f8f8;
    border: 1px solid #ddd;
    color: #1a1a2e;
  }

  textarea:focus {
    outline: none;
    border-color: #4ecca3;
  }

  textarea::placeholder {
    color: #5a5a6a;
  }

  .sync-status {
    font-size: 11px;
    color: #4ecca3;
    margin-top: 6px;
    font-family: system-ui, -apple-system, sans-serif;
  }

  .panel-actions {
    display: flex;
    gap: 8px;
    margin-top: 12px;
    justify-content: flex-end;
  }

  .btn {
    padding: 6px 14px;
    border-radius: 4px;
    font-size: 13px;
    font-family: system-ui, -apple-system, sans-serif;
    cursor: pointer;
    border: 1px solid transparent;
    transition: all 0.2s ease;
  }

  .btn.secondary {
    background: #0f3460;
    border-color: #1a4a8a;
    color: #e0e0e0;
  }

  :global([data-theme="light"]) .btn.secondary {
    background: #e8e8e8;
    border-color: #d0d0d0;
    color: #333;
  }

  .btn.secondary:hover {
    background: #1a4a8a;
  }

  :global([data-theme="light"]) .btn.secondary:hover {
    background: #d8d8d8;
  }

  .btn.primary {
    background: #4ecca3;
    color: #0a0a1a;
    font-weight: 500;
  }

  .btn.primary:hover {
    background: #3dbb92;
  }
</style>
