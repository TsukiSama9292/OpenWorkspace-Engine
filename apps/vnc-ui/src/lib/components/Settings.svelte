<script>
  import { theme } from '$lib/stores/theme.js';

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
          <label>Theme</label>
          <div class="theme-toggle">
            <button class="theme-btn" class:active={$theme === 'dark'} onclick={() => theme.set('dark')}>
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z"/>
              </svg>
              Dark
            </button>
            <button class="theme-btn" class:active={$theme === 'light'} onclick={() => theme.set('light')}>
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <circle cx="12" cy="12" r="5"/>
                <path d="M12 1v2M12 21v2M4.22 4.22l1.42 1.42M18.36 18.36l1.42 1.42M1 12h2M21 12h2M4.22 19.78l1.42-1.42M18.36 5.64l1.42-1.42"/>
              </svg>
              Light
            </button>
          </div>
        </div>

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
    border-radius: 8px;
    width: 360px;
    max-width: 90vw;
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.4);
    transition: background 0.3s ease;
  }

  :global([data-theme="dark"]) .settings-panel {
    background: #1e2a3a;
    border: 1px solid #0f3460;
  }

  :global([data-theme="light"]) .settings-panel {
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
    font-family: system-ui, -apple-system, sans-serif;
    display: flex;
    align-items: center;
    gap: 8px;
    cursor: pointer;
  }

  :global([data-theme="dark"]) .setting-group > label {
    color: #a0a0b0;
  }

  :global([data-theme="light"]) .setting-group > label {
    color: #555;
  }

  .theme-toggle {
    display: flex;
    gap: 8px;
  }

  .theme-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 12px;
    border-radius: 6px;
    font-size: 12px;
    font-family: system-ui, -apple-system, sans-serif;
    cursor: pointer;
    border: 1px solid transparent;
    transition: all 0.2s ease;
  }

  :global([data-theme="dark"]) .theme-btn {
    background: #0a0a1a;
    border-color: #0f3460;
    color: #a0a0b0;
  }

  :global([data-theme="light"]) .theme-btn {
    background: #f0f0f0;
    border-color: #ddd;
    color: #555;
  }

  .theme-btn.active {
    border-color: #4ecca3;
    color: #4ecca3;
  }

  .theme-btn:hover {
    border-color: #4ecca3;
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
    border-radius: 2px;
    outline: none;
    transition: background 0.3s ease;
  }

  :global([data-theme="dark"]) input[type="range"] {
    background: #0f3460;
  }

  :global([data-theme="light"]) input[type="range"] {
    background: #ddd;
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
