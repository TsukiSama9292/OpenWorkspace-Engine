<script>
  import { tick } from 'svelte';

  let { open = $bindable(false), onSend } = $props();
  let clipboardText = $state('');
  let textarea = $state(null);

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
  }

  async function handlePaste() {
    try {
      const text = await navigator.clipboard.readText();
      clipboardText = text;
    } catch (e) {
      // Clipboard API may be blocked
    }
  }
</script>

{#if open}
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div class="clipboard-overlay" onclick={() => open = false} onkeydown={handleKeydown} role="dialog" tabindex="-1">
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <div class="clipboard-panel" onclick={(e) => e.stopPropagation()} onkeydown={handleKeydown} role="document">
      <div class="panel-header">
        <h3>Clipboard</h3>
        <button class="close-btn" onclick={() => open = false}>×</button>
      </div>
      <div class="panel-body">
        <textarea
          bind:this={textarea}
          bind:value={clipboardText}
          placeholder="Paste or type text to send to remote..."
          rows="6"
        ></textarea>
        <div class="panel-actions">
          <button class="btn secondary" onclick={handlePaste}>Read from clipboard</button>
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
    background: #1e2a3a;
    border: 1px solid #0f3460;
    border-radius: 8px;
    width: 400px;
    max-width: 90vw;
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.4);
  }

  .panel-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 16px;
    border-bottom: 1px solid #0f3460;
  }

  .panel-header h3 {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
    color: #e0e0e0;
    font-family: system-ui, -apple-system, sans-serif;
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

  textarea {
    width: 100%;
    background: #0a0a1a;
    border: 1px solid #0f3460;
    border-radius: 4px;
    color: #e0e0e0;
    padding: 10px;
    font-size: 13px;
    font-family: 'SF Mono', 'Fira Code', monospace;
    resize: vertical;
    min-height: 120px;
    box-sizing: border-box;
  }

  textarea:focus {
    outline: none;
    border-color: #4ecca3;
  }

  textarea::placeholder {
    color: #5a5a6a;
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

  .btn.secondary:hover {
    background: #1a4a8a;
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
