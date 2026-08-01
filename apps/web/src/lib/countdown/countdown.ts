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

export function deadlineRemaining(
  deadline: string | null | undefined,
  now: number
): number | null {
  return remainingMs(deadline, now);
}

export interface SelectedDeadline {
  deadline: string;
  action: TimeoutAction | null;
}

export function selectDeadline(
  auto_sleeps_at: string | null | undefined,
  timeout_action: TimeoutAction | null | undefined,
  keep_time_deadline: string | null | undefined,
  keep_time_action: TimeoutAction | null | undefined
): SelectedDeadline | null {
  const auto = auto_sleeps_at && !Number.isNaN(Date.parse(auto_sleeps_at)) ? auto_sleeps_at : null;
  const keep =
    keep_time_deadline && !Number.isNaN(Date.parse(keep_time_deadline))
      ? keep_time_deadline
      : null;

  if (auto && keep) {
    if (Date.parse(keep) < Date.parse(auto)) {
      return { deadline: keep, action: keep_time_action ?? null };
    }
    return { deadline: auto, action: timeout_action ?? null };
  }
  if (auto) return { deadline: auto, action: timeout_action ?? null };
  if (keep) return { deadline: keep, action: keep_time_action ?? null };
  return null;
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

export function keepTimePolicyLine(
  keepTimeSeconds: number | null | undefined,
  keepTimeAction: TimeoutAction | null | undefined
): string | null {
  if (!keepTimeSeconds || keepTimeSeconds <= 0) return null;
  if (!keepTimeAction) return null;
  const duration = keepTimeSeconds % 3600 === 0
    ? `${keepTimeSeconds / 3600} 小時`
    : keepTimeSeconds % 60 === 0
      ? `${keepTimeSeconds / 60} 分鐘`
      : `${keepTimeSeconds} 秒`;
  const action = TIMEOUT_ACTION_LABELS[keepTimeAction] ?? TIMEOUT_ACTION_LABELS.pause;
  return `閒置 ${duration}後${action}`;
}
