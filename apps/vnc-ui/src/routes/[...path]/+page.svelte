<script>
  import { onMount } from 'svelte';
  import VncViewer from '$lib/components/VncViewer.svelte';
  import StatusBar from '$lib/components/StatusBar.svelte';
  import Clipboard from '$lib/components/Clipboard.svelte';
  import Settings from '$lib/components/Settings.svelte';

  let viewer = $state(null);
  let status = $state('idle');

  let clipboardOpen = $state(false);
  let settingsOpen = $state(false);

  let settings = $state({
    quality: 5,
    compression: 5,
    viewOnly: false,
    clipboard: true
  });

  function getWebSocketUrl() {
    if (typeof window === 'undefined') return '';
    const loc = window.location;
    const protocol = loc.protocol === 'https:' ? 'wss:' : 'ws:';
    const base = loc.pathname.endsWith('/') ? loc.pathname : loc.pathname + '/';
    return `${protocol}//${loc.host}${base}websockify`;
  }

  function handleFullscreen() {
    if (!document.fullscreenElement) {
      document.documentElement.requestFullscreen();
    } else {
      document.exitFullscreen();
    }
  }
</script>

<div class="vnc-container">
  <StatusBar
    {status}
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
  .vnc-container {
    width: 100vw;
    height: 100vh;
    display: flex;
    flex-direction: column;
    background: #0a0a1a;
    overflow: hidden;
  }

  .vnc-viewport {
    flex: 1;
    width: 100%;
    height: 100%;
    overflow: hidden;
  }
</style>
