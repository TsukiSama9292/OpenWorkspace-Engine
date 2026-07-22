<script>
  import { onMount, onDestroy } from 'svelte';
  import RFB from '../vnc/rfb.js';
  import { MouseButtonMapper, XVNC_BUTTONS } from '../vnc/mousebuttonmapper.js';

  let {
    url = '',
    password = 'password',
    status = $bindable('idle'),
    onClipboardText = null
  } = $props();

  let container = $state(null);
  let touchInput = $state(null);
  let rfb = $state(null);
  let errorMessage = $state('');
  let resizeObserver = $state(null);

  onMount(() => {
    if (!url || !container) return;
    connect();
  });

  onDestroy(() => {
    disconnect();
  });

  function connect() {
    if (rfb) disconnect();

    try {
      rfb = new RFB(container, touchInput, url, {
        credentials: { username: '', password }
      });

      const mapper = new MouseButtonMapper();
      mapper.set(0, XVNC_BUTTONS.LEFT_BUTTON);
      mapper.set(1, XVNC_BUTTONS.MIDDLE_BUTTON);
      mapper.set(2, XVNC_BUTTONS.RIGHT_BUTTON);
      mapper.set(3, XVNC_BUTTONS.BACK_BUTTON);
      mapper.set(4, XVNC_BUTTONS.FORWARD_BUTTON);
      rfb.mouseButtonMapper = mapper;

      rfb.addEventListener('connect', () => {
        status = 'connected';
        errorMessage = '';
        // Scale canvas to fill container and tell server to match browser viewport
        rfb.resizeSession = true;
        rfb.updateConnectionSettings();
      });

      rfb.addEventListener('disconnect', (e) => {
        status = 'disconnected';
        if (e.detail && !e.detail.clean) {
          errorMessage = 'Connection lost';
        }
      });

      rfb.addEventListener('credentialsrequired', () => {
        rfb.sendCredentials({ username: '', password });
      });

      rfb.addEventListener('clipboard', (e) => {
        if (onClipboardText && e.detail && e.detail.text) {
          onClipboardText(e.detail.text);
        }
      });

      rfb.addEventListener('error', (e) => {
        errorMessage = e.detail || 'Connection error';
        status = 'error';
      });

      status = 'connecting';
    } catch (e) {
      errorMessage = e.message;
      status = 'error';
    }
  }

  function disconnect() {
    if (rfb) {
      rfb.disconnect();
      rfb = null;
    }
  }

  export function sendCtrlAltDel() {
    if (rfb) rfb.sendCtrlAltDel();
  }

  export function clipboardPaste(text) {
    if (rfb) rfb.clipboardPasteFrom(text);
  }

  export function reconnect() {
    connect();
  }
</script>

<div class="vnc-viewer" bind:this={container}>
  <input type="text" class="touch-input" bind:this={touchInput} />
  {#if errorMessage}
    <div class="vnc-error">
      <span>{errorMessage}</span>
    </div>
  {/if}
</div>

<style>
  .vnc-viewer {
    width: 100%;
    height: 100%;
    position: relative;
    overflow: hidden;
  }

  .touch-input {
    position: absolute;
    opacity: 0;
    width: 0;
    height: 0;
    pointer-events: none;
  }

  .vnc-error {
    position: absolute;
    bottom: 16px;
    left: 50%;
    transform: translateX(-50%);
    background: rgba(233, 69, 96, 0.9);
    color: white;
    padding: 8px 16px;
    border-radius: 4px;
    font-size: 14px;
    font-family: system-ui, -apple-system, sans-serif;
    z-index: 10;
    max-width: 80%;
    text-align: center;
    pointer-events: none;
  }
</style>
