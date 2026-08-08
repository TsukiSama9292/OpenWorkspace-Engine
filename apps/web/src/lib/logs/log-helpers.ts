//! Pure helpers shared by the log surfaces: compact/full audit time
//! formatting, the pinned-to-bottom autoscroll decision, and the persisted
//! log font-size setting. DOM-free and unit-testable, mirroring `ansi.ts`.

function parseIso(iso: string): Date | null {
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? null : d;
}

/** Compact `YYYY-MM-DD HH:MM` rendering of an audit timestamp. */
export function formatAuditTime(iso: string): string {
  const d = parseIso(iso);
  if (!d) return iso;
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

/** Full locale rendering of an audit timestamp, for the `title` tooltip. */
export function fullAuditTime(iso: string): string {
  const d = parseIso(iso);
  if (!d) return iso;
  return d.toLocaleString();
}

/**
 * Decide whether the log viewport should auto-scroll to the newest line:
 * true only while the viewport is pinned to the bottom (within `threshold` px).
 */
export function shouldAutoscroll(
  scrollTop: number,
  scrollHeight: number,
  clientHeight: number,
  threshold = 24
): boolean {
  return scrollTop >= scrollHeight - clientHeight - threshold;
}

/** `localStorage` key shared by every container-log modal. */
export const LOG_FONT_KEY = 'ow-log-font-size';
export const LOG_FONT_MIN = 12;
export const LOG_FONT_MAX = 16;
export const LOG_FONT_DEFAULT = 13;

/** Clamp a font size into the 12-16px range (non-finite → default). */
export function clampFontSize(size: number): number {
  if (!Number.isFinite(size)) return LOG_FONT_DEFAULT;
  return Math.min(LOG_FONT_MAX, Math.max(LOG_FONT_MIN, Math.round(size)));
}

function storage(): Storage | null {
  try {
    return typeof localStorage !== 'undefined' ? localStorage : null;
  } catch {
    return null;
  }
}

/** Load the persisted log font size, falling back to the default. */
export function loadLogFontSize(): number {
  const s = storage();
  if (!s) return LOG_FONT_DEFAULT;
  const raw = s.getItem(LOG_FONT_KEY);
  if (raw === null) return LOG_FONT_DEFAULT;
  const parsed = Number(raw);
  if (!Number.isFinite(parsed)) return LOG_FONT_DEFAULT;
  return clampFontSize(parsed);
}

/** Persist a clamped log font size. */
export function saveLogFontSize(size: number): void {
  storage()?.setItem(LOG_FONT_KEY, String(clampFontSize(size)));
}
