<script lang="ts">
  import { onMount } from 'svelte';

  interface Props {
    status?: string;
    onCtrlAltDel?: () => void;
    onReconnect?: () => void;
    onClipboard?: () => void;
    onFullscreen?: () => void;
    onSettings?: () => void;
    collapsed?: boolean;
  }

  let {
    status = 'idle',
    onCtrlAltDel,
    onReconnect,
    onClipboard,
    onFullscreen,
    onSettings,
    collapsed = $bindable(false)
  }: Props = $props();

  const statusLabels: Record<string, string> = {
    idle: 'Ready',
    connecting: 'Connecting...',
    connected: 'Connected',
    disconnected: 'Disconnected',
    credentialsrequired: 'Credentials required',
    error: 'Error'
  };

  const statusColors: Record<string, string> = {
    idle: 'bg-success-500',
    connecting: 'bg-warning-500',
    connected: 'bg-success-500',
    disconnected: 'bg-error-500',
    credentialsrequired: 'bg-warning-500',
    error: 'bg-error-500'
  };

  const STORAGE_KEY = 'vnc-toggle-y';

  let btnTop = $state(50);
  let dragging = $state(false);
  let dragMoved = $state(false);
  let dragStartY = 0;
  let dragStartTop = 0;

  onMount(() => {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (saved !== null) {
      const pct = parseFloat(saved);
      if (pct >= 0 && pct <= 100) btnTop = pct;
    }
  });

  function onPointerDown(e: PointerEvent) {
    if (e.button && e.button !== 0) return;
    dragging = true;
    dragMoved = false;
    dragStartY = e.clientY;
    dragStartTop = btnTop;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }

  function onPointerMove(e: PointerEvent) {
    if (!dragging) return;
    const dy = e.clientY - dragStartY;
    if (Math.abs(dy) > 3) dragMoved = true;
    const btnH = 48;
    const minPct = (btnH / 2 / window.innerHeight) * 100;
    const maxPct = 100 - minPct;
    let newPct = dragStartTop + (dy / window.innerHeight) * 100;
    newPct = Math.max(minPct, Math.min(maxPct, newPct));
    btnTop = newPct;
  }

  function onPointerUp() {
    if (dragging) {
      dragging = false;
      localStorage.setItem(STORAGE_KEY, String(btnTop));
    }
  }

  function toggle() {
    if (dragMoved) return;
    collapsed = !collapsed;
  }
</script>

<button
  class="fixed left-0 z-51 flex items-center justify-center w-6 h-12 bg-surface-900/70 backdrop-blur-md border-none rounded-r-md text-surface-400 cursor-grab transition-all duration-200 touch-none hover:text-primary-400 hover:w-[30px] hover:bg-surface-900/90 {collapsed ? '' : 'left-[160px] cursor-pointer'}"
  style="top: {collapsed ? btnTop : 50}%;"
  onpointerdown={onPointerDown}
  onpointermove={onPointerMove}
  onpointerup={onPointerUp}
  onclick={toggle}
  title={collapsed ? 'Expand' : 'Collapse'}
>
  <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
    {#if collapsed}
      <path d="M9 18l6-6-6-6"/>
    {:else}
      <path d="M15 18l-6-6 6-6"/>
    {/if}
  </svg>
</button>

{#if !collapsed}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="fixed inset-0 z-49 bg-black/30" onclick={toggle}></div>
  <aside class="fixed left-0 top-0 bottom-0 w-40 z-50 flex flex-col backdrop-blur-xl border-r border-primary-500/15 bg-surface-900/92 font-sans select-none">
    <div class="flex flex-col flex-1 overflow-hidden">
      <div class="flex items-center gap-2 px-3 py-2.5">
        <span class="w-2 h-2 rounded-full shrink-0 transition-colors {statusColors[status] || 'bg-error-500'}"></span>
        <span class="text-xs font-medium text-surface-300 truncate">{statusLabels[status] || status}</span>
      </div>

      <div class="h-px bg-primary-500/10 mx-3"></div>

      <nav class="flex flex-col gap-0.5 px-1.5 py-2 flex-1">
        {#if status === 'connected'}
          <button class="flex items-center gap-2 px-2.5 py-2 bg-transparent border-none rounded-md text-surface-400 cursor-pointer transition-all text-xs font-sans text-left whitespace-nowrap hover:bg-primary-500/10 hover:text-surface-200" onclick={onClipboard} title="Clipboard">
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path d="M16 4h2a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h2"/>
              <rect x="8" y="2" width="8" height="4" rx="1" ry="1"/>
            </svg>
            <span class="overflow-hidden text-ellipsis">Clipboard</span>
          </button>
          <button class="flex items-center gap-2 px-2.5 py-2 bg-transparent border-none rounded-md text-surface-400 cursor-pointer transition-all text-xs font-sans text-left whitespace-nowrap hover:bg-primary-500/10 hover:text-surface-200" onclick={onCtrlAltDel} title="Ctrl+Alt+Del">
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <rect x="2" y="6" width="20" height="12" rx="2"/>
              <path d="M6 10h0M10 10h0M14 10h0"/>
              <path d="M8 14h8"/>
            </svg>
            <span class="overflow-hidden text-ellipsis">Ctrl+Alt+Del</span>
          </button>
          <button class="flex items-center gap-2 px-2.5 py-2 bg-transparent border-none rounded-md text-surface-400 cursor-pointer transition-all text-xs font-sans text-left whitespace-nowrap hover:bg-primary-500/10 hover:text-surface-200" onclick={onFullscreen} title="Fullscreen">
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path d="M8 3H5a2 2 0 0 0-2 2v3m18 0V5a2 2 0 0 0-2-2h-3m0 18h3a2 2 0 0 0 2-2v-3M3 16v3a2 2 0 0 0 2 2h3"/>
            </svg>
            <span class="overflow-hidden text-ellipsis">Fullscreen</span>
          </button>
          <button class="flex items-center gap-2 px-2.5 py-2 bg-transparent border-none rounded-md text-surface-400 cursor-pointer transition-all text-xs font-sans text-left whitespace-nowrap hover:bg-primary-500/10 hover:text-surface-200" onclick={onSettings} title="Settings">
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <circle cx="12" cy="12" r="3"/>
              <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z"/>
            </svg>
            <span class="overflow-hidden text-ellipsis">Settings</span>
          </button>
        {/if}

        <button class="flex items-center gap-2 px-2.5 py-2 bg-transparent border-none rounded-md text-surface-400 cursor-pointer transition-all text-xs font-sans text-left whitespace-nowrap hover:bg-primary-500/10 hover:text-surface-200" onclick={onReconnect} title="Reconnect">
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M23 4v6h-6M1 20v-6h6"/>
            <path d="M3.51 9a9 9 0 0 1 14.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0 0 20.49 15"/>
          </svg>
          <span class="overflow-hidden text-ellipsis">Reconnect</span>
        </button>
      </nav>

      <div class="h-px bg-primary-500/10 mx-3"></div>
    </div>
  </aside>
{/if}
