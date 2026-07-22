<script>
  import { onMount } from 'svelte';
  import VncViewer from '$lib/components/VncViewer.svelte';
  import StatusBar from '$lib/components/StatusBar.svelte';
  import Clipboard from '$lib/components/Clipboard.svelte';
  import Settings from '$lib/components/Settings.svelte';
  import { theme } from '$lib/stores/theme.js';

  let viewer = $state(null);
  let status = $state('idle');
  let sidebarCollapsed = $state(false);

  let clipboardOpen = $state(false);
  let settingsOpen = $state(false);

  let settings = $state({
    quality: 5,
    compression: 5,
    viewOnly: false,
    clipboard: true,
    scaleViewport: true
  });

  let isFullscreen = $state(false);

  function getWebSocketUrl() {
    if (typeof window === 'undefined') return '';
    const loc = window.location;
    const protocol = loc.protocol === 'https:' ? 'wss:' : 'ws:';
    const base = loc.pathname.endsWith('/') ? loc.pathname : loc.pathname + '/';
    return `${protocol}//${loc.host}${base}websockify`;
  }

  async function handleFullscreen() {
    if (!document.fullscreenElement) {
      await document.documentElement.requestFullscreen();
    } else {
      await document.exitFullscreen();
    }
  }

  function handleClipboardRemote(text) {
    if (navigator.clipboard && navigator.clipboard.writeText) {
      navigator.clipboard.writeText(text).catch(() => {});
    }
  }

  function toggleTheme() {
    theme.toggle();
  }

  onMount(() => {
    theme.init();
    document.addEventListener('fullscreenchange', () => {
      isFullscreen = !!document.fullscreenElement;
    });
  });
</script>

<div class="vnc-container" class:fullscreen={isFullscreen}>
  <StatusBar
    {status}
    bind:collapsed={sidebarCollapsed}
    onCtrlAltDel={() => viewer?.sendCtrlAltDel()}
    onReconnect={() => viewer?.reconnect()}
    onClipboard={() => clipboardOpen = true}
    onFullscreen={handleFullscreen}
    onSettings={() => settingsOpen = true}
  />

  <div class="vnc-viewport">
    <VncViewer
      bind:this={viewer}
      bind:status
      url={getWebSocketUrl()}
      onClipboardText={settings.clipboard ? handleClipboardRemote : null}
    />
  </div>
</div>

<Clipboard
  bind:open={clipboardOpen}
  onSend={(text) => viewer?.clipboardPaste(text)}
/>

<Settings
  bind:open={settingsOpen}
  bind:settings
/>

<style>
  :global(:root) {
    --bg-primary: #0a0a1a;
    --bg-secondary: #101830;
    --bg-surface: #1e2a3a;
    --bg-hover: rgba(78, 204, 163, 0.1);
    --bg-active: rgba(78, 204, 163, 0.18);
    --border-primary: rgba(78, 204, 163, 0.15);
    --border-input: #0f3460;
    --text-primary: #e0e0e0;
    --text-secondary: #a0a0b0;
    --text-muted: #5a5a6a;
    --accent: #4ecca3;
    --accent-hover: #3dbb92;
    --danger: #e94560;
    --warning: #f0a500;
    --sidebar-bg: rgba(16, 24, 48, 0.92);
  }

  :global([data-theme="light"]) {
    --bg-primary: #f5f5f5;
    --bg-secondary: #ffffff;
    --bg-surface: #ffffff;
    --bg-hover: rgba(0, 100, 80, 0.08);
    --bg-active: rgba(0, 100, 80, 0.15);
    --border-primary: rgba(0, 100, 80, 0.2);
    --border-input: #d0d0d0;
    --text-primary: #1a1a2e;
    --text-secondary: #4a4a5a;
    --text-muted: #8a8a9a;
    --accent: #00a878;
    --accent-hover: #009068;
    --danger: #d93545;
    --warning: #d89000;
    --sidebar-bg: rgba(255, 255, 255, 0.95);
  }

  :global(html, body) {
    margin: 0;
    padding: 0;
    width: 100%;
    height: 100%;
    overflow: hidden;
    background: var(--bg-primary);
    color: var(--text-primary);
    font-family: system-ui, -apple-system, sans-serif;
    transition: background 0.3s ease, color 0.3s ease;
  }

  :global(*) {
    box-sizing: border-box;
  }

  .vnc-container {
    width: 100vw;
    height: 100vh;
    display: flex;
    background: var(--bg-primary);
    overflow: hidden;
    position: relative;
  }

  .vnc-container.fullscreen {
    position: fixed;
    inset: 0;
    z-index: 9999;
  }

  .vnc-viewport {
    flex: 1;
    min-width: 0;
    height: 100%;
    overflow: hidden;
    position: relative;
  }
</style>
