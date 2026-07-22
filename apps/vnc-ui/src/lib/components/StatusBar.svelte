<script>
  import { theme } from '$lib/stores/theme.js';

  let {
    status = 'idle',
    onCtrlAltDel,
    onReconnect,
    onClipboard,
    onFullscreen,
    onSettings,
    collapsed = $bindable(false)
  } = $props();

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

  function toggle() {
    collapsed = !collapsed;
  }
</script>

<aside class="sidebar" class:collapsed>
  <button class="toggle-btn" onclick={toggle} title={collapsed ? 'Expand' : 'Collapse'}>
    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
      {#if collapsed}
        <path d="M9 18l6-6-6-6"/>
      {:else}
        <path d="M15 18l-6-6 6-6"/>
      {/if}
    </svg>
  </button>

  {#if !collapsed}
    <div class="sidebar-content">
      <div class="status-indicator">
        <span class="status-dot" style="background: {statusColors[status] || '#e94560'}"></span>
        <span class="status-label">{statusLabels[status] || status}</span>
      </div>

      <div class="sidebar-divider"></div>

      <nav class="sidebar-actions">
        {#if status === 'connected'}
          <button class="sidebar-btn" onclick={onClipboard} title="Clipboard">
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path d="M16 4h2a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h2"/>
              <rect x="8" y="2" width="8" height="4" rx="1" ry="1"/>
            </svg>
            <span class="btn-label">Clipboard</span>
          </button>
          <button class="sidebar-btn" onclick={onCtrlAltDel} title="Ctrl+Alt+Del">
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <rect x="2" y="6" width="20" height="12" rx="2"/>
              <path d="M6 10h0M10 10h0M14 10h0"/>
              <path d="M8 14h8"/>
            </svg>
            <span class="btn-label">Ctrl+Alt+Del</span>
          </button>
          <button class="sidebar-btn" onclick={onFullscreen} title="Fullscreen">
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path d="M8 3H5a2 2 0 0 0-2 2v3m18 0V5a2 2 0 0 0-2-2h-3m0 18h3a2 2 0 0 0 2-2v-3M3 16v3a2 2 0 0 0 2 2h3"/>
            </svg>
            <span class="btn-label">Fullscreen</span>
          </button>
          <button class="sidebar-btn" onclick={onSettings} title="Settings">
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <circle cx="12" cy="12" r="3"/>
              <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z"/>
            </svg>
            <span class="btn-label">Settings</span>
          </button>
        {/if}

        <button class="sidebar-btn" onclick={onReconnect} title="Reconnect">
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M23 4v6h-6M1 20v-6h6"/>
            <path d="M3.51 9a9 9 0 0 1 14.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0 0 20.49 15"/>
          </svg>
          <span class="btn-label">Reconnect</span>
        </button>
      </nav>

      <div class="sidebar-divider"></div>

      <button class="sidebar-btn theme-btn" onclick={() => theme.toggle()} title="Toggle theme">
        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          {#if $theme === 'dark'}
            <circle cx="12" cy="12" r="5"/>
            <path d="M12 1v2M12 21v2M4.22 4.22l1.42 1.42M18.36 18.36l1.42 1.42M1 12h2M21 12h2M4.22 19.78l1.42-1.42M18.36 5.64l1.42-1.42"/>
          {:else}
            <path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z"/>
          {/if}
        </svg>
        <span class="btn-label">{$theme === 'dark' ? 'Light' : 'Dark'}</span>
      </button>
    </div>
  {/if}
</aside>

<style>
  .sidebar {
    position: fixed;
    left: 0;
    top: 0;
    bottom: 0;
    z-index: 50;
    display: flex;
    flex-direction: column;
    backdrop-filter: blur(12px);
    border-right: 1px solid rgba(78, 204, 163, 0.15);
    transition: width 0.25s ease, background 0.3s ease;
    width: 160px;
    font-family: system-ui, -apple-system, sans-serif;
    user-select: none;
  }

  :global([data-theme="dark"]) .sidebar {
    background: rgba(16, 24, 48, 0.92);
  }

  :global([data-theme="light"]) .sidebar {
    background: rgba(255, 255, 255, 0.92);
    border-right-color: rgba(0, 0, 0, 0.1);
  }

  .sidebar.collapsed {
    width: 36px;
  }

  .toggle-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 100%;
    height: 36px;
    background: none;
    border: none;
    border-bottom: 1px solid rgba(78, 204, 163, 0.1);
    color: #a0a0b0;
    cursor: pointer;
    transition: color 0.2s, background 0.2s;
    flex-shrink: 0;
  }

  :global([data-theme="light"]) .toggle-btn {
    border-bottom-color: rgba(0, 0, 0, 0.1);
    color: #555;
  }

  .toggle-btn:hover {
    color: #4ecca3;
    background: rgba(78, 204, 163, 0.08);
  }

  .sidebar-content {
    display: flex;
    flex-direction: column;
    flex: 1;
    overflow: hidden;
  }

  .status-indicator {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 12px;
  }

  .status-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    flex-shrink: 0;
    transition: background 0.3s ease;
  }

  .status-label {
    font-size: 12px;
    font-weight: 500;
    color: #c0c0d0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  :global([data-theme="light"]) .status-label {
    color: #333;
  }

  .sidebar-divider {
    height: 1px;
    background: rgba(78, 204, 163, 0.1);
    margin: 0 12px;
  }

  :global([data-theme="light"]) .sidebar-divider {
    background: rgba(0, 0, 0, 0.1);
  }

  .sidebar-actions {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 8px 6px;
    flex: 1;
  }

  .sidebar-btn {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 10px;
    background: none;
    border: none;
    border-radius: 6px;
    color: #a0a0b0;
    cursor: pointer;
    transition: all 0.2s ease;
    font-size: 12px;
    font-family: inherit;
    text-align: left;
    white-space: nowrap;
  }

  :global([data-theme="light"]) .sidebar-btn {
    color: #555;
  }

  .sidebar-btn:hover {
    background: rgba(78, 204, 163, 0.1);
    color: #e0e0e0;
  }

  :global([data-theme="light"]) .sidebar-btn:hover {
    background: rgba(0, 0, 0, 0.05);
    color: #1a1a2e;
  }

  .sidebar-btn:active {
    background: rgba(78, 204, 163, 0.18);
  }

  .theme-btn {
    margin-top: auto;
    margin-bottom: 4px;
  }

  .btn-label {
    overflow: hidden;
    text-overflow: ellipsis;
  }

  @media (max-width: 640px) {
    .sidebar {
      width: 36px;
    }
    .btn-label {
      display: none;
    }
  }
</style>
