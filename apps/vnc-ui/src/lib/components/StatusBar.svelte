<script>
  let { status = 'idle', onCtrlAltDel, onReconnect, onClipboard, onFullscreen, onSettings } = $props();

  const statusLabels = {
    idle: 'Ready',
    connecting: 'Connecting...',
    connected: 'Connected',
    disconnected: 'Disconnected',
    credentialsrequired: 'Credentials required',
    error: 'Error'
  };

  const statusColors = {
    idle: '#4ecca3',
    connecting: '#f0a500',
    connected: '#4ecca3',
    disconnected: '#e94560',
    credentialsrequired: '#f0a500',
    error: '#e94560'
  };
</script>

<div class="status-bar">
  <div class="status-left">
    <span class="status-dot" style="background: {statusColors[status] || '#e94560'}"></span>
    <span class="status-text">{statusLabels[status] || status}</span>
  </div>

  {#if status === 'connected' || status === 'disconnected' || status === 'error'}
    <div class="status-actions">
      {#if status === 'connected'}
      <button class="action-btn" onclick={onClipboard} title="Clipboard">
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M16 4h2a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h2"/>
          <rect x="8" y="2" width="8" height="4" rx="1" ry="1"/>
        </svg>
      </button>
      <button class="action-btn" onclick={onCtrlAltDel} title="Ctrl+Alt+Del">
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <rect x="2" y="6" width="20" height="12" rx="2"/>
          <path d="M6 10h0M10 10h0M14 10h0"/>
          <path d="M8 14h8"/>
        </svg>
      </button>
      <button class="action-btn" onclick={onFullscreen} title="Fullscreen">
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M8 3H5a2 2 0 0 0-2 2v3m18 0V5a2 2 0 0 0-2-2h-3m0 18h3a2 2 0 0 0 2-2v-3M3 16v3a2 2 0 0 0 2 2h3"/>
        </svg>
      </button>
      <button class="action-btn" onclick={onSettings} title="Settings">
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <circle cx="12" cy="12" r="3"/>
          <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z"/>
        </svg>
      </button>
    {/if}

    {#if status === 'connected' || status === 'disconnected' || status === 'error'}
      <button class="action-btn reconnect-btn" onclick={onReconnect} title="Reconnect">
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M23 4v6h-6M1 20v-6h6"/>
          <path d="M3.51 9a9 9 0 0 1 14.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0 0 20.49 15"/>
        </svg>
      </button>
    {/if}
    </div>
  {/if}
</div>

<style>
  .status-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 6px 12px;
    background: #16213e;
    border-bottom: 1px solid #0f3460;
    color: #e0e0e0;
    font-family: system-ui, -apple-system, sans-serif;
    font-size: 13px;
    min-height: 36px;
    user-select: none;
  }

  .status-left {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .status-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    transition: background 0.3s ease;
    flex-shrink: 0;
  }

  .status-text {
    font-weight: 500;
  }

  .status-actions {
    display: flex;
    gap: 4px;
  }

  .action-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    background: transparent;
    border: 1px solid transparent;
    color: #a0a0b0;
    border-radius: 4px;
    cursor: pointer;
    transition: all 0.2s ease;
    padding: 0;
  }

  .action-btn:hover {
    background: rgba(255, 255, 255, 0.1);
    border-color: rgba(255, 255, 255, 0.2);
    color: #e0e0e0;
  }

  .reconnect-btn:hover {
    color: #4ecca3;
  }
</style>
