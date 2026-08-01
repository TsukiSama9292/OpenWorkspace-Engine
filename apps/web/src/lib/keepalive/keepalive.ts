import { api } from '$lib/api/client';

export interface KeepaliveOptions {
  intervalMs?: number;
  isActive?: () => boolean;
  onHeartbeat?: (at: number) => void;
}

function iframeHasFocus(): boolean {
  for (const iframe of document.querySelectorAll('iframe')) {
    try {
      const innerDocument = iframe.contentWindow?.document;
      if (innerDocument && innerDocument.hasFocus()) {
        return true;
      }
    } catch {
      // Cross-origin iframe: cannot inspect it; treat as not focused.
    }
  }
  return false;
}

export function tabHasFocus(): boolean {
  if (typeof document === 'undefined') return false;
  if (document.hasFocus()) return true;
  if (document.activeElement instanceof HTMLIFrameElement) return true;
  return iframeHasFocus();
}

export function isActive(): boolean {
  if (typeof document === 'undefined') return false;
  return document.visibilityState === 'visible' && tabHasFocus();
}

export function startKeepalive(instanceId: string, opts: KeepaliveOptions = {}): () => void {
  if (typeof document === 'undefined') {
    return () => {};
  }

  const intervalMs = opts.intervalMs ?? 10_000;
  const active = opts.isActive ?? isActive;

  const postHeartbeat = () => {
    const res = api.post(`/instances/${instanceId}/heartbeat`);
    void res.then(r => {
      if ('data' in r) opts.onHeartbeat?.(Date.now());
    });
  };

  let wasActive = active();
  if (wasActive) {
    postHeartbeat();
  }

  const send = () => {
    const nowActive = active();
    if (nowActive) {
      postHeartbeat();
    }
    wasActive = nowActive;
  };

  const recompute = () => {
    const nowActive = active();
    if (nowActive && !wasActive) {
      postHeartbeat();
    }
    wasActive = nowActive;
  };

  const interval = window.setInterval(send, intervalMs);

  window.addEventListener('focus', recompute);
  window.addEventListener('blur', recompute);
  document.addEventListener('visibilitychange', recompute);
  document.addEventListener('focusin', recompute);
  document.addEventListener('focusout', recompute);

  return () => {
    window.clearInterval(interval);
    window.removeEventListener('focus', recompute);
    window.removeEventListener('blur', recompute);
    document.removeEventListener('visibilitychange', recompute);
    document.removeEventListener('focusin', recompute);
    document.removeEventListener('focusout', recompute);
  };
}
