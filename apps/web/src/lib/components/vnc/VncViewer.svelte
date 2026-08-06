<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import RFB from '$lib/vnc/rfb.js';
  import { MouseButtonMapper, XVNC_BUTTONS } from '$lib/vnc/mousebuttonmapper.js';

  interface Props {
    url?: string;
    password?: string;
    status?: string;
    onClipboardText?: ((text: string) => void) | null;
  }

  // Minimal structural type for the vendored, untyped noVNC RFB client
  // (src/lib/vnc/rfb.js). The real object emits CustomEvent-like events whose
  // `detail` carries connection/clipboard payloads.
  interface RfbEvent {
    detail?: { clean?: boolean; text?: string };
  }

  interface RfbInstance {
    addEventListener(type: string, listener: (e: RfbEvent) => void): void;
    removeEventListener(type: string, listener: (e: RfbEvent) => void): void;
    disconnect(): void;
    sendCredentials(creds: { username: string; password: string }): void;
    sendCtrlAltDel(): void;
    clipboardPasteFrom(text: string): void;
    mouseButtonMapper: unknown;
    resizeSession: boolean;
    updateConnectionSettings(): void;
  }

  let { url = '', password = 'password', status = $bindable('idle'), onClipboardText = null }: Props = $props();

  let container = $state<HTMLDivElement | null>(null);
  let touchInput = $state<HTMLInputElement | null>(null);
  let rfb = $state<RfbInstance | null>(null);
  let errorMessage = $state('');
  let retryCount = $state(0);
  const MAX_RETRIES = 30;
  const RETRY_DELAY = 1000;
  const CONNECT_TIMEOUT = 5000;
  let retryTimer: ReturnType<typeof setTimeout> | null = null;
  let connectTimer: ReturnType<typeof setTimeout> | null = null;
  let destroyed = false;

  let handlers: {
    onConnect: (() => void) | null;
    onDisconnect: ((e: RfbEvent) => void) | null;
    onCredentials: (() => void) | null;
    onClipboard: ((e: RfbEvent) => void) | null;
    onError: (() => void) | null;
  } = { onConnect: null, onDisconnect: null, onCredentials: null, onClipboard: null, onError: null };

  let prevPassword = $state<string | null>(null);

  onMount(() => {
    prevPassword = password;
    if (!url || !container) return;
    connect();
  });

  $effect(() => {
    if (password !== prevPassword && rfb) {
      prevPassword = password;
      retryCount = 0;
      connect();
    } else {
      prevPassword = password;
    }
  });

  onDestroy(() => {
    destroyed = true;
    clearRetry();
    clearConnectTimeout();
    detachRfb();
  });

  function clearRetry() {
    if (retryTimer !== null) {
      clearTimeout(retryTimer);
      retryTimer = null;
    }
  }

  function clearConnectTimeout() {
    if (connectTimer !== null) {
      clearTimeout(connectTimer);
      connectTimer = null;
    }
  }

  function detachRfb() {
    if (!rfb) return;
    if (handlers.onConnect) rfb.removeEventListener('connect', handlers.onConnect);
    if (handlers.onDisconnect) rfb.removeEventListener('disconnect', handlers.onDisconnect);
    if (handlers.onCredentials) rfb.removeEventListener('credentialsrequired', handlers.onCredentials);
    if (handlers.onClipboard) rfb.removeEventListener('clipboard', handlers.onClipboard);
    if (handlers.onError) rfb.removeEventListener('error', handlers.onError);
    handlers = { onConnect: null, onDisconnect: null, onCredentials: null, onClipboard: null, onError: null };
    try { rfb.disconnect(); } catch { /* already disconnected */ }
    rfb = null;
  }

  function scheduleRetry() {
    if (destroyed) return;
    clearRetry();
    clearConnectTimeout();
    if (retryCount >= MAX_RETRIES) {
      errorMessage = `Connection failed after ${MAX_RETRIES} attempts`;
      return;
    }
    retryCount++;
    errorMessage = `Reconnecting... (${retryCount}/${MAX_RETRIES})`;
    retryTimer = setTimeout(() => {
      connect();
    }, RETRY_DELAY);
  }

  function startConnectTimeout() {
    clearConnectTimeout();
    connectTimer = setTimeout(() => {
      if (destroyed) return;
      if (status !== 'connected') {
        errorMessage = 'Connection timed out';
        scheduleRetry();
      }
    }, CONNECT_TIMEOUT);
  }

  function connect() {
    if (destroyed) return;
    clearConnectTimeout();
    detachRfb();

    try {
      rfb = new RFB(container, touchInput, url, {
        credentials: { username: '', password }
      }) as unknown as RfbInstance;
      const instance = rfb;

      const mapper = new MouseButtonMapper();
      mapper.set(0, XVNC_BUTTONS.LEFT_BUTTON);
      mapper.set(1, XVNC_BUTTONS.MIDDLE_BUTTON);
      mapper.set(2, XVNC_BUTTONS.RIGHT_BUTTON);
      mapper.set(3, XVNC_BUTTONS.BACK_BUTTON);
      mapper.set(4, XVNC_BUTTONS.FORWARD_BUTTON);
      instance.mouseButtonMapper = mapper;

      handlers.onConnect = () => {
        clearConnectTimeout();
        status = 'connected';
        errorMessage = '';
        retryCount = 0;
        instance.resizeSession = true;
        instance.updateConnectionSettings();
      };

      handlers.onDisconnect = (e: RfbEvent) => {
        clearConnectTimeout();
        if (e.detail && !e.detail.clean) {
          scheduleRetry();
        }
      };

      handlers.onCredentials = () => {
        instance.sendCredentials({ username: '', password });
      };

      handlers.onClipboard = (e: RfbEvent) => {
        if (onClipboardText && e.detail && e.detail.text) {
          onClipboardText(e.detail.text);
        }
      };

      handlers.onError = () => {
        clearConnectTimeout();
        scheduleRetry();
      };

      instance.addEventListener('connect', handlers.onConnect);
      instance.addEventListener('disconnect', handlers.onDisconnect);
      instance.addEventListener('credentialsrequired', handlers.onCredentials);
      instance.addEventListener('clipboard', handlers.onClipboard);
      instance.addEventListener('error', handlers.onError);

      status = 'connecting';
      startConnectTimeout();
    } catch (e) {
      errorMessage = e instanceof Error ? e.message : String(e);
      status = 'error';
      scheduleRetry();
    }
  }

  export function sendCtrlAltDel() {
    if (rfb) rfb.sendCtrlAltDel();
  }

  export function clipboardPaste(text: string) {
    if (rfb) rfb.clipboardPasteFrom(text);
  }

  export function reconnect() {
    retryCount = 0;
    clearRetry();
    connect();
  }
</script>

<div class="relative w-full h-full overflow-hidden" bind:this={container}>
  <input type="text" class="absolute opacity-0 w-0 h-0 pointer-events-none" bind:this={touchInput} />
  {#if errorMessage}
    <div class="absolute bottom-4 left-1/2 -translate-x-1/2 bg-error-500/90 text-white px-4 py-2 rounded text-sm z-10 max-w-[80%] text-center pointer-events-none">
      {errorMessage}
    </div>
  {/if}
</div>
