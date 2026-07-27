<script lang="ts">
  import { onMount } from 'svelte';
  import VncViewer from '$lib/components/vnc/VncViewer.svelte';
  import StatusBar from '$lib/components/vnc/StatusBar.svelte';
  import Clipboard from '$lib/components/vnc/Clipboard.svelte';
  import Settings from '$lib/components/vnc/Settings.svelte';
  import type { VncSettings } from '$lib/types';

  interface Props {
    token: string;
    password?: string;
  }

  let { token, password = 'password' }: Props = $props();

  let viewer = $state<VncViewer | null>(null);
  let status = $state('idle');
  let sidebarCollapsed = $state(false);

  let clipboardOpen = $state(false);
  let settingsOpen = $state(false);

  let settings = $state<VncSettings>({
    quality: 5,
    compression: 5,
    viewOnly: false,
    clipboardSync: true,
    scaleViewport: true
  });

  let isFullscreen = $state(false);

  function getWebSocketUrl(): string {
    if (typeof window === 'undefined') return '';
    const loc = window.location;
    const protocol = loc.protocol === 'https:' ? 'wss:' : 'ws:';
    return `${protocol}//${loc.host}/vnc/${token}/websockify`;
  }

  async function handleFullscreen() {
    if (!document.fullscreenElement) {
      await document.documentElement.requestFullscreen();
    } else {
      await document.exitFullscreen();
    }
  }

  function handleClipboardRemote(text: string) {
    if (navigator.clipboard && navigator.clipboard.writeText) {
      navigator.clipboard.writeText(text).catch(() => {});
    }
  }

  onMount(() => {
    document.addEventListener('fullscreenchange', () => {
      isFullscreen = !!document.fullscreenElement;
    });
  });
</script>

<div class="w-screen h-screen flex bg-surface-950 overflow-hidden relative {isFullscreen ? 'fixed inset-0 z-[9999]' : ''}">
  <StatusBar
    {status}
    bind:collapsed={sidebarCollapsed}
    onCtrlAltDel={() => viewer?.sendCtrlAltDel()}
    onReconnect={() => viewer?.reconnect()}
    onClipboard={() => clipboardOpen = true}
    onFullscreen={handleFullscreen}
    onSettings={() => settingsOpen = true}
  />

  <div class="flex-1 min-w-0 h-full overflow-hidden relative">
    <VncViewer
      bind:this={viewer}
      bind:status
      url={getWebSocketUrl()}
      {password}
      onClipboardText={settings.clipboardSync ? handleClipboardRemote : null}
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
