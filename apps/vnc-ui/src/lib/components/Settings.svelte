<script>
  let { open = $bindable(false), settings = $bindable({}) } = $props();

  function handleKeydown(e) {
    if (e.key === 'Escape') {
      open = false;
    }
  }
</script>

{#if open}
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div class="settings-overlay" onclick={() => open = false} onkeydown={handleKeydown} role="dialog" tabindex="-1">
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <div class="settings-panel" onclick={(e) => e.stopPropagation()} onkeydown={handleKeydown} role="document">
      <div class="panel-header">
        <h3>Settings</h3>
        <button class="close-btn" onclick={() => open = false}>×</button>
      </div>
      <div class="panel-body">
        <div class="setting-group">
          <!-- svelte-ignore a11y_label_has_associated_control -->
          <label>Quality</label>
          <div class="setting-row">
            <input type="range" min="1" max="10" bind:value={settings.quality} />
            <span class="setting-value">{settings.quality || 5}</span>
          </div>
        </div>

        <div class="setting-group">
          <!-- svelte-ignore a11y_label_has_associated_control -->
          <label>Compression</label>
          <div class="setting-row">
            <input type="range" min="1" max="9" bind:value={settings.compression} />
            <span class="setting-value">{settings.compression || 5}</span>
          </div>
        </div>

        <div class="setting-group">
          <label>
            <input type="checkbox" bind:checked={settings.viewOnly} />
            View only (no input)
          </label>
        </div>

        <div class="setting-group">
          <label>
            <input type="checkbox" bind:checked={settings.clipboard} />
            Sync clipboard
          </label>
        </div>
      </div>
    </div>
  </div>
{/if}

<style>
  .settings-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.5);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 200;
    backdrop-filter: blur(2px);
  }

  .settings-panel {
    background: #1e2a3a;
    border: 1px solid #0f3460;
    border-radius: 8px;
    width: 360px;
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
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .setting-group {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .setting-group > label {
    font-size: 13px;
    color: #a0a0b0;
    font-family: system-ui, -apple-system, sans-serif;
    display: flex;
    align-items: center;
    gap: 8px;
    cursor: pointer;
  }

  .setting-row {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .setting-value {
    font-size: 12px;
    color: #4ecca3;
    font-family: 'SF Mono', 'Fira Code', monospace;
    min-width: 20px;
    text-align: right;
  }

  input[type="range"] {
    flex: 1;
    height: 4px;
    -webkit-appearance: none;
    appearance: none;
    background: #0f3460;
    border-radius: 2px;
    outline: none;
  }

  input[type="range"]::-webkit-slider-thumb {
    -webkit-appearance: none;
    appearance: none;
    width: 14px;
    height: 14px;
    background: #4ecca3;
    border-radius: 50%;
    cursor: pointer;
  }

  input[type="range"]::-moz-range-thumb {
    width: 14px;
    height: 14px;
    background: #4ecca3;
    border-radius: 50%;
    cursor: pointer;
    border: none;
  }

  input[type="checkbox"] {
    accent-color: #4ecca3;
    width: 14px;
    height: 14px;
  }
</style>
