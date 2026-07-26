import { writable } from 'svelte/store';
import type { VncSettings } from '$lib/types';

export type VncConnectionStatus = 'idle' | 'connecting' | 'connected' | 'disconnected' | 'credentialsrequired' | 'error';

function createVncSettingsStore() {
  const { subscribe, update } = writable<VncSettings>({
    quality: 5,
    compression: 5,
    viewOnly: false,
    clipboardSync: true,
    scaleViewport: true
  });

  return {
    subscribe,
    setQuality: (quality: number) => update(s => ({ ...s, quality })),
    setCompression: (compression: number) => update(s => ({ ...s, compression })),
    setViewOnly: (viewOnly: boolean) => update(s => ({ ...s, viewOnly })),
    setClipboardSync: (clipboardSync: boolean) => update(s => ({ ...s, clipboardSync })),
    setScaleViewport: (scaleViewport: boolean) => update(s => ({ ...s, scaleViewport }))
  };
}

export const vncSettings = createVncSettingsStore();
