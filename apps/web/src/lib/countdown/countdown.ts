import type { RemoteType, TimeoutAction } from '$lib/types';

export const WARNING_THRESHOLD_MS = 10 * 60 * 1000;
export const CRITICAL_THRESHOLD_MS = 60 * 1000;

export function remainingMs(
  auto_sleeps_at: string | null | undefined,
  now: number
): number | null {
  if (!auto_sleeps_at) return null;
  const deadline = Date.parse(auto_sleeps_at);
  if (Number.isNaN(deadline)) return null;
  return Math.max(0, deadline - now);
}

export function formatRemaining(ms: number): string {
  const totalSeconds = Math.max(0, Math.ceil(ms / 1000));
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;
  const mm = String(minutes).padStart(2, '0');
  const ss = String(seconds).padStart(2, '0');
  return hours > 0 ? `${hours}:${mm}:${ss}` : `${mm}:${ss}`;
}

export type CountdownSeverity = 'normal' | 'warning' | 'critical';

export function severity(ms: number): CountdownSeverity {
  if (ms < CRITICAL_THRESHOLD_MS) return 'critical';
  if (ms < WARNING_THRESHOLD_MS) return 'warning';
  return 'normal';
}

export function wrapperUrl(remoteType: RemoteType, token: string): string {
  if (remoteType === 'kasmvnc') {
    return `/kasmvnc/${token}/`;
  }
  return `/open/${token}/`;
}

export function iframeSrc(
  remoteType: Exclude<RemoteType, 'kasmvnc'>,
  token: string,
  password: string
): string {
  if (remoteType === 'jupyter') {
    return `/jupyter/${token}/lab?token=${encodeURIComponent(password)}`;
  }
  return `/ttyd/${token}/`;
}

export const TIMEOUT_ACTION_LABELS: Record<TimeoutAction, string> = {
  pause: '暫停',
  stop: '停止',
  remove: '移除'
};
